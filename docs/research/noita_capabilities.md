# Noita engine capabilities and EW mechanisms (research, 2026-11-18)

> Source note: written by a research agent before vanilla `data.wak` was unpacked; vanilla script claims
> here are unverified — see `vanilla_pickups.md` / `vanilla_world_and_saves.md` for verified ones.
> §8.5 picks gold "reading A" (one shared balance); the user chose **D4 = shared income, individual
> spending** (reading B′ in §8.5). The duplication analysis (S1–S5) still applies.

## Summary

1. **Terrain sync is feasible only in unsafe mode.** The stock API has no per-cell read or write. EW gets it from NoitaPatcher plus a native DLL, using offsets pinned to build `9dbd52…`. Unsafe mode rules out Steam Workshop distribution. I found no stock-API route [INFERENCE].
2. **EW already implements most required mechanisms:** chunk-authority terrain, DES entity sync, a NotPlayer revive bot, orb sync, a shared HP pool, first-claim flags, and a wallet protocol. Reuse is realistic. Several defects matter for multi-day runs (§10).
3. **Gold (req. 5): the wallet protocol does not inflate.** Each pickup enters the host balance once (`files/system/player_sync.lua:24–47`). The N-fold risk is **physical duplication**. Every client that holds or respawns a `drops_gold` enemy runs its death script and creates its own gold piles (`ewext/src/modules/entity_sync.rs:207–269`, `ewext/src/modules/entity_sync/diff_model.rs:2379–2443`). The only gate is on `goldnugget_200.xml` (`files/system/wand_charm/append.lua:5`), and it does not hold on respawned copies (§8.3). Fix this at the spawn and claim layer, not in the wallet.
4. **Hearts are personal by construction.** They are not DES-synced. Full heals are divided by player count (`files/system/heart_pickups/heart_pickups.lua:20`).
5. **Security:** the transport disables TLS server verification (`noita_proxy/tangled/connection_manager.rs:250–251`, `helpers.rs:9–26`). Peer identity rests on an app-level exchange [INFERENCE, unaudited]. Fix this before any internet play.
6. **Persistence:** the host proxy `save_state` is the single authority. Noita `save00` is per client. The disconnect fix and the DES fix are applied. The explosion-forwarding patch is not (§5.2).

---

## 1. Scope and method

- **Read-only.** Nothing was built, run, or tested. No git commands were run.
- **Sources:**
  - Noita install `/data/Games/steamapps/common/Noita` (build hash `9dbd52ced019a643169a2db02f46c77f8766c6e5`, `_version_hash.txt`).
  - EW fork `/home/mora/noita_entangled_worlds_fix`. Unprefixed `path:line` refers to this root.
  - Second checkout `/home/mora/noita_entangled_worlds`, with uncommitted heart investigation.
  - Installed mod `INST/mods/quant.ew/`, where `INST` is the Noita install. It matches the fork on the world_sync, local_health, and player_sync anchors. `explosion_cuts.lua` differs (§5.2).
- `data/data.wak` is binary and was not parsed. Vanilla script bodies (`drop_money`, gold pickup, orb pickup, chest, boss death, DamageModel) are unverified. Claims about them are `[INFERENCE]`.

## 2. Architecture

```
Noita process (LuaJIT 5.1, i386 PE)
 ├─ quant.ew Lua mod (init.lua, files/system/*)   CrossCall registry, rpc, hooks
 ├─ ewext native DLL (Rust, in-process)           DES entity sync, chunk decode, addr_grabber
 └─ NoitaPatcher nsew_native.dll (LuaJIT FFI)     raw grid/chunk read/write, build-pinned
          │  Lua ⇄ proxy: net.proxy_send / proxy_bin_send → NetMsg
Local proxy (Rust, noita_proxy): tangled QUIC transport, or Steam networking
 ├─ net.rs              flags, settings, explosions, mod messages, continue-run fix
 ├─ net/world.rs        chunk authority map, chunk persistence
 └─ net/des.rs          entity authority + storage, liveness-aware transfer
Host proxy: single authority; persists save_state/*.bit
```

## 3. Engine and modding surface

### 3.1 Runtime and build pinning
- `noita.exe` is an i386 PE. Scripting is LuaJIT 2.0.4 (`lua51.dll`). The version hash is in `_version_hash.txt`.
- **Build-pinned memory access.** `noitapatcher/nsew/world_ffi.lua` uses game-global offsets: `game_global+0xc → world_data`, `+0x44 → grid_world` (~:387), material stride `0x290` (~:403), `ChunkMap cells[0x40000]`.
- **Build-pinned addresses.** `ewext/src/addr_grabber.rs:1–150` hard-codes addresses such as `0x007aa7ce`, `0x007bf3c9`, `0x00439c17`, decodes them with iced_x86, and fails hard on any game update. It is x86-only.

### 3.2 Stock Lua API (v12, `INST/tools_modding/lua_api_documentation.txt`)

| Need | Stock call (line) | Note |
|---|---|---|
| Per-cell read/write | none | Requires NoitaPatcher |
| Area exists | `DoesWorldExistAt` :172 | |
| Load pixel scene | `LoadPixelScene` :131 | |
| Material conversion | `ConvertMaterialOnAreaInstantly` :343, `ConvertMaterialEverywhere` :342 | coarse |
| Texture pixels | `ModImageMakeEditable/GetPixel/SetPixel` :372–375 | textures only |
| Materials | `ModMaterialsFileAdd` :379 | init only |
| Seeds | `SetWorldSeed` :207, `SetRandomSeed` :233 | global RNG |
| Kill entity | `EntityKill` :9 | no description in doc, so death-script behaviour is unknown |
| Damage | `EntityInflictDamage` :139 | has `entity_who_is_responsible` |
| Death hook | component `script_death` (`component_documentation.txt:1540`) | called on `Message_Death` |

### 3.3 Engine callbacks (undocumented in the API doc)
`OnModPreInit`, `OnModInit`, `OnModPostInit`, `OnMagicNumbersAndWorldSeedInitialized`, `OnWorldInitialized`, `OnWorldPreUpdate`, `OnWorldPostUpdate`, `OnPlayerSpawned`, `OnPlayerDied`, `OnPausedChanged`, `OnPausePreUpdate`, `OnProjectileFired`.
Examples: `mods/example/init.lua:7–37`, `mods/quant.ew/init.lua:193,381,402,415,612,663,706,717,721,725`.

### 3.4 Sandbox, distribution, native layers
- `mods/quant.ew/mod.xml` sets `request_no_api_restrictions="1"`, which unlocks `ffi`, `io`, `os`, and native DLLs.
- `INST/READ_ME_FIRST.txt:65`: unsafe mods do not work from Steam Workshop. Distribution must be manual.
- `NoitaPatcher/load.lua` sets `__nsew_path`. `noitapatcher/nsew/native_dll.lua:4` runs `ffi.load("nsew_native.dll")`.
- NoitaPatcher API (external doc, `dexter.xn--dpping-wxa.eu/NoitaPatcher/noitapatcher.html`): `SerializeEntity`/`DeserializeEntity`, `SetPlayerEntity`, `ComponentUpdatesSetEnabled`, `EnableGameSimulatePausing`, `SetProjectileSpreadRNG`, `SetGameModeDeterministic`, `CrossCallAdd`, `ForceLoadPixelScene`, `PhysBodySet/GetTransform`, `MagicNumbersSetValue/GetList`, `GetGameModeNr`.
- **Serialization.** `files/core/util.lua:350, :361–363` calls `np.SerializeEntity`/`DeserializeEntity`. The ewext side (`noita_api/src/serialize.rs:4–32`) calls the Lua globals `EwextSerialize`/`EwextDeserialize`, set in `ewext_init/ewext_init.lua:15–16`.
- **Terrain encoding.** `NoitaPatcher/noitapatcher/nsew/world.lua` run-length encodes regions up to 256×256 as `PixelRun {uint16 length; int16 material; uint8 flags}`. Open TODO: transparent texture pixels make `construct_cell` return nil.

### 3.5 Injection patterns EW uses (reusable)
- Source injection via `util.prepend` and `ModLuaFileAppend`. Example: `entity_sync_helper.lua:8–43` wraps `EntityKill` (:15–16) for broken wands only.
- Global function override: `files/system/wand_charm/append.lua:3–7` overrides `EntityLoad`.
- CrossCall registry: `quant.ew/init.lua:373` `util.add_cross_call("ew_do_i_own", util.do_i_own)`.
- Death override: `files/resource/append/no_default_death_handling.lua` replaces `OnPlayerDied` (prints only). A second copy sits at `files/append/…`. Equality not verified.

## 4. Determinism
- `SetRandomSeed` is global (`data.wak:449552`, grep only).
- **Per-peer seeds.** `files/system/rnd_wands/rnd_wands.lua:1` prepends a `SetRandomSeed` wrapper to 22 `data/scripts/gun/procedural/*.lua` files and `items/potion_starting.lua`, using `CrossCall("ew_per_peer_seed")`. `override_perk_list.lua:4` does the same for perks.
- Sim pausing is disabled at `quant.ew/init.lua:11` (`EnableGameSimulatePausing(false)`).
- Clients that joined at different times may diverge on frame-number seeds [INFERENCE].

## 5. Persistence and reconnect

### 5.1 Storage layers
- **Host proxy:** `~/.local/share/entangledworlds/save_state/`. Files: `run_info_v2.bit`, `des_entity_storage.bit`, `world_chunks.bit`, `flags.bit` (probably `state.flags` [INFERENCE]).
- **Client Noita `save00`:** `…/compatdata/881100/pfx/drive_c/users/steamuser/AppData/LocalLow/Nolla_Games_Noita/save00/`. Contents: `world/world_<x>_<y>.png_petri`, `world/entities_*.bin`, `world/area_*.bin`, `persistent/` (`flags/`, `orbs_new/`, `bones_new/`), `player.xml`, `world_state.xml`, `mod_settings.bin`, `session_numbers.salakieli`. The formats of `area_*.bin`, `world_tree.bin`, `world_sim.bin`, `.png_petri` and the `.salakieli` encryption are unverified.

### 5.2 Reconnect mechanisms and plan status

| Mechanism | Location | Status |
|---|---|---|
| Local Noita disconnect relinquishes chunk authority | `noita_proxy/src/net/world.rs:664` (`local_noita_disconnected`); called at `net.rs:589`; test `world.rs:3436` (`late_relinquish_keeps_data`) | Applied (`.omp/plans/WORLD_ENTITY_PERSISTENCE_PLAN.md`) |
| DES transfer to an absent peer returns to storage, delivered after reconnect | `noita_proxy/src/net/des.rs:177–199`; tests `:279`, `:299` | Applied (`DES_TRANSFER_TO_ABSENT_PEER_PLAN.md`) |
| Continue-run DES wipe fix | `net.rs:687` (`my_id() == host_id() && id != my_id()`) | Applied in working tree only. **Not in** `fix.patch`, `fix2.patch`, or `explosion_forwarding.patch` (`CONTINUE_RUN_DES_WIPE_PLAN.md`) |
| Explosion forwarding dedupe | `explosion_forwarding.patch` (`WorldCut::Explosions` :139, `recent_explosions` :83, `is_duplicate_of` :18, `if !self.is_host` :116) | **Not applied** |
| Host-only authority requests and peer-leave handling | `world.rs:742–803`, `handle_peer_left` `:1224–1248` | Present |
| Missing heart for late joiners | `docs/heart_pickup_investigation.md` (uncommitted, other checkout) | Hypothesis, not reproduced |

## 6. EW synchronisation mechanisms

### 6.1 Terrain (req. 1)
- **Client streaming** (`files/system/world_sync/world_sync.lua`):
  - Keys: `KEY_WORLD_FRAME=0` (:35), `KEY_WORLD_END=1` (:36).
  - `send_chunks` (:100–110): `DoesWorldExistAt` → `encode_area` → `net.proxy_bin_send`. Chunk size is 128 px.
  - `get_all_chunks` (:116). Ring 0 at `frame%4==0` (:126). Ring 1 at `%4==2` (:141), 4 chunks per iteration. Ring 2 at `%16==3` (:168).
  - Ring 3 at `%12==1` (:205), only when `priority==0` and the game has not ended. It covers ±3 chunks (:175–178, :201–203).
  - **Consequence:** only chunks near players are streamed. "All chunks" is not literal. Far chunks live in the host's `world_chunks.bit` [INFERENCE on the completeness].
  - `on_world_update` (:211–235) overwrites the interval from `ModSettingGet("quant.ew.world_sync")` (:220). The proxy-option comment at :114 is stale.
  - Decode: native `ewext.init_world_decode`/`decode_area`, with a Lua fallback after 3 failures.
- **Proxy authority** (`noita_proxy/src/net/world.rs`):
  - `WorldNetMessage` :40, `GotAuthority` :69, `RelinquishAuthority` :75, `ChunkState` :140, `WorldManager` :176.
  - `authority_map: FxHashMap<ChunkCoord,(OmniPeerId,u8)>` :191. Single writer per chunk.
  - `nice_terraforming` :177 (default true).
  - `load` :257, `update` :509, `emit_got_authority` ~:689–718.
  - `handle_msg` :742–803: `RequestAuthority` is host-only. A lower priority number wins unless `can_wait`.
- **Explosions.** The Lua side calls `net.proxy_send("cut_through_world_explosion")`. That goes to `net.rs:1470`, then `state.explosion_data` (`net.rs:91`), then `flush_exp` (`net.rs:1507–1510`), then `world.cut_through_world_explosion` (`world.rs:1735`, `_list` :1790, `_chunk` :2056).
- **Fork/installed mismatch.** The fork's `files/system/explosion_cuts/explosion_cuts.lua` guards `on_new_entity` with `ctx.is_host` (:174) and returns early in `on_world_update` when not host (:187). It sends at :209, :220, :233. The installed copy has no host guard and sends unguarded at the same lines.

### 6.2 Entities: DES (req. 1, 2, 4)
- **Proxy** `noita_proxy/src/net/des.rs`: `DesManager` :25, `authority` :29, `noita_down` :33/:56, `handle_noita_msg` :111 (TODO :115), `RequestAuthority` :137, `TransferAuthorityTo` :177 (`add_gid_to_tree` at :179 if the target is down), `noita_disconnected` :196 (insert :197, TODO :199). Shared types: `shared/src/des.rs:42,141` (`drops_gold: bool`).
- **Client** `ewext/src/modules/entity_sync.rs`: `look_current_entity` :45, `handle_proxytodes` :350, `on_new_entity` :556.
- **Diff model** `ewext/src/modules/entity_sync/diff_model.rs`:
  - `DES_TAG="ew_des"` :35, `update_entity` :373.
  - Ownership variable `ew_gid_lid` (written at :1002, :2722–2731). Its `value_bool` is true for the owner and false for remote copies.
  - Owner-authority transfer when the owning player leaves `AUTHORITY_RADIUS` of the camera (≈:1360–1372).
- **Exclusions** (`entity_sync.rs` `entity_is_excluded`, ≈:124–134): `ew_no_enemy_sync`, `polymorphed_player`, `gold_nugget` (:132), `nightmare_starting_wand`, `player_unit`, `ENTITY_EXCLUDES`, `wand_good_*`, `greed_curse.xml`, and any non-root entity without `ew_sync_child`. Hearts are excluded per the heart doc. The exact term was not re-read. The full `ENTITY_EXCLUDES` list was not read.

### 6.3 Death handling (the root of the gold issue)
- **`drops_gold` derivation** (`diff_model.rs:1032–1035`): true iff a LuaComponent's `script_death` is `data/scripts/items/drop_money.lua`.
- **Owner death** (`diff_model.rs:1319–1355`): for each death of a tracked entity:
  - Read `drops_gold` from the entry (:1336–1338).
  - Push `EntityUpdate::KillEntity{lid, wait_on_kill, responsible_peer}` (:1341).
  - Push `SpawnOnce::Enemy(file, drops_gold, responsible_peer)` into `dead` (:1347).
  - Broadcast `dead` as `RemoteDes::DeadEntities` to peers **not** in `interest_tracker` (`entity_sync.rs:777–799`, send at :788). Receivers append to `spawn_once` (:464). Single-entry `SpawnOnce` push is at :455.
- **Respawn drain** (`entity_sync.rs:207–269`):
  - For each entry within 768 px of the local position (`pos.contains(x,y,512+256)`, ≈:222), load the enemy file at the death position with `EntityID::load`.
  - Tag `ew_no_enemy_sync` and call `init_remote_entity(entity, None, None, drops_gold, …)` (:229).
  - Clear the boss-spirit and boss-sky script hooks. Kill `protection` children. Disable UI damage reporting.
  - `boss_centipede` takes a special path (`kill_later`). Everything else gets lethal damage: `damage_multipliers.curse=1.0`, then `inflict_damage(max_hp*100, DamageCurse, "kill sync", responsible)` (≈:262–269).
  - Each entry is consumed once per client. Entries are never pruned, so `spawn_once` grows for the whole run (comment ≈:207–213).
- **Tracked remote kill** (`diff_model.rs:1780–1786` pushes to `pending_death_notify`; `kill_entities` at :2355; closure at :2379–2443):
  - If the copy has `ExplodeOnDamageComponent`, sets explode-on-death to 100%. The copy will explode on each client.
  - Kills `inventory_quick` and `protection` children.
  - If it has a DamageModel: optionally clears the wait-for-kill flag. Sets curse multiplier 1.0. Inflicts `hp + ε`, then `max_hp*100`, both as "kill sync" attributed to the responsible player. Or sets kill-now if `wait_on_kill`.
  - If it has no DamageModel: calls `entity.kill()`.
  - Calls `remove_ent`. The tracked entry is kept deliberately for disconnect sweeps.
- **Localisation handover** (`diff_model.rs` ≈:1771–1778) calls `safe_entitykill` (`diff_model.rs:2977–2998`). It removes audio components. For wands it uses `_safe_wandkill`. Otherwise it kills inventory children and calls `entity().kill()`.
- **Copy preparation** (`init_remote_entity`, `diff_model.rs:2539`):
  - Removes camera-bound, keep-alive, platforming, physics-AI, fish-AI, IK-limb, and lifetime components. Sets lifetime to max. Disables pickup drops.
  - Sets `ew_gid_lid` **only if `lid` is `Some`** (≈:2722–2731), with `value_bool=false`.
  - Removes the `drop_money.lua` death script unless `drops_gold`. Also removes a fixed list of other death and damage scripts (≈:2625–2640).
- **Ownership check** (`files/core/util.lua:50–58`): `util.do_i_own(ent)` returns true when the entity has no `ew_gid_lid` (that is, `gid == nil`). Otherwise it returns `value_bool`. Exposed as `ew_do_i_own` (`init.lua:373`).

### 6.4 Flags and first-claim arbitration
- **Mod to proxy.** Raw variant `0` (`NoitaOutbound::Raw`) becomes `NetMsg::Flags` at the host (`noita_proxy/src/net.rs:1223–1237`). The mod-side emitter for variant `0` was not located.
- **Host handler** (`net.rs:903–955`, file inferred from neighbouring anchors [INFERENCE]):
  - `Normal` (:906–911): `new = duplicate || state.flags.insert(flag)`, then replies `RespondFlagNormal`.
  - `Slow` (:917): always replies.
  - `Moon` (:926) and `Stevari` (:937): reply only if `new`. `Stevari` also broadcasts.
  - Requester writes `normal_flag "<flag> <new>"` (:953–955).
- **Global toggle.** `duplicate` is a single `AtomicBool` (`net.rs:244`, init :285, store :1036–1037). There is no per-flag policy. `RespondFlagNormal(String, bool)` is at `noita_proxy/src/net/messages.rs:33`.

### 6.5 Orbs (req. 2)
- `files/system/orb_sync/orb_sync.lua`: `WorldStateComponent.orbs_found_thisrun` is authoritative. The host's `rpc.update_orbs` kills collected orbs on other clients and spawns missing ones.
- Vanilla references (bodies unverified): `GameGetOrbCountThisRun()` at `data.wak:208214, :210254, :211984, :211994, :212707, :213195, :213497`; `AddFlagPersistent("progress_orb_pickup_")` at `:533164, :533175`.

### 6.6 Health and bosses
- `files/system/damage/damage.lua`: shared pool `ew_shared_hp` / `ew_shared_max_hp` (default `"4"`). The host applies damage in `on_world_update_host` every 4 frames. Clients send `rpc.deal_damage` every 15 frames. Each joiner adds `health_per_player/25` (:109).
- `files/system/boss_damage/boss_damage.lua`: `rpc.run_hp(hpp)`, `rpc.run_deer(dmg)`.
- `files/system/heart_pickups/heart_pickups.lua:20` reads `HEARTS_MORE_EXTRA_HP_MULTIPLIER`. Full heal is divided by `ew_player_count`. Shared mode broadcasts `rpc.heart_pickup`. Heart variants: `heart`, `heart_better`, `heart_evil`, `heart_fullhp`, `heart_fullhp_temple`. Their effects are unverified.

### 6.7 Personal versus shared spawns
- **Personal:** per-peer seed for 22 procedural gun scripts, potion, and perks (`rnd_wands.lua:1`, `override_perk_list.lua:4`).
- **Shared once:** `files/system/wang_hooks/wang_hooks.lua` with `wang_scripts.csv`. About 70 allow-listed spawns. `spawn_heart` and `spawn_perk` are excluded. Host flags `wspwn_<fn>_<x>_<y>`. Syncs pixel scenes via `rpc.sync_pixel_scene`.
- **Shop:** `files/system/gen_sync/gen_sync.lua` wraps `generate_shop_item`/`generate_shop_wand` with run flags `spwn_<fn>_<x>_<y>`. The author's TODO says "probably a bad idea".
- **Greed:** `files/system/greed/greed.lua` prepends `CrossCall("ew_greed")` to `effect_curse_radioactive.lua`.

### 6.8 Chests
- `files/resource/cbs/chest_sync.lua`: if `ew_gid_lid` exists, call `CrossCall("ew_chest_opened")`.
- `entity_sync_helper.lua:172` (`ew_chest_opened`) and :199 (`ewext.des_chest_opened`). The same helper queues `ew_thrown`, `ew_broken_wand`, and `ew_death_notify`.
- Chest claims are first-claim through flags (§6.4).

### 6.9 Gold wallet protocol (`files/system/player_sync.lua`, hand-traced, not run)
- :5 `local last_money`.
- Receiver `rpc.send_money_and_ingestion` (:24–47):
  - share_gold branch at :28.
  - Host: `cm + delta` (:36).
  - Non-host, message from host: `my_delta` (:38–41), `last_money = money` (:42), `money + my_delta` (:43).
  - Always `wallet.money = money` (:47).
- Sender tick `GameGetFrameNum()%37==9` (:291–313):
  - Polymorph reset at :294–298 (`money = last_money` at :296).
  - `delta` at :301 and :306–307. `last_money` at :309. Send at :311–313.
- **Verdict [INFERENCE]:** the host balance is the single authority. Each pickup enters it once. Non-host wallets equal the last host absolute plus unsent local change. Worst-case latency is about 37 frames (~0.6 s). The polymorph reset can discard up to ~37 frames of unsent pickups.
- `WalletComponent` (`component_documentation.txt` ~:2630): `money` int64, `money_spent`, `mMoneyPrevFrame`, `mHasReachedInf`. `WalletValuableComponent.money_value` default 10.
- Vanilla wallet read-modify-write sites (grep only): `data.wak:213219–213228, :213513–213517, :495673–495687, :531480–531487, :532462–532501`.

### 6.10 Revive (req. 3): `files/system/local_health/local_health.lua`
- `get_gold`/`set_gold` :36–50. `reduce_hp` ≈:250–262 (`health_lost_on_revive` at :250, `global_hp_loss` at :252, `rpc.loss_hp()`).
- `fake_polymorph_into_entity` :309 stores `ew_local_player_dead` (base64 of `util.serialize_entity`). `fake_unpolymorph` :327–340.
- `no_notplayer()` :343–352 spawns `heart_statue_running.xml` or `heart_statue.xml` (the no-bot variant). `player_died` :360. No-bot branch :383–384.
- NotPlayer polymorph :416 (`notplayer/notplayer.xml`), flag `ew_flag_notplayer_active`.
- Bot death timer :636–660: checks `perma_death`, `notplayer_active`, and `hp_new<=0`. Waits 5 frames (:651–655). Calls `ctx.cap.health.on_poly_death()` (:655).
- Heart-based revive gate :579–608: `ew_flag_heart_picked_up` (:579), `heart_statue_should_revive` (:582–585), gate (:588–596), required variants `heart_fullhp` or `heart_fullhp_temple` (:607–608).
- `on_poly_death` (:775–800): returns early if `ending_game_completed` and not `ew_kill_player`. Otherwise: `rpc.revive_message()`, `local gold = get_gold()`, `inventory_helper.get_item_data`, `remove_inventory()`, clear `ew_flag_notplayer_active`, `fake_unpolymorph()`, then `inventory_helper.set_item_data(item_data, ctx.my_player, true, false)`.
- **Gold write-back after the swap is not confirmed** [INFERENCE].
- **Bot AI** `files/system/notplayer_ai/notplayer_ai.lua` (1673 lines): `MAX_RADIUS=128*5`, `INVIS_RANGE=30`, `update` :1580. `GetGameModeNr()==2` checks at :1010 and :1124. Meaning of mode 2 not verified.

### 6.11 Settings (`noita_proxy/src/game_settings.rs` defaults ≈:54–113)

| Setting | Default | Notes |
|---|---|---|
| `share_gold` | false | Sent to Lua at `net.rs:1025`, `1054–1079` |
| `duplicate` | false | Global flag toggle, see §6.4 |
| `revive_on_drop` | false | ≈:109–133 |
| `health_per_player` | 100 | :96 |
| `health_lost_on_revive` | 0 | :97 |
| `global_hp_loss` | — | :28, :62 |
| `nice_terraforming` | true | :177 in world.rs |
| `dont_steal` | true | |
| `pvp_kill_steal` | 50 | Forced to 0 when `share_gold` is on (`net.rs:1101–1105`) |
| `wait_on_players` | true | |
| `chest_on_win` | true | |
| `timed` | true | |
| `local_health_alternate_dont_run` | false | |

## 7. Prior art
- **Entangled Worlds (EW):** synced world, 11 global perks, Apache-2.0 (wiki/README).
- **Noita Together:** separate worlds with shared resources. The README says "You can not directly affect other player's worlds."
- **Noita Arena:** PvP.
- **Noita COOP:** couch co-op.
- **Iota Multiplayer:** local.

## 8. Requirement feasibility

