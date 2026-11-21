use std::net::{SocketAddr, TcpListener};
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Duration;

use clap::{Args, Parser, Subcommand};
use noita_coop_protocol::RunSettings;
use noita_coop_server::engine::Engine;
use noita_coop_server::error::OpenError;
use noita_coop_server::net::{ServeOptions, Server};
use noita_coop_server::storage::NewRun;
use noita_coop_server::{backup, storage};

#[derive(Parser)]
#[command(version, about = "Run server of the Noita co-op project")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Start a NEW run. Refuses to touch an existing database.
    Create(CreateArgs),
    /// Continue an EXISTING run. Refuses to create one.
    Serve(ServeArgs),
}

#[derive(Args)]
struct CreateArgs {
    /// Database file of the run (must not exist).
    #[arg(long)]
    db: PathBuf,
    /// Player name; repeat 2 to 4 times. The order defines the slots.
    #[arg(long = "player", required = true, num_args = 1)]
    players: Vec<String>,
    /// World seed. Random if omitted.
    #[arg(long)]
    seed: Option<u64>,
    /// Content of Noita's `_version_hash.txt` (the build the run is pinned to, D19).
    #[arg(long)]
    game_hash: String,
    /// NG+ count the run starts with (D26).
    #[arg(long, default_value_t = 0)]
    ng_count: u32,
    /// Allowed mod (D23); repeat for several.
    #[arg(long = "mod")]
    mods: Vec<String>,
    #[arg(long, default_value = env!("CARGO_PKG_VERSION"))]
    mod_version: String,
}

#[derive(Args)]
struct ServeArgs {
    #[arg(long)]
    db: PathBuf,
    #[arg(long, default_value = "127.0.0.1:7777")]
    listen: SocketAddr,
    /// Backup directory. Default: `<db>.backups`.
    #[arg(long)]
    backup_dir: Option<PathBuf>,
    #[arg(long, default_value_t = backup::INTERVAL.as_secs())]
    backup_interval_secs: u64,
    /// Allow listening on a non-loopback address. The transport is plain TCP: no encryption,
    /// no server authentication (see `net.rs`). Use only inside a trusted virtual network.
    #[arg(long)]
    insecure_plaintext: bool,
}

fn main() -> ExitCode {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    let result = match Cli::parse().command {
        Command::Create(args) => create(args),
        Command::Serve(args) => serve(args),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("error: {message}");
            ExitCode::FAILURE
        }
    }
}

fn create(args: CreateArgs) -> Result<(), String> {
    let seed = match args.seed {
        Some(seed) => seed,
        None => {
            let mut bytes = [0u8; 8];
            getrandom::fill(&mut bytes).map_err(|e| e.to_string())?;
            u64::from_le_bytes(bytes)
        }
    };
    let new = NewRun {
        seed,
        ng_count: args.ng_count,
        game_hash: args.game_hash.trim().to_owned(),
        mod_version: args.mod_version,
        mods: args.mods,
        player_names: args.players,
        settings: RunSettings::default(),
    };
    let (engine, tokens) = Engine::create(&args.db, &new).map_err(|e| e.to_string())?;
    let info = engine.info();
    println!("run {} created in {}", info.run_id, args.db.display());
    println!("seed {}", info.seed);
    println!("Tokens are shown once; only their hashes are stored. Give each player their own:");
    for t in tokens {
        println!("slot {} {:<16} {}", t.slot, t.name, t.token);
    }
    Ok(())
}

fn serve(args: ServeArgs) -> Result<(), String> {
    if !args.listen.ip().is_loopback() && !args.insecure_plaintext {
        return Err(format!(
            "{} is not a loopback address and the transport is unencrypted; \
             pass --insecure-plaintext to accept that",
            args.listen
        ));
    }
    let backup_dir = args.backup_dir.unwrap_or_else(|| {
        let mut s = args.db.as_os_str().to_owned();
        s.push(".backups");
        PathBuf::from(s)
    });

    let engine = match Engine::resume(&args.db) {
        Ok(engine) => engine,
        Err(OpenError::RunEnded { run_id, ended_at }) => {
            // Housekeeping for a finished run: the last backup is kept for 7 days (D40).
            let _ = backup::prune(&backup_dir, storage::unix_now(), Some(ended_at));
            return Err(format!("run {run_id} has ended; nothing to serve"));
        }
        Err(e) => return Err(e.to_string()),
    };
    let listener = TcpListener::bind(args.listen).map_err(|e| format!("cannot listen: {e}"))?;
    let server = Server::start(
        engine,
        listener,
        ServeOptions {
            backup_dir: Some(backup_dir),
            backup_interval: Duration::from_secs(args.backup_interval_secs),
        },
    )
    .map_err(|e| e.to_string())?;

    // The first stdout line is machine-readable: tests and launch scripts read the port from it.
    println!("listening {}", server.local_addr());

    let handle = server.shutdown_handle();
    ctrlc::set_handler(move || handle.shutdown()).map_err(|e| e.to_string())?;
    server.wait().map_err(|e| e.to_string())
}
