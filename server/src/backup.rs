//! Backups of the run database and their rotation (D40).
//!
//! * a backup every 30 s while the run changes;
//! * kept: the 10 newest, plus the newest of each UTC day for the last 7 days;
//! * after the run ended: only the newest backup, and it is deleted 7 days after the end.
//!
//! Restoring is a deliberate host action (copy a backup over the database file while the server
//! is stopped); the server never rolls back by itself (D40: rollback by the host only).

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::error::StorageError;
use crate::storage::Store;

pub const INTERVAL: Duration = Duration::from_secs(30);
const KEEP_NEWEST: usize = 10;
const DAY: i64 = 86_400;
const DAILY_WINDOW: i64 = 7 * DAY;
const ENDED_RETENTION: i64 = 7 * DAY;

pub fn file_name(unix_time: i64) -> String {
    format!("run-{unix_time:012}.sqlite")
}

fn parse_time(name: &str) -> Option<i64> {
    name.strip_prefix("run-")?
        .strip_suffix(".sqlite")?
        .parse()
        .ok()
}

/// Which of `times` (unix seconds of existing backups) to delete.
///
/// `ended_at` is the end time of the run if it is over.
pub fn plan_prune(times: &[i64], now: i64, ended_at: Option<i64>) -> Vec<i64> {
    let mut sorted = times.to_vec();
    sorted.sort_unstable();
    sorted.dedup();

    if let Some(ended_at) = ended_at {
        let expired = now >= ended_at + ENDED_RETENTION;
        let keep = if expired {
            None
        } else {
            sorted.last().copied()
        };
        return sorted.into_iter().filter(|t| Some(*t) != keep).collect();
    }

    let mut keep: Vec<i64> = sorted.iter().rev().take(KEEP_NEWEST).copied().collect();
    // Newest of each day inside the window: iterate newest first, remember days already kept.
    let mut seen_days = Vec::new();
    for &t in sorted.iter().rev() {
        if t < now - DAILY_WINDOW {
            break;
        }
        let day = t.div_euclid(DAY);
        if !seen_days.contains(&day) {
            seen_days.push(day);
            keep.push(t);
        }
    }
    sorted.into_iter().filter(|t| !keep.contains(t)).collect()
}

fn existing(dir: &Path) -> io::Result<Vec<i64>> {
    let mut times = Vec::new();
    for entry in fs::read_dir(dir)? {
        if let Some(t) = entry?.file_name().to_str().and_then(parse_time) {
            times.push(t);
        }
    }
    Ok(times)
}

/// Write a backup named after `now` and return its path.
pub fn write(store: &Store, dir: &Path, now: i64) -> Result<PathBuf, StorageError> {
    fs::create_dir_all(dir)?;
    let path = dir.join(file_name(now));
    store.backup_to(&path)?;
    Ok(path)
}

/// Apply the retention policy to `dir`. Returns the number of deleted backups.
pub fn prune(dir: &Path, now: i64, ended_at: Option<i64>) -> io::Result<usize> {
    if !dir.exists() {
        return Ok(0);
    }
    let doomed = plan_prune(&existing(dir)?, now, ended_at);
    for t in &doomed {
        fs::remove_file(dir.join(file_name(*t)))?;
    }
    Ok(doomed.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: i64 = 100 * DAY + 12 * 3600;

    #[test]
    fn name_roundtrip() {
        assert_eq!(parse_time(&file_name(1_700_000_000)), Some(1_700_000_000));
        assert_eq!(parse_time("run-1.sqlite.partial"), None);
        assert_eq!(parse_time("notes.txt"), None);
    }

    #[test]
    fn nothing_is_deleted_while_few_recent_backups_exist() {
        let times: Vec<i64> = (0..10).map(|i| NOW - i * 30).collect();
        assert!(plan_prune(&times, NOW, None).is_empty());
    }

    #[test]
    fn keeps_ten_newest_and_one_per_day_for_a_week() {
        // 30 s backups for 2 hours, then one backup per hour for 9 days.
        let mut times: Vec<i64> = (0..240).map(|i| NOW - i * 30).collect();
        times.extend((2..9 * 24).map(|h| NOW - h * 3600));
        let doomed = plan_prune(&times, NOW, None);
        let kept: Vec<i64> = times
            .iter()
            .copied()
            .filter(|t| !doomed.contains(t))
            .collect();

        // The 10 newest survive.
        for i in 0..10 {
            assert!(kept.contains(&(NOW - i * 30)));
        }
        // Older than the window and not among the 10 newest: gone.
        assert!(
            kept.iter()
                .all(|t| *t >= NOW - DAILY_WINDOW || *t > NOW - 10 * 30)
        );
        // At most one survivor per UTC day beyond the newest ten.
        let mut per_day = std::collections::HashMap::new();
        for t in kept.iter().filter(|t| **t <= NOW - 10 * 30) {
            *per_day.entry(t.div_euclid(DAY)).or_insert(0) += 1;
        }
        assert!(per_day.values().all(|n| *n == 1), "{per_day:?}");
        // And the days of the window are all represented.
        assert!(per_day.len() >= 7);
    }

    #[test]
    fn ended_run_keeps_only_the_newest_for_a_week() {
        let times = [10, 20, 30];
        let ended = 1_000;
        assert_eq!(plan_prune(&times, ended + DAY, Some(ended)), vec![10, 20]);
        assert_eq!(
            plan_prune(&times, ended + 7 * DAY, Some(ended)),
            vec![10, 20, 30]
        );
    }
}
