# G4/G7 static spike: player serialization, death without game over, bot control

Date: 2026-11-19. ROADMAP stage 1, gates **G4** (player/wand/flask serialization) and **G7** (death without game over, bot
control), static part only. Nothing was launched; the Noita install, saves, Proton prefix and the EW checkouts were only read.
Anything not backed by a doc line, a vanilla file, the engine's own schema or a real engine-written save is marked
`[INFERENCE]`. This is not in-game verification; the last section lists the live experiments.

Source tags. `V/` = `.cache/noita-data/data/` (unpacked `data.wak`). `API:N`, `CD:N` = line N of
`tools_modding/lua_api_documentation.txt`, `component_documentation.txt`. `SCH` = `<Noita>/data/schemas/c8ecfb341d22516067569b04563bff9c.xml`,
the engine's entity schema for the pinned build (hash from the `entities_N.bin` header, `vanilla_world_and_saves.md` §3.4).
`SAVE-A`/`SAVE-B` = two real engine-written `player.xml` files (+ `world_state.xml` of B) read from the `noita-switcher`
tarballs `c9cafc86`/`3cebf0b7` into `.cache/spikes/g4g7/` (gitignored scratch). `EXE "…"` = string or RTTI name in `noita.exe`
(presence only, no line numbers). `EW file:line` = Entangled Worlds fork, knowledge only (D7), nothing copied.
`[needs NoitaPatcher]` = a candidate requiring a native API; independent provenance, signatures and limits are now available in
[the G1 report](g1_noitapatcher.md). Initial symbol discovery used EW references only; it is not runtime or license evidence.

## Verdict

- **G4, field-wise snapshot: partially reachable, completeness unknown.** Two saves and selected schema entries omit the
  documented Privates examined below. That bounds those artifacts; it does not prove that all gameplay state is exposed,
  writable from Lua, absent from native/custom serialization, or reconstructible from schema fields. Status vectors,
  LensValue semantics and child/state reconstruction still have explicit gaps. Native `SerializeEntity`/`DeserializeEntity`
  is a candidate, not a demonstrated fidelity guarantee. Confidence: medium in the inventory, low in complete restoration.
- **G7: stock Lua offers a game-over interception candidate**, not a proved final-death veto. The deciding unknown is
  behaviour at hp <= 0 with `wait_for_kill_flag_on_death=1`, including callback/stat ordering and RESPAWN/SAVING_GRACE.
  The only found vanilla Lua game-over call site is `V/scripts/init.lua:212–215`, overridable by `ModLuaFileAppend`;
  this does not exclude native or polymorph bypasses. Confidence: medium for the layered plan, low for polymorph.
- **Bot control: a Controls-driven candidate exists** in vanilla scripts and EW's source. This is source evidence only,
  not proof for our separate bot with a real inventory wand. Equipping, mana/uses/cooldowns and authority transfer remain
  unverified. G4 and G7 stay open.

## Sources checked

- Project: `AGENTS.md`, `DECISIONS.md` (D7-D9, D17-D19, D22, D27-D31, D34), `ACCEPTANCE.md` (G4, G7, A16-A18, A22), `EVIDENCE.md`,
  `research/vanilla_world_and_saves.md` §3-5, `REQUIREMENTS.md` R1.3, R3.
- Docs: `lua_api_documentation.txt` (whole), `component_documentation.txt` (Damage, Controls, Ability, Item, Inventory2,
  GameEffect, Lua, Wallet, StatusEffectData, Ingestion, MaterialInventory, Gun, AnimalAI, WorldState, PlatformShooterPlayer, …).
- Vanilla: `scripts/init.lua`, `scripts/perks/{perk,perk_list,perk_utilities,give_all_perks,perk_gamble_spawn,perk_pickup}.lua`,
  `scripts/gun/gun.lua` and `gun/procedural/{gun_action_utils,gun_procedural}.lua`, `scripts/lib/utilities.lua`,
  `scripts/status_effects/status_list.lua`, `scripts/essences/essence_pickup.lua`, `scripts/items/heart*.lua`,
  `entities/{player,player_base,base_wand*,base_item,base_humanoid,base_helpless_animal}.xml`, `entities/misc/effect_*.xml`,
  `entities/animals/boss_centipede/*`, `genome_relations.csv`; Nolla's own mods `starting_loadouts`, `daily_practice` (install `mods/`).
- Engine artifacts: `SCH` (165 component types), two real `player.xml`, `world_state.xml`, `stats/sessions/*_stats.xml`
  (field names only), `noita.exe` strings/RTTI (message types, listeners, enum names), `data/translations/common.csv`.
- EW (knowledge only): `files/core/{early_init,util,player_fns,perk_fns,net_handling}.lua`, `files/system/local_health/*`,
  `files/system/notplayer_ai/notplayer_ai.lua`; bundled `noitapatcher.dll` string table (symbol names only).
- Counting helpers in `.cache/spikes/g4g7/*.py` (scratch, not deliverables): doc-vs-save privates check, SCH type census, perk census.

## Findings

### F1. What a player is, and where each piece of state lives

| Piece | Where / how built | Engine save | Stock Lua access |
|---|---|---|---|
| Root entity | name `DEBUG_NAME:player`, tags `mortal,human,hittable,peasant,prey,player_unit,teleportable`, `_Transform` (x, y, rotation, scale.x = facing) (`V/entities/player.xml:1-3`; SAVE-A root `_version="1" serialize="0"`) | yes | `EntityGetName/GetTags/GetTransform`, `EntitySetTransform` (API:18-20,28,30) |
| Components | 38 types on the root (`V/entities/player_base.xml:5-623`): Controls, DamageModel, Inventory2, InventoryGui, ItemPickUpper, Wallet, StatusEffectData, Ingestion, MaterialInventory (`ingestion` = stomach), DrugEffect, Gun, Kick, GameStats, GameLog, CharacterData/Platforming, GenomeData, PlatformShooterPlayer, VariableStorage x4 (`ocarina_song`, `kantele_song`, `*_pos`), Lua x3 (`amulet.lua` and `orb_map_init.lua` self-remove, `player_biome_check.lua` every 80 frames), cosmetics | yes, Members only (F2) | per component, F4 |
| Children | `arm_r`, `cape` (verlet), `no_heal_in_meat_biome` (GameEffect `NO_HEAL`, BiomeTracker, Lua `script_biome_entered`, `player_base.xml:515-518`), `inventory_quick`, `inventory_full` (`player.xml:14-19`) | yes | `EntityGetAllChildren` (API:22) |
| Wands | children of `inventory_quick`, tags `teleportable_NOT,wand,item`: AbilityComponent (mana, `gun_config`, `gunaction_config`, timers), ItemComponent, Hotspot `shoot_pos`, Sprite `item`, Light, ManaReloader. Components carry `enabled_in_world/hand/inventory` tags and `_enabled` is toggled by the inventory state (`V/entities/base_item.xml`, `base_wand_pickup.xml`; SAVE-A wand in `inventory_quick`: `AbilityComponent _enabled="0"`, `ItemComponent _enabled="0"` with tag `enabled_in_world`; which wand was held is not recorded in the XML except via `mSavedActiveItemIndex`) | yes | `ComponentGetValue2`, `ComponentObjectGetValue2` (API:59-61) |
| Spells | children of the wand, tag `card_action`; deck order = child order + `ItemComponent.inventory_slot.x`; `ItemActionComponent.action_id`; `ItemComponent.uses_remaining / permanently_attached (Always Cast) / is_frozen / is_identified`. Loose spells = children of `inventory_full` | yes | `EntityGetAllChildren(wand)`; no stock "GetWandSpells". `EntityGetWandCapacity` (API:146) excludes Always Cast |
| Flasks, pouches | tags `…item_physics,item_pickup,potion`; `MaterialInventoryComponent.count_per_material_type`, PotionComponent, Physics* disabled in the pocket (`V/entities/items/pickup/potion.xml`; SAVE-A `<Material material="water" count="1000">`) | yes | table, Lua index = material id + 1 (EW `notplayer_ai.lua:141-147`); `Add/RemoveMaterialInventoryMaterial` (API:80-81) |
| Perks | five stores (F5) | children/components yes; Globals, flags, WorldState fields in `world_state.xml` | F5 |
| Status effects | GameEffect child entities (`frames` countdown); `StatusEffectDataComponent` vectors (42 slots in SAVE-A, positional in `V/scripts/status_effects/status_list.lua` order: "add new effects to the end", `:1-3`); `DamageModelComponent.is_on_fire`; stomach; `IngestionComponent`; `DrugEffectComponent`; `SpriteStainsComponent` | yes | vectors: `ComponentGetVector` (API:64-66), no documented writer |
| Polymorph | GameEffect `POLYMORPH*` with `polymorph_target`, `frames`, `mSerializedData` ("polymorph stores the serialized entity here", CD:1001-1002); the current body is a different entity with tag `polymorphed_player` and without `player_unit` (F9) | the blob is a Member | read only; no documented way to produce a valid blob |
| Wallet | `WalletComponent{money, money_spent, mMoneyPrevFrame, mHasReachedInf}` (CD:2630-2635) | yes | Get/SetValue2 |
| HP | `DamageModelComponent{hp, max_hp, max_hp_cap, max_hp_old}` raw units, UI = x25 (`V/scripts/items/heart.lua:11-23`); `no_heal_max_hp_cap` on the `NO_HEAL` effect | yes | Get/SetValue2 |
| Orbs | not on the player: `WorldStateComponent.orbs_found_thisrun`, `GameGetOrbCountThisRun` (API:101) | `world_state.xml` | run state (R2.1) |
| "Known" spells, progress | profile persistent flags (`action_*`, `perk_picked_*`, `essence_*`, `progress_*`: `AddFlagPersistent`/`HasFlagPersistent`, API:275-277). Per entity only `ItemComponent.is_identified` | profile files | outside the snapshot (D34) |
| Run state written by the player's own scripts | `player_biome_check.lua` writes Globals `visited_biomes`, `HOLY_MOUNTAIN_DEPTH/VISITS` | `world_state.xml` `lua_globals` | process-local; merging is a run-state/G9 matter |

### F2. The fidelity bar: what the engine itself persists

- In SAVE-A/SAVE-B (57 component types: player, wands, cards, flask, effects), none of the 281 documented Private fields
  checked appears by name. The three `m*` fields found on DamageModel are listed as Members (`CD:717–813`).
  The 11 checked schema components likewise omit their documented Privates: Ability has 46 members and 0/3 privates;
  Controls 6 variables and no 80 privates; Inventory2 4 and no 12 privates; Lua 39 and no 4 privates; DamageModel 57 and
  no 34 privates; Gun has 0 variables. These observations do **not** establish the serialization policy of every component.
- `[INFERENCE]` Gun VM deck/hand/discard state, active-item pointers and several private accumulators may be reset or
  reconstructed on ordinary Save&Quit. Absence of their names does not exclude reconstruction or encoding elsewhere.
  Reconnect from a live server checkpoint is also not automatically equivalent to a normal Save&Quit.
- Engine-written `player.xml` before/after restore is one useful comparison artifact, not a complete G4 oracle. Compare
  world-state/perk stores and actual subsequent casts, flask contents, statuses and item identities in a restarted process.
  Matching XML alone cannot establish equivalent gameplay, native state or loss-free restoration.
- The pinned schema (165 component types; other schema files are migration history) can drive an inventory of candidate
  fields. Lua read/write coverage and field semantics must be measured before generating an authoritative codec.

### F3. Reachability from stock Lua (by field type)

The 57 component types in the real player trees have 796 schema-persisted fields: 707 scalars/strings (89%), 25 object members, 23 vec2,
21 LensValue, 9 enum, 8 vectors, 3 other (cape cloth arrays, `SpriteStains`).

- Scalars/strings/int64: `ComponentGetValue2/SetValue2` (API:59-60). vec2: two values (vanilla reads `mAimingVector` this way,
  `V/scripts/perks/projectile_eater_sector.lua:13`). Enums: strings (EW compares `ComponentGetValue2(c,"effect") == "CHARM"`).
- Objects (`gun_config`, `gunaction_config`, `damage_multipliers`, `drug_fx_*`): `ComponentObjectGetValue2/SetValue2`, `ComponentObjectGetMembers`
  (API:61-62,70). `ComponentGetMembers` (API:69) returns strings; whether it includes Privates is unknown (E4-1).
- Vectors: `ComponentGetVector` for int/float/string (API:64-66). `StatusEffectDataComponent` is readable, not writable. `count_per_material_type`
  works as a table (EW usage above).
- LensValue (CharacterPlatforming `run_velocity`/`velocity_*`/`fly_*`, CharacterData, Kick, Genome `herd_id`): saved as plain scalars
  (SAVE-A `run_velocity="154"`, `herd_id="player"`); whether `ComponentGetValue2` returns the base or the effective value is `[unknown]` (E4-7).
- Not reachable: entity <-> XML/blob (`EntitySave` is dev-only, API:7), `LuaComponent.mPersistentValues`, gun VM state, message queue, safe
  active-item selection, writing the status vectors.
- Vanilla helpers: `AddGunAction`, `AddGunActionPermanent`, `find_the_wand_held`, `find_all_wands_held`
  (`V/scripts/gun/procedural/gun_action_utils.lua:1-36,39-104`), `get_players`, `get_variable_storage_component`
  (`V/scripts/lib/utilities.lua:255-271,431-433`), `GameRegenItemActionsInPlayer` (API:122).

### F4. Proposed snapshot schema

Skeleton (names, never ids: materials, herds, effects, spells; no entity ids; frame fields tagged for rebasing):

```
PlayerSnapshot v1 { meta{game_build, schema_hash, frame_src, source: alive|downed|bot}, body{name, tags[], x,y,rot,sx,sy},
  components{ <type>: {field: value} },  stomach{material: amount},  effects[ {tags, components{GameEffectComponent{..}}, children[]} ],
  inventory_quick[ Item ], inventory_full[ Card ],
  perks{ counts{ID:n}, run_flags[], globals{k:v}, world_state{perk_*:v}, children[ {name, tags, components} ] }, polymorph{effect, frames_left} | null }
Item { kind: wand|potion|other, tags[], slot{x,y}, components{AbilityComponent, ItemComponent, ...}, cards[Card], material{name: amount} }
Card { action_id, uses_remaining, permanently_attached, is_frozen, is_identified, slot_x }
```

Class: **L** lossless, **R** absolute frame (rebase by `dst_frame - src_frame`, keep sentinels `-2147483648`, `-10000`, `-120`, `-1`, `0`),
**A** approximate, **X** not recoverable from Lua, **N** never snapshot.
These classes describe intended fidelity only, not verified round trips. An `L` entry is still conditional on actual
read/write semantics and all dependent state. Omission or approximation of gameplay state is not accepted by this report.


| Component / state | Fields | Read -> write | Class |
|---|---|---|---|
| Body | name, vanilla tags, x, y, rotation, scale.x/y | `EntityGetTransform` -> `EntityApplyTransform` (API:19) | L |
| DamageModelComponent | hp, max_hp, max_hp_cap, max_hp_old, invincibility_frames, is_on_fire, air_in_lungs(_max), `damage_multipliers`{15 types}, materials_that_damage/how_much_damage, blood_*, ragdoll_*, `mLastElectricityResistanceFrame`, `mLastFrameReportedBlock`, `mLastMaxHpChangeFrame` | Get/ObjectGet -> Set/ObjectSet | L; 3 frame members R; the 34 privates X |
| `no_heal_in_meat_biome` child | `GameEffectComponent{_enabled, frames, no_heal_max_hp_cap}`: write the cap **after** restoring hp or the effect clamps hp back (EW writes it for the same reason, `local_health.lua:626-640`) | Get/Set | L |
| WalletComponent | money, money_spent, mHasReachedInf, mMoneyPrevFrame | Get/Set | L (server-authoritative, D4) |
| Inventory2Component | quick/full slot counts, `mSavedActiveItemIndex` | Get/Set | L; active item re-selection `[unknown]` |
| Wand: AbilityComponent | ui_name, sprite_file, gun_level, mana, mana_max, mana_charge_speed, `gun_config{actions_per_round, deck_capacity, reload_time, shuffle_deck_when_empty}`, `gunaction_config{fire_rate_wait, spread_degrees, speed_multiplier, ...}`, stat_times_player_has_shot/edited, current_slot_durability, never_reload, `mNextFrameUsable`, `mCastDelayStartFrame`, `mReloadNextFrameUsable`, `mReloadFramesLeft`, `mChargeCount`, `mIsInitialized` | Get/ObjectGet -> Set/ObjectSet | candidate L; frame members R; private ammo/charge/recoil state X, gameplay consequences unverified |
| Wand: ItemComponent and aux | item_name, always_use_item_name_in_ui, inventory_slot, is_frozen, ui_sprite, preferred_inventory; Sprite tag `item` (image_file, offsets); Hotspot `shoot_pos` offset; Light colour (rare wands); `next_frame_pickable`, `npc_next_frame_pickable`, `mFramePickedUp` | same | L; 3 frames R |
| Spells | per child: action_id, uses_remaining, permanently_attached, is_frozen, is_identified, inventory_slot.x, child order | rebuild with `AddGunAction`/`AddGunActionPermanent` (they call `CreateItemActionEntity`, API:217; Permanent also grows `deck_capacity`), then patch ItemComponent | L |
| Flask, pouch | `count_per_material_type` by **name** (`CellFactory_GetName(id-1)`, API:106), max_capacity, PotionComponent flags | table -> `RemoveMaterialInventoryMaterial("")` + `AddMaterialInventoryMaterial(name, int)` | A (integer API) |
| Stomach, digestion | stomach `count_per_material_type`, IngestionComponent{ingestion_size, ingestion_capacity, m_ingestion_cooldown_frames}, DrugEffectComponent targets | same | L; cooldown frame R |
| StatusEffectDataComponent | stain_effects, stain_effect_cooldowns, effects_previous, ingestion_effects, ingestion_effect_causes(_many) (index = position in `status_list.lua`; store by id) | `ComponentGetVector` -> approximate via `EntityIngestMaterial` (API:140), `EntityAddRandomStains` (API:143); remove via `EntityRemoveIngestionStatusEffect` (API:141) | A/X |
| Effect children | `GameEffectComponent{effect, frames, custom_effect_id, exclusivity_group, causing_status_effect, caused_by_*, mCounter, mIsSpent, teleportation_*}`, entity tag `perk_entity`, component tag `perk_component`, `LifetimeComponent` frames | `GetGameEffectLoadTo(e, name, true)` (API:153) then Set | L (`frames` A by transfer latency; `mIsSpent` matters for RESPAWN) |
| Perk markers and counters | F5 | Globals/flags API (API:204-205,275-280) | L |
| Genome, movement | herd **name** (`HerdIdToString`, API:174), CharacterPlatforming/CharacterData/Kick LensValue fields (perks edit them, `perk_utilities.lua:37-64`) | Get/Set | A (LensValue semantics `[unknown]`) |
| VariableStorageComponent | name, value_int/float/string/bool for `ocarina_song`, `kantele_song`, `*_pos` (D15 instruments) | Get/Set | L |
| Polymorph | effect id, frames left, target; `mSerializedData` opaque | read | X (F6) |
| Exclusions / separate stores | purely cosmetic state and process-local IDs are not durable identities; GameStats/GameLog need an explicit policy | restore our mod-owned components idempotently; retain perk-added gameplay components; remap referenced entities through stable IDs | no blanket exclusion of mod components or gameplay references |

### F5. Perks: five stores, no re-application on load

Of 157 entries in `V/scripts/perks/perk_list.lua`, 63 sit inside `--[[ ]]--` blocks; 94 are live: 22 effect-only (`game_effect`), 15 effect + `func`,
57 `func`-only. `perk_pickup(entity_item, who, item_name, do_fx, kill_others, no_perk_entity)` (`V/scripts/perks/perk.lua:169-364`) does, in order:

1. `Globals PERK_PICKED_<ID>_PICKUP_COUNT += 1` (`:201-207`; flag name from `perk_list.lua:3337-3339`); persistent flag `perk_picked_<id>` (lower-case) and run
   flag `new_perk_picked_<id>` unless run flag `no_progress_flags_perk` (`:210-218`); run flag `PERK_PICKED_<ID>` (`:219`).
2. `GetGameEffectLoadTo(who, game_effect, true)` with `frames=-1`, component tag `perk_component`, entity tag `perk_entity` (`:224-246`); same for `game_effect2`.
3. `particle_effect` child once (`:249-257`); `remove_other_perks` bumps their counters and flags (`:260-271`).
4. `func(entity_item, who, item_name, pickup_count)` once per pickup (`:273-275`), then a `UIIconComponent` child tagged `perk_entity` (`:281-293`).

What `func` touches (parsed from the file; classes overlap): (a) numeric component edits (BREATH_UNDERWATER, FASTER_LEVITATION, STRONG_KICK, TELEKINESIS,
VAMPIRISM, GLASS_CANNON, BLEED_*, ATTACK_FOOT, LEGGY_FEET); (b) LuaComponent/ShotEffectComponent added to the player (radars, REVENGE_*, ORBIT, projectile modifiers);
(c) child entities tagged `perk_entity` (MAP, FREEZE_FIELD, FIRE_GAS, DISSOLVE_POWDERS, TELEPORTITIS_DODGE, SHIELD, ghosts, homunculus, lukki limbs, rats);
(d) WorldState fields via `GameGetWorldStateEntity` (GOLD_IS_FOREVER `perk_gold_is_forever`, TRICK_BLOOD_MONEY `perk_trick_kills_blood_money`, NO_WAND_EDITING
`perk_hp_drop_chance`, UNLIMITED_SPELLS `perk_infinite_spells`, rats `perk_rats_player_friendly`); (e) Globals/run flags: `HEARTS_MORE_EXTRA_HP_MULTIPLIER`,
`PERK_SHIELD_COUNT`, `PERK_ATTACK_FOOT...`, `PERK_ATTRACT_ITEMS_RANGE`, `PERK_NO_MORE_SHUFFLE_WANDS`, `PLAYER_{RATTINESS,FUNGAL,GHOSTNESS,LUKKINESS,HALO}_LEVEL`
(`perk_utilities.lua:37-200`; reset list `perk.lua:734-738`), run flags `exploding_gold`, `ATTACK_FOOT_CLIMBER`, `player_status_*`; (f) **one-off** (`one_off_effect`): EXTRA_HP,
ALWAYS_CAST, EXTRA_MANA, FASTER_WANDS, EXTRA_SLOTS, RESPAWN (also `do_not_remove`): baked into hp and into the wands in reach, not replayable
(`create_all_player_perks` skips them, `perk.lua:771`; `IMPL_remove_all_perks` keeps `do_not_remove`, `:676`; EW's ignore lists agree, `core/perk_fns.lua:6-33`).

- Per-perk entity counters: `perk_pickup_count` VariableStorage on the perk child (`perk_utilities.lua:2-17`, `utilities.lua:273-296`). Essences: Globals
  `ESSENCE_<ID>_PICKUP_COUNT`, run flag `essence_<id>`, children tagged `essence_effect`, multiplicative `damage_multipliers` edits (`V/scripts/essences/essence_pickup.lua`).
- Multiplicative stacking (`add_halo_level` x0.9 on fire/holy per level, `perk_utilities.lua:147-200`; essences x1.3): **copy `damage_multipliers`, never recompute**.
- Programmatic grant exists: `perk_pickup(0, player, perk_id, false, false, true)` (`perk.lua:172-186`; `give_all_perks.lua:18`; Nolla's `starting_loadouts/init.lua:115-123`).
  It runs `func` and increments counters, so pre-set counters must be reset first.
- Child entities lose their source file after save/load (save root carries no `filename`; `EntityGetFilename` is wrong after streaming, API:40) `[INFERENCE]`:
  identify perk/effect children by tags + `GameEffectComponent.effect` + scripts, or record them at creation by wrapping `perk_pickup`.

### F6. Known lossy / unrecoverable state

| State | Why | Class | Mitigation |
|---|---|---|---|
| Wand cast state (deck/hand/discard cursor, `state_cards_drawn`, mid-reload) | gun.lua globals in the shooter's `GunComponent` VM (`CD:1090–1093`; `gun.lua:12–40,454–484,547–573`); not represented by the inspected named schema fields; EW resets by re-equipping (`local_health.lua:292–307`) | X | measure reset/reconstruction against native save and live checkpoints; resetting to first card is a candidate loss, not accepted behaviour |
| Absolute frame timers | `GameGetFrameNum()` is per process | R | shift; keep sentinels |
| Mana, uses | members | L | `uses_remaining` consumed through Lua (`gun.lua:394-409`, API:394) |
| Flask contents | integer add API; reactions in flight not captured | A | measure counts and rounding; no arbitrary loss tolerance accepted (E4-6) |
| DamageModel private timers and accumulators | no safe complete Lua restoration established | X | measure resulting damage/status behaviour; no silent acceptance of lost state |
| Effect remaining time | `frames` is a member | A | subtract transfer latency |
| Stains / ingestion effects | vector writes not documented | A/X | ingestion/stain APIs are lossy candidates, not an equivalent restore path; quantify before accepting |
| LuaComponent state (`mPersistentValues`, `mNextExecutionTime`, coroutines) | privates | X | perk scripts restart their timers |
| Entity-id references | ids differ per process | remap | preserve semantic targets through stable IDs; clear only references proved invalid/ephemeral, never all references blindly |
| Polymorph original | only inside `mSerializedData` | X | see F9, E4-11 |
| Active held item | `mActiveItem` private | A | `mSavedActiveItemIndex` at creation; later `SetActiveHeldEntity` `[needs NoitaPatcher]` or Controls pulses |
| Source filename of child entities | not persisted | N | content-based identification |
| LensValue effective vs base | unknown semantics | A | E4-7 |

### F7. How vanilla restores the player, and how we restore the same way

Vanilla: (1) engine writes `player.xml` + `world_state.xml` at Save&Quit and by autosave (`vanilla_world_and_saves.md` §3.5); (2) on continue it reads the XML
entity and calls `OnPlayerSpawned(player_entity)`. That callback also runs on continue: Nolla's mods guard their one-time edits with a run flag
(`mods/starting_loadouts/init.lua:4-10`) or a Global (`mods/daily_practice/init.lua:222-229`), and EW's components accumulate across loads
(`vanilla_world_and_saves.md:474-480`); this is inference from those guards, not a direct observation; (3) perks are **not** re-applied: all consequences are already in
the XML (children, GameEffect components, edited fields) and in `world_state.xml` (Globals, flags, WorldState fields); `perk_pickup` has only pickup-time callers
(`perk_pickup.lua:21`, `give_all_perks.lua:18`, `perk_gamble_spawn.lua:16`, streaming events, `starting_loadouts`); (4) a new run builds `player.xml` with random starting items
(`player.xml:14-18`). Nolla's `starting_loadouts` is the stock recipe for editing a spawned player from Lua: kill defaults with `GameKillInventoryItem` (`:74-83`), `EntityLoad` + `EntityAddChild(inventory_quick)`
(`:85-113`), then `perk_spawn` + `perk_pickup` (`:115-123`).

Restore options, ranked:

| # | Option | Needs | Notes |
|---|---|---|---|
| R1 | Whole-entity `SerializeEntity`/`DeserializeEntity` for player/items/effects plus host Globals/flags/WorldState | independent NoitaPatcher candidate (G1) | first fidelity experiment; opaque format and preserved state unknown, see G1; never copy a client's world-state entity over host state |
| R2 | Field-wise rebuild on a fresh engine-created player; non-procedural wand bases (`wand_level_01.xml:51–57` rerolls; `gun_procedural.lua:912–966` lists fields), empty flask bases (`potion.lua:107–158` randomizes), explicit effects/children and server HP/wallet | stock Lua | incomplete until vector/LensValue/child restoration is demonstrated. Restore baked perk consequences without replaying one-off rewards or multiplying fields again. XML comparison is necessary, not sufficient |
| R3 | Write captured engine-dialect XML (`_version`, `_Transform`, schema members) and load it with `EntityLoad` | unsafe `io` and an isolated test mod directory | `[INFERENCE]` runtime parser, caching and state fidelity unknown (E4-12); not ranked as highest fidelity without measurement |

### F8. Death path map

| # | Step | Source | Lua-hookable? | Notes |
|---|---|---|---|---|
| 1 | Damage enters (projectile, melee, explosion, material, fire, drowning, `EntityInflictDamage`) | native; Lua producer `EntityInflictDamage` (API:139) | produce: yes; veto: via 2 | 21 `DAMAGE_TYPES` in EXE: MELEE, PROJECTILE, EXPLOSION, BITE, FIRE, MATERIAL, FALL, ELECTRICITY, DROWNING, PHYSICS_BODY_DAMAGED, DRILL, SLICE, ICE, HEALING, PHYSICS_HIT, RADIOACTIVE, POISON, MATERIAL_WITH_FLASH, OVEREATING, CURSE, HOLY |
| 2 | `Message_DamageAboutToBeReceived` -> `LuaComponent.script_damage_about_to_be_received(damage,x,y,who,crit)` returns new damage | CD:1532; vanilla `V/scripts/perks/ingest_projectiles.lua:3-20` returns `damage*0.0` | **yes** (modify) | EXE RTTI: `GameEffectComponent` also listens to this message and to `...ReceivedPost` and `...AboutToBeApplied`: protections, SAVING_GRACE and probably RESPAWN likely run in those phases, partly after the Lua hook `[INFERENCE]`. Whether material/drowning damage reaches it: EW's `immortal.lua:1-11` handles every type, suggesting yes `[INFERENCE]` (`GetDamageDetails` is not stock `[needs NoitaPatcher]`) |
| 3 | hp reduced (`DamageModelComponent.hp`, raw units) | CD:717-813 | read/write: yes | |
| 4 | `Message_DamageReceived` -> `script_damage_received(damage, message, who, is_fatal, projectile)` | CD:1531 | **yes**, observation only | `is_fatal` is the earliest stock "this kills" signal; `message` is the damage description (becomes `killed_by`) |
| 5 | hp <= 0: kill at once, or wait | `wait_for_kill_flag_on_death`, `kill_now` (CD:759-760); Kolmi `boss_centipede.xml:136`, polls `hp <= 0` and runs a ~120-frame sequence before `kill_now=1` (`boss_centipede_update.lua:712-754`); also `boss_limbs_update.lua:306`, `vacuum_powder.lua:8` | flags writable: yes; hp <= 0 only by polling, no callback | with wait=1 the entity stays alive at hp <= 0 (vanilla proof). EW sets it on the local player at spawn and on remote proxies (`local_health.lua:529,709,725`) and reads `hp <= 0` in its world update (`:625-`) |
| 6 | Second-life effects: SAVING_GRACE ("if you would die and have more than 1 HP, HP is set to 1", `perk_list.lua:463-475`), RESPAWN ("Upon death you respawn with 100 health", `:753-768`, log `$log_gamefx_respawn`, `GameEffectComponent.mIsSpent` CD:1019) | `data/translations/common.csv:179-180,939-940,953-954` | no (add/remove the effect, read `mIsSpent`) | position, timing relative to step 5 and `Message_Death`: unknown (E7-3) |
| 7 | `Message_Death` broadcast | EXE RTTI listeners: ItemChest, ItemPickUpper, ExplodeOnDamage, MaterialInventory, GameLog, Inventory2, PhysicsBody(2), Lua, GameStats | `LuaComponent.script_death(damage_type_bit_field, message, who, drop_items)` (CD:1540): yes, observe only | the vanilla player has no `script_*` damage/death hooks (`player_base.xml:495-513` holds only three `script_source_file` Lua components). Data flags: `ItemPickUpperComponent.drop_items_on_death=0` (`:334-336`), `MaterialInventory on_death_spill=0` (`:388-393`), `GameLogComponent report_death=1` (`:348-351`), ragdoll fields (`:150-153`) |
| 8 | Entity destroyed: EXE "HandleEvent - Player Entity Destroyed" -> `Message_PlayerDied{Player ID, Player entity}`; listeners WorldStateComponent, GameStatsComponent | EXE RTTI | **no** | writes session stats (`stats/sessions/*_stats.xml`: `dead`, `death_pos.x/y`, `killed_by`, `killed_by_extra`; `StatsGetValue`, API:212); effect on `StatsGlobalGetValue("death_count")` (feeds biome modifiers, `vanilla_world_and_saves.md` §1.8) unobserved |
| 9 | Engine calls `OnPlayerDied(player_entity)` | `V/scripts/init.lua:212-215`: `GameDestroyInventoryItems(player_entity); GameTriggerGameOver()` | **yes**: `ModLuaFileAppend("data/scripts/init.lua", f)` (API:366; EW `core/early_init.lua:2`, `resource/append/no_default_death_handling.lua:1-3`) | a mod's own `OnPlayerDied` does not remove vanilla's `[INFERENCE]` |
| 10 | `GameTriggerGameOver()` (API:130) | only vanilla call `init.lua:214`; EW calls it itself after killing the player with `hp = 2^-38` + 1e6 `DAMAGE_CURSE` damage and wait=false (`local_health.lua:497-519`) | yes: do not call | native afterwards: game-over GUI (`event_cues/game_over/create`, `$menugameover_*`), flags `ending_no_game_over_menu` (EXE; `V/scripts/_examples/credits.lua:115`) and `ending_game_completed` (`sampo_start_ending_sequence.lua:79`, changes the screen), "Clearing bones", "Deleting save " (EXE), `DEBUG_NO_SAVEGAME_CLEAR_ON_GAME_OVER` (EXE). Order of GUI vs deletion not observed |
| 11 | Polymorph death | EXE `$death_polymorph`, `HAX_polymorph_death`; `GameStatsComponent.extra_death_msg`, `player_polymorph_count` (CD:1036-1043) | `[INFERENCE]` | the sheep has none of our LuaComponents; which `OnPlayerDied` argument is passed is unknown (E7-5) |
| 12 | Run completion | `GameOnCompleted`, `GameDoEnding2` (API:84,86; `sampo_start_ending_sequence.lua:90,134`) | callable, not interceptable | separate "game over (completed)" path |

### F9. Every way HP reaches 0 or the player entity disappears

| Path | Evidence | Passes the Lua hook? |
|---|---|---|
| Enemy/spell damage by type (`damage_multipliers`: curse, drill, electricity, explosion, fire, healing, holy, ice, melee, overeating, physics_hit, poison, projectile, radioactive, slice) | CD:717-813 | step 2 yes |
| `EntityInflictDamage` by scripts: lethal `sun_collision.lua:32` (500 curse, "DISINTEGRATED"), `islandspirit.lua:12`, `orb_blood_damage.lua:12,20`, `boss_sky_damage.lua:39`, `curse.lua:23`, `petrify.lua:26`; deliberately non-lethal: `perks/food_clock.lua:23` (only if hp > damage), `gun_actions.lua:4901` (only if hp >= 0.4) | `V/scripts/…` | step 2 yes |
| Environment: `materials_that_damage` (20 materials, `player_base.xml:147-149`), fire (`is_on_fire`, `fire_damage_amount`), drowning (`air_in_lungs=7`, `air_lack_of_damage=0.6`), overeating (`IngestionComponent.overingestion_damage`), fall (`falling_damages=0` for the player), physics (`physics_objects_damage=0`; EW turns it on), wet status | player_base.xml:132-161 | `[INFERENCE]` EW's blanket hook suggests yes |
| Direct writes: `ComponentSetValue2(dmc,"hp",0)`, `kill_now=1` by any mod | CD:759-760 | no hook; polling sees it |
| `max_hp` lowered below `hp`: no vanilla path (GLASS_CANNON sets both, `perk_list.lua:673-713`); `NO_HEAL` cap clamps healing in the meat biome | | n/a |
| Polymorph: POLYMORPH 600 f, POLYMORPH_RANDOM 600 f, POLYMORPH_UNSTABLE 1200 f, POLYMORPH_CESSATION 20 f then `nibbana` (permanent) (`V/entities/misc/effect_polymorph*.xml`); on expiry the original is rebuilt from `mSerializedData` `[INFERENCE: new entity id]` | | our LuaComponents live only on the original |
| While polymorphed the body has tag `polymorphed_player`, not `player_unit`; vanilla falls back to it (`boss_sky_damage.lua:13-16`, `null_room/check.lua:94-116`, `heart_fullhp_temple.xml:15`) | | poll `get_players()` then `polymorphed_player` |
| `EntityKill(player)`: no vanilla caller; semantics undocumented (API:9); may raise "Player Entity Destroyed" without `Message_Death` `[INFERENCE]` | | no |
| NG+ `BiomeMapLoad_KeepPlayer` keeps the entity (`V/scripts/newgame_plus.lua`); Sampo ending only moves it (`sampo_start_ending_sequence.lua:64-76`) | | n/a |
| Falling out of the world: no handler found in vanilla Lua or EXE strings | | `[unknown]` (E7-6) |
| `perk_pickup`: cannot kill; HP effects only EXTRA_HP (raises), GLASS_CANNON (max_hp = 50/25, hp = max), VAMPIRISM; `IMPL_remove_all_perks` (null room) strips children/Globals | `perk.lua:668-754` | n/a |

### F10. Bot control facts

- NPC pattern in vanilla: `ControlsComponent enabled="0"` on humanoids and animals (`V/entities/base_humanoid.xml:161-163`, `base_helpless_animal.xml:140-143`);
  scripts write its privates (`scripts/animals/blob_damage.lua:13-15` sets `mJumpVelocity`) and read them (`scripts/buildings/racing_cart_move.lua:52`,
  `perks/attack_foot_climb.lua:20`, `perks/map.lua:26`, `perks/projectile_eater_sector.lua:13`).
- EW bot = player-class entity with `ControlsComponent enabled="0"` (`notplayer/notplayer.xml:125-130`). Per frame it writes `mButtonDownLeft/Right/Up/Down/Fly`, `mButtonFrame* = frame+1` on the
  press edge, `mFlyingTargetY`, `mAimingVector`, `mAimingVectorNormalized`, `mMousePosition`, `mButtonDownFire/Fire2` with `mButtonFrameFire = frame+1` on the edge and `mButtonLastFrameFire = frame`
  while held, `mButtonDownKick`, `mButtonDownThrow/RightClick` (`notplayer_ai.lua:398,455-537,1605-1644`). EW applies remote players' inputs to proxy entities the same way (`core/player_fns.lua:73-110`).
- Gun system: each shooter needs a `GunComponent` (VM owner); `Message_UseItem` has only two listeners, `AbilityComponent` and `DroneLauncherComponent` (EXE RTTI), so `AnimalAIComponent.attack_ranged_use_message=1`
  (CD:172) fires the entity's **own** AbilityComponent (the Drone spell entity, `V/entities/misc/player_drone.xml`), not an inventory wand.
- Observation hooks: `script_wand_fired(gun_entity)` (CD:1549; used by perks, `perk_list.lua:1083,2149`), `script_shot` (CD:1534-); `OnProjectileFired` is NoitaPatcher only.
- Projectile without the wand: `GameShootProjectile` (API:138) via `shoot_projectile` (`utilities.lua:441-460`: `EntityLoad` + `GameShootProjectile` + `mWhoShot`/`mShooterHerdId` + `mVelocity`); no mana, delay, deck.
- NPC item holding: `ItemPickUpperComponent{is_in_npc=1, only_pick_this_entity}` + `GamePickUpInventoryItem` (`V/scripts/animals/wand_ghost.lua:8-31`; wand ghosts hold the wand but shoot their own projectile).
- Herds: `genome_relations.csv` row `ghost` is 0 toward `player` and 90-100 toward most others; a custom row can be added at init with `ModTextFileSetContent` (API:370). Set by `ComponentSetValue2(genome,"herd_id",StringToHerdId(name))` (API:173).
- Engine vestiges of local co-op: `COOP_RESPAWN_TIMER_FRAMES`, `$log_coop_*`, `coop_respawn` marker (`V/entities/buildings/coop_respawn.xml`, `V/scripts/biomes/temple_altar.lua:137`), NP exports `SetPlayerEntity(entity, slot)`/`GetPlayerEntity(slot)`/
  `RegisterPlayerEntityId` (DLL strings): the engine may have player *slots* `[INFERENCE]`.

## Risks and options

### Suppressing game over (ranked)

| Rank | Option | Evidence | Doubts / experiments |
|---|---|---|---|
| A1 | **Layered**: (a) override `OnPlayerDied` via `ModLuaFileAppend` (no `GameDestroyInventoryItems`, no `GameTriggerGameOver`); (b) player `wait_for_kill_flag_on_death=1`; (c) `script_damage_received` + `OnWorldPreUpdate` watchdog turn `hp <= 0` into a "downed" event; (d) snapshot cadence from D40 so bypasses are recoverable | steps 5, 9, 10; EW does (a)+(b)+(c) | RESPAWN/SAVING_GRACE order, `script_death`/`OnPlayerDied`/stats at hp <= 0, damage types: E7-1, E7-3 |
| A2 | Clamp damage in `script_damage_about_to_be_received` | CD:1532; `ingest_projectiles.lua` | pre-empts RESPAWN/SAVING_GRACE (breaks D29), sees damage before protection effects (F8 #2): use only as a supplement for overkill; E7-4 |
| A3 | Let the player die, `OnPlayerDied` no-op, rebuild from the last snapshot | `init.lua:212-215` | unknown what `Inventory2`/`Message_PlayerDied` do (stats, `death_count`, bones); loses the entity: worst; E7-2 |
| A4 | Magic number `DEBUG_PLAYER_NEVER_DIES` through `ModMagicNumbersFileAdd` (API:378) | name in EXE | release effect unknown; cheap probe only (E7-7) |

### Dead player -> bot entity (ranked)

| Rank | Option | For | Against / doubts |
|---|---|---|---|
| B1 | **Separate bot entity built from the snapshot** (clone of `player_base.xml`, `player_unit` removed, `ControlsComponent enabled=0`, no `InventoryGui`/`AudioListener`/camera centring/`StreamingKeepAlive`, `GameStats is_player=0`, own herd); the dead human's own body is parked (invulnerable, invisible, untargetable, camera via `GameSetCameraFree`/`GameSetCameraPos`, API:117-118). Items move by reparenting (`EntityRemoveFromParent` + `EntityAddChild`, API:21,25) or by recreate | keeps a valid `player_unit` (vanilla scripts use `players[1]`; EW toggles `player_unit`, `core/net_handling.lua:233-237`); bot is a plain mob with one authority (R3.3); same code revives an *offline* bot from the snapshot (R3.4) | two entities per dead player; reparenting unverified (E7-8); parked body must not attract mobs; bot needs its own Gun VM (new cast state) |
| B2 | In-place role flip (EW style): the human's player entity becomes the bot (herd, controls, `polymorphed_player` tag), "bot death" = hp <= 0 again, revive = flip back | no entity swap; reuses A1 machinery; EW `local_health.lua:309-340,400-470` | authority sits in the dead human's process (R3.4 fails when they leave unless B1 respawn-from-snapshot exists anyway); enemies/`player_unit` semantics while a hostile "player" exists |
| B3 | AnimalAI humanoid (`base_humanoid.xml` + `AnimalAIComponent`) holding the wand through `ItemPickUpper` | free pathfinding/perception | firing an inventory wand has no known route (`Message_UseItem` targets the entity's own AbilityComponent) |
| B4 | NoitaPatcher player slots (`SetPlayerEntity`) | would make the bot a "real" player for engine code | `[needs NoitaPatcher]`, `[INFERENCE]` that `OnPlayerDied` then fires per slot |

### Controlling the bot (ranked)

| Rank | Option | API calls | Doubts |
|---|---|---|---|
| K1 | **Controls-driven** (source precedent, not runtime proof) | per frame `ComponentSetValue2(ctrl,"mButtonDown*",b)`, button-edge frame fields, aiming/mouse vectors and flying target | PreUpdate/PostUpdate ordering, required player/Gun components, item selection and mana/uses/cooldown fidelity need E7-9; vanilla APIs do not promise these semantics |
| K2 | AnimalAI + own AbilityComponent (Drone pattern) | `attack_ranged_use_message=1` | not a real inventory wand |
| K3 | `shoot_projectile` per spell | `GameShootProjectile` (`API:138`) | bypasses mana/deck/delay/modifiers; **not an acceptable substitute** for the real inventory wand required by R3/D11, even for simple AI |
| K4 | NoitaPatcher helpers | `SetActiveHeldEntity`, `OnProjectileFired` | `[needs NoitaPatcher]` |

### Risk register

1. Order of RESPAWN/SAVING_GRACE vs the wait flag unknown: may break D29 (E7-3). 2. Polymorph replaces the entity (our hooks vanish; stats path unknown): poll `polymorphed_player`, or `[needs NoitaPatcher]`.
3. Leaking `Message_PlayerDied` pollutes profile stats (`death_count`, D34 intersection). 4. Save deletion is native and unhookable: never call `GameTriggerGameOver` except at run end. 5. Mod components accumulate on every load:
whitelist. 6. Absolute frames across processes: rebase. 7. LensValue semantics unknown. 8. R1/R3 dependencies: NoitaPatcher licence is unconfirmed (D18); `io` makes the mod "unsafe" (fine for personal use).
9. A second human-less `player_unit` (parked body) may be targeted by vanilla homing/herd logic: needs E7-11.

## What remains to be verified in game

Fixture **F**: wand 1 (Always Cast + trigger/timer spells + modifiers), wand 2 (shuffle, low uses), one flask with 3 materials, a pouch, 3 loose spells, ~8 perks covering F5 classes (EXTRA_MANA, ALWAYS_CAST, PROTECTION_FIRE, BREATH_UNDERWATER, STRONG_KICK, FREEZE_FIELD, SHIELD x2, GOLD_IS_FOREVER, GLASS_CANNON), 2 essences, wet + oiled + poisoned, a 600-frame `PROTECTION_ALL`, changed hp/max_hp, money. Disposable profile only (D17).

| ID | Input | Expected observable | Pass / fail |
|---|---|---|---|
| E4-1 | Schema-driven dumper (field list from `SCH`) over fixture F; try `ComponentGetValue2/ObjectGetValue2/GetVector/GetMembers` on every field | per-type read coverage; does `GetMembers` include Privates; LensValue/vector/USTRING behaviour | pass: every scalar/vec2/enum/object field readable; the rest listed |
| E4-2 | Save&Quit to player/world-state XML A; restart, restore R2, Save&Quit to B; then compare live fixture behaviour | XML differences explicitly explained; no ignored gameplay fields, lost perk effects or duplicated one-offs | XML equality alone is not a G4 pass; include casts, inventories, effects, native/global state and process restart |
| E4-3 | Same fixture through `SerializeEntity`/`DeserializeEntity` [needs NoitaPatcher] | diff vs E4-2; bytes and ms per snapshot | informs R1 vs R2 |
| E4-4 | Fresh player + `perk_pickup(0,p,id,false,false,true)` per perk class, counters pre-reset | components/children/Globals equal to captured; one-off perks excluded | pass: identical except one-off |
| E4-5 | Fire 20 eligible casts from original/restored fixtures, pinning or recording frame/entity RNG dependencies | compare cards/children, mana, uses, cast/reload timing and Always Cast | unexpected reset/consumption/shot divergence fails; classify any native Save&Quit reset separately, do not accept it implicitly |
| E4-6 | Shake, drink, spill and react mixed flask contents; capture/restore in another process | measure exact per-material amounts and whether the API rounds fractional values | exact state or an explicitly approved loss policy; an arbitrary one-cell tolerance is not automatic acceptance |
| E4-7 | Read `run_velocity` with/without MOVEMENT_FASTER; write via Set | base vs effective, persistence | decides LensValue handling |
| E4-8 | `EntityGetFilename` of wands, perk and effect children before/after Save&Quit | empty/wrong after reload? | confirms content-based identification |
| E4-9 | Set `mSavedActiveItemIndex` right after creating the entity; later switch via Controls pulses | active wand correct; pulses work for any slot | else needs NoitaPatcher |
| E4-10 | Count `OnPlayerSpawned` on new game and on continue; Globals guard | called on both | confirms F7 inference |
| E4-11 | Apply POLYMORPH; read `mSerializedData` (type, size, format); try feeding a captured blob to a fresh GameEffect | blob inspectable/reusable? | decides polymorph snapshot |
| E4-12 | Runtime-written XML (R3): `io.open` under the mod dir, `EntityLoad`; rewrite same path with other content | loads; privates ignored; caching | decides R3 |
| E4-13 | Status vectors: readable by `ComponentGetVector`; setter fails/no-ops?; approximate restore with `EntityIngestMaterial` | durations within tolerance | records A/X class |
| E7-1 | wait=1, `OnPlayerDied` no-op, our `script_damage_received` + `script_death` + world-update logger; lethal hit of each type (projectile, explosion, melee, fire, acid/lava, drowning, curse via `EntityInflictDamage`, electricity, ice, overeating, physics with `physics_objects_damage=1`) | hp trace; entity alive; call counts of `script_death`/`OnPlayerDied`; `StatsGetValue("dead")`; game-over screen | pass: no game over, entity alive, exactly one downed event per hit |
| E7-2 | Same with wait=0 and `OnPlayerDied` no-op | entity fate, inventory alive?, `dead`/`death_count`/bones/save files, can the run continue | tells how bad A3 is |
| E7-3 | Add RESPAWN / SAVING_GRACE through `GetGameEffectLoadTo` (`frames=-1`) and lethal hits, wait=1 and wait=0 | hp after, `mIsSpent`, log text, place, who acts first | pass: effect acts before our downed detection (D29); else redesign |
| E7-4 | Lua hook on `about_to_be_received` with PROTECTION_FIRE/EXPLOSION, SAVING_GRACE active | damage the hook sees vs final hp loss | decides if clamping is usable |
| E7-5 | Polymorph (4 variants): kill the form, wait for timer expiry, `EntityKill` the effect, with RESPAWN | which entity has `polymorphed_player`, our LuaComponents gone?, argument of `OnPlayerDied`, original restored, stats | pass: every end observed once and hooks re-attachable by polling |
| E7-6 | `EntityKill(player)`; `kill_now=1`; `hp=0` write; teleport below the world bottom | `OnPlayerDied`/game over fire? | each bypass class known |
| E7-7 | `DEBUG_PLAYER_NEVER_DIES` via `ModMagicNumbersFileAdd` | hp floor? | cheap probe |
| E7-8 | Spawn B1 bot from `player_base.xml`; give it a real wand by reparenting from a test player, and by `GamePickUpInventoryItem` | `enabled_in_*` states, no duplicate item, held/active wand | pass: single instance, usable wand |
| E7-9 | Drive Controls in PreUpdate/PostUpdate: walk, jump, fly, aim, switch items and make 20 cooldown/mana-eligible casts with the real wand | correct projectiles and exact mana/uses/reload consumption; log expected rejection of deliberately ineligible requests | no unexplained lost or duplicate eligible cast; compare human control, then test required player/Gun component variants |
| E7-10 | Bot hp <= 0 with wait=1; items returned to the human body | no `OnPlayerDied`; uses/mana preserved; no ghost copy | pass: single instance, state kept (A16) |
| E7-11 | Parked human body: invisible, invulnerable, camera free | mobs ignore it, vanilla `players[1]` scripts silent, chunks stay loaded | pass: no aggro, no log errors |
| E7-12 | Offline bot: restore the bot from a snapshot in a fresh process | equals live bot (E4-2 criteria) | R3.4 |
