> Research report, 2026-11-18. Unverified statements are marked `[INFERENCE]`.

# EW failure catalogue for a co-op mod with multi-day runs and reconnects

## Summary

Entangled Worlds (EW) mostly fails in three places. The first is persistence: DES entities and chunk data are dropped when a Noita instance disconnects, when the host restarts with Continue, or when a `RelinquishAuthority` races a disconnect. The second is authority: the host hands entities to peers whose Noita is already gone, and entity lookup falls back to stale camera positions. The third is shared-state arithmetic, where hearts, orbs and gold are generated or applied per client, so results depend on who is near what.

Your uncommitted fixes address the first two categories directly. Nothing addresses hearts, shared gold or revive as design-level problems. Those are open.

**Repository state [INFERENCE, git CLI unavailable]:**
- `~/noita_entangled_worlds_fix` has one user commit, `236fe9a8` ("fix(des,world): stop losing DES items and chunk data around disconnects"), on top of upstream `99674cda`. Source: `.git/logs/HEAD`.
- Working-tree changes exist beyond that commit. Grep confirms the following are present: the StartGame guard at `noita_proxy/src/net.rs:687`, `noita_down` in `des.rs` (33, 56, 112, 179, 197), `local_noita_disconnected` (`world.rs:664`, `net.rs:589`), `is_entity_tracked` (`diff_model.rs:266, 316`, `entity_sync.rs:505, 567–568`), `failed_authority` (`diff_model.rs:67, 362, 1206`, `entity_sync.rs:682`), `cam_pos.remove` (`entity_sync.rs:367`), and the explosion-dedup `WorldCut` forwarding from `fix2.patch` and `explosion_forwarding.patch`.
- The fix repo's `docs/perks_that_dont_work_properly.md` lists ABILITY_ACTIONS_MATERIALIZED, RESPAWN, TELEKINESIS, SAVING_GRACE, INVISIBILITY (stains not synced), CORDYCEPS and HOMUNCULUS.

## 1. Problem table

Category key: P = persistence, A = authority, T = terrain, I = items, F = fairness/shared state, U = UX, X = perf/crash.