### 8.1 R1: Full world sync
- **Verdict: feasible. Large effort.** EW supplies the reference architecture.
- Terrain: ring streaming around players, authority map, persistence in `world_chunks.bit`. A full-world background transfer would need new work.
- Entities: DES with the exclusions in §6.2. Dropped items and wands are DES entities unless excluded [INFERENCE: the full exclusion list was not read].
- Risks: build pinning, x86-only, unsafe mode, and the explosion-forwarding gap.

### 8.2 R2: Shared world pickups (orbs and others)
- **Verdict: feasible.** Orbs follow the authoritative-`orbs_found_thisrun` pattern (§6.5). Generalise via DES first-claim (§8.5).
- Risk: pickups outside DES (gold nuggets, hearts) duplicate or stay personal by construction.

### 8.3 R3: Revive with bot
- **Verdict: EW's default design matches the spec.** The NotPlayer bot takes the dead player's serialised entity and inventory. Survivors must kill it. Items are restored to the revived player (§6.10).
- Risks: bot AI fidelity is unknown. Gold write-back after the swap is unconfirmed. The revive gate semantics need re-checking. The bot's timeout behaviour was not checked.

### 8.4 R4: Fairness (boss drops, world spells and wands, chests, hearts)

| Item | Personal option | Shared-once option | N× option | Mechanism in EW today |
|---|---|---|---|---|
| Hearts | Already personal (not DES-synced). Full heal divided by `ew_player_count` (`heart_pickups.lua:20`) | Shared mode broadcasts `rpc.heart_pickup` | Spawn N copies | Personal |
| HP pool | Per-player pool | `ew_shared_hp` single pool; joiners add `health_per_player/25` (`damage.lua:109`) | — | Shared |
| Boss drops | Per-peer loot seed (needs boss script review) | DES kill; host spawns once | Per-peer spawn | Death runs on each copy (`diff_model.rs:1296–1299` adds `boss_dragon_death.lua` on remote copies). Duplication risk, see §8.5 |
| World wands and spells | Per-peer seed (`rnd_wands.lua:1`) | `wspwn_` host flags (`wang_hooks`) | Per-peer spawn | Shared once |
| Shop items | Per-peer seed (not verified) | `spwn_` run flags (`gen_sync`) | — | Shared once per run |
| Random wands and perks | Per-peer seed (`rnd_wands.lua:1`, `override_perk_list.lua:4`) | — | — | Personal |
| Chests | Per-peer reroll (not in EW) | First-claim via flags (`entity_sync_helper.lua:172,:199`) | — | Shared once |
| Gold | Not allowed by spec | See §8.5 | — | Wallet is shared; physical piles duplicate |

- The global `duplicate` toggle cannot express "first-claim for gold, duplicate for hearts". Add a per-flag policy in the proxy.

### 8.5 R5: Shared gold

**Reading.** I use reading **A**: one shared balance, and each physical unit of gold is counted once. This is what EW's `share_gold` protocol already does, and it matches "added to everyone's balance, without inflating total gold N-fold."
- **Reading B** (rejected by the spec): each wallet gets +X per pile. The sum across wallets is N·X.
- **Reading B′** (a variant): one physical pile, but every wallet gets +X. This is only consistent with the spec if "total gold" means the physical supply, not the sum of balances. It needs a different protocol.

**Where the inflation comes from.** The wallet protocol is not the cause (§6.9). The duplication sources are:

| # | Source | Code | Gate today | Effect |
|---|---|---|---|---|
| S1 | Owner's own death | Vanilla death on owner (`drops_gold` copy) | Owner only | One pile set (intended) |
| S2 | Tracked remote copy killed via `KillEntity` | `diff_model.rs:1780–1786` → `kill_entities` closure `:2379–2443` (lethal damage, responsible player). `drop_money` kept for `drops_gold` copies (≈:2625–2630) | `goldnugget_200` only, through `ew_gid_lid=false` (≈:2722–2731) and `wand_charm/append.lua:5` | Each tracked client creates its own non-200 piles |
| S3 | Respawn-and-kill for untracked peers (`DeadEntities`) | `entity_sync.rs:777–799` (broadcast), `:464` (store), `:207–269` (drain). Respawned copies have **no** `ew_gid_lid`, so `do_i_own` returns true (`util.lua:50–58`) | **Bypassed**, since the 200 gate passes on respawned copies | Each peer that comes within 768 px, at any later time, creates its own piles, including 200s |
| S4 | Non-DES nuggets | `entity_sync.rs:132` excludes `gold_nugget` | — | Each client sees and collects its own piles, and each pickup feeds the shared balance |
| S5 | Boss death on remote copies | `diff_model.rs:1296–1299` | — | [INFERENCE] possible duplicate boss drops |

- **Denominations.** `files/resource/interned_filenames.txt:1135–1176` lists `goldnugget_10`, `_50`, `_200`, `_1000`, `_10000`, `_200000`. The gate covers only `_200`. Whether vanilla `drop_money.lua` spawns all of them [INFERENCE].
- **Unknown death semantics.** Whether `EntityKill` (and so `entity.kill()` and `safe_entitykill`) runs `script_death` is not documented (`lua_api_documentation.txt:9`). If it does, the no-DamageModel path and localisation handovers can also drop gold.
- **Explosion side effect.** The remote kill sets explode-on-death to 100% (`diff_model.rs:2379–2443`). Each client's copy therefore explodes locally [INFERENCE on the effects].

**Ledger design (recommended).**
1. **Single spawner.** For each `drops_gold` death, exactly one client runs the death script: the owner, or the host if the owner is absent. Remote copies and respawns have `drop_money.lua` removed. Extend the `drops_gold` gate in `init_remote_entity` (≈:2625–2630) so it also covers respawns. Do not respawn-and-kill drops-gold enemies on peers. Deliver the resulting piles through DES instead.
2. **Piles are DES entities.** Remove `gold_nugget` from the exclusion list (`entity_sync.rs:132`). Each pile gets one gid and exists once.
3. **First-claim collection.** A pile is claimed by gid through the proxy (a new claim message or a per-flag policy). On a successful claim, the pickup adds to the shared balance via the existing delta path (`player_sync.lua:24–47`).
4. **Host ledger.** The host keeps `claimed: HashSet<Gid>` in `save_state`. On reconnect it re-sends the absolute balance and the claimed set. It never re-adds.
5. **Keep the existing wallet protocol** (`player_sync.lua`). It already does reading A. Only the spawn layer changes.
6. **Alternative: cosmetic piles.** The host credits the balance once at death time, and clients see non-collectable piles. This needs the amount computed by `drop_money`, which is unverified.

## 9. Architecture caveats for the design
- Keep the proxy `save_state` as the only authority. Treat each client's Noita save as a cache.
- Keep single-writer chunk authority and liveness-aware DES transfer (both applied).
- Use a per-flag policy instead of the global `duplicate` toggle.

## 10. Defects and risks (consolidated)
1. **Gold duplication** via respawn-and-kill (S3) and tracked-copy death (S2). The 200 gate covers only S2's 200 piles (`entity_sync.rs:207–269`, `diff_model.rs:2379–2443`, `wand_charm/append.lua:5`).
2. **Non-DES gold nuggets** (`entity_sync.rs:132`).
3. **`spawn_once` never pruned.** It grows for the whole run (`entity_sync.rs` ≈:207–213).
4. **Remote kill forces explode-on-death** (`diff_model.rs:2379–2443`). Local explosions per client, interacting with the unapplied explosion forwarding.
5. **Explosion forwarding not applied** (`explosion_forwarding.patch`). The installed `explosion_cuts.lua` lacks the host guard (fork :174, :187; installed copy unguarded at :209, :220, :233).
6. **Continue-run fix only in the working tree** (`net.rs:687`). Not in any patch file. Risk of loss.
7. **TLS server verification disabled** (`tangled/connection_manager.rs:250–251`, `helpers.rs:9–26`).
8. **Build-pinned offsets and x86-only address grabber** (`world_ffi.lua`, `addr_grabber.rs:1–150`).
9. **Unsafe mode** prevents Workshop distribution (`READ_ME_FIRST.txt:65`).
10. **Global `duplicate` toggle** (`net.rs:244, :285, :1036–1037`).
11. **Flag variant `0` emitter** not located.
12. **Polymorph gold reset** discards up to ~37 frames of unsent pickups (`player_sync.lua:294–298`) [INFERENCE].
13. **Open TODOs:** `des.rs:115`, `des.rs:199`, `gen_sync` ("probably a bad idea"), `world.lua` transparent-pixel nil path.
14. **Stale comment** at `world_sync.lua:114` (proxy option).
15. **Heart late-joiner bug** is a hypothesis only.
16. **Frame-based seed divergence** for late joiners [INFERENCE].
17. **`pvp_kill_steal` forced to 0** when `share_gold` is on (`net.rs:1101–1105`). Intended, but note it.
18. **Stream-only terrain.** Far chunks are not streamed, so "all chunks" depends on persistence (§6.1).

## 11. Unverified items
- Vanilla script bodies: `drop_money` denominations and spawn logic, gold pickup, orb pickup, chest, boss death, DamageModelComponent.
- Whether `EntityKill` runs `script_death` (`lua_api_documentation.txt:9` has no description).
- Meaning of `interest_tracker` and the exact partition between KillEntity and DeadEntities recipients [INFERENCE].
- The mod-side emitter for flag variant `0`.
- Send-side authority gating in `world_sync`.
- App-level peer authentication in `tangled`.
- File formats: `area_*.bin`, `world_tree.bin`, `world_sim.bin`, `.png_petri`, and `.salakieli` encryption.
- Gold write-back after the `on_poly_death` swap.
- Frame-seed divergence for late joiners.
- Full contents of `ENTITY_EXCLUDES`, and the exact heart exclusion term.
- Bot AI fidelity against the spec. Bot timeout behaviour.
- Which build the user runs (fork or installed copy).
- Fork git state.
- Equality of the two `no_default_death_handling.lua` copies.

## 12. Recommendations
1. **Requirement 5:** adopt reading A. Implement the single-spawner death model, DES-synced piles, first-claim collection, and the host ledger (§8.5). Do not rely on the 200 gate.
2. **Explosions:** apply `explosion_forwarding.patch` (or an equivalent). Host-guard the installed `explosion_cuts.lua`. Decide whether remote kills should keep explode-on-death at 100%.
3. **Flags:** replace the global `duplicate` toggle with per-flag policy.
4. **`spawn_once`:** bound it by entity age or by a claim/ack.
5. **Transport:** pin the server certificate or key before any internet use.
6. **Build:** pin one Noita build for the whole campaign. Keep the offsets in one table. The existing hard-fail on address mismatch is correct; keep it.
7. **Patches:** commit the continue-run fix (`net.rs:687`) and the disconnect fix. The working tree is the only copy.
8. **Test before building on top:** run the gold tests in §13 first.

## 13. Test plan (not run; for the main agent)
Two clients, `share_gold` on. Host A, joiner B.
- **T1 (S2):** B has a tracked copy of a `drops_gold` enemy. A kills it. Count piles per client. Expect 2 with the bug.
- **T2 (S3):** B is far away. A kills the enemy. B walks to the location later. Count piles that appear on B. Expect a duplicate.
- **T3 (200 gate):** repeat T1 and T2 with a 200-value pile. Expect the 200 to be suppressed in T1 and not suppressed in T2.
- **T4 (EntityKill):** kill a tracked copy with no DamageModel and watch for drops. Repeat for a localisation handover (`safe_entitykill`).
- **T5 (wallet):** pick up one pile on one client. Check that both wallets change by X exactly once. Polymorph within 37 frames of a pickup and check whether the pickup is lost (`player_sync.lua:294–298`).
- **Logs:** ewext and proxy logs for `KillEntity` and `DeadEntities` events.

## 14. Files index

| Path (fork root unless noted) | What it holds |
|---|---|
| `quant.ew/init.lua` | Mod entry: callbacks (193–725), CrossCall registry (373), setup (10–13) |
| `quant.ew/files/core/util.lua` | `util.prepend` (:47), `do_i_own` (:50–58), serialise (:350, :361–363) |
| `quant.ew/files/system/world_sync/world_sync.lua` | Terrain streaming, rings, send/decode (:35–235) |
| `quant.ew/files/system/player_sync.lua` | Wallet protocol (:5, :24–47, :291–313) |
| `quant.ew/files/system/local_health/local_health.lua` | Revive, bot, gold/inventory swap (:36–800) |
| `quant.ew/files/system/notplayer_ai/notplayer_ai.lua` | Bot AI (1673 lines) |
| `quant.ew/files/system/entity_sync_helper/entity_sync_helper.lua` | Chest/throw/broken-wand/death CrossCalls (:15–16, :172, :199) |
| `quant.ew/files/system/wand_charm/append.lua` | `EntityLoad` override, 200-gold gate (:3–7, :9–14) |
| `quant.ew/files/system/explosion_cuts/explosion_cuts.lua` | Explosion cut sender (fork :174–233) |
| `quant.ew/files/system/damage/damage.lua` | Shared HP pool (:109) |
| `quant.ew/files/system/boss_damage/boss_damage.lua` | Boss HP sync |
| `quant.ew/files/system/heart_pickups/heart_pickups.lua` | Heart heals (:20) |
| `quant.ew/files/system/orb_sync/orb_sync.lua` | Orb sync |
| `quant.ew/files/system/rnd_wands/rnd_wands.lua` | Per-peer seed (:1) |
| `quant.ew/files/system/wang_hooks/` | Allow-listed world spawns |
| `quant.ew/files/system/gen_sync/gen_sync.lua` | Shop run flags |
| `quant.ew/files/system/greed/greed.lua` | Greed curse CrossCall |
| `quant.ew/files/resource/cbs/chest_sync.lua` | Chest-open claim |
| `quant.ew/files/resource/interned_filenames.txt` | Goldnugget denominations (:1135–1176) |
| `quant.ew/NoitaPatcher/noitapatcher/nsew/world_ffi.lua` | Raw grid access, build-pinned offsets (425 lines) |
| `quant.ew/NoitaPatcher/noitapatcher/nsew/world.lua` | Run-length encoding of regions |
| `ewext/src/modules/entity_sync.rs` | DES client: exclusions (:124–134), spawn drain (:207–269), DeadEntities (:455, :464, :777–799) |
| `ewext/src/modules/entity_sync/diff_model.rs` | DES diff model: `drops_gold` (:1032–1035), death notify (:1319–1355), KillEntity (:1780–1786), kill closure (:2379–2443), `safe_entitykill` (:2977), `init_remote_entity` (:2539) |
| `ewext/src/addr_grabber.rs` | x86 hard-coded addresses (1–150) |
| `noita_api/src/serialize.rs` | Ewext serialisation globals (:4–32) |
| `noita_proxy/src/net.rs` | Flags (:903–955), duplicate (:244, :285, :1036–1037), settings send (:1025, :1054–1079), explosions (:91, :1470, :1507–1510), continue-run fix (:687) |
| `noita_proxy/src/net/world.rs` | Chunk authority (:40–803), disconnect (:664), explosions (:1735–2056), test (:3436) |
| `noita_proxy/src/net/des.rs` | DES authority and transfer (:25–199), tests (:279, :299) |
| `noita_proxy/src/net/messages.rs` | `RespondFlagNormal` (:33) |
| `noita_proxy/src/game_settings.rs` | Defaults (≈:54–133) |
| `noita_proxy/tangled/connection_manager.rs`, `helpers.rs` | Transport, TLS verification disabled (:250–251; :9–26) |
| `noita_proxy/src/steam_networking.rs`, `util/steam_helper.rs` | Steam transport (:106–108) |
| `shared/src/des.rs` | `drops_gold` field (:42, :141) |
| `explosion_forwarding.patch`, `fix.patch`, `fix2.patch` | Patches (explosion patch not applied) |
| `.omp/plans/*.md` | Plan status (§5.2) |
| `INST/tools_modding/lua_api_documentation.txt`, `component_documentation.txt` | Stock API and component docs |
| `INST/_version_hash.txt`, `INST/READ_ME_FIRST.txt:65` | Build pin and Workshop limitation |