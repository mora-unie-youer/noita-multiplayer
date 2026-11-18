# Entangled Worlds architecture report (read-only)

> Research report, 2026-11-18. Nothing was modified, built, or run; claims come from static reads. **[INFERENCE]** marks anything not verified in code or in game. Written before vanilla `data.wak` was unpacked: the "drop_money not readable" blocker in §7b is now resolvable via `.cache/noita-data/data/scripts/items/drop_money.lua` (see `vanilla_pickups.md`).

Paths are relative to `~/noita_entangled_worlds_fix/` unless absolute.

## Summary

- **Three layers.** Lua mod `quant.ew/` runs inside Noita. Native Rust module `ewext/` loads in-process and handles DES, world decode, and projectile hooks. External Rust proxy `noita_proxy/` handles GUI, networking, and persistence. Peers connect through either a QUIC star relayed by the host (tangled) or a Steam full mesh.
- **World.** Chunks stream near the camera, each with single-writer authority that can be taken over by priority. The host saves chunks on `Drop`. Entities use DES with a global `Gid` and per-peer `Lid`, and authority moves by distance.
- **Players.** Remote avatars are re-simulated locally from replicated controls, not moved by position alone.
- **Shared health.** The host holds a pool. One unit is 25 HP, and each player adds `health_per_player/25` units.
- **Revive.** Only in local-health modes. A bot (`notplayer.xml` + `notplayer_ai.lua`) gets the dead player's inventory, and the revive fires when the bot dies.
- **Shared gold.** Pickups are counted once, through a host ledger of client deltas. **Drops are the main risk.** Every copy of a DES enemy keeps the vanilla `drop_money.lua`, kills on copies are lethal, and far peers replay deaths. If vanilla `drop_money` pays on any death, gold multiplies with the number of holders, with or without `share_gold`. **Not confirmed in game.**
- **Highest correctness risks.** Gold drop multiplication. Melee on non-host clients is discarded in shared-health mode. Non-host immortality may zero damage before forwarding. Unreliable absolute snapshots have no sequence numbers. Persistence depends on host `Drop`. `spawn_once` is never pruned. The TLS server identity is not verified. There is no per-message protocol version. Engine offsets are hard-coded.

## Files