| # | Symptom (source) | Root cause (file:line) | Cat. | Addressed by your fixes? | Design implication |
|---|---|---|---|---|---|
| 1 | Host restarts with Continue and all DES items (sticks, spells on ground) vanish. 268 saved elements shrink to 436 bytes. (`CONTINUE_RUN_DES_WIPE_PLAN.md`) | The host self-sends `NetMsg::StartGame{init:true}` on PeerConnected (`net.rs` ~684–697). The StartGame handler calls `world.reset(); des.reset(); flags.clear()` (`net.rs` ~837–847). | P | Yes. Guard `id != self.peer.my_id()` at `net.rs:687`. | Restore is a distinct code path from new-game init. Never reset on reconnect. |
| 2 | Items disappear after the host closes Noita while the proxy stays open. Temple items go to an absent friend. (`WORLD_ENTITY_PERSISTENCE_PLAN.md` §3) | `update()` and `should_kill` use a stale `my_pos` (plan cites `ewext`). Chunks freeze. | A/P | Partially (`noita_down` in `des.rs`). | Authority holder must not depend on a live client camera. Use a server-side or explicit location. |
| 3 | Host gives entity to a peer whose Noita is down, and the entity is lost. (`DES_TRANSFER_TO_ABSENT_PEER_PLAN.md`) | `try_ms_write` drops the message when `ms == None` (`noita_proxy` DES transfer path). | A/I | Yes. Host no longer transfers to disconnected peer (commit `236fe9a8`). | Authority transfer must check peer liveness and fall back to host. |
| 4 | Entity placed with the wrong player's position after a disconnect. | `update_entity` calls `locate_player_within_except_me` (`diff_model.rs` ~696–723), which falls back to stale `camera_pos` (`ewext/src/modules.rs` ~20–58). | A | Yes. `cam_pos` removed on disconnect (`entity_sync.rs:367`). | Use a live peer list. A camera position is not an identity. |
| 5 | `RemoveEntities` never clears `cam_pos`. | Nested `with_global` borrow fails silently (`entity_sync.rs` ~361–370). | X/A | Yes (`entity_sync.rs:367`). | Avoid nested global borrows. Make removal errors loud. |
| 6 | Destruction (barrel explosion) when a friend is absent is rolled back when the friend returns. Terrain reappears. (plan 2) | Destruction outside the sync window (±3 chunks of 128 px, `world_sync.lua` `get_all_chunks`) is not recorded. Client `explosion_cuts.lua` is disabled for non-host (`if not ctx.is_host`). | T/P | Partially. `is_host` gating removed from `explosion_cuts.lua` in `fix2.patch` / `explosion_forwarding.patch`. Not in the commit. | Terrain changes need a persistent authoritative log independent of player proximity. |
| 7 | Chunk data lost on RelinquishAuthority race. | `noita_proxy/src/net/world.rs` ~883–912 drops data when the relinquish arrives after disconnect. | T/P | Yes. Late data stored (`world.rs:906`). | Make relinquish idempotent and carry the chunk payload. |
| 8 | Just-granted entity killed and then permanently deleted. | `on_new_entity(kill=true)` for tag `ew_des` (`entity_sync.rs` ~554). The `look_current_entity` cursor (~845) lags on early return. `DeleteEntity` then removes it. | I | Partially. Log `killed stale DES copy` at `entity_sync.rs:592, 620`. | Do not use a cursor that can drift from entity lifecycle. Use IDs. |
| 9 | Spawn failure orphans a granted gid. | `update_pending_authority` (`diff_model.rs` ~1177–1278). | I/A | Partially (`failed_authority`, `diff_model.rs:1206`). | Failed grants must return the entity to a known state. |
| 10 | On joining, about 40 `ewext: killed stale DES copy` lines appear. Duplicate DES copies exist. | Noita-side DES copies are killed when a player loads (`noita.log`, 'slot empty, requesting items'). | I | Logged, not fixed. | Establish one owner per entity at join. Do not rely on kill-and-replace. |
| 11 | Each Noita close logs `Game closed (Lost connection to noita instance: Couldn't receive the length from stream)`, then `Peer 0 disconnected, freed 35 entities` (`proxy.log:159, 161`). | The proxy releases every entity owned by the peer back to the tree on disconnect, including items. | A/I | Partly. The fix keeps entities with the host. | Disconnect must not silently free or drop items. |
| 12 | Wands and items disappear when a run is continued (GitHub #493). | Same DES persistence path as rows 1–2 [INFERENCE]. | I/P | Likely, not verified against #493. | Verify with the new mod's restore test. |
| 13 | Dead player's heart item deleted from inventory in alternate revive (GitHub #522). | Revive removes the item from the dead player's inventory [INFERENCE]. | I/F | No. | Dead player's inventory must be preserved by design, not moved. |
| 14 | Hearts: A collects while B is far; B later finds no heart. (`heart_pickup_investigation.md`) | Hearts are excluded from DES (`entity_sync.rs` `should_be_tracked`, `entity_is_excluded`). Each client generates its own normal hearts via vanilla `spawn_heart`; `wang_scripts.csv` leaves them default-undetoured. Root cause unconfirmed per the doc. | F/I | No. Diagnostics only. | Hearts need a single authoritative spawn-and-claim, or an explicit per-player model. |
| 15 | Hearts/chests: spawn queue lost on restart. | Deferred `spawn_once`/chest replay queue is an in-memory `Vec` in `EntitySync`. Lost on restart, not in `DesManager` saved data. `spawn_once` lacks a terrain-existence check. (`heart_pickup_investigation.md`) | P/T | No. | Deferred spawns must be persisted with the run. |
| 16 | Global-flag gap for `coalmine.lua:spawn_bbqbox`. (`heart_pickup_investigation.md`) | Spawn is gated by a global flag that is not synced [INFERENCE from doc]. | F | No. | Every generation flag must be explicitly classified as shared or personal. |
| 17 | Hearts excluded from `hold_over_ungenerated_world`. | Heart exclusion list (doc). | T/F | No. | Same as 14. |
| 18 | Wands in world differ between singleplayer and multiplayer on the same seed (GitHub #491). | Per-client generation differs [INFERENCE]. | F/T | No. | Generation must be seeded per chunk and per player if personal. |
| 19 | Holy Mountain item-holding bricks sometimes missing (GitHub #495). | [INFERENCE] shop/brick state not synced. | I/T | No. | Shop/brick state needs a shared record. |
| 20 | Invincible enemies, enemies not spawning (GitHub #403, #445, #477). | Ownership mismatch between peers [INFERENCE from #403 text]. | A | No. | Single owner per mob, with reassignment on disconnect. |
| 21 | Shared gold: each peer's wallet is set to the sender's total, then adjusted by a local delta (`quant.ew/files/system/player_sync.lua:28–50`). | Host adds `delta` to its own wallet (`cm + delta`). Clients set `money + my_delta` where `my_delta = cm - last_money`. `last_money` is only updated on clients. | F | No. | Shared gold must be a ledger with a single total, not per-peer deltas. Edge cases [INFERENCE]: double-counting when host and client both change `last_money`; `last_money` is nil until the first sync. |
| 22 | PvP gold split. `pvp.lua:324–334` `give_gold` adds gold to the killer. `pvp.lua:441–446` splits gold by ratio `rt`. | Gold is a local `WalletComponent` value sent over RPC. | F | No. | Gold split must be computed once, on the authority, and then applied. |
| 23 | Orbs spawn at the local player position, deduplicated only against `orbs_found_thisrun` and an in-memory `spawned_orbs` (`orb_sync/orb_sync.lua:9–45`). | Orb pickups are re-spawned per client by RPC `update_orbs`. `spawned_orbs` is lost on reload. | F/P | No. | Orb pickup needs a persisted "claimed by" record. Per-client spawn is not safe across reloads [INFERENCE]. |
| 24 | Revive restores gold and resets inventory from a saved copy. `local_health.lua:406` saves gold, `:472` restores it. | Gold is captured locally, not from the ledger [INFERENCE]. | F | No. | Revive state must come from the authoritative ledger. |
| 25 | Revive: dead player's heart item and inventory handling, `kill_on_revive` (`local_health.lua:431`). | Iron entity tagged `kill_on_revive`. | F/I | No. | Define what the revived bot gets and what the dead player keeps. |
| 26 | Potion thrown at heart does not revive (GitHub #461). | Revive trigger path [INFERENCE]. | F | No. | Revive trigger should be one authoritative event. |
| 27 | Boss cannot be triggered correctly (GitHub #466). | Boss-trigger logic per client [INFERENCE]. | F | No. | Boss state needs one owner. |
| 28 | Kolmi shield orb counts are per client (`kolmi/kolmi.lua:183, 222`; `boss_update.lua:29, 39`). | `GameGetOrbCountThisRun()` read locally. | F | No. | Boss progress counters belong in the shared ledger. |
| 29 | Earthquake staff breaks sync (GitHub #488). | Terrain-moving effect not synced [INFERENCE]. | T | No. | Terrain effects need a sync point or a whitelist. |
| 30 | Explosions duplicated or lost (`explosion_forwarding.patch`). Dedup window 2 s, ±8 px tolerance in `recent_explosions`. | Explosion forwarding had no dedup. | T | Yes, in working tree. | Event dedup by ID, not by position heuristic. |
| 31 | Performance: extreme lag (GitHub #465), LocQ low (GitHub #469), teleport field inconsistent (GitHub #520). | Not investigated in code. | X | No. | Budget sync traffic and chunk streaming explicitly. |
| 32 | Crash on hovering wands (GitHub #449). Crash in late game (GitHub #462). | Logs show `Peer 123467 disconnected` before crash (#462 excerpt). Lua `attempt to call a nil value` in `noita.log`. | X | No. | Unknown; instrument crash paths. |
| 33 | Noita `EntityManager::InsertComponent - had to resize, performance killer?` (`noita.log`). | Vanilla engine message from component churn. | X | No. | Avoid per-frame component add/remove. |
| 34 | Saves do not persist (GitHub #524). Game files and character files. | Not investigated [INFERENCE]. | P | No. | Atomic save and a manifest. |
| 35 | Steam path detection fails when Noita is on another drive (GitHub #504). | Proxy install detection. | U | No. | Non-issue for the new design beyond configuration. |
| 36 | Launch-command problems (GitHub #526, #532, #502). | Launcher UX. | U | No. | Ship a single launcher path. |
| 37 | Polymorphed player picked up as Potion Mimic (GitHub #501). | Polymorph state not synced [INFERENCE]. | I | No. | Polymorph state must be replicated. |
| 38 | Wand/item trajectories inconsistent (GitHub #521). | Physics not authoritative. | I | No. | Projectile physics stay local; only results sync. |

Notes on uncertainty:
- Rows 4, 5, 8 and 11 are verified from the plans and patches. Their line numbers come from the working tree, and the fix commit may not match them exactly.
- Rows 12–13, 18–20, 24–29 and 31–38 are GitHub titles only, or inferred mechanisms. I did not read the issue bodies.

## 2. Log evidence

- `/data/Games/steamapps/common/Noita/ew_log.txt` (2026-11-17, 21:41–21:43): `Has savestate true`, RTree of 327 elements, many `Granting authority … to 0`, many `0 deleted entity Gid(…) (authority: Some(OmniPeerId(0)))`, then `Game closed (Lost connection…)` at 21:43:34, `Peer 0 disconnected, freed 36 entities`, then save of flags, world_chunks, des_entity_storage and run_info_v2 at 21:43:42.
- `~/noita_entangled_worlds_fix/proxy.log`: lines 159/161 and 259/261 show two disconnect cycles, freeing 35 and 51 entities. Line 31 shows an audio stream error. Lines 78–101 show many `0 deleted entity` with authority Some(0).
- `~/Noita-multiplayer/ew_log.txt` (2026-10-18 12:25): host alone, `Has savestate false`, start-game message right after Peer 0 connects.
- `noita.log` (2026-11-17 20:11): ~40+ `killed stale DES copy` lines on load; `DeleteFileImp(… _stats.xml) file failed – error code: 2` at line 1834; Lua nil-call error; missing `data/temp/temp_gun.png`.
- `/data/Games/steamapps/common/Noita/crashes/`: contains only `_FOLDER_DESC.txt`. No crash dumps.
- `logger.txt` (Noita dir): only vanilla warnings in the first 50 matches. No EW errors surfaced.

Pattern: most of the deletion and freeing comes from the disconnect path, not from normal play.

## 3. Heart diagnostics (second checkout)

`~/noita_entangled_worlds/quant.ew/files/system/heart_diagnostics/heart_diagnostics.lua` is opt-in, temporary tracing:
- Appends `LuaComponent` lifecycle hooks to the four heart XMLs (`heart`, `heart_better`, `heart_evil`, `heart_fullhp`). `heart_fullhp` is inherited by the temple heart.
- Wraps chest scripts (`utility_box`, `chest_random`, `chest_random_super`, `chest_steel`, `chest_light`, `chest_dark`, `chest_darkness`).
- Stores an origin key in `VariableStorageComponent` `ew_heart_trace_origin`. This is a correlation key, not a dedup identity (the file says so).
- Samples state once per second. Logs `state_changed` and `gone_without_remove_callback`.
- Calls `ewext.set_heart_trace(true)` if present. Falls back to Lua-only tracing otherwise.

`scripts/test_heart_diagnostics.lua` is a mock-API test. It asserts 8 added/removed pairs, chest callback forwarding, and that an older DLL does not abort init. It does not test multiplayer.

## 4. Upstream issues (GitHub, fetched 2026-11-18 via API)

Open issues relevant to this design (title, number, date):
- #522 Alternate Revive: dead player heart item deleted (2026-09-28)
- #524 Game/character files don't save (2026-10-09)
- #493 Wands and items disappear when continuing a run (2026-07-15, 2 comments)
- #495 Holy Mountain item-holding bricks sometimes missing (2026-07-20)
- #491 Wands differ SP vs MP on same seed (2026-06-14)
- #488 Earthquake staff breaks sync (2026-05-17)
- #466 BOSS can't be triggered (2026-02-11, 3 comments)
- #465 Extreme lag (2026-01-26)
- #462 Crashing in late game (2026-01-12); log excerpt shows peer disconnect before crash
- #461 Potion at heart not reviving another player (2026-01-11)
- #469 LocQ low / unplayable (2026-02-25)
- #403 Invincible enemies, enemies with mismatched ownership (2025-10-12, closed)
- #445 Extremely high health / invincible enemies (closed as duplicate)
- #501 Polymorphed into Potion Mimic (2026-07-25)
- #457 Unique shops for everyone in Holy Mountain: a feature request, confirming shop state is per-client today.

Sources: https://github.com/IntQuant/noita_entangled_worlds/issues?q=is%3Aissue ; https://github.com/IntQuant/noita_entangled_worlds/issues/462 ; https://github.com/IntQuant/noita_entangled_worlds/issues/403 .

I did not read issue bodies. Only titles and the search excerpts were checked.

## 5. Requirements derived from EW failures

1. **Restore is not init.** A host restart with saved state must never run the new-game reset path. Keep the guard at `net.rs:687` as a rule, not a patch. (Rows 1, 12.)
2. **Single authoritative owner per entity**, with explicit reassignment on disconnect to a live peer or the host. Never transfer to a peer whose client is down. (Rows 3, 20.)
3. **No camera-based identity.** Use peer liveness and entity IDs. Camera position is only for culling. (Rows 4, 5.)
4. **Persist destruction and terrain changes server-side** regardless of player proximity. Do not gate terrain persistence on `is_host`. (Rows 6, 7, 30.)
5. **Relinquish and transfer must be idempotent and carry payload.** A late message must not drop data. (Rows 7, 9.)
6. **Persist all deferred spawns** (chests, hearts, shop bricks) with the run, not in memory. (Rows 15, 16, 17, 23.)
7. **Classify every generation as shared or personal** before writing code. Make hearts and chests explicit: either one authoritative spawn-and-claim, or a per-player deterministic spawn. Never leave it to client-side vanilla code. (Rows 14, 16, 17, 18.)
8. **Shared gold is a ledger.** One total on the authority; clients apply signed deltas from that ledger, not local wallet diffs. Splitting or pickup must happen once. (Rows 21, 22, 24.)
9. **Shared orbs need a persisted "claimed" set**, not in-memory dedup. (Row 23.)
10. **Revive and polymorph state come from the authoritative record.** Define the revived bot's inventory and gold source explicitly. (Rows 13, 24, 25, 26, 37.)
11. **Event dedup by ID**, not by position heuristic. (Row 30.)
12. **Budget sync traffic and chunk streaming**, with a measured limit, to avoid the lag reports. (Row 31.)
13. **Atomic saves with a manifest** so a disconnect cannot leave partial state. (Row 34.)
14. **Instrument crash and disconnect paths** before shipping, since the crash cause in #462 is unconfirmed. (Row 32.)
15. **Hearts: decide the fairness model first** (personal, multiplied, or shared). Diagnostics alone will not fix it. (Row 14, `heart_pickup_investigation.md`.)

## 6. Open items

- I could not read issue bodies, so the upstream rows are title-level.
- I did not confirm the uncommitted fix set against a git diff. Only the grep evidence in section 0 supports it.
- Shared-gold edge cases in row 21 are inferred from reading `player_sync.lua`. They are not reproduced.