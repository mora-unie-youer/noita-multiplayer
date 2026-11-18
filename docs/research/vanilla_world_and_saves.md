# Vanilla world generation, streaming, saves and entity lifecycle

Research for the analysis phase, 2026-11-18. Target build: `_version_hash.txt` =
`9dbd52ced019a643169a2db02f46c77f8766c6e5` (`noita.exe` "Noita (Jan 25 2025)", `LOG:1`).

Method: vanilla Lua/XML read from the unpacked `data.wak`; Lua and component docs read from
`tools_modding`; the user's Proton save and the `noita-switcher` backup tarballs were **read only**
(binary files were decompressed in memory, nothing was written). Nothing was built, launched or tested.

## Conventions

| Prefix | Meaning |
|---|---|
| `.cache/noita-data/data/…:N` | Vanilla data unpacked from `data.wak` (the only source for vanilla behaviour claims) |
| `API:N` | `/data/Games/steamapps/common/Noita/tools_modding/lua_api_documentation.txt:N` |
| `CD:N` | `/data/Games/steamapps/common/Noita/tools_modding/component_documentation.txt:N` |
| `SAVE/` | `/data/Games/steamapps/compatdata/881100/pfx/drive_c/users/steamuser/AppData/LocalLow/Nolla_Games_Noita/save00/` |
| `EXE "…"` | String literal present in `/data/Games/steamapps/common/Noita/noita.exe` (`strings -n 5`; no line numbers) |
| `LOG:N` | `/data/Games/steamapps/common/Noita/logger.txt:N` (session `20261118-004157`, EW active) |
| `RN:N` | `/data/Games/steamapps/common/Noita/_release_notes.txt:N` |
| `INST/mods/…` | Mods shipped with or installed into the game (`example`, `daily_practice`, `quant.ew` = Entangled Worlds) |
| `EW …` | `INST/mods/quant.ew/…` — prior art, cited for knowledge only (D7: no code reuse) |
| `NP` | NoitaPatcher bundled with EW: `INST/mods/quant.ew/NoitaPatcher/noitapatcher.dll` (string table) and `…/noitapatcher/nsew/world_ffi.lua` |
| [INFERENCE] | Not verified against code or data; must be checked in game on a throwaway profile |

Each mechanic lists hook points as **(a) detect**, **(b) suppress**, **(c) replicate N× / apply to all players**.

---

## 0. Key findings

1. **Terrain is reproducible from (world seed, NG+ count, data files) only after pinning four
   profile- or clock-dependent inputs**: biome modifiers (profile `death_count` and persistent flags),
   weather-driven FREEZING (UTC date/hour), flag-gated pixel scenes (mountain pillars, sun/scale
   scenes) and the order-dependent `is_unique` scene. With those pinned, clients can generate
   untouched areas locally and only touched areas need authoritative data (§1.9).
2. **Generation-time entity spawns are not reproducible** and must be authored once by the host:
   they read local date, run flags/globals, a run-global perk cursor, profile unlocks, bones,
   terrain raycasts and the local camera (§1.8, §1.10).
3. **The persistence unit is a 512×512 "area"**: `world_X_Y.png_petri` (cells), `entities_N.bin`
   (full entity serialisation), `area_N.bin` (pending camera-bound spawns), with
   `N = 2000·⌊y/512⌋ + ⌊x/512⌋`. All are a FastLZ container that I decoded successfully (§3.3).
4. **Noita writes an area when it streams it out**, writes everything else at save & quit, writes
   persistent flags immediately, and has a periodic crash autosave (§3.5). The engine silently
   reloads `entities_N.bin`/`area_N.bin` from the *local* save when an area streams back in — EW's
   log shows 36 "killed stale DES copy" cleanups in one session (`LOG:53`…).
5. **No stock entity serialiser and no stock "area generated" callback.** Wands and potions can be
   rebuilt field by field. Absolute frame numbers inside components must be rebased (§5).
6. **Player death is Lua-driven** (`OnPlayerDied` → `GameTriggerGameOver`) and can be suppressed;
   the player can be held at 0 HP with `wait_for_kill_flag_on_death` and/or
   `damage_about_to_be_received` (§4.4).
7. **`OnProjectileFired` is not a stock callback** (absent from `noita.exe`, present in NoitaPatcher).
   Spell RNG is seeded from frame numbers, entity IDs and an engine-internal spread RNG, so a
   faithful remote replay needs NoitaPatcher plus identical wand/deck/world state (§6).
8. **Global effects live in `WorldStateComponent`** (`flags`, `lua_globals`, `changed_materials`,
   `time`, weather) and are persisted in `world_state.xml`; `ConvertMaterialEverywhere` is a
   cumulative, order-dependent log (§7).

---

## 1. World generation pipeline

### 1.1 Inputs and initialisation order

| Step | Evidence |
|---|---|
| World seed chosen/loaded. Logged as "World seed: 4203465" and "ProceduralTerrain - worldseed: 4203465" | `LOG:6`, `LOG:11` |
| Seed persisted per save in `SAVE/world/.stream_info` (decoded field 2 = `0x004023C9` = 4203465) | §3.4 |
| Mods can force the seed with `SetWorldSeed(new_seed)` | `API:207` |
| Vanilla precedent: the Daily Practice game mode calls `SetWorldSeed( GetDailyPracticeRunSeed() )` at `init.lua` file scope | `INST/mods/daily_practice/init.lua:536` |
| `OnMagicNumbersAndWorldSeedInitialized` = "last point where the Mod* API is available"; the example calls `ProceduralRandom(0,0)` there | `INST/mods/example/init.lua:37-38` |
| `BIOME_MAP="data/scripts/biome_map.lua"` | `.cache/noita-data/data/magic_numbers.xml:46` |
| `OnBiomeConfigLoaded` → `init_biome_modifiers()` | `.cache/noita-data/data/scripts/init.lua:2`, `:204-206` |
| `ProceduralTerrain::Init( data/scripts/biome_map.lua )`, "Generating world…", "World generation took: 174.918 ms". This is the global layout; areas are generated lazily while streaming [INFERENCE] | `LOG:10-15` |
| Callback list compiled into the engine: `OnModPreInit`, `OnModInit`, `OnModPostInit`, `OnPlayerSpawned`, `OnPlayerDied`, `OnMagicNumbersAndWorldSeedInitialized`, `OnBiomeConfigLoaded`, `OnWorldInitialized`, `OnWorldPreUpdate`, `OnWorldPostUpdate`, `OnPausePreUpdate`, `OnPausedChanged`, `OnModSettingsChanged`, `OnCountSecrets` | `EXE` strings; also commented list at `.cache/noita-data/data/scripts/init.lua:4-15` |
| `OnWorldInitialized`: "Doesn't ensure any world chunks actually exist. Use OnPlayerSpawned…" | `INST/mods/example/init.lua:23` |

**NG+.** `do_newgame_plus()` increments the session number `NEW_GAME_PLUS_COUNT`
(`.cache/noita-data/data/scripts/newgame_plus.lua:5-8`), sets enemy scaling session numbers (`:11-19`),
loads `data/biome_impl/biome_map_newgame_plus.lua` with `BiomeMapLoad_KeepPlayer` and calls
`SessionNumbersSave()` (`:72-73`). The NG+ biome map seeds `SetRandomSeed( 4573, 4621 )` and reads
`NEW_GAME_PLUS_COUNT` (`.cache/noita-data/data/biome_impl/biome_map_newgame_plus.lua:118`, `:137`).
"NG+ cleans up the old world files" (`RN:725`). The world is therefore a function of
`(seed, NEW_GAME_PLUS_COUNT)`.

### 1.2 Biome map

- `BiomeMapSetSize( 70, 48 )` + `BiomeMapLoadImage( 0, 0, "data/biome_impl/biome_map.png" )`
  (`.cache/noita-data/data/scripts/biome_map.lua:3-4`; the PNG is 70×48).
- `_biomes_all.xml` maps map colours to biome XMLs, with `biome_offset_y="14"`
  (`.cache/noita-data/data/biome/_biomes_all.xml:1-3`); e.g. coalmine = `ffd57917` (`:59-63`).
- One biome-map pixel covers one 512×512 world area [INFERENCE: the biome `init(x,y,w,h)` is called
  per "biome pixel" — `.cache/noita-data/data/scripts/biomes/hills.lua:204-213` halves `w` "to limit
  spawn inside this biome pixel" — and save areas are 512×512]. Main world ≈ x∈[−17920, 17920),
  y∈[−7168, 17408) [INFERENCE from 70×48 and offset 14]. Parallel worlds repeat horizontally
  (`GetParallelWorldPosition`, `API:87`; used by `.cache/noita-data/data/scripts/biome_scripts.lua:101-123`).
- Map edits are only possible inside the BIOME_MAP script context (`API:224-230`).

### 1.3 Biome XML

`.cache/noita-data/data/biome/coalmine.xml` is representative:

| Element | Lines | Role |
|---|---|---|
| `<Topology type="BIOME_WANG_TILE" wang_template_file lua_script wang_map_width/height …>` | `:2-20` | Terrain generator, wang template, biome script |
| `<RandomMaterials><RandomColor input_color output_colors>` | `:21-24` | Seeded colour substitutions inside templates |
| `<BitmapCaves>` + `<CaveStructure image_file …>` | `:27-57` | Procedural caves and stamped structures |
| `<Materials><MaterialComponent …>` | `:60-216` | Material layers (limit_y, polka/perlin "rare" veins) |
| `<VegetationComponent tree_material …>` | `:217-372` | Engine-placed vegetation (becomes `tree_entity.xml` entities, §2.5) |

Other topologies: `SIN_CAPPED_SIMPLEX` (desert, hills: no template) and `static_tile` ("wang biomes
without randomization", `RN:237`). Biome configs can be edited during init with
`BiomeSetValue`/`BiomeObjectSetValue`/`BiomeMaterialSetValue`/`BiomeVegetationSetValue` (`API:90-95`);
this is how biome modifiers work (§1.8).

### 1.4 Spawn dispatch: pixel colour → Lua function

- **Global colour table** `wang_scripts.csv` with columns `COLOR, FUNCTION_NAME, IN_PATH_MIN,
  IN_PATH_MAX, OUTSIDE_PATH_MIN, OUTSIDE_PATH_MAX` (`.cache/noita-data/data/scripts/wang_scripts.csv:1`).
  Examples: `ff78FFFF spawn_heart` (`:19`), `ff50A0F0 spawn_wands,-3,-3,-4,-4` (`:20`, the only row with
  non-default path columns; semantics [INFERENCE]), `ffffd171 spawn_orb` (`:23`), `ffffd181 spawn_perk`
  (`:24`), `ffffff81 spawn_all_perks` (`:25`), `ff00FF5A spawn_apparition` (`:18`). Removed:
  `spawn_persistent_teleport`, `spawn_chest`, `spawn_blood`
  (`.cache/noita-data/data/scripts/wang_scripts_removed.txt:1-3`). The engine loads the CSV itself
  (`EXE "data/scripts/wang_scripts.csv"`).
- **Per-biome colours**: `RegisterSpawnFunction( color, function_name )` (`API:75`) at biome-script
  file scope, e.g. `.cache/noita-data/data/scripts/biomes/coalmine.lua:10-28`,
  `.cache/noita-data/data/scripts/biomes/temple_altar.lua:11-28`.
- **Where the colours live** (decoded in memory):
  - Wang templates: `.cache/noita-data/data/wang_tiles/coalmine.png` (348×448) contains 413 px
    `spawn_small_enemies`, 324 `spawn_lamp`, 177 `spawn_items`, 52 `spawn_big_enemies`, 6 `spawn_heart`,
    4 `spawn_apparition`, 3 `spawn_chest`, 7 `load_pixel_scene`, and biome-registered colours such as
    102 `spawn_nest`, 319 `load_i_structures`, 3 `load_oiltank`.
  - Pixel-scene material images: `.cache/noita-data/data/biome_impl/temple/altar.png` contains one
    pixel each of `spawn_all_perks` (ffffff81), `spawn_all_shopitems` (ff33934c), `spawn_hp`
    (ff6d934c), `spawn_workshop`, `spawn_fish`, `spawn_spell_visualizer`, `spawn_areachecks`,
    `spawn_worm_deflector`, `spawn_trigger_check_stats`, and five `spawn_lamp`. **Holy Mountain perks
    and shops are spawned by the temple pixel scene**, which is itself loaded by the biome `init`
    (`temple_altar.lua:128-131`).
- **Calling convention**: most functions get `(x, y)`; enemy functions get
  `(x, y, w, h, is_open_path)` (`coalmine.lua:1069`, `:1083`). Colour `0xffffeedd` registered as
  `init` (`fungicave.lua:9`, `temple_altar.lua:11`, `hills.lua:9`) receives the cell rectangle
  `init(x, y, w, h)` (`fungicave.lua:424-430`). Fungicave's template contains zero `0xffffeedd`
  pixels, so the engine calls `init` once per biome cell, not per pixel [INFERENCE].
- **When**: once, at first generation of an area. Afterwards the spawned entities live in
  `entities_N.bin`/`area_N.bin` and the spawn functions do not run again on reload [INFERENCE strongly
  supported by §3.4: the files contain the spawned trees, props, perks, wands, fish…].
- Each biome script runs in its own Lua context that `dofile_once`s the shared helpers
  (`coalmine.lua:4-8`) [INFERENCE: one VM per biome script].

### 1.5 Spawn helper semantics (`director_helpers.lua`)

| Function | Behaviour | Lines |
|---|---|---|
| `random_from_table(what, x, y)` | `ProceduralRandom(x,y) * total_prob`; honours `spawn_check()`, `ngpluslevel` (session number), `parallel` | `.cache/noita-data/data/scripts/director_helpers.lua:28-44`, `:3-26` |
| `entity_load_camera_bound(data, x, y, rx, ry)` | counts and jitter via `ProceduralRandom(x+i, y, …)`; spawns with `EntityLoadCameraBound` | `:48-107` (`:61`, `:66-70`, `:89`, `:98-103`) |
| `spawn` / `spawn2` | pick + `entity_load_camera_bound`; `spawn` adds a +5,+5 offset | `:149-166` |
| `load_random_pixel_scene(what, x, y)` | `ProceduralRandom(x,y)`; colour→material tables seeded `SetRandomSeed(x+11, y-21)`; `is_unique` entries set `prob=0` in the shared table | `:187-248` (`:204`, `:218-222`) |

`is_unique = 1` makes generation **order-dependent** within a session. Vanilla uses it once
(`.cache/noita-data/data/scripts/biomes/coalmine_alt.lua:686-689`, shrine01_alt).

### 1.6 Pixel scenes

- `_pixel_scenes.xml`: `PixelSceneFiles` (spliced `.plz` scenes, `.cache/noita-data/data/biome/_pixel_scenes.xml:3-28`),
  `BackgroundImages` (`:30-72`), `mBufferedPixelScenes` (`:80-250`). 61 entries use
  `just_load_an_entity`, e.g. the music machines at (14698,−90), (51050,−90), (−21654,−90)… (`:172-181`).
  NG+ swaps in `_pixel_scenes_newgame_plus.xml` (`newgame_plus.lua:72`).
- Runtime API: `LoadPixelScene(materials, colors, x, y, background, skip_biome_checks,
  skip_edge_textures, color_to_material_table, background_z_index, load_even_if_duplicate=false)`
  (`API:131`); duplicate loads are skipped unless the last argument is true ("now with allow_duplicate
  param", `RN:174`). `LoadBackgroundSprite` (`API:132`).
- `DoesWorldExistAt` returns false while "pixel scenes are loading in the area" (`API:172`).
- Pixel scenes trigger the colour dispatch of §1.4 (Lua) and `just_load_an_entity` (engine, no Lua).
- Persistence: `SAVE/world/world_pixel_scenes.bin` (§3.4) holds 275 scene records, including temple
  altars, spliced boss-arena `.plz` parts and `music_machine_00.xml` at (14698,−90) — the engine's
  registry/queue of placed scenes [INFERENCE on exact semantics].

### 1.7 Entity-level generation at load time

These run inside the spawned entity (LuaComponent `execute_on_added`), so they run wherever the
entity is first created:

| Entity | Seed | Extra inputs | Evidence |
|---|---|---|---|
| Procedural wands (`wand_level_XX.xml`) | `SetRandomSeed(x, y)` of the entity at creation | Spell pool filtered by persistent unlocks | `.cache/noita-data/data/entities/items/wand_level_01.xml:51-57`; `.cache/noita-data/data/scripts/gun/procedural/gun_procedural.lua:968-978` (`:971`), cards `:792-903` |
| Spells (`GetRandomAction*`) | `(x, y, level, i)` + world seed | 111 actions carry `spawn_requires_flag` (e.g. `.cache/noita-data/data/scripts/gun/gun_actions.lua:474`), passed to the engine (`.cache/noita-data/data/scripts/gun/gun.lua:85`); shop code prints an error if an unset-flag action comes back (`.cache/noita-data/data/scripts/items/generate_shop_item.lua:84-89`) → engine filters by **this profile's** persistent flags [INFERENCE] | `API:218-219` |
| Potions | `SetRandomSeed(x, y)` | Local date (30 Apr/1 May, Midsummer "jussi", Easter "mammi", 14 Feb), global `EXTRA_POTION_CAPACITY_LEVEL` | `.cache/noita-data/data/scripts/items/potion.lua:107-108`, `:131-158` |
| Chests | `PositionSeedComponent` (spawn position) + offsets 509.7/683.1 | `make_random_card` deliberately unseeded; persistent unlocks; `drop_random_reward` can seed with `GameGetFrameNum()` | `.cache/noita-data/data/entities/items/pickup/chest_random.xml:7`; `.cache/noita-data/data/scripts/items/chest_random.lua:7-35`, `:60-66`, `:317-349` |
| Starting wand / bomb wand / potion | Player spawn position (`SetRandomSeed(x, y-11)`, `(x-1, y)`, `(x, y)`) | Local date | `.cache/noita-data/data/scripts/gun/procedural/starting_wand.lua:32-33`, `starting_bomb_wand.lua:32-33`, `.cache/noita-data/data/scripts/items/potion_starting.lua:35-48` |
| HM shop items/wands | `SetRandomSeed(x, y)` "makes the shop items deterministic" | Global `TEMPLE_SHOP_ITEM_COUNT`; persistent unlocks | `generate_shop_item.lua:7-10`, `:183-185`; `temple_altar.lua:150-184` |
| HM perks | Per-run deck `SetRandomSeed(1, 2)` | Run cursor `TEMPLE_NEXT_PERK_INDEX`, `TEMPLE_PERK_COUNT`, per-perk `_PICKUP_COUNT` globals | `.cache/noita-data/data/scripts/perks/perk.lua:54-70`, `:141-157`, `:486-535` |

Consequences:
- All players spawning at `DESIGN_PLAYER_START_POS` (227,−85,
  `.cache/noita-data/data/magic_numbers.xml:17-18`) in one seed get **identical starting loadouts**.
- The perks shown at a Holy Mountain depend on how many Holy Mountains were generated earlier in the
  run (`perk_spawn_many` advances a run-global cursor, `perk.lua:499-530`), not on its position.
  `LOG:85-87` shows `spawn_perk REPELLING_CAPE -22 2946` etc. at generation, and the same perks appear
  as run flags in `SAVE/world_state.xml` (set at spawn, `perk.lua:532`).

### 1.8 Determinism inventory

`SetRandomSeed` is global for all Lua scripts ("SetRandomSeed sets the seed globally for all lua
scripts", `.cache/noita-data/data/scripts/init.lua:183`). Both `SetRandomSeed(x,y)` and
`ProceduralRandom(x,y)` mix in the world seed [INFERENCE, strongly supported: the per-run perk deck is
seeded with the constant `(1,2)` yet must differ per run (`perk.lua:55`, `:70`); weather rain "is
based on world seed" through `random_create`→`ProceduralRandomf` (`init.lua:122-129`,
`.cache/noita-data/data/scripts/lib/utilities.lua:25-36`)].

Counts over vanilla Lua: 349 `SetRandomSeed` calls, 169 of them include `GameGetFrameNum()`. Frame-seeded
calls cluster in `scripts/projectiles` (34 files), `scripts/buildings` (25), animals (34), perks (14).
`scripts/biomes/*` contains **no** `GameGetFrameNum` call at all.

| Class | Effect on generation | Examples |
|---|---|---|
| A. Pure (seed, position) | Reproducible | `random_from_table` (`director_helpers.lua:33`), `spawn_potions` (`biome_scripts.lua:86-95`), `spawn_chest` RNG (`coalmine.lua:1194`), temple tops (`.cache/noita-data/data/scripts/biomes/temple_altar_top_shared.lua:2-3`), wands, shop |
| B. Run state (flags, globals, session numbers) | Depends on what happened earlier in the run | `greed_curse` raises super-chest odds (`coalmine.lua:1196-1198`); `TEMPLE_SPAWN_GUARDIAN`, `TEMPLE_SHOP_ITEM_COUNT` (`temple_altar.lua:159`, `:226`); `TEMPLE_LEAKED_*`/`TEMPLE_COLLAPSED_*` (`.cache/noita-data/data/scripts/biomes/temple_shared.lua:54-71`); NG+ level (`director_helpers.lua:3-7`) |
| C. Profile (persistent flags, stats, all-time orbs, bones) | **Differs between players** | Biome modifiers (§ below); mountain-tree pillars `HasFlagPersistent` (`.cache/noita-data/data/scripts/biomes/mountain_tree.lua:326-335`); sun/darksun scenes (`.cache/noita-data/data/scripts/biomes/scale.lua:31-54`); temple fish = `GameGetOrbCountAllTime()` (`temple_altar.lua:253-258`); spell unlocks; apparitions (`SpawnApparition`, `biome_scripts.lua:29-62`, bones in `persistent/bones_new`, `APPARITION_MIN_BONES_REQUIRED`/`BIOME_APPARITION_CHANCE` `magic_numbers.xml:43-44`) |
| D. Wall clock | Differs across midnight / timezones | `GameGetDateAndTimeLocal` in 12 biome scripts + `biome_scripts.lua` (hearts 14 Feb `:149-150`; santa miners 24–26 Dec `coalmine.lua:140-148`; pumpkins 31 Oct `hills.lua:207-219`); weather uses UTC month/day/hour (`init.lua:121-129`, `:184-188`) |
| E. Generation order / session | Depends on which areas a client generated first | `is_unique` scenes; perk cursor; unseeded `Random()` continuing the global stream (`chest_random.lua:8-9`) |
| F. Terrain at spawn time | Depends on prior terrain edits | `spawn_mimic_sign` raycasts (`biome_scripts.lua:125-141`); deferred spawners poll `DoesWorldExistAt` then raycast (`.cache/noita-data/data/entities/props/physics/bridge_spawner.lua:15-20`, `.cache/noita-data/data/scripts/buildings/drain_pipe_spawn.lua:8-29`) |
| G. Mods | Differs if mod sets differ | `ModIsEnabled("nightmare")` (`.cache/noita-data/data/scripts/biomes/excavationsite.lua:1124`) |

**Biome modifiers (class C, affects terrain).** `get_modifier_mappings()` walks the biome list
starting with coalmine (`.cache/noita-data/data/scripts/biome_modifiers.lua:13-24`, `:1208-1236`)
using `random_create(347893, 90734)` (`:1219`). `has_modifiers` returns false for coalmine **without
consuming RNG** when `StatsGlobalGetValue("death_count") < 8` and the mode is not fully deterministic
(`:1192-1195`, `:1222-1223`). A player with <8 lifetime deaths therefore shifts the stream and gets
different modifiers in every later biome. Two modifiers are also gated by persistent flags
(`PROTECTION_FIELDS` needs `moon_is_sun`, `OMINOUS` needs `darkmoon_is_darksun`, `:852-890`,
`:1088-1092`). Weather snow (UTC month Dec–Feb, chance per hour/day) applies FREEZING modifiers to the
hills and mountain biomes at run time (`init.lua:126-144`), so terrain generated after that hour
differs.

### 1.9 Can clients generate terrain locally and sync only diffs? (R1.1.6, R1.1.2)

Yes for **cells**, with conditions:
1. Seed, `NEW_GAME_PLUS_COUNT`, game build and mod set match (R1.1.6). The entity schema hash
   (`c8ecfb34…`, §3.4) is a cheap per-build check.
2. Pin class-C/D terrain inputs before `OnBiomeConfigLoaded`: patch `biome_modifiers.lua` (via
   `ModLuaFileAppend`, `API:366`) so `has_modifiers` ignores `death_count` and `requires_flag` uses a
   fixed set; stop `weather_init` from applying FREEZING, or derive it from the seed only; pin the
   mountain-pillar and scale scenes to a host-defined flag set. The host must be able to send these
   before world init [INFERENCE: the mod's own init must read them synchronously, e.g. from the
   proxy].
3. Handle class E/F scenes: `is_unique` (one scene) and raycast-placed scenes such as the mimic sign
   must be host-authored or disabled.
4. "Pristine generation + diff" only holds for areas no one has simulated. Once any client loads an
   area, liquids, sand, fire and physics evolve, and the multithreaded simulator ("GridThreads size:
   64", `LOG:16`) is not deterministic across machines [INFERENCE]. Practical model: **untouched area
   → generate locally; touched area → authoritative snapshot or delta from the host**.

Entities spawned during generation must **not** be generated by every client (R1.2.6): see §1.10.

### 1.10 Generation-time spawns: policy and hook points

Common hooks:
- **(a)/(b)**: `ModLuaFileAppend(biome_script, wrapper)` for every `lua_script` named in the biome
  XMLs. The append runs after the original definitions (`API:366`), so it can wrap the global spawn
  functions or override `EntityLoad`, `EntityLoadCameraBound` (`API:3`, `API:5`), `LoadPixelScene`,
  `CreateItemActionEntity` inside that biome's context only. Appending to `biome_scripts.lua` alone is
  not enough, because biome scripts redefine functions after including it (e.g. `coalmine.lua:1049`).
  EW does the same with an allow-list (`EW files/system/wang_hooks/wang_hooks.lua`, see
  `docs/research/noita_capabilities.md` §6.7).
- **(c)**: the host's wrapper calls the original N times with perturbed positions or seeds
  (different loot, Q11), or spawns once and clones the serialised result (identical copies).

| Category | Source | Non-positional inputs | Policy | (a) detect | (b) suppress on clients | (c) ×N / all |
|---|---|---|---|---|---|---|
| Terrain cells, caves, biome materials | Engine (biome XML, wang tiles) | Modifiers, FREEZING | Local + host deltas (R1.1) | `DoesWorldExistAt` polling; biome `init` | Pin inputs instead (§1.9) | n/a |
| Static pixel scenes | `_pixel_scenes.xml`, `LoadPixelScene` in spawn functions | Flags, raycasts, order | Local (pinned); host for class E/F | Wrap `LoadPixelScene` | Drop class E/F on clients | n/a |
| Vegetation (`tree_entity.xml`) | Engine `VegetationComponent` | None | Local, never synced | — | — | — |
| Enemies, nests, apparitions, parallel-world NPCs | `spawn_*_enemies`, `spawn_apparition`, `parallel_check` | Date, bones, `apparitions_per_level` | Host-authored, one authority (R1.2.1, R1.2.6) | Wrap spawn functions | Wrapper returns early | Host spawns once |
| World items (wands, potions, altars, traps) | `spawn_items/wands/potions`, `spawn_trapwand` | Unlock flags, date | Host-authored (R1.2.2); N-fold per Q11 | same | same | Original ×N with offsets |
| Hearts and chests | `spawn_heart` (`biome_scripts.lua:144-180`), `spawn_chest` | 14 Feb, `greed_curse` | Host-authored S (D6, R2.2) | same | same | — |
| Orbs | `spawn_orb` (`.cache/noita-data/data/scripts/biomes/orbrooms/orbroom_00.lua:29-34`) | — | Host-authored S (R2.1) | same | same | — |
| HM perks + reroll | `spawn_all_perks` → `perk_spawn_many` | Run cursor, counts | **Personal P (D5)**: each client spawns its own and never syncs it | Wrap `spawn_all_perks` | Do not suppress; tag as personal | Natural: per client |
| HM shop | `spawn_all_shopitems` | Unlock flags, `TEMPLE_SHOP_ITEM_COUNT` | P or N (Q10) | Wrap | Per Q10 | Per Q10 |
| HM heal / refresh / `coop_respawn` | `spawn_hp` (`temple_altar.lua:133-138`) | — | Q12 | Wrap `spawn_hp` | Per Q12 | Per Q12 |
| Music machines and other `just_load_an_entity` | Engine (`_pixel_scenes.xml:172-181`) | — | Host-authored (R1.2.3, Q16) | No Lua hook | Remove entries with `ModTextFileSetContent` at init (`API:370`); host spawns them itself when the area exists | — |
| Camera-bound spawns | `EntityLoadCameraBound` | **Local camera** (§2.5) | Host-authored | Override in biome contexts | same | — |
| Deferred spawners (bridges, pipes) | Entities polling `DoesWorldExistAt` | Terrain state | Host-authored | Spawner entity creation | Strip on clients | — |

Requirement map: R1.2.6, R1.2.1–R1.2.3, R2.1, R2.2, R4 (D5, Q10, Q11), R1.1.6.

---

## 2. Chunk streaming

### 2.1 Units

- **Persistence and streaming unit: 512×512 world pixels ("area")**. Evidence: `.png_petri` headers
  say 512×512 (§3.4); file names use multiples of 512; NoitaPatcher's reverse-engineered `ChunkMap`
  has `cells[0x40000]` (0x40000 = 512²) per chunk (`NP world_ffi.lua:249-255`).
- Area index: `N = 2000·⌊y/512⌋ + ⌊x/512⌋`, e.g. `world_-1024_-1024` ↔ `area_-4002`,
  `world_14336_-7168` ↔ `area_-27972` (verified on both saves).
- Internal allocation in 64×64 blocks (`EXE "Allocate64x64"`). Render tiles 32 px
  (`.cache/noita-data/data/magic_numbers.xml:103` `GRID_RENDER_TILE_SIZE`).
- EW streams its own 128×128 network chunks on top (`docs/research/noita_capabilities.md` §6.1).

### 2.2 Generate vs load

- When the streamer needs an area, it loads `world_X_Y.png_petri` + `entities_N.bin` + `area_N.bin`
  if they exist, otherwise it generates the area and runs the spawn functions [INFERENCE from
  `EXE "IMPL_CreateNewChunk_PART2()"`, `"IMPL_SaveWorldChunk()"`, `"??SAV/world/entities_"`,
  `"??SAV/world/area_"`, `"LoadEntityToCameraBound() - couldn't load file: "` and the file contents].
- `DoesWorldExistAt(min_x, min_y, max_x, max_y)`: true if the box "has been streamed in and no pixel
  scenes are loading" (`API:172`). Vanilla spawners poll it before raycasting
  (`bridge_spawner.lua:18`, `drain_pipe_spawn.lua:8`).
- `data/scripts/streaming_integration/` is **Twitch voting** (`streaming_events`,
  `.cache/noita-data/data/scripts/streaming_integration/event_list.lua:7-14`; `API:358-365`), not
  world streaming. World streaming is engine-only.

### 2.3 What keeps areas loaded

- `StreamingKeepAliveComponent` (no documented fields, `CD:2455-2458`) is on the player
  (`.cache/noita-data/data/entities/player_base.xml:409-410`), Kolmi, the sun items, `maggot_tiny`
  and the greed ghost. These are entities that must not be lost when far from the camera; whether
  the component keeps the *entity* alive or keeps *areas around it* streamed in is unverified
  [INFERENCE].
- Magic numbers: `STREAMING_ENABLED`, `STREAMING_FREQUENCY`, `STREAMING_CHUNK_TARGET` (=12 in
  `.cache/noita-data/data/magic_numbers.xml:218`), `STREAMING_PERSISTENT_WORLD`,
  `STREAMING_AUTOSAVE_PERIOD_SECONDS`, `DEBUG_STREAMING_DISABLE_SAVING` (`EXE` names; defaults are
  compiled in, readable with `MagicNumbersGetValue`, `API:206`).
- Messages `EXE "Too many chunks!"`, `"Warning - streaming didn't find any chunks it could stream
  away..."` suggest a target count of resident areas [INFERENCE].
- Unloading is gradual: in the "experiments" profile, 18 spawn-side areas were written one by one,
  ~1 s apart, over 27 s after the player left (§3.5).
- "Player could be deleted when traveling back and forth extreme distances instantly" (`RN:25`):
  teleports stress the streamer.

### 2.4 Callbacks for "area generated / loaded / unloaded"

None in the stock callback list (§1.1). Options:
- Generated: biome `init(x,y,w,h)` and wrapped spawn functions (only on first generation).
- Loaded: poll `DoesWorldExistAt` for areas near players (`API:172`).
- Unloaded: no signal; infer from `DoesWorldExistAt` turning false [INFERENCE].
- NoitaPatcher exposes chunk-level access (`world_ffi.chunk_loaded`, `NP world_ffi.lua:363-370`), Q1.

### 2.5 Entities in unloaded areas

- **Full serialisation**: when an area streams out, its entities are written to `entities_N.bin`
  with all components (schema-based, §3.4) and restored when it streams in. The decoded files contain
  vegetation, props, physics lanterns with their particle emitters, gold nuggets, HM perks, shop
  cards, wands with card children, fish, and EW's `ew_des`/`ew_synced` tags (`SAVE/world/entities_3999.bin`).
- The player is **not** stored there; `player_unit` appears only as `CollisionTriggerComponent`
  `required_tag` in triggers (`entities_-2000.bin`, `entities_2004.bin`, `entities_9999.bin`); the player
  goes to `player.xml` (§3.4).
- **Camera-bound entities** (`CameraBoundComponent`, `CD:395-403`): `distance` (kill if farther from
  the camera), `distance_border`, `max_count` (kill the farthest beyond this count),
  `freeze_on_distance_kill`/`freeze_on_max_count_kill` (store to respawn later). `area_N.bin` stores
  these pending/frozen spawns as **file + position only** (state such as HP is lost), together with
  `(distance − border)²` and `max_count` copied from the entity XML: `physics_cart.xml` has
  `distance=500 max_count=50` (`.cache/noita-data/data/entities/props/physics_cart.xml:51-54`) and the
  file stores 480², 50; enemies inherit `distance=160000 max_count=20`
  (`.cache/noita-data/data/entities/base_enemy_basic.xml:85-88`) and the file stores 159980², 20/30.
- These decisions use **the local camera** [INFERENCE: vanilla has one camera; the component's text
  speaks of "the camera"]. Two clients therefore freeze and unfreeze different entities.
- `EntityGetFilename` returns a wrong value after the entity "passed through the world streaming
  system" (`API:40`); entity IDs grow linearly per process (`API:41`) and are reassigned on reload
  (the decoded files store no IDs). **Stable identity must be stored in a component**, e.g. a
  `VariableStorageComponent` — `SAVE/player.xml` shows EW's `ew_gid_lid` surviving save/load.

Requirement map: R1.2.5 (host-held entity state for unloaded areas), R1.2.6, R6.5, R6.7.

---

## 3. Save directory

### 3.1 Location and layout

- Real path (Steam library on `/data`): `SAVE/` above. `~/.local/share/Steam/steamapps/compatdata/881100`
  does not exist. Inside Wine the engine uses `C:\users\steamuser\AppData\LocalLow\Nolla_Games_Noita\save00`
  (`LOG:1834`).
- Slots `save00`…`save06` exist; only `save00` holds data. `save_shared/config.xml` is user config.
  Virtual roots in `EXE`: `??SAV/` = current slot, `??S00/` = `save00` root (persistent, mod config),
  `??STA` = `save00/stats`, `??USR` = `save_shared`. Command-line flags: `-save_slot`, `-gamemode`,
  `-clean_save` ("Deleting your save game!"), `-always_store_userdata_in_workdir`,
  `-always_store_userdata_in_appdata` (`EXE`). Game-mode mods may use multiple slots via
  `game_mode_supports_save_slots="1"` (`INST/mods/example/mod.xml:9`, `RN:487`).
- The current `save00` is the `duo-easily` (coop) profile, restored by `noita-switcher` at 00:49:31
  (`SAVE/noita-switcher.json`); all mtimes equal the restore time. The switcher keeps the other
  profiles as `noita-switcher.<id>.tar.gz` in the slot root; their tar headers preserve Noita's
  original mtimes (§3.5).

### 3.2 Inventory of `save00`

| Path | Count / size | Content |
|---|---|---|
| `player.xml` | 213,704 B | Full player entity tree (XML) |
| `world_state.xml` | 702,660 B | World-state entity (XML); 694,255 B of it are 35 `CARTOGRAPHER_*` `lua_globals` written by a map mod |
| `session_numbers.salakieli` | 345 B | Encrypted session numbers |
| `mod_config.xml`, `mod_settings.bin` | 1,993 B, 1,085 B | Enabled mods, mod settings (`EXE "??S00/mod_config.xml"`) |
| `steam_autocloud.vdf` | 53 B | Steam Cloud marker (one per directory) |
| `world/world_X_Y.png_petri` | 53 files, 4.24 MB | Cells of each area; 1,071 B = an all-air area; max 136,743 B |
| `world/entities_N.bin` | 53 files, 114 KB | Entities of each area; 52 B = empty |
| `world/area_N.bin` | 53 files, 1.9 KB | Camera-bound pending spawns; 28 B = empty |
| `world/.stream_info` | 3,001 B | Seed, frame counter, playtime, background sprites |
| `world/world_pixel_scenes.bin` | 6,122 B | Pixel-scene registry |
| `world/world_tree.bin` | 17,354 B | Unknown index (§3.4) |
| `world/world_sim.bin` | 1,164 B | Fog of war 4096×4096 bits |
| `world/DONT PLACE ANY FILES HERE - THIS FOLDER MIGHT BE CLEARED BY THE GAME` | 0 B | Marker (`EXE` string) |
| `persistent/flags/<flag>` | 131 files | One file per persistent flag |
| `persistent/orbs_new/<id>` | 2 files | Orbs collected all-time (ids 0, 1) |
| `persistent/bones_new/` | empty here; 5 `item*.xml` in the "default" profile | Wand XMLs from earlier deaths |
| `stats/` | 84 entries (+28 in `sessions/`) | `_stats.salakieli`, `sessions/<start>_stats.xml` / `_kills.xml`, `stats_<enemy>.xml` |

### 3.3 Binary container (verified)

Every `.png_petri`, `entities_N.bin`, `area_N.bin`, `.stream_info`, `world_tree.bin`, `world_sim.bin`,
`world_pixel_scenes.bin` is:

```
u32le compressed_size, u32le uncompressed_size,
payload: raw if sizes are equal, else a FastLZ stream (level 1 or 2 per first byte)
```

A 40-line FastLZ decoder reproduced the declared uncompressed size exactly for every file tried.
Payloads are **big-endian**.

### 3.4 Formats

**`world_X_Y.png_petri`** (verified):
```
u32 version = 24, u32 width = 512, u32 height = 512
u8 cell[512*512]          bits 0-6: index into the local material table; bit 7: has custom colour
u32 material_count, then material_count × string     (names, not engine ids: "air", "soil", "coal", …)
u32 colour_count (= number of cells with bit 7), then colour_count × u32 ARGB
u32 n, then physics bodies (Box2D) [INFERENCE: n = 5 in world_0_0; layout not decoded]
```
Strings are `u32 length + bytes`. Material names make files robust to material-id changes.

**`entities_N.bin`** (header verified, records [INFERENCE]):
```
u32 version = 2, string schema_hash ("c8ecfb341d22516067569b04563bff9c"), u32 entity_count
entity: string name, u8 ?, string filename, string tags, f32 x, f32 y, f32 scale_x, f32 scale_y,
        f32 rotation, u32 component_count, components {string type, u8, u8, string tags,
        fields in schema order}, children
```
The schema hash names `INST/data/schemas/<hash>.xml` (159 schemas ship with the game: per-component
field names, sizes and types). `EXE "Error - LoadEntities() - trying to load an old version, but can't
find schema for hash: "` and `"Warning - loading entities from an old entity file"` show that old saves
are migrated through old schemas. The format is engine-build specific (R7.5).

**`area_N.bin`** (fully decoded, parse consumed every byte of all 53 files):
```
i64 area_index (= N), u32 version = 1,
u32 spawn_count, spawn_count × { u32 file_index, f32 x, f32 y },
u32 file_count,  file_count  × { string entity_file, f32 (distance - distance_border)^2, u32 max_count }
```
Example (`experiments` profile, `area_2002.bin`): 21 pending spawns of `physics_cart`,
`firemage_weak`, `giantshooter_weak`, `longleg`, `miner_weak`, `shotgunner_weak`, ….

**`.stream_info`**: `u32 24, u32 world_seed (4203465), u32 frame_counter (19701), f32 playtime_s
(328.31)`, then background-sprite placements (`data/weather_gfx/…`, `data/biome_impl/…_background.png`)
[INFERENCE for the tail]. The experiments profile: seed 1158946533, frame 4523, 75.38 s.
**`GameGetFrameNum()` is per save and persists across sessions** (EW's `ew_player_rng = 19700` global
matches).

**`world_sim.bin`**: `u32 1, u32 4096, u32 4096`, then compressed bits — matches
`"WorldLightAndFog::BitSerialize w,h: 4096, 4096 count: 16777216"` at load (`LOG:19`) and right after
"player saved" (`LOG:1836`): fog of war.

**`world_tree.bin`**: `u32 0, u32 n (1923 here, 1915 in experiments)`, then n × `{u32 index, 64 bytes}`.
Meaning unknown; loaded at "Loading 2" (`EXE` order "Loading 2 : ", `"??SAV/world/world_tree.bin"`);
debug dumps `"temptemp/worldtree_loaded.png"` / `"worldtree_saved.png"` [INFERENCE: streaming index].

**`world_pixel_scenes.bin`**: `u32 3, u32 0x02F0AA9F (hash?), u32 275`, records
`{i32 x, i32 y, string material, string visual, string background, u8, u8, u32 z_index (50),
string entity_file, …}`; spliced `.plz` records use a longer tail [INFERENCE beyond the first
fields].

**`player.xml`**: entity XML (`_version="1"`), all components with private fields (e.g. wand
`AbilityComponent mNextFrameUsable="17602"`, `mana="112"`), children `arm_r`, `inventory_quick`
(wands, potion), `inventory_full` (loose spells), `cape`, effect entities. It also contains every
component a mod ever added: 6 duplicate `count_times_wand_fired.lua`, 7 `death_notify.lua`, 9
`item_notify.lua` LuaComponents from EW. **Mod-added player components accumulate across save/load
unless de-duplicated**; vanilla Daily Practice guards its one-time player edits with a Globals key
(`INST/mods/daily_practice/init.lua:222-230`).

**`world_state.xml`**: entity tagged `world_state` with `WorldStateComponent` (all fields, `CD:2641-2707`)
including `lua_globals` (Globals map: `TEMPLE_NEXT_PERK_INDEX=7`, `TEMPLE_COLLAPSED_0_2=1`,
`HOLY_MOUNTAIN_DEPTH`, `ORB_MAP_STRING`, `visited_biomes`…), `flags` (run flags: `PERK_ALWAYS_CAST`,
`new_action_bubbleshot_trigger`, EW's `ew_flag_this_is_host`…), `changed_materials`,
`cuts_through_world`, `orbs_found_thisrun`, `pending_portals`, `apparitions_per_level`, `npc_parties`,
weather/time fields; child `player_stats` with `PlayerStatsComponent lives="1" max_hp="4" speed="1"`.

**`session_numbers.salakieli`**: encrypted ("salakieli" ≈ "secret language"); `EXE` also lists
`??SAV/session_numbers.xml`. Access through `SessionNumbersGetValue/SetValue/Save` (`API:208-210`). Keys
used by vanilla: `NEW_GAME_PLUS_COUNT`, `DESIGN_SCALE_ENEMIES`, `DESIGN_NEW_GAME_PLUS_HP_SCALE_MIN/MAX`,
`DESIGN_NEW_GAME_PLUS_ATTACK_SPEED` (`newgame_plus.lua:5-19`).

**`persistent/`**: a flag is set iff `persistent/flags/<name>` exists; each file contains
`why are you looking here\r\n` (also an `EXE` string). `orbs_new/<id>` mirror all-time orbs
[INFERENCE]. `bones_new/item<N>.xml` are wand entity XMLs (`teleportable_NOT,hittable,item,wand`) from
earlier deaths, used for apparitions [INFERENCE: `.cache/noita-data/data/entities/animals/playerghost.xml:2-3`
"spawned player ghost (that has an old wand)"; `EXE "??S00/persistent/bones_new/item"`]. `EXE` also
lists `persistent/orbs/` (legacy) and `persistent/identified_items/`.

**`stats/`**: `sessions/<start>_stats.xml` hold `dead`, `killed_by`, `playtime`, `world_seed`; the
"default" profile shows several `dead="1"` sessions. `_stats.salakieli` holds global stats read by
`StatsGlobalGetValue` (`API:213`), including `death_count` used by biome modifiers.

### 3.5 When Noita writes

Evidence from the `experiments` tarball (`SAVE/noita-switcher.c9cafc86.tar.gz`, original mtimes):

| Time | Files written |
|---|---|
| 16:33:25 | Session start (stats file name `20261117-163325`) |
| 16:33:28 → 16:33:55 | 18 areas near spawn (x∈[−1024,1024], y∈[−1024,512]), each as `world_*.png_petri` + `entities_N` + `area_N`, ~1 s apart |
| 16:33:41 | `persistent/flags/action_light_bullet` (mid-session) |
| 16:35:00–01 | `session_numbers.salakieli`, `stats/*`, `player.xml`, `world_state.xml`, the 10 areas around the final position (x∈[13824,14848], y∈[−7168,−5632]) plus one more, `.stream_info`, `world_pixel_scenes.bin`, `world_tree.bin`, `world_sim.bin`, `mod_config.xml`, `mod_settings.bin` |

Interpretation: the player jumped far from spawn within ~3 s (teleport [INFERENCE]); spawn-side areas
were written **as they streamed out**; persistent flags are written **immediately**; everything else
is written at **save & quit**. The log confirms the quit write: `"player saved to: ??SAV/player.xml"`
(`LOG:1835`) followed by the fog serialisation (`LOG:1836`).

**Autosave exists and is on in release builds**: `DEBUG_ENABLE_AUTOSAVE="1"` in the release override
`.cache/noita-data/data/magic_numbers_disable_debug.xml:17`; period `STREAMING_AUTOSAVE_PERIOD_SECONDS`
(compiled default). Files: `EXE "??SAV/world/.autosave"`, `".autosave_player"`,
`".autosave_world_state"`, `".autosave_world_pixel_scenes"`. No such files exist after a clean quit
[INFERENCE: deleted on clean exit]. `AutosaveDisable()` turns it off (`API:211`).

**Crash**: on start the engine runs `"Checking for autosaves"`, logs `"AUTOSAVE DETECTED in save slot "`
/ `"!! AUTOSAVE DETECTED - the game had crashed?!?"` and asks "Autosave found. Looks like the game
crashed? Should the game attempt to restore the autosave?" with "Restore" / "Nah. Destroy it"
(`INST/data/translations/common.csv`, keys `menuautosave_*`). Restores are counted in
`WorldStateComponent.DEBUG_LOADED_FROM_AUTOSAVE` (`CD:2686`, `LOG:40`). Areas streamed out before the
crash are already on disk, so player, world state and areas can come from different moments
[INFERENCE]: duplicated or lost items are possible after a crash.

**Game over**: `OnPlayerDied` → `GameTriggerGameOver()` (`.cache/noita-data/data/scripts/init.lua:212-215`)
→ game-over screen (`EXE "event_cues/game_over/create"`, `"$menugameover_newgame"`,
`"$menugameover_savereplay"`, `"$menugameover_quit"`) → the run save is cleared (`EXE "Deleting save "`,
debug switch `"DEBUG_NO_SAVEGAME_CLEAR_ON_GAME_OVER"`) and bones are written/cleared
(`"??S00/persistent/bones_new/"`, `"Clearing bones"`). The "default" profile snapshot after its last
runs has no `player.xml`/`world_state.xml` and an empty `world/` [INFERENCE: exact moment of deletion
not observed]. **NG+** deletes old world files (`RN:725`).

**Steam Cloud** syncs the slot (`steam_autocloud.vdf`; `STEAM_CLOUD_SIZE_WARNING` in
`magic_numbers_disable_debug.xml:2`): it can bring back an older local save on another machine.

### 3.6 Can a mod read or write these files while running?

- `request_no_api_restrictions="1"` unlocks `io`/`os` (`INST/mods/example/mod.xml:12`); unsafe mods
  cannot ship through the Workshop (`docs/research/noita_capabilities.md` §3.4).
- Reading is feasible: the formats above are simple; build the path from the Wine user directory.
  Files are rewritten at unload, autosave and quit; atomicity is unverified [INFERENCE].
- Writing area files for **unloaded** areas could inject cells and entities when they stream in, but
  `world_tree.bin` or in-memory caches may conflict [INFERENCE, untested]. Test only on a dedicated
  slot or throwaway profile, never on the user's run (AGENTS.md).
- Stock helpers: `SessionNumbersSave()` (`API:210`), `AutosaveDisable()` (`API:211`).

Requirement map: R6.2, R6.3, R6.4 (continue vs NG+ paths), R6.7, R6.8, R7.5, Q8, Q22.

---

## 4. Player entity

### 4.1 Composition

`.cache/noita-data/data/entities/player.xml:1-21`: tags `mortal,human,hittable,peasant,prey,player_unit,teleportable`,
base `player_base.xml` with children, `inventory_quick` (starting wand, bomb wand, potion) and empty
`inventory_full`. Daily runs use `player_rng_items.xml`.

| Component (player_base.xml) | Lines | Note for MP |
|---|---|---|
| `ControlsComponent enabled="1"` | `:125-130` | Reads local input; `mButtonDown*`, `mAimingVector`, `mMousePosition` privates (`CD:585-674`); `polymorph_hax` fields |
| `DamageModelComponent hp="4"` (×25 in UI), `invincibility_frames="60"`, material damage list, player ragdoll | `:132-161` | `wait_for_kill_flag_on_death` default 0 (`CD:759`) |
| `Inventory2Component quick_inventory_slots="10" full 16×1` | `:190-194` | `mSavedActiveItemIndex` keeps the held item over save/load (`CD:1240`); `mActiveItem` must not be written from Lua (`CD:1242`) |
| `ItemPickUpperComponent drop_items_on_death="0"` | `:334-336` | Player does not drop wands on death |
| `GameLogComponent report_death="1"`, `GameStatsComponent is_player="1"` | `:348-353` | Session stats, death message |
| `WalletComponent`, `StreamingKeepAliveComponent` | `:406-410` | |
| `PlatformShooterPlayerComponent` | `:36` | `center_camera_on_this_entity` (`CD:2126`) |
| `AudioListenerComponent` | `:323` | Audio is heard from this entity |
| LuaComponents `amulet.lua`, `player_biome_check.lua`, `orb_map_init.lua` | `:495-513` | |

`get_players()` = `EntityGetWithTag("player_unit")` (`.cache/noita-data/data/scripts/lib/utilities.lua:431-433`);
many vanilla scripts take `players[1]` (e.g. `.cache/noita-data/data/scripts/magic/null_room/check.lua:95-96`,
the Sampo ending `sampo_start_ending_sequence.lua:128`, `:245-264`), and triggers use
`required_tag="player_unit"` (`.cache/noita-data/data/entities/items/pickup/heart_fullhp_temple.xml:11-18`
uses `polymorphed_player`). **Remote-player proxies must not carry `player_unit`**, or vanilla logic
will target them.

### 4.2 Inventory model

Wands and items are children of `inventory_quick`; loose spells are children of `inventory_full`;
cards are children of their wand. Position = `ItemComponent.inventory_slot` (`CD:1366`): decoded
`player.xml` shows wands at slots x=0,1,2 and the two loose spells at x=0,1. API:
`GameGetAllInventoryItems` (`API:125`), `GamePickUpInventoryItem` (`API:124`), `GameKillInventoryItem`
(`API:123`), `GameDropAllItems` / `GameDropPlayerInventoryItems` / `GameDestroyInventoryItems`
(`API:126-128`).

### 4.3 Death flow

1. `DamageModelComponent.hp ≤ 0` → `Message_Death`. Components with a death listener (`EXE` RTTI
   `MessageListener<…, Message_Death>`): `ItemChestComponent` (enemy loot), `ItemPickUpperComponent`,
   `Inventory2Component`, `MaterialInventoryComponent`, `ExplodeOnDamageComponent`, `LuaComponent`
   (`script_death`, `CD:1540`), `GameLogComponent`, `GameStatsComponent`, `PhysicsBodyComponent`,
   `PhysicsBody2Component`.
2. Ragdoll and kill FX; `EXE "HandleEvent - Player Entity Destroyed"`.
3. Engine calls `OnPlayerDied(player_entity)`; vanilla `GameDestroyInventoryItems(player_entity)` +
   `GameTriggerGameOver()` (`.cache/noita-data/data/scripts/init.lua:212-215`). The entity still exists
   at that point (its inventory is destroyed there) [INFERENCE on exact ordering].
4. Game-over UI, save cleared, bones (§3.5).

### 4.4 Keeping a player alive at 0 HP

| Mechanism | Evidence | Notes |
|---|---|---|
| `wait_for_kill_flag_on_death=1`: no kill/FX/ragdoll until `kill_now=1` | `CD:759-760`; Kolmi: `boss_centipede.xml:134-136`, its update polls `hp <= 0.0` (`boss_centipede_update.lua:711-720`) and sets `kill_now` after the death sequence (`:751-755`); also `lukki_minion.xml:60`, `vacuum_powder.lua:8` | The flag is persisted: decoded `SAVE/player.xml` has it set by EW with `hp="3.344"`. Unknown whether `Message_Death`/`script_death`/`OnPlayerDied` fire at hp≤0 or only at `kill_now` [INFERENCE] |
| `script_damage_about_to_be_received` returns `(new_damage, new_crit)` | `CD:1532`; vanilla returns `damage*0.0` (`.cache/noita-data/data/scripts/perks/ingest_projectiles.lua:3`, `:20`) | Clamp to `hp − ε` and raise a "downed" event. Unverified that material, fire, drowning and curse damage pass through it [INFERENCE] |
| `script_damage_received(damage, msg, who, is_fatal, projectile)` | `CD:1531` | Detection only (`is_fatal`) |
| Redefine `OnPlayerDied` | `ModLuaFileAppend("data/scripts/init.lua", …)`; EW does exactly this (`EW files/core/early_init.lua:2`, `EW files/resource/append/no_default_death_handling.lua:1-3`) | Prevents game over but not entity death |
| Engine effects `RESPAWN` (extra life) and `SAVING_GRACE` | `.cache/noita-data/data/scripts/perks/perk_list.lua:754-769`, `:464-476` | Engine-side; `GetGameEffectLoadTo` (`API:153`) can add them [INFERENCE] |
| `invincibility_frames`, `PROTECTION_ALL` effect | `CD:724`; `.cache/noita-data/data/entities/misc/effect_protection_all.xml` | Temporary |
| `DEBUG_PLAYER_NEVER_DIES` | `EXE` magic number | Debug only |

Recommended: `wait_for_kill_flag_on_death=1` + a damage clamp + overridden `OnPlayerDied` + explicit
"downed" handling; use `kill_now`/curse damage only when the run really ends (EW does the inverse at
`EW files/system/local_health/local_health.lua:490-520`, `:525-530`). Requirement map: R3.1, R3.5, Q6, Q8.

### 4.5 Polymorph

- Effects `POLYMORPH` (600 frames, sheep), `POLYMORPH_RANDOM` (`[RANDOM]`), `POLYMORPH_UNSTABLE`,
  `POLYMORPH_CESSATION` (`.cache/noita-data/data/entities/misc/effect_polymorph.xml:5-11`, …`_random.xml:6-9`).
  `GameEffectComponent.polymorph_target` and `mSerializedData` "polymorph stores the serialized entity
  here" (`CD:1001-1002`). On expiry the original is rebuilt from that blob [INFERENCE: new entity ID].
- While polymorphed the creature is tagged `polymorphed_player` and has no `player_unit`: vanilla falls
  back to that tag when `get_players()` is empty (`.cache/noita-data/data/entities/animals/boss_sky/boss_sky_damage.lua:13-16`,
  `null_room/check.lua:94-116`); Cessation uses `polymorphed_cessation` (`.cache/noita-data/data/entities/animals/nibbana.xml:1`).
- Hooks: `script_polymorphing_to(target_xml)` (`CD:1552`); polymorph counters in `WorldStateComponent`
  and `GameStatsComponent` (`CD:2664`, `CD:1043`); `PolymorphTable*` (`API:154-157`). Death while
  polymorphed kills the player (`EXE "$death_polymorph"`, `"HAX_polymorph_death"`).
- Anything keyed on the player's entity ID breaks across polymorph. Requirement map: R1.3.1, Q17.

### 4.6 A player-shaped enemy with an inventory (revive bot, R3)

Vanilla patterns:
- **Hold a wand**: `ItemPickUpperComponent is_in_npc="1"` + `only_pick_this_entity` +
  `GamePickUpInventoryItem(ghost, wand, false)`, then verify with `GameGetAllInventoryItems`
  (`.cache/noita-data/data/scripts/animals/wand_ghost.lua:8-31`, `CD:1380-1384`). Many NPCs carry
  `ItemPickUpperComponent is_in_npc` (e.g. `.cache/noita-data/data/entities/animals/miner.xml:103-105`).
- **Fire through the gun system**: `AnimalAIComponent attack_ranged_use_message="1"` sends
  `Message_UseItem` (`CD:172`); `player_drone.xml` does it with its own `AbilityComponent`
  (`.cache/noita-data/data/entities/misc/player_drone.xml:3`, `:35-51`, herd `player` `:76-77`).

| Base | Gains | Gaps / risks |
|---|---|---|
| Copy of `player_base.xml` + saved inventory (EW NotPlayer approach, `docs/research/noita_capabilities.md` §6.10) | Same sprite, arm, cape, `Inventory2`, card handling, perks and effects as components | Disable `ControlsComponent` and drive its `mButton*`/`mAimingVector` from AI [INFERENCE]; remove `player_unit`, `AudioListenerComponent`, camera centring, `GameStatsComponent is_player`, `StreamingKeepAlive` as needed |
| `entities/animals/apparition/playerghost.xml` + `base_apparition.xml` | Player-ghost sprite, AI, wand via pickup | Flying, melee-only AI (`base_apparition.xml:20`), drops a random chest loot (`ItemChestComponent`, `:3`), herd `apparition`, 10/50 HP |
| `entities/misc/player_drone.xml` | AI that fires a gun-system ability | Robot sprite; ability not an inventory |
| Humanoid NPC (`miner.xml`, `alchemist.xml`) | Walks, picks up wands | Different sprite; AI decides use |

For any base: no `CameraBoundComponent` (§2.5), a stable ID component, `ItemChestComponent` removed,
`drop_items_on_death` per Q5, `StreamingKeepAliveComponent` if it must outlive its area.
**Vanilla co-op precedent**: each Holy Mountain spawns a `coop_respawn` marker (`temple_altar.lua:137`,
`.cache/noita-data/data/entities/buildings/coop_respawn.xml:1-3`) and the engine has local co-op
resurrection ("PROCEED TO NEXT ALTAR IN", "s TO RESURRECT THE DEAD", `$log_coop_resurrected_player`
in `EXE`) — the engine tolerates several player entities [INFERENCE].

---

## 5. Entity serialisation

### 5.1 Stock API

There is no Lua function that turns an entity into data and back:
- `EntitySave` works only in dev builds (`API:7`); `EntityLoad` loads files (`API:3`);
  `EntityLoadToEntity` loads components but not tags (`API:6`); `ModTextFileSetContent` is init-only
  (`API:370`), so a generic "write XML then load it" needs unsafe `io` and a loadable path
  [INFERENCE].
- The engine has two serialisers (XML for `player.xml`, binary schema format for areas, plus the
  polymorph blob `mSerializedData`), none exposed. NoitaPatcher wraps them (`NP`
  `SerializeEntity`/`DeserializeEntity`) — depends on Q1 and is build-pinned.
- Field-level path: `EntityGetAllComponents`, `ComponentGetTypeName`, `ComponentGetMembers`
  (members as strings), `ComponentObjectGetMembers` (sub-objects such as `gun_config`),
  `ComponentGetValue2` (typed; also returns `count_per_material_type` as a table, used by
  `EW files/system/notplayer_ai/notplayer_ai.lua:141`), `ComponentGetVector`, `ComponentGetTags`,
  `ComponentGetIsEnabled` → `EntityCreateNew` + `EntityAddComponent2` (`_tags`, `_enabled`) +
  `EntityAddChild` (`API:8`, `11`, `13`, `21`, `59-71`). Custom types and some privates cannot be set.

### 5.2 Wand

Prefer: load a **non-procedural** base (no `generate_gun` LuaComponent — loading `wand_level_XX.xml`
re-rolls it, `wand_level_01.xml:51-57`) and apply fields, then attach cards with
`AddGunAction`/`AddGunActionPermanent` (`.cache/noita-data/data/scripts/gun/procedural/gun_action_utils.lua:1-36`).

| Part | Fields | Evidence |
|---|---|---|
| `AbilityComponent` | `ui_name`, `sprite_file`, `mana`, `mana_max`, `mana_charge_speed`, `gun_level`, `current_slot_durability`, `use_gun_script=1`, recoil fields; privates `mNextFrameUsable`, `mCastDelayStartFrame`, `mReloadFramesLeft`, `mReloadNextFrameUsable` | `CD:36-89`; `gun_procedural.lua:920-932` |
| `gun_config` | `actions_per_round`, `shuffle_deck_when_empty`, `reload_time`, `deck_capacity` | `.cache/noita-data/data/scripts/gun/gun_generated.lua:1-5` |
| `gunaction_config` | `fire_rate_wait`, `spread_degrees`, `speed_multiplier` for normal wands; any field of `ConfigGunActionInfo` for unusual ones | `.cache/noita-data/data/scripts/gun/gunaction_generated.lua:1-66` |
| Sprite + tip | `SpriteComponent` tag `item` (`image_file`, `offset_x/y`), `HotspotComponent shoot_pos` offset | `gun_procedural.lua:340-357` |
| Wand `ItemComponent` | `item_name`, `always_use_item_name_in_ui`, `ui_sprite`, `inventory_slot`, `is_frozen` | `CD:1326-1368` |
| Each card (child) | `ItemActionComponent.action_id`; `ItemComponent.uses_remaining`, `permanently_attached` (Always Cast), `is_frozen`, `is_identified`, `inventory_slot.x` (order) | `CD:1302-1305`, `CD:1333-1336`, `CD:1366`; decoded `SAVE/player.xml` |

**Frame rebasing**: `mNextFrameUsable`, `mCastDelayStartFrame`, `mReloadNextFrameUsable`,
`ItemComponent.next_frame_pickable`/`npc_next_frame_pickable`/`mFramePickedUp` (`CD:1354-1362`),
`LifetimeComponent.creation_frame`/`kill_frame` (`CD:1440-1441`, `serialize_duration` `:1434`),
`DamageModelComponent.mLastDamageFrame` (`CD:809`) store absolute frames, and frame counters differ
per save (§3.4). Add `(dst_frame − src_frame)` when moving an entity between clients.

### 5.3 Potion and pouch

- Contents: `ComponentGetValue2(mic, "count_per_material_type")` → table indexed by material id + 1
  (`CD:1641`; EW usage above). Rebuild with `RemoveMaterialInventoryMaterial(entity)` +
  `AddMaterialInventoryMaterial(entity, name, count)` (`API:80-81`). Send **names**, not ids; the
  save does the same (`<Material material="water" count="879">`).
- Base: avoid `potion.xml` (its init picks a random material, `potion.lua:107-158`); use an empty
  potion base. Copy `MaterialInventoryComponent` flags (`max_capacity`, `on_death_spill`,
  `do_reactions`…, `CD:1619-1646`), `PotionComponent` (`CD:2178`), `ItemComponent`, `UIInfoComponent`.

### 5.4 `EntityKill` vs `Message_Death`

`EntityKill` has no documented semantics (`API:9`). Evidence that it **does not** send
`Message_Death` [INFERENCE]: vanilla code that needs death behaviour uses `kill_now`
(`boss_centipede_update.lua:751-755`) or lethal damage (`EntityInflictDamage(v, 500, "DAMAGE_CURSE", …)`,
`.cache/noita-data/data/scripts/buildings/sun/sun_collision.lua:28-34`); the engine distinguishes
"send MessageDeath" as an explicit action (`MaterialInventoryComponent.kill_when_empty`, `CD:1630`);
EW kills remote copies with damage when it wants drops. `execute_on_removed` LuaComponents run on
removal (`CD:1521`; polymorph explosion `effect_polymorph.xml:78-82`) [INFERENCE: also on `EntityKill`].
Must be tested: enemy with `drop_money.lua` death script + `EntityKill` → check for gold.

Requirement map: R1.2.2, R1.2.4, R3.2, R3.4, R3.7, R5.

---

## 6. Projectiles

### 6.1 Casting pipeline

`AbilityComponent use_gun_script=1` (`CD:70`) → the gun Lua VM: `_start_shot`
(`.cache/noita-data/data/scripts/gun/gun.lua:454-484`), `_draw_actions_for_shot` → `draw_shot`
(`:486-518`, `:154-165`), `order_deck` (`:170-200`), `_handle_reload` (`:520-535`),
`register_action`/`ConfigGunActionInfo_PassToGame` (`:65-68`), actions in `gun_actions.lua` →
`BeginProjectile`/`EndProjectile`/`SetProjectileConfigs` (`API:383-405`) → the engine creates the
projectile entities and applies spread (`gunaction_config.spread_degrees`,
`ProjectileComponent.direction_random_rad`, `CD:2211`).

### 6.2 Callbacks

| Hook | Stock? | Signature |
|---|---|---|
| `LuaComponent script_shot` on the shooter (`Message_Shot`) | Yes | `shot(projectile_entity_id)` (`CD:1534`; e.g. `perk_list.lua:1129`, `.cache/noita-data/data/scripts/_examples/roboguard_shot.lua:1-8`) |
| `LuaComponent script_wand_fired` (`Message_WandFired`) | Yes | `wand_fired(gun_entity_id)` (`CD:1549`; `perk_list.lua:1083`) |
| Gun-VM functions `OnActionPlayed`, `OnNotEnoughManaForAction` | Yes | `API:397-398` |
| `ProjectileComponent.mWhoShot`, `mShooterHerdId`, `mEntityThatShot` | Yes (read) | `CD:2289-2293` |
| `GameShootProjectile(shooter, x, y, tx, ty, projectile, send_message, verlet_parent)` | Yes (spawn) | `API:138` |
| `OnProjectileFired` / `OnProjectileFiredPost` | **No** — absent from `noita.exe`; present in `NP` with `InstallShootProjectileFiredCallbacks`, `SetProjectileSpreadRNG` | As used by EW: `(shooter_id, projectile_id, initial_rng, position_x, position_y, target_x, target_y, send_message, unknown1, multicast_index, unknown3)` (`EW init.lua:193-205`, `:341-353`) |

### 6.3 Spell RNG

- Deck shuffle: `SetRandomSeed( GameGetFrameNum(), GameGetFrameNum() )` (`gun.lua:170-186`).
- Random spells: frame (+ deck size) seeds, e.g. `gun_actions.lua:2339`, `:3802`, `:5611`, `:8809`,
  `:8851`, `:8893`, `:8935`, `:8977`, `:9033`, `:9091`, `:10041`.
- Projectile scripts: entity ID (`.cache/noita-data/data/scripts/projectiles/random_explosion.lua:9`,
  `fizzle.lua:23`, `big_magic_shield_start.lua:11`, `colour_spell.lua:57`), position
  (`transmutation.lua:10`), frame (`teleport_cast.lua:8`).
- Spread: engine RNG, not reachable from stock Lua (NoitaPatcher exposes `SetProjectileSpreadRNG`).

### 6.4 What a faithful remote replay needs

1. Identical wand (cards, order, `uses_remaining`, mana, cooldown privates) **and** the gun VM's
   deck/discard/hand state, which lives inside the shooter's Lua VM between shots.
2. Identical `GameGetFrameNum()` at cast (frame counters differ per client, §3.4).
3. Identical entity IDs for ID-seeded scripts (impossible across processes).
4. Control of the engine spread RNG (NoitaPatcher).
5. Same shooter position, aim and terrain; collisions, physics and cells still diverge.

EW's implementation shows the cost: seed chains per projectile, special-casing transmutation /
random_explosion / fizzle / teleport_cast and eggs, `SetProjectileSpreadRNG` (`EW init.lua:219-338`).

Options for R1.2.7: (i) deterministic replay (NoitaPatcher, fragile); (ii) replicate projectile
entities (spawn + periodic state) for long-lived or physics projectiles; (iii) cosmetic replay with
damage decided by the authority of the target. The architecture must pick per projectile class.

---

## 7. Global world effects

### 7.1 Material conversion

- `ConvertMaterialEverywhere(from, to)`: converts everywhere, swaps the CellData table entry, marks
  `from` as "Transformed", "spread over multiple frames", each call appends to `WorldStateComponent`
  (`API:342`) — `changed_materials` "stored so these can be restored when loading a save" (`CD:2696`).
  The list is cumulative and order-dependent (A→B then B→C).
- `ConvertEverythingToGold(dyn, static)` (`API:341`), `ConvertMaterialOnAreaInstantly` (local,
  `API:343`), `EntityConvertToMaterial` (`API:340`).
- Vanilla callers (non-debug): fungal shift (`.cache/noita-data/data/scripts/magic/fungal_shift.lua:188`);
  Sopanen boss death `water→smoke` plus heart, super chest, portal and flags
  (`.cache/noita-data/data/entities/animals/boss_fish/death.lua:3-14`); sun collision converts ten
  static materials to fire (`.cache/noita-data/data/scripts/buildings/sun/sun_collision.lua:36-45`);
  fun-room secret (`.cache/noita-data/data/scripts/buildings/funroom_check.lua:13-14`); Sampo midas
  `ConvertEverythingToGold` (`.cache/noita-data/data/entities/animals/boss_centipede/ending/midas_convert_everything_to_gold.lua:1`,
  `midas.xml:22-27`).
- Hooks: (a) poll `changed_materials` length or wrap the callers; (b) clients replace the call with a
  host request; (c) host broadcasts ordered `(index, from_name, to_name)`; clients apply in order and
  skip indices already present. Joiners must apply the whole list **before generating or loading any
  area** (R1.1.5, R6.5).

### 7.2 Fungal shift

- Trigger: `TRIP` status level 3 → `effect_trip_03.xml` (`.cache/noita-data/data/scripts/status_effects/status_list.lua:277`,
  `.cache/noita-data/data/entities/misc/effect_trip_03.xml:28`) → `effect_trip_03.lua:24` →
  `fungal_shift(entity, x, y, false)`.
- Gates: cooldown `fungal_shift_last_frame` + 5 min of **frames** (`fungal_shift.lua:119-123`),
  `EVERYTHING_TO_GOLD` (`:125-128`), max 20 shifts via `fungal_shift_iteration` (`:130-133`).
- RNG: `seed2 = 42345 + iter + 1000*tries`; `SetRandomSeed(89346, seed2)` and
  `random_create(9123, seed2)` (`:140-145`) → deterministic sequence per seed.
- Player-dependent input: the tripping player's held potion/pouch main material (75%/50% chance,
  `:146-165`, `get_held_item_material` `:85-100`).
- Effects: `ConvertMaterialEverywhere` per source material (`:186-189`), globals and UI child
  (`:201-248`).
- Hooks: (a) wrap `fungal_shift` via `ModLuaFileAppend` on `fungal_shift.lua`/`effect_trip_03.lua`;
  (b) on clients send `{player, held_material}` to the host instead; (c) host runs the vanilla
  selection with the host's `iteration` and broadcasts the pairs. Cooldown must use host frames.

### 7.3 Sampo endings and NG+

`sampo_start_ending_sequence.lua` (run from the Sampo, `.cache/noita-data/data/entities/animals/boss_centipede/sampo.xml:43-49`)
reads `GameGetOrbCountThisRun()` and NG+ count (`:12-25`), may run `do_newgame_plus()` (`:48-83`),
otherwise sets `ending_game_completed`, calls `GameOnCompleted()` (`:86-90`), `GameDoEnding2()` for 33+
orbs (`:98-140`), or spawns midas entities and parents them to **the first `player_unit`**
(`:235-285`), setting `INFINITE_GOLD_HAPPENING` and persistent progress flags. Host-authored,
apply-to-all events (Q15).

### 7.4 Weather

`weather_update()` runs every frame from `OnWorldPreUpdate` (`init.lua:208-210`): snow if the UTC month
is Dec–Feb and `random_create(hour+day, hour+day+1)` passes (`init.lua:121-129`, `:184-188`); rain from
the world seed; sets `fog_target_extra`/`rain_target_extra` (`:166-171`) and emits **real material
particles** around the camera with `GameEmitRainParticles` (`:190-196`, `API:222`); snow applies
FREEZING modifiers (`:133-144`). Rain lasts `4*60*60` frames from run start (`:21`, `:174-176`).
Hooks: redefine `weather_update` via `ModLuaFileAppend("data/scripts/init.lua", …)`; host decides,
clients mirror the fog/cloud targets; emission per client is cosmetic but deposits cells (R1.1.4).

### 7.5 Day/night and wind

`WorldStateComponent.time`, `time_total`, `time_dt`, `day_count`, `wind`, `wind_speed`… (`CD:2644-2653`).
`DESIGN_DAY_CYCLE_SPEED="0.0015"` (`.cache/noita-data/data/magic_numbers.xml:14`); the saved
`time_total="0.492578"` equals 328.31 s × 0.0015, so a day lasts ≈ 667 s of play. Each client advances
its own clock; the host should broadcast `time`/`day_count` periodically.

### 7.6 Flags, globals and their scope

| Store | API | Scope / persistence | Notes |
|---|---|---|---|
| Run flags | `GameAddFlagRun/RemoveFlagRun/HasFlagRun` (`API:278-280`) | Run; `WorldStateComponent.flags` (`CD:2695`) in `world_state.xml` | Perks set them at **spawn** (`perk.lua:532`) and pickup (`:219`) |
| Persistent flags | `AddFlagPersistent/RemoveFlagPersistent/HasFlagPersistent` (`API:275-277`) | Profile; one file per flag, written immediately (§3.5) | Gate spell pools, modifiers, pillars, secrets (`init.lua:217-236`) |
| Globals | `GlobalsSetValue/GetValue` (`API:204-205`) | Run; `lua_globals` (`CD:2690`) | Perk cursor, temple state, fungal state, potion capacity; mods dump large data here |
| Session numbers | `SessionNumbers*` (`API:208-210`) | Run (encrypted file) | NG+ |
| Orbs | `GameGetOrbCountThisRun`, `GameGetOrbCountAllTime` (`API:100-105`) | Run: `orbs_found_thisrun` (`CD:2694`); profile: `persistent/orbs_new` | |
| Stats | `StatsGetValue`, `StatsGlobalGetValue` (`API:212-216`) | Session / profile | `death_count` feeds biome modifiers |
| Cut-through-world | `GameCutThroughWorldVertical` (`API:223`) | Run; `cuts_through_world` (`CD:2697`) | |

Hooks: no global interception exists — each Lua context has its own globals table. Detect by diffing
`WorldStateComponent` vectors/maps once per frame (`ComponentGetValue2` support for `VECTOR_STRING` /
`MAP_STRING_STRING` unverified [INFERENCE]) or by wrapping callers file by file. Run flags and globals
belong to the host state (R1.4.1); persistent flags stay per profile (Q22) but must not influence
shared generation (§1.9).

---

## 8. Requirement map

| Requirement | Findings |
|---|---|
| R1.1.1–R1.1.4 | 512² areas; untouched → local generation; touched → host data (§1.9, §2) |
| R1.1.5 | Ordered `changed_materials` log, fungal shift inputs, endings (§7.1–7.3) |
| R1.1.6 | Seed + NG+ + build/schema hash + mods; pin modifiers/weather/flag scenes (§1.8–1.9) |
| R1.2.1, R1.2.6 | Generation spawns host-authored; camera-bound and engine `just_load_an_entity` spawns (§1.10, §2.5) |
| R1.2.2, R1.2.4 | Wand/potion recipes, frame rebasing, stable IDs (§5) |
| R1.2.3 | Music machines come from `_pixel_scenes.xml` entity entries (§1.6, §1.10) |
| R1.2.5 | Engine stores area entities only in the local `entities_N.bin` (§2.5) |
| R1.2.7 | No stock fire callback; replay needs NoitaPatcher (§6) |
| R1.3.1–R1.3.3 | `player.xml` composition, polymorph, proxies without `player_unit` (§4) |
| R1.4.1 | Flag/global stores and scopes (§7.6) |
| R2.1, R2.2 | Orbs/hearts are generation spawns → host-authored (§1.10) |
| R3 | Keep-alive mechanisms, bot bases, inventory transfer (§4.4–4.6, §5) |
| R4 / D5 / Q10 / Q11 | Perks local per client; shop/chest seeds (§1.7, §1.10) |
| R6.2–R6.8 | Write timing, autosave, crash, game-over deletion, NG+ cleanup (§3) |
| R7.5 | Schema hash and NoitaPatcher are build-specific (§3.4) |

---

## 9. Experiments still needed (throwaway profile or separate slot only)

1. Does `EntityKill` run `script_death` / `ItemChestComponent` loot / `drop_money.lua`?
2. With `wait_for_kill_flag_on_death=1` on the player, at hp≤0: do `Message_Death`, `script_death`,
   `OnPlayerDied` fire before `kill_now`?
3. Do all damage types pass through `damage_about_to_be_received`?
4. `StreamingKeepAliveComponent`: keeps entity, keeps areas, or both?
5. `STREAMING_AUTOSAVE_PERIOD_SECONDS`, `STREAMING_CHUNK_TARGET` defaults via `MagicNumbersGetValue`.
6. Is the biome `init` called exactly once per biome cell per run?
7. Pre-writing a `world_X_Y.png_petri` for an unloaded area: does the engine load it?
8. `ComponentGetValue2` on `WorldStateComponent.flags` / `lua_globals` / `changed_materials`.

---

## 10. Implications for sync design

- **Terrain**: generate untouched areas locally from (seed, NG+ count); keep authoritative snapshots
  or deltas only for areas someone has simulated. Before world init, pin biome modifiers (ignore
  `death_count`, fixed `requires_flag` set), weather FREEZING, and flag-gated pixel scenes.
- **Unit**: use the engine's 512×512 area as the persistence and authority unit (it matches the
  save files, the biome cell and `DoesWorldExistAt` checks), with finer network deltas inside it.
- **Spawns**: wrap biome spawn functions in every biome script; on clients they never spawn shared
  entities; the host runs them once, assigns a run-unique ID and replicates the result (R1.2.6).
  Remove `just_load_an_entity` entries from `_pixel_scenes.xml` on clients and have the host spawn
  music machines and similar (R1.2.3, Q16).
- **Personal objects** (HM perks, reroll, possibly shop): let each client's vanilla code spawn them;
  tag them personal; never send them to the host or persist them there (D5). Note that perk
  pickup kills every `perk`-tagged entity (`perk.lua:328-337`), so remote copies must not carry that
  tag.
- **No engine streaming persistence on clients**: the engine reloads local `entities_N.bin` and
  `area_N.bin` without asking. Run the co-op game in a dedicated save slot or game mode and start each
  client session from a clean `world/`, or detect and kill stale reloads (EW's 36 cleanups). Never
  touch `save00` of solo runs (Q22).
- **Camera-bound entities**: remove `CameraBoundComponent` from replicated entities and from
  host-spawned enemies; otherwise each client freezes/unfreezes them by its own camera and loses state.
- **Identity**: entity IDs and `EntityGetFilename` are not stable; every synced entity carries a
  run-unique ID in a `VariableStorageComponent`, which survives the engine's own serialisation.
- **Serialisation**: without NoitaPatcher, rebuild wands/potions/cards field by field from
  non-procedural bases; transmit material names; rebase every absolute frame field when an entity
  changes process (R1.2.2, R3.7).
- **Mod-added components** on the player persist in `player.xml`; add them idempotently (check before
  adding, or a Globals guard as in Daily Practice).
- **Player death**: suppress vanilla game over (`OnPlayerDied` override), hold the player at 0 HP
  (`wait_for_kill_flag_on_death` + damage clamp), convert to a host-authored "downed/bot" event
  with ID; call `GameTriggerGameOver` only when the host ends the run (R3.1, Q8). Noita deletes the
  local run save on game over; the host's state must not depend on it.
- **Revive bot**: build it from a player clone with input disabled or from an NPC that holds the
  dead player's items via `GamePickUpInventoryItem`; no `player_unit` tag, no camera/audio listener,
  no `ItemChestComponent`, no camera-bound kill, stable ID, `StreamingKeepAlive` if it must persist
  outside its area (R3.2–R3.4).
- **Proxies of remote players** must not carry `player_unit` (vanilla targets `players[1]`, triggers,
  endings) and need their own representation of polymorph state (R1.3.1).
- **Projectiles**: no stock fire callback; choose per class between entity replication and cosmetic
  replay with authoritative damage. Deterministic replay requires NoitaPatcher (Q1) and identical
  deck state, frame numbers and entity IDs, which a networked mod cannot guarantee (R1.2.7).
- **Global effects** (`ConvertMaterialEverywhere`, fungal shift, endings, NG+): host-only decision,
  ordered event log with indices, idempotent apply; joiners replay the log before loading terrain
  (R1.1.5, R1.4.2).
- **Run flags and globals** are host state; diff `WorldStateComponent` or wrap callers. Persistent
  flags stay per profile and must not influence shared generation or loot decisions.
- **Weather and time**: host decides weather and broadcasts fog/cloud targets and day time; clients
  disable their own `weather_update`; rain/snow deposits are local noise to tolerate or disable
  (R1.1.4, Q19).
- **Saves and crashes**: Noita's save is mixed-time after a crash (areas at unload, the rest at
  quit/autosave). Host state needs its own atomic snapshots plus journal (R6.3). Client saves are a
  disposable cache (R6.7). Consider `AutosaveDisable()` on clients.
- **Starting loadouts**: every player gets the same starting wands from the same spawn position;
  decide whether that is wanted (EW makes them per-peer).
- **Version gate**: compare `_version_hash.txt`, the entity schema hash and the mod list at
  connection time (R1.1.6, R7.5).