| Path | Role | Anchors |
|---|---|---|
| `noita_proxy/src/net.rs` (1673) | Proxy net loop, Lua↔proxy messages, peer events, settings to Lua | `send` 306, `broadcast` 326, `start_inner` 366, `PeerConnected` 676-692 (Welcome 685, `StartGame{init:true}` 687), `Raw` outbound 1209-1229, `share_gold` to Lua 1025, `pvp_kill_steal` forced 0 at 1102-1107, `RequestMods`/`Mods`/`ModRaw`/`ModCompressed` 819-851, continue-run guards 687/874 |
| `noita_proxy/src/net/steam_networking.rs` (851) | Steam transport, full mesh | `SendBacklog` 130-203, `connect` 368, `poll_status` 339-373, `send_message` 394/631, `broadcast_message` 640-650, `ew_version` 531/582-599 |
| `noita_proxy/tangled/src/lib.rs` (341), `connection_manager.rs`, `helpers.rs` | QUIC host-relayed star | `lib.rs` 1-133 (self-push 29-44, `send_internal` 94-107); `connection_manager.rs` 290-352, 369-400, 400-412, 423-432 (`server_broadcast_internal_message` skips sender), 475 (clients send to host); `helpers.rs` 9-26 (`SkipServerVerification`, `verify_server_cert` 18-25 returns `assertion()`) |
| `noita_proxy/src/net/des.rs` (311) | Proxy DES state | `noita_down` ~33-56, `TransferAuthorityTo` ~179, `noita_disconnected` ~197, TODO 111-120 |
| `noita_proxy/src/net/world.rs` (3498) | Chunk authority, persistence | `WorldNetMessage` 40-140, `ChunkState` 138-160, `WorldManager` 176, `add_end` 338, `chunk_updated_locally` 375, `update` 509, `reset` 653, `handle_msg` 743, `handle_peer_left` 1195, `should_process_chunk` 2406, stored-chunk load 883-905, `Drop` ~2420-2430 |
| `noita_proxy/src/chunk.rs` (7.6 KB) | Pixel format | 12-bit `((material+1)<<1)\|flag`, `UNKNOWN_RAW=4095`, `MAX_MATERIAL=2046`, `Chunk{pixels:[u16;16384], changed:[bool;16384]}` |
| `noita_proxy/src/game_settings.rs` (581) | Settings and modes | `GameSettings` 13-49, defaults 51-122 (`share_gold` 104, UI 507-509), `LocalHealthMode` 126, `GameMode` 134 |
| `noita_proxy/src/bookkeeping/save_state.rs` | Persistence | `SaveState` 24, `new` 31, `save` 55, `load` 70, `mark_game_started` 89, `reset` 95, `has_savestate` 101 |
| `noita_proxy/src/bookkeeping/mod_manager.rs` | Mod version check | `is_mod_ok` 394-428; `NP_SKIP_MOD_CHECK` 410 |
| `noita_proxy/src/lobby_code.rs` | Lobby code version | `CodeVersionMismatch` 20, `VERSION='0'` 33, check 45; handled at `app.rs:865` |
| `shared/src/des.rs`, `shared/src/lib.rs` | DES radii, `RemoteDes`, `SpawnOnce` | `des.rs` 3, 219-220; `lib.rs` 29 |
| `shared/src/message_socket.rs` | TCP framing | 4-byte LE length + bitcode, 64 MiB max, 256 KiB BufWriter, TCP_NODELAY |
| `ewext/src/lib.rs` (938) | Lua-callable native API | `module_on_projectile_fired` 455, `des_death_notify` 719-746, chest `SpawnOnce` 866-868, `BrokenWand` 898-900 |
| `ewext/src/modules/entity_sync.rs` (1021) | DES glue, `spawn_once`, replay | `spawn_once` field 55, `gold_nugget` exclusion 132, replay 209-266, `RemoteDes` handling 439-464, `ChestOpenRequest` 439-456, `cross_death_notify` 512-520, `sync_projectile` 530-545, persistence step 561, `DeadEntities` send 779-792, `on_projectile_fired` 929-975 |
| `ewext/src/modules/entity_sync/diff_model.rs` (3228) | DES core | `pending_death_notify` 51 (tracker) and 279 (model), `drops_gold` on copies 139/181/244, `upload_new_items` 224, `remove_entities` 319, `release_authority` 851, `transfer_authority_to` 870, `track_entity_inner` 939 (`drops_gold` 1032-1040), `ew_gid_lid` 978-983/1002/2731, `death_notify.lua` attach 999, `update_tracked_entities` 1311-1347, transfer hysteresis 1371-1403, `init_granted_entity` ~1275-1309 (drop removal 1292), `death_notify` 1685-1697, `apply_entities` budget 2186-2240, `kill_entities` 2355-2443, `spawn_projectiles` 2495-2520, `init_remote_entity` 2539-2640 (drop removal 2623-2629) |
| `ewext/src/noita/decode.rs` (677), `pixel.rs` | Pixel decode | `RUST_DECODE_MAX_FAILURES=3`, `native_world_decode` default true |
| `ewext/src/addr_grabber.rs` | Engine offsets | Hard-coded for one 32-bit build, e.g. `0x007aa7ce-0x007aa540`, inline asm `__thiscall` |
| `quant.ew/init.lua` (728) | Mod entry and hooks | `OnProjectileFired` 193, `ew_projectile_position_sync` 297, `OnProjectileFiredPost` 341, `ew_player_count` default 421-422, local spawn 425-493, `ew_kill_player` 479, seed chain ~221-265, `load_modules`, `OnWorldPostUpdate` flush |
| `quant.ew/files/core/net.lua` | RPC layer | `opts_reliable` 22-23, `opts_everywhere` 27-30, `create_var` 33-55, `rpc_meta.__newindex` 60-80, `send` 207, `send_to_host` 223 |
| `quant.ew/files/core/player_fns.lua` (737) | Player spawn, input and position (de)serialize | `serialize_inputs` 280-313, `deserialize_inputs` 74-113, `deserialize_position` 476-500, `spawn_player_for` 542-606 |
| `quant.ew/files/system/player_sync.lua` (388) | Controls and position replication, gold, inventory request | gold `last_money` 5, `send_money_and_ingestion` 24-56, sender 179-208, `ew_client` 20, avatar spawn 76, `deserialize_inputs` call 99, gate 241, gold sender 279-312 |
| `quant.ew/files/system/damage/damage.lua` (241) | Shared-health pool | see §8 |
| `quant.ew/files/system/damage/cbs/*.lua` | Damage callbacks | `send_damage_to_host` 5-25, `host_adjust_received_damage` 5-21, `send_damage_to_client` 2-31, `adjust_damage` 5-12 |
| `quant.ew/files/resource/cbs/immortal.lua` (20), `files/system/local_health/immortal.lua` (12) | Immortality callbacks | see §8 |
| `quant.ew/files/system/local_health/local_health.lua` (931) | Local-health death, bot, revive | `player_died` 360-483, `fake_polymorph_into_entity` 309-325, `on_world_update` 562, death check 630-672, `on_client_spawned` 695-727, `on_poly_death` 775-819, `rpc.melee_damage_client` 882 |
| `quant.ew/files/system/local_health/notplayer/notplayer.xml` (675) | Bot entity | `drop_items_on_death="0"` 310, DamageModel `hp="4"`, `invincibility_frames="60"`, quick slots 10, `ControlsComponent enabled="0"` |
| `quant.ew/files/system/local_health/notplayer_ai.lua` (1673) | Bot AI [path not re-verified] | `MAX_RADIUS` 11 (=128·5), `INVIS_RANGE` 30, `is_suitable_target` ~666-668, `find_target` ~1336 (`ew_client`), activation 1660 |
| `quant.ew/files/system/world_sync/world_sync.lua` (308) | Chunk streaming (Lua) | `send_chunks`, `KEY_WORLD_FRAME=0`, `KEY_WORLD_END=1`, cadence int 4, priority 16 for `ew_notplayer` |
| `quant.ew/files/system/orb_sync/orb_sync.lua` (~110) | Orbs [INFERENCE on size] | `orbs_found_this_run`, `actual_orbs_update`, `rpc.update_orbs` |
| `quant.ew/files/system/heart_pickups/heart_pickups.lua` (~70) | Hearts [INFERENCE on size] | `rpc.heart_pickup`, multiplier 20, heal formula |
| `quant.ew/files/system/entity_sync_helper/death_notify.lua`, `entity_sync_helper.lua` | DES death hook | `death` 1, `CrossCall ew_death_notify` 9; `util.add_cross_call` 168; `ewext.des_death_notify` 210; tag 141 |
| `quant.ew/files/system/world_sync_cuts/…/explosion_cuts.lua` (7.8 KB) | Explosion replication | Host-only gates 174 and 187 (exact directory not re-checked) |
| `quant.ew/files/system/pvp.lua` (743) | PvP | `give_gold` 314-348 |
| `docs/…`, `.omp/plans/*.md`, `~/noita_entangled_worlds/docs/heart_pickup_investigation.md` | Design notes | Plans: WORLD_ENTITY_PERSISTENCE (step 1 applied, 4a applied, 4b not), DES_TRANSFER_TO_ABSENT_PEER (applied), CONTINUE_RUN_DES_WIPE (applied) |
| `/data/Games/steamapps/common/Noita/tools_modding/` | Engine docs | `lua_api_documentation.txt:9` (`EntityKill`, no description); `component_documentation.txt:761, 1384` (`drop_items_on_death`), `1531-1532` (`damage_about_to_be_received` / `damage_received`), `1540` (`script_death` gets `drop_items`) |

## Architecture

```
Noita process (Proton)
 ├─ quant.ew Lua: init.lua, core/net.lua (RPC), core/player_fns.lua, system/*
 │    └─ require("ewext") → in-process Rust cdylib: DES, world decode, projectile hooks
 │          └─ TCP localhost 21251, 4-byte LE length + bitcode
 ▼
noita_proxy: net.rs, net/des.rs, net/world.rs, bookkeeping/save_state.rs
 ├─ tangled: QUIC (quinn 0.11.5) star; clients → host; host relays, excluding sender
 └─ Steam: SteamNetworkingSockets full mesh; lower ID initiates
```

Event flows:
1. **Gold pickup:** local wallet change → `send_money_and_ingestion` (unreliable) → host adds the delta to its ledger → host broadcasts the absolute value → other clients adopt it.
2. **Enemy death:** authority's `death_notify` → `pending_death_notify` → `KillEntity` to near copies (lethal damage) and `DeadEntities` to far peers → far peers replay later.
3. **Hit on shared pool:** client `damage_received` → `deal_damage` (reliable) → host `inflict_damage` → pool broadcast every 4 frames (unreliable).
4. **Death in local mode:** HP ≤ 0 → bot polymorph, inventory serialized to a global → owner's `on_poly_death` → `revive_message` → inventory and gold restored → flags cleared.
5. **Terrain change:** camera chunk → `encode_area` → proxy `world.rs` → chunk authority → remote `decode` → paint.

## 1. Process model and communication

- **Lua↔ewext.** `ewext_init.lua:116-129` calls `ewext.module_on_projectile_fired`; `ewext/src/lib.rs` registers functions with `add_lua_fn!` (e.g., 746).
- **Lua↔proxy.** TCP on localhost, default port 21251 (`NP_NOITA_ADDR` overrides), proxy `listen(1)`. Framing is a 4-byte LE length plus bitcode, 64 MiB max, 256 KiB BufWriter, TCP_NODELAY (`shared/src/message_socket.rs`).
- **Outbound routing.** `net.rs` ~1209-1229: byte0 `&3==2` means ModRaw broadcast, `&4` means reliable; byte1 is the destination (1 proxy, 2 broadcast, 3 proxy-bin).
- **Proxy wire.** Bitcode plus lz4 (`net.rs` ~306-335). Decompression is capped at 64 MiB (~125-131). **No per-message protocol version.**
- **Message set.** `NetMsg` in `noita_proxy/src/net/messages.rs`: Welcome, RequestMods, Mods, EndRun, Kick, PeerDisconnected, StartGame{settings,init}, ModRaw, ModCompressed, WorldMessage, PlayerColor, RemoteMsg, ForwardDesToProxy, ForwardProxyToDes, NoitaDisconnected, Flags, RespondFlag{Normal,Slow,Moon,Stevari}, PlayerPosition, AudioData, MapData, MatData.
- **Mod gating.** Lobby char `'0'` (`lobby_code.rs:33,45`). Mod version file must equal `Version::current()` (`mod_manager.rs:394-428`), skipped by `NP_SKIP_MOD_CHECK` (410). Joiners can receive mods via RequestMods/Mods/ModCompressed (`net.rs:819-851`) [INFERENCE on exact conditions].
- **Lua RPC layer** (`core/net.lua`).
  - `net.send` (207) → `send_internal` (195/212), default destination broadcast. `send_to_host` at 223.
  - Reliability and "everywhere" flags are captured when `function rpc.X` is defined (`rpc_meta.__newindex` 60-80). `opts_reliable()` applies only to the next definition. Default is unreliable and broadcast.
  - Wire key is the registration index [INFERENCE: fragile if load order differs between clients].
  - `create_var` (33-55) replicates last values and re-sends them on `on_should_send_updates`.

## 2. Lobby and transport

**Tangled (QUIC star).**
- Clients send everything to the host (`connection_manager.rs:475`). The host relays with `server_broadcast_internal_message(excluded, …)` (423-432), which skips the sender.
- Host is a single point of failure. Client loss does not affect others.
- **TLS encrypts but does not authenticate the server.** `SkipServerVerification` returns `assertion()` (`helpers.rs:18-25`). A MITM on the path can impersonate the host.

**Steam (SteamNetworkingSockets).**
- Full mesh: N(N−1)/2 pairwise sessions. `connect(peer)` (~368-373) waits when `peer > my_id` (AwaitingIncoming), so the lower-ID side initiates [INFERENCE: initiation body not read].
- Reconnect handled in `poll_status` (339-373) via `pending_reconnect`.
- `broadcast_message` (640-650) skips self.
- Version: `ew_version` lobby key written at 531, read at 582-583. Mismatch gives `ConnectError::VersionMismatch` (585-592). Missing or unparsable gives `VersionMissing` (598-599).
- App ID 881100 (`steam_helper.rs:106`); 480 for testing.

**Shared.** `OmniPeerId(u64)` (`net/omni.rs`) wraps both `PeerVariant::{Tangled, Steam}`.

**Host authority.** Host is peer 0 (tangled) or the lobby host (Steam). No host migration was observed [INFERENCE: `is_host` is fixed per lobby]. If the host leaves, the shared pool, the gold ledger, and DES authority all go with it.

## 3. World and pixel sync

**Lua side** (`world_sync/world_sync.lua`, 308 lines).
- `send_chunks` → `world.encode_area` → `ewext.encode_area` → `net.proxy_bin_send(KEY_WORLD_FRAME=0)`.
- `KEY_WORLD_END=1` carries priority and `ocx:ocy:cx:cy:n:world_num`.
- Cadence default is every 4 frames. Chunks near `ew_notplayer` (the bot) get priority 16, so the bot's surroundings stay live.

**Wire.**
- Header, 12 B: `{i32 x, i32 y, u8 w, u8 h, u16 run_count}`.
- Run, 5 B: `{u16 len-1, i16 material, u8 flags}`. `PIXEL_RUN_MAX=16385`. `encode_area` limit is 128×128.

**Proxy** (`net/world.rs`).
- Each chunk has a single writer. `ChunkState` (138-160): `RequestAuthority{priority,can_wait}`, `WaitingForAuthority`, `Listening{authority,priority}`, `Authority{listeners,priority,new_authority,stop_sending}`, `UnloadPending`, `Transfer`, `WantToGetAuth{…}`. Takeover is by priority. The transition logic was not read [INFERENCE].
- `should_process_chunk` (2406) filters by distance and ray occlusion, **not** ownership.
- Persistence: stored chunks load at 883-905; `Drop` (~2420-2430) saves chunk storage **only on the host**.

**Decode.** `ewext/src/noita/decode.rs` (677 lines) and `pixel.rs`. Fallback after `RUST_DECODE_MAX_FAILURES=3`.

**Explosions.** `explosion_cuts.lua` gates on host at 174 and 187, so client explosions are not replicated. Plan step 4b (client explosion forwarding) is **not applied**.

**Limits.**
- Live sync covers only chunks near a player. Far terrain is persisted, not live-synced.
- No terrain check in chest logic (see §6).
- Authority races are possible at chunk boundaries [INFERENCE].
- `world.lua` (318 lines) has a TODO on null `construct_cell`. `world.rs` has a TODO on exits (~173).

## 4. Entity sync (DES)

**Identity.** `Gid(u64)` is global. `Lid(u32)` is per peer. `ew_gid_lid` is a VariableStorage component on DES-owned entities holding gid/lid and an owner bool (`diff_model.rs:978-983`; writers 1002, 2731).

**Radii** (`shared/src/des.rs`): request authority 512, transfer 512, authority 640, global transfer 768, global authority 896, interest request 1024.

**Authority transfer.** The local player beyond `AUTHORITY_RADIUS` (640) from the camera decrements `wait_to_transfer`. Otherwise it resets to 16. Transfer fires when `should_transfer && wait_to_transfer==0` (`diff_model.rs:1371-1403`).

**Budget.** `APPLY_ENTITIES_FRAME_BUDGET_US=5000`, minimum 1000 (2186-2190). `is_cheap_pass` (2201) lets entities before the start index run even when out of budget.

**Grant and remote copy.**
- `init_granted_entity` (~1275-1309): the new owner removes `drop_money.lua` only if `!drops_gold` (1292). For `boss_dragon`, it adds `boss_dragon_death.lua`. It then calls `track_entity` and `dont_upload.insert`.
- `init_remote_entity` (2539-2640): removes physics, AI, camera, streaming, and lifetime components. Zeroes ranged and melee counts. Makes animal AI inert. Zeroes worm and dragon bite damage. Adds the DES tag and `polymorphable_NOT`. Drop logic at 2623-2629: removes `drop_money.lua` when `!drops_gold`, plus a list of other death, damage, and kick scripts that would duplicate effects.

**Death path.**
1. `death_notify.lua` (`entity_sync_helper/`) calls `ew_death_notify` (`:9`). `entity_sync_helper.lua:168` registers it and `:210` calls `ewext.des_death_notify`.
2. `lib.rs:719-746` → `entity_sync.rs:512-520` → `diff_model.rs:1685-1697`, which pushes to `tracker.pending_death_notify` with **no ownership check**.
3. `update_tracked_entities` (1311-1347) drains that list: `tracked.remove_by_right` (1325-1333), `KillEntity` pushed (1342), `dead.push((pos, SpawnOnce::Enemy(file, drops_gold, responsible)))` (1347), untrack, remove.

**Kill on copies.** `kill_entities` (2355-2443) applies `inflict_damage(hp + MIN_POSITIVE, DamageCurse, "kill sync")` and then `inflict_damage(max_hp*100, …)`. It calls `entity.kill()` only if no kill flag is set. Lookups go through `tracked`, so copies are in `tracked` (2357-2363).

**Far peers.**
- `DeadEntities` goes only to peers **not in `interest_tracker`** and not self (`entity_sync.rs:779-792`). Near peers get `KillEntity`. Far peers get `SpawnOnce` records.
- Records land in `spawn_once` (`:55`) via `RemoteDes::DeadEntities` (extend, :464) or `RemoteDes::SpawnOnce` (push, :455).
- Replay (209-266): when the local `(x,y)` is within `512+256` of a record, the batch size is `len/20` per `frame%20`. For an enemy it loads the file at the position, tags `ew_no_enemy_sync`, and calls `init_remote_entity(…, drops_gold, …)` (:229). Boss special cases apply, and `kill_later` handles the centipede. Otherwise it sets `damage_multipliers.curse=1.0` and calls `inflict_damage(max_hp*100, DamageCurse, "kill sync", responsible)` (:255-266). **The replayed enemy dies immediately.**
- Comment at 214-220: `spawn_once` is drained only when the camera comes within range, and nothing prunes it, so it grows without bound over a session. The removal line is in the part of the replay not shown in this read.

**Chests.** `ChestOpenRequest` (`entity_sync.rs:439-456`): near peers (`has_interest`) get `ChestOpen`. Far peers get `SpawnOnce::Chest` (reliable). `lib.rs:866-868` and `898-900` also create `SpawnOnce` records.

**Exclusions.** Gold nuggets are not DES (`entity_sync.rs:132`; `ENTITY_EXCLUDES` 30-41).

**Proxy.** `net/des.rs` tracks `noita_down` and reassigns authority. Session log evidence: `Peer 0 disconnected, freed 35 entities`, `Granting authority over 25 entities near (-264,1350) to 1`, `ewext: killed stale DES copy` ×41. On peer leave, `remove_entities` (`diff_model.rs:319`) frees copies.

**Projectile replication.** See §5.

**Items.** DES items are exclusive. The first pickup removes them for others [INFERENCE: `temporary_untrack_item` 766 and `upload_new_items` 224 were not read in full]. Wand data travels in `FullEntityData` (`DES_TRANSFER_TO_ABSENT_PEER_PLAN.md:97`).

## 5. Player sync and projectiles

**Sender** (`player_sync.lua:179-208`). Every frame: `rpc.player_update(input_data, pos_data, phys_info, current_slot, my_team)`, unreliable.
- `input_data` from `serialize_inputs` (`player_fns.lua:280-313`): kick, fire, fire2, action, throw, interact, left, right, up, down, jump, fly, leftClick, rightClick, aim vectors, mouse, gamepad.
- `frame%300==53`: `rpc.check_gamemode(mode, world_seed, world_num, ending_completed)`. A mismatch stops world sync.

**Receiver.**
- `rpc.player_update` (defined after the gold RPC in `player_sync.lua`): spawns the avatar if missing (:76). Team sets `GenomeSetHerdId` `player_pvp` or `player`. Inputs go through `deserialize_inputs` (`player_fns.lua:74-113`, sets `ControlsComponent` `mButtonDown*`).
- Position goes through `deserialize_position` (`player_fns.lua:476-500`): sets position, zeroes platforming velocity, then `util.set_phys_info` or `EntityApplyTransform`. Velocity is scaled by `util.fps_ratio`.

**Consequence.** Remote avatars are simulated by the local engine from replicated controls. Weapon fire, melee, and RNG must match across machines. EW seeds the RNG chain (`player_fns.set_projectile_seed`, `init.lua` ~221-265). Divergence under physics or RNG differences was **not tested** [INFERENCE].

**Projectiles.**
- Player shots are not DES. `on_projectile_fired` (`entity_sync.rs:929-975`) serializes only projectiles whose shooter is DES-tracked. It skips `m_entity_that_shot` and a filename list (`boss_wizard/summon.xml`, `bat.xml`, `potion_aggressive.xml`, `pebble.xml`, `cocktail.xml`, `fungus.xml`).
- Pipeline: `init.lua:193` → `ewext_init.lua:116-129` → `lib.rs:455` → `modules.rs:75` → `entity_sync.rs:929-975`. Remote copies re-spawn through `diff_model.rs:2495-2520` (`shooter_entity.shoot_projectile`). `sync_projectile` is at `entity_sync.rs:530-545`.
- Projectile hits on owners therefore happen locally on each client, because the projectile exists everywhere.

**Other.**
- `rpc.request_items(peer_id)` (~70): the owner sends inventory every 240 frames via `net.send_player_inventory`.
- Gate on `ew_flag_notplayer_active` at 241.

## 6. Shared pickups: orbs, hearts, chests

**Orbs** (`orb_sync.lua`, ~110 lines [INFERENCE]).
- The host reads `WorldStateComponent.orbs_found_thisrun` and sends the list with `rpc.update_orbs(found)` (reliable).
- Each client spawns every not-yet-found orb as an auto-pickup at its **own** player's position. Tags `ew_no_enemy_sync`, `ItemComponent{enable_orb_hacks, auto_pickup}`, `OrbComponent{orb_id}`, `script_item_picked_up = data/scripts/items/orb_pickup.lua`.
- Sends on orb-count change or `frame%300==23`. **Result: every player collects every orb.**

**Hearts** (`heart_pickups.lua`, ~70 lines [INFERENCE]).
- Global `ew_heart_pickup` → `rpc.heart_pickup(heart)` (reliable) plus a local apply. **Every client applies the effect to its own player, host included.**
- `multiplier = GlobalsGetValue("HEARTS_MORE_EXTRA_HP_MULTIPLIER","1")` (line 20). No writer exists in EW code.
- `player_count = ew_player_count` (default "1", `init.lua:421-422`). Heal is `min(max_hp-hp, max_hp/player_count)`.
- Max-HP table `{normal={1,false}, better={2,false}, evil={2,false}, temple={0.4,true}}` [INFERENCE].
- Readers of `ew_player_count`: `heart_fullhp.lua:10`, `heart_fullhp_temple.lua:11`, `perk_patches.lua:26`.
- Shared-health mode routes heart math through the pool (§8), so the max gain applies **once**.

**Heart statue** (`heart_statue.lua`, 13.3 KB). Local-health only. `ew_flag_heart_picked_up` is set at 579 and read at 585. Possibly dead code: `is_entity_in_quick_inventory` may always be false [INFERENCE].

**Chests.**
- `chest_sync.lua:on_open` sends the open event. Near peers get `ChestOpen`, far peers get `SpawnOnce::Chest` (§4).
- **Who generates contents** (one client, or each copy) was not traced. [INFERENCE: if each near copy generates its own contents, chests are N-fold, the same risk as gold.]
- The second checkout's uncommitted investigation (`~/noita_entangled_worlds/docs/heart_pickup_investigation.md`, lines 1-303 read; 304-308 not) lists these gaps: no terrain check, an in-memory replay queue, and late joiners who miss chest and death events. Root cause unconfirmed. A mock-only hypothesis: `coalmine.lua:spawn_bbqbox` → `spawn_heart`, gated by EW's global flag.

## 7. Shared gold

### 7a. Pickups (ledger). Verified in code.

- `local last_money` (`player_sync.lua:5`) is a **single module-level watermark** shared by sender and receiver.
- **Sender** (`on_world_update`, `frame%37==9`, 279-312):
  - If the polymorph state changed, restore `wallet.money = last_money` (296).
  - `delta = wallet.money − last_money` (306-307). `last_money = wallet.money` (309).
  - `rpc.send_money_and_ingestion(last_money, delta, ingestion_size)` (311-312). **Unreliable.**
- **Receiver** `rpc.send_money_and_ingestion(money, delta, ingestion_size)` (24-56), active when `ctx.proxy_opt.share_gold and last_money ~= nil` (28):
  - **Host:** `wallet.money = cm + delta` (the host branch in 24-46). The host is the ledger.
  - **Non-host, sender is host:** `my_delta = cm − last_money` if changed (39-41). `last_money = money` (42). `wallet.money = money + my_delta` (44-46).
  - **Any sender:** the sender's avatar wallet is set to `money` (50). Ingestion is set at 52-55.
- **Verdict.** Each pickup is counted once in the host ledger. Absolute host values are broadcast, and each non-host adds its own pending delta. No loopback double-count: the relay excludes the sender, and Steam excludes self.
- **Lost delta.** If an unreliable delta is lost, `last_money` has already advanced, so the next host snapshot reverts that pickup. This is deflation for that player, not inflation [INFERENCE].
- **Reordering.** Unreliable absolute values have **no sequence numbers**. An out-of-order stale snapshot can regress a wallet until the next snapshot [INFERENCE].
- **PvP.** `pvp.lua:314-348` `give_gold`. `pvp_kill_steal` is forced to 0 when `share_gold` is on (`net.rs:1102-1107`).
- **Setting.** `share_gold` defaults to false (`game_settings.rs:104`). The UI label is at 507-509. The proxy passes it to Lua at `net.rs:1025`.

### 7b. Drops. Main risk. Code-derived, not tested.

**Chain:**

1. **Flag at track time.** `drops_gold` is true when the death script is `drop_money.lua` and the entity has no `no_gold_drop` variable (`diff_model.rs:1032-1040`). It is copied into `EntityInit` data (139/181/244).

2. **Copies keep the script.** `init_remote_entity` removes `drop_money.lua` **only when `drops_gold` is false** (2623-2629). For normal enemies, every copy keeps it.

3. **Copies die on kill sync.** `kill_entities` (2355-2443) applies lethal damage to each copy. If the vanilla death handler pays gold, every copy pays.

4. **Far replays.** Replay (`entity_sync.rs:222-266`) respawns the enemy, keeps the drop script (flag true), and kills it. Each far peer that comes within range pays once per record.

5. **Echo.** `death_notify` is not gated on ownership (`diff_model.rs:1685-1697`), and `update_tracked_entities` drains everything. So a copy's kill-sync death may re-enter the pipeline, producing a `KillEntity` and `DeadEntities` to far peers, and more replays [INFERENCE: requires `death_notify` to be attached to copies; `diff_model.rs:999` is in the tracking path, copy attachment not confirmed].

6. **Nuggets are local.** Nuggets are not DES (`entity_sync.rs:132`), so each client that runs the death creates its own nuggets, and only its own player collects them [INFERENCE: depends on vanilla `drop_money.lua`].

**Blocker.** The vanilla script is not extracted (`/data/Games/steamapps/common/Noita/data/scripts/items/drop_money.lua` does not exist in the install, and `drop_items` semantics are not documented: `component_documentation.txt:1540` and `EntityKill` at `lua_api_documentation.txt:9` have no description). **Whether a non-owner death pays gold is therefore unresolved.**

**Consequence if it pays.** Gold multiplies with the number of holders (near copies) plus far replays. With `share_gold`, each such nugget is also summed into the host ledger, so the total is N-fold regardless of ownership. Without `share_gold`, the same multiplication occurs for each player's own wallet. The repo's docs and plans contain no related issue. A grep for gold and nugget across `.omp/plans`, `docs`, and `README.md` found nothing relevant.

**Falsifier.** If vanilla `drop_money` ignores kill-sync deaths (e.g., it checks `drop_items`), this risk disappears.

**Test to run in game.** Two clients in the same room, `share_gold` on and off. Kill a common enemy and count nuggets and gold on each client. Then walk a far client to an earlier death site.

## 8. Shared-health pool and damage routing

**Pool.** Globals `ew_shared_hp` and `ew_shared_max_hp` live on the host (`damage.lua:129-142`). Clients never write them, apart from these paths: `on_new_player_seen` (110-114), `heart_pickups.lua:53-54`, `perk_patches.lua:29-35`, `patch_meat_biome.lua:21`.

**Units.** 1 unit = 25 HP (`rpc.deal_damage` prints `damage*25`). `on_new_player_seen` adds `health_per_player/25` units to both hp and max (default 100 → 4 units).

**Host publish.** `on_world_update_host` (94-108): every 4 frames (`frame%4==3`), `do_health_diff` (non-polymorphed entity) runs, then `rpc.update_shared_health(hp,max)` (unreliable). If hp ≤ 0, `do_game_over` runs (37-51) and `rpc.trigger_game_over` (reliable).

**Clients.** `do_health_diff` (81-92) sets the avatar's entity HP to the pool. If the difference from the last value is nonzero, it calls `damage_received(diff, nil)`. Clients write no pool globals.

**Client forward.** `on_world_update_client` (68-77): every 15 frames (`frame%15==2`), `rpc.deal_damage(recent_damage, recent_message)` (reliable). Host calls `inflict_damage` (clamped to [0, max], 144-147).

**Routing by who got hit** (`damage/cbs/`):

| Victim | Callback | What it does | Forwarded? |
|---|---|---|---|
| Host's own player | `host_adjust_received_damage.lua` 5-21 | Adjust, restore `hp = health + damage` (18), `CrossCall ew_ds_damaged` **always** (21) | Yes, always |
| Client's own player | `send_damage_to_host.lua` 5-25 | Return if world (5-7). Restore `hp = max(health+damage, 0.01)` (15). Adjust (19-20). Forward **only** if `dtypes ~= 1 or responsible == 0` (22-25) | Non-melee and environmental only |
| Remote avatar (monster hit) | `send_damage_to_client.lua` 2-31 | Return if world, projectile, or `ew_peer` (2-8). Find `ew_gid_lid` (15-28). Forward if `dtypes==1 and gid and value_bool` (29-31) | Melee from DES entities, owned by this peer |

- Line 10 of `send_damage_to_client.lua` says "Change our health back", but the file does not restore HP. **Stale comment.**
- Melee forwarding goes through `rpc.melee_damage_client(target_peer, damage, message)` (reliable, `local_health.lua:882`; cross-call 887). On the target client it calls `EntityInflictDamage(entity, damage, "DAMAGE_MELEE", …)`.

**Two suspected defects (code-derived, untested):**
- **(a) Client-authoritative melee is discarded.** A DES monster whose authority is a non-host client hits that client's own player. `send_damage_to_host.lua:15` restores HP, and 22-25 do not forward melee from entities. Net effect: no damage reaches the pool. Melee works only when the **host** holds authority over the monster [INFERENCE].
- **(b) Non-host immortality may zero forwarded damage.** `resource/cbs/immortal.lua` makes non-world damage on non-host avatars 2^-38 (11-13) and sends negatives to −2^-38 (15-17). It passes damage only under the host flag (3-8). `immortal.lua` returns `damage, crit`, which matches the `damage_about_to_be_received` signature (`component_documentation.txt:1531-1532`). `damage_received` (`send_damage_to_client.lua:31`) would then see the post-immortal amount, about 0 [INFERENCE: engine ordering not documented]. Projectile hits are unaffected, because projectiles are re-spawned on every client.

**Damage scaling.**
- `adjust_damage.lua` 5-7: fire, toxic, and poison (types 16, 32768, 65536) are divided by `ew_player_count`.
- 8-12: glass cannon (`PERK_PICKED_GLASS_CANNON_PICKUP_COUNT ~= "0"`) multiplies by 5 (line 11).
- `ew_player_count` only increases (see §10), so divisors go stale after disconnects.

**Hearty effect.** `rpc.effect_hearty(applied)` (reliable, everywhere, host-only): applied halves health (min 0.04) and max, and increments `ew_effect_hearty`. Removal reverses it. Early return if max ≤ 0.4.

**Local mode immortality.** `local_health/immortal.lua` (12 lines) passes types 1, 524288, and 8192, and sets any other damage above 0 to 2^-38. No world or host check. Applied to all clients (`local_health.lua:695-727`).

## 9. Local-health death, bot, revive (local modes only)

**Modules loaded** (`init.lua`, `load_modules`). Shared-health mode loads damage, heart_pickups, patch_meat_biome, kivi_patch. Local-health modes load local_health, notplayer_ai, spectator_helps, end_fight, heart_statue. Both add randomize_perks, rnd_wands (unless `same_loadout`), and pvp.

**Death detection.**
- `local_health.lua:630-672`: at HP ≤ 0, HP is reset to `max(3·max/20, min(2/5, max))` (×5 if `no_notplayer`, 646-647), then `player_died()`.
- Second path: `polymorph.lua:55-75`, HP ≤ 0 calls `on_poly_death()` (73), with a 5-frame wait.

**`player_died()`** (`local_health.lua:360-483`). Sets `ew_flag_notplayer_active` (382). Perma-death branches at 387 and 480. Tags `kill_on_revive` (431). Flag clear at 477.

**Bot creation.** `fake_polymorph_into_entity` (309-325):
- Loads `notplayer.xml` (`drop_items_on_death="0"` at line 310, so the bot drops nothing on death).
- Tags `ew_notplayer`, `polymorphed`, `polymorphed_player`.
- Serializes the dead player into the global `ew_local_player_dead` via base64 `util.serialize_entity` (319). Inventory travels inside this global, not through the ledger.
- `fake_unpolymorph` (327-338) reverses it.

**Bot AI** (`notplayer_ai.lua`). `MAX_RADIUS = 128*5 = 640` (line 11), `INVIS_RANGE = 30` (13). Targets `ew_client` entities (`find_target` ~1336; `is_suitable_target` ~666-668). Active only while `GameHasFlagRun("ew_flag_notplayer_active") and not perma_death` (1660).

**Revive** (`ctx.cap.health.on_poly_death`, 775-819):
1. If `notplayer_active`: if `ending_game_completed` and not `ew_kill_player`, return (778). If `no_notplayer`, end the poly effect and call `player_died()`.
2. Otherwise: `rpc.revive_message()` (759, everywhere); `get_gold()`; `rpc.remove_homing(true)`; `inventory_helper.get_item_data`; `remove_inventory()`; `GameRemoveFlagRun("ew_flag_notplayer_active")` (794); `fake_unpolymorph()`; `remove_stuff()`; `inventory_helper.set_item_data(…)`.
3. Lines 803-833: `hearty_end.lua` `execute_on_removed` is disabled. If `ew_kill_player` is set, clear it (813-814), switch the entity, then apply delayed `EntityInflictDamage` (a real death). If `ending_game_completed`, heal −1000000. Otherwise HP = 2^-38.

**Revive HP.** `rpc.loss_hp()` (~754-758) scales HP and max by `(100 − health_lost_on_revive)/100` (default 0).

**Reconnect while dead.** `init.lua:479`: if the bot is active on join, HP → 2^-38, `EntityInflictDamage(1000000, "DAMAGE_CURSE", "dont rejoin")`, and `GameAddFlagRun("ew_kill_player")`. Result: a rejoining dead player dies for real.

**Revive kill.** Survivors' hits on the bot happen on the owner's client, through replicated controls (§5) and DES projectiles [INFERENCE: in-game test pending]. Bot chunks get priority 16 (§3).

**Shared mode has no bot.** `load_modules` does not load local_health or notplayer_ai in shared mode, so **there is no revive in shared-health mode**.

**Perma-death.** `LocalHealthMode::PermaDeath` (`game_settings.rs:126`; `net.rs:1074`). The bot's activity check includes `not perma_death` (`notplayer_ai.lua:1660`).

## 10. Persistence and reconnect

**Save state** (`save_state.rs`).
- `SaveState` (24) with `new` (31), `save<D>` (55), `load<D>` (70), `mark_game_started` (89), `reset` (95), `has_savestate` (101), `path_for_filename` (105). Bitcode plus `compress_prepend_size`.
- `game_started` gates saving (`SaveStateInner` 18).
- Files: `des_entity_storage.bit`, `world_chunks`, `flags`, `run_info_v2`. Directory per plan: `~/.local/share/entangledworlds/save_state`. `save_paths.rs`: `SavePaths{settings_path, save_state_path}`, `Prefer::{Custom, NextToExe, ProjectDirs}`.

**Save points.**
- Host-only chunk save on `WorldManager::Drop` (`net/world.rs` ~2420-2430).
- DES persistence step 1 at `entity_sync.rs:561` (applied).
- Chunk load at `net/world.rs:883-905` (step 4a, applied).
- **No periodic save observed** [INFERENCE: a crash between saves, or a crash that skips Drop, loses state]. For multi-day runs this is the key gap.

**Continue run.** `CONTINUE_RUN_DES_WIPE_PLAN.md` applied. `net.rs:687` and `:874` skip the wipe for `id == self.peer.my_id()`.

**Reconnect.**
- New peers get `Welcome` (685). The host sends `StartGame{settings, init:true}` to new peers (687).
- `remove_entities` (`diff_model.rs:319`) and proxy `handle_peer_left` (`net/world.rs:1195`) handle departure.
- `check_gamemode` every 300 frames: a mismatch stops world sync.
- Late joiners miss in-memory `SpawnOnce` chest and death records, which are also lost on ewext restart (§6).

**Monotonic player count.** `ew_player_count` increases on first sight of each remote peer (`player_fns.lua:583-606`, global `ew_seen_<peer>`) and is **never decremented**. Heart division and fire division (§6, §8) drift after disconnects.

## 11. Settings and game modes

**Modes.** `GameMode` (134): Shared → SharedHealth. LocalNormal, LocalAlternate, LocalPermadeath → LocalHealth(Normal/Alternate/PermaDeath). PvP.

**Defaults** (`game_settings.rs` 84-122): `game_mode` LocalHealth(Normal); `share_gold` false (104); `health_per_player` 100; `max_players` 250; `health_lost_on_revive` 0; `randomize_perks` true; `same_loadout` false; `physics_damage` true; `friendly_fire` false; `pvp_kill_steal` 50 (forced 0 with `share_gold`); `dont_steal` true; `wait_on_players` true; `time_in_hm` 300; `time_out_hm` 600; `timed` true; `chest_on_win` true; `revive_on_drop` false (usage not traced); `enemy_hp_mult` 1.0; `debug_mode` false.

**Flags.** `'0'` Normal, `'1'` Slow, `'2'` Moon, `'3'` Stevari. Sent via `RespondFlag` variants.

## 12. Lines of code per component

| Component | Size |
|---|---|
| `init.lua` | 728 |
| `player_fns.lua` | 737 |
| `player_sync.lua` | 388 |
| `world_sync.lua` | 308 |
| `world.lua` | 318 |
| `local_health.lua` | 931 (33.5 KB) |
| `notplayer_ai.lua` | 1673 (58.2 KB) |
| `notplayer.xml` | 675 |
| `pvp.lua` | 743 (23.7 KB) |
| `damage.lua` | 241 |
| `spectate.lua` | 15.7 KB |
| `player_tether.lua` | 14.4 KB |
| `heart_statue.lua` | 13.3 KB |
| `explosion_cuts.lua` | 7.8 KB |
| `orb_sync.lua` | ~110 [INFERENCE] |
| `heart_pickups.lua` | ~70 [INFERENCE] |
| `ewext/src/lib.rs` | 938 |
| `entity_sync.rs` | 1021 (41.8 KB) |
| `diff_model.rs` | 3228 (132.2 KB) |
| `decode.rs` | 677 |
| `noita_proxy/src/net.rs` | 1673 |
| `net/world.rs` | 3498 (122.6 KB) |
| `net/des.rs` | 311 |
| `steam_networking.rs` | 851 (30.1 KB) |
| `game_settings.rs` | 581 (24.4 KB) |
| `app.rs` | 63.9 KB |
| `chunk.rs` | 7.6 KB |
| `encoding.rs` | 4.2 KB |
| `world_model` | 21.3 KB |
| `tangled/src/lib.rs` | 341 |
| `tangled/connection_manager.rs` | **unresolved** (reads gave 525 and 851) |

## 13. Requirement mapping

| Requirement | What EW does | Verdict |
|---|---|---|
| **1. Full world sync** (terrain, dropped items, wands, mobs) | Chunks stream near cameras, with per-chunk authority and host save. Mobs and items are DES. Wands travel in `FullEntityData`. | **Partial.** Live sync only near players. Host-only save, no periodic save. Authority races at boundaries. |
| **2. Shared pickups** (orbs and similar to all) | Orbs and hearts apply on every client. Chests go through `ChestOpen`/`SpawnOnce`. Items are DES and exclusive. | **Works by design** for orbs and hearts. Chest duplication **unverified**. |
| **3. Revive via bot** | Local-health modes only. Bot = `notplayer.xml` with inventory in `ew_local_player_dead`. Revive fires on bot death. | **Exists, but not in shared mode.** Kill path unverified in game. Reconnect while dead kills. |
| **4. Fair drops** (bosses, chests, hearts, wands) | Boss and chest spawns are DES single events, but copies may repeat them (see 7b, 6). Hearts apply per player, or once to the pool in shared mode. | **Partial.** Per-copy drops unverified, same risk as gold. |
| **5. Shared gold, no N-fold** | Pickups are correctly counted once via host ledger. Drops run on copies and far replays. | **Not satisfied by design.** Top risk (§7b). |

## 14. Open risks, ranked

| # | Risk | Anchors | Severity | Test |
|---|---|---|---|---|
| 1 | Gold multiplied by copies and far replays | `diff_model.rs:1032-1040, 2623-2629, 2355-2443, 1685-1697`; `entity_sync.rs:222-266, 779-792` | **High** if vanilla pays on any death | Two clients, count nuggets per kill; far-replay walk |
| 2 | Client-authoritative melee dropped (shared mode) | `damage/cbs/send_damage_to_host.lua:15, 22-25` | **High** if confirmed | Non-host monster melee on own player; check pool |
| 3 | Non-host immortality may zero forwarded melee | `resource/cbs/immortal.lua:3-17`; `send_damage_to_client.lua:31` | **High** if engine ordering matches | Melee from monster on remote avatar; check owner HP |
| 4 | Stale absolute snapshots, no sequence numbers | `player_sync.lua:24-46, 306-312` | Medium | Inject reordering; watch wallets |
| 5 | Host-only save; no periodic save | `net/world.rs` Drop; `save_state.rs` | Medium for multi-day runs | Kill host process; compare state |
| 6 | Unbounded `spawn_once` | `entity_sync.rs:55, 214-220` | Medium over long runs | Memory after many deaths |
| 7 | TLS server not authenticated | `tangled/src/helpers.rs:18-25` | Low to medium | Not tested |
| 8 | No per-message protocol version; only lobby char and Steam key | `lobby_code.rs:33,45`; `steam_networking.rs:531,582-599` | Medium across mod updates | Mismatched versions |
| 9 | `NP_SKIP_MOD_CHECK` bypasses mod check | `mod_manager.rs:410` | Low (user opt-in) | n/a |
| 10 | Hard-coded engine offsets, one build | `ewext/src/addr_grabber.rs` | **High maintenance** | Next Noita update |
| 11 | `ew_player_count` never decremented | `player_fns.lua:583-606` | Medium | Disconnect, then heal |
| 12 | Chest and heart duplication hypothesis | `coalmine.lua:spawn_bbqbox` (mock only) | Medium, unconfirmed | Investigation diagnostics |

## 15. Design lessons for the new implementation

1. **One writer per fact.** Host owns the gold ledger, the health pool, and the revive state. Clients send events, never absolute authoritative values.
2. **Idempotent events.** Every pickup, drop, hit, and kill carries `(peer, seq)`. The host applies each once and broadcasts the event ID with the resulting value. Clients drop stale sequence numbers.
3. **Gold without physical nuggets in shared mode.** Credit the ledger directly on the authority's kill event. Strip `drop_money.lua` from **all** copies, not only when `drops_gold` is false. Replays must not pay loot.
4. **Replay records with lifecycle.** Give each `SpawnOnce` an ID and mark it consumed. Prune by peer ack or age. Replayed enemies need a silent kill (no death script), not lethal damage.
5. **Single damage path.** Victims report damage events. Never restore HP on clients. Melee and projectiles go through the same event. Immortality is a host decision, not a per-client callback.
6. **Revive state in the ledger.** Store the inventory in the host ledger, not a base64 global. Make the bot death a single event with a killer ID, so revive is one atomic transition.
7. **Transport.** Sequence numbers per channel. Pin the host's key in the lobby code, or use a join secret. Version handshake per connection plus a protocol number per message. Reconnect keyed by stable player ID, not per-session peer ID.
8. **Persistence.** Periodic checkpoints (time-based and on events), atomic writes (tmp + rename), schema version, and a write-ahead log for ledger items (gold, hearts, orbs) so crashes lose at most a few events.
9. **Engine hooks.** Avoid hard-coded offsets. If native patches are needed, use signature scanning, a build-hash check at startup, and a small isolated layer.
10. **Testing.** A deterministic two-client harness with recorded input tapes. Unit tests for ledger math at N = 1–4, damage routing, and replay idempotency.

## 16. Known weaknesses (code anchors)

- `spawn_once` grows unbounded (`entity_sync.rs:214-220`).
- `addr_grabber.rs` hard-codes offsets for one 32-bit build and uses inline asm `__thiscall`. `ExtState` is thread-local while `NETMANAGER` is global. `lib.rs` `__gc` TODO is ineffective (thread-local).
- TLS server identity unverified (`helpers.rs:18-25`).
- Version check only via lobby char `'0'` (`lobby_code.rs:33,45`) and Steam `ew_version` (`steam_networking.rs:531,582-592`). No per-message protocol version. `NP_SKIP_MOD_CHECK` (`mod_manager.rs:410`).
- Unreliable gold deltas and absolutes (`player_sync.lua:306-312`).
- `ew_player_count` never decremented (`player_fns.lua:583-606`).
- Stale client pool globals and clients' non-writes (`damage.lua:81-92, 188-192`).
- Shared-mode melee asymmetry (`send_damage_to_host.lua:22-25`) and `damage_received` argument question (`send_damage_to_client.lua:31`).
- Stale comment at `send_damage_to_client.lua:10`.
- Explosion forwarding not applied; host-only gates at `explosion_cuts.lua:174,187`.
- Chest and death replay queues are in memory (lost on ewext restart); late joiners miss them.
- TODOs: `world.rs` ~173 (exits), `des.rs:111-120`, `world.lua` (null `construct_cell`), `lib.rs` `__gc`.
- Remote avatars are immortal on non-host clients (`damage.lua:122-125`).
- `heart_statue.lua` likely has dead code (`ew_flag_heart_picked_up` at 579/585).
- Possible DES echo of copy deaths (`diff_model.rs:1325-1347`) [INFERENCE].

## 17. Verification status

**Verified by reading code** (anchors above): transports and framing; RPC reliability semantics; the gold sender and receiver watermark arithmetic; the damage callback routing table; DES radii and replay mechanics; `DeadEntities` destination; `kill_entities` lethal damage; `init_remote_entity` drop-script condition; bot creation and revive flow; settings defaults; save points.

**Not verified** (marked [INFERENCE] above):
1. Whether a non-owner death pays gold (vanilla `drop_money.lua` is not readable; `drop_items` undocumented). Decides the whole gold N-fold question.
2. Whether `damage_received` gets the post-immortal amount (engine docs silent).
3. Whether client-authoritative melee is dropped in practice (code-derived).
4. Whether `death_notify` is attached to copies (echo).
5. Writer of `HEARTS_MORE_EXTRA_HP_MULTIPLIER` (not in EW code).
6. Whether heart statue code is dead.
7. `connection_manager.rs` line count (525 vs 851).
8. Steam `connect` initiation body (`steam_networking.rs` 374+).
9. `ChunkState` transition logic in `net/world.rs`.
10. Host migration: none observed.
11. Input-driven avatar replay under RNG and physics divergence: not tested.
12. Chest content generation: which client rolls it.
13. `orb_sync.lua` and `heart_pickups.lua` exact sizes and behavior.

**Not read:** `explosion_cuts.lua` body; `spectate.lua`; `world_sync_cuts/` directory; `proxy_opt.rs`; `cli.rs`; `app.rs` body; `noita_launcher.rs`; `docs/testing_wands.md`; `last_release_notes.md`; `contributing.md`; `distributed_world_sync.drawio`; `heart_pickup_investigation.md` lines 304-308; `local_health.lua` 834-863; `player_fns.lua` 610-737; `steam_networking.rs` 374+; `OnProjectileFired` beyond 265; the `~/Noita-multiplayer/ew_log*.txt` logs.

**Not run:** no build, test, or in-game session. The second checkout's note claims 14 `ewext` tests passed (per its doc, unverified here).

**Working tree:** no git commands were run. Per the plan notes, `explosion_cuts.lua` was modified about an hour before the session, and the second checkout has uncommitted heart-diagnostic files.