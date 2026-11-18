# Vanilla pickups, rewards and global-state mechanics

Status: research note for the analysis phase (2026-11-18). No code. Author: agent `VanillaPickups`.

## 0. Scope, method, conventions

- **Question answered.** For every vanilla mechanic that creates or consumes a "reward" (gold, orbs, hearts,
  chests, boss drops, Holy Mountain contents, music/secret rewards, unique world items, essences, Sampo,
  endings, NG+), plus the perks and the fungal shift that mutate *global* state: which entities and
  scripts are involved, how the RNG is seeded, which callbacks fire, what state is written, and where a mod
  can (a) detect the event, (b) suppress it, (c) replicate it N times or apply it to every player.
- **Sources.** Vanilla data unpacked from `data.wak` at `.cache/noita-data/data/` (build
  `9dbd52ced019a643169a2db02f46c77f8766c6e5`). API docs:
  `/data/Games/steamapps/common/Noita/tools_modding/lua_api_documentation.txt` (cited as `API:<line>`) and
  `component_documentation.txt` (cited as `CDOC:<line>`). A few engine facts come from `strings noita.exe`;
  they are marked as such.
- **Citation format.** `.cache/noita-data/data/<path>:<line>`. To keep the tables readable, paths inside this
  document are written relative to that root, e.g. `scripts/items/drop_money.lua:63` means
  `.cache/noita-data/data/scripts/items/drop_money.lua:63`.
- **[INFERENCE]** marks anything not directly visible in vanilla files or API docs (engine-side behaviour,
  intent, proposals).
- **Policies** use the letters from `REQUIREMENTS.md` R4: **S** shared effect, **N** multiply by N players,
  **P** personal copy, **1** single object.
- **Units.** `DamageModelComponent.max_hp`/`hp` are internal units; the UI shows `×25`
  (`scripts/items/heart.lua:34` prints `max_hp*25`). "+1 max_hp" = "+25 HP" on screen.

### 0.1 Cross-cutting engine facts used throughout

| Fact | Evidence |
|---|---|
| `script_item_picked_up` calls `item_pickup(entity_item, entity_pickupper, item_name)` on `Message_ItemPickUp`. It runs in the process that performed the pickup. | `CDOC:1533` |
| `script_death` calls `death(damage_type_bit_field, damage_message, entity_thats_responsible, drop_items)` on `Message_Death`. | `CDOC:1540` |
| `script_interacting`, `script_kick`, `script_collision_trigger_hit`, `script_physics_body_modified`, `script_material_area_checker_*` exist with the signatures listed. | `CDOC:1534-1547` |
| `ModLuaFileAppend(to, from)` = `dofile(from)` at the end of `to`; init-time only. Redefining a global function in the appended file replaces the vanilla one for every later call (Lua last-definition-wins) [INFERENCE: standard Lua semantics]. | `API:366` |
| `ModTextFileSetContent` replaces an XML/Lua file before load; init-time only. | `API:370` |
| `SetRandomSeed(x, y)` seeds the **global** script RNG used by `Random()`. Vanilla treats `SetRandomSeed(1,2)` as "world-seed dependent" (`scripts/perks/perk.lua:53,70`), so the world seed is mixed in [INFERENCE]. | `API:233-234` |
| `ProceduralRandom*(x, y, ...)` is a stateless hash of (world seed, x, y) [INFERENCE: same argument as above; used for spawn decisions, e.g. `scripts/biome_scripts.lua:145`]. | `API:238-240` |
| `GameGetFrameNum()` returns an engine frame number. Its epoch across save/reload is not documented here; peers may be on different frames, so frame-seeded outputs must be resolved on one authority [INFERENCE]. | `API:185` |
| Run state lives on the world-state entity: `WorldStateComponent.flags` (run flags), `lua_globals` (Globals), `orbs_found_thisrun`, `changed_materials`, `perk_*` fields. | `CDOC:2641-2706` (fields at `:2673-2702`) |
| Persistent (profile) flags: `AddFlagPersistent/HasFlagPersistent` are per **profile**, i.e. per player machine, not per run. | `API:275-277` |
| Magic numbers: `NUM_ORBS_TOTAL="12"`; `GAMEEFFECT_EXTRA_MONEY_TRICK_KILL_MULTIPLIER="2"`; `DESIGN_RANDOMIZE_TEMPLE_CONTENTS="0"`. Lua constant `ORB_COUNT_IN_WORLD = 11`. | `magic_numbers.xml:98,51,12`; `scripts/lib/utilities.lua:2` |

Consequence for the whole document: **every vanilla effect below is applied to `entity_who_picked` (or to
the closest/first `player_unit`) inside the local process**, and every "global" consequence is written to the
local `WorldStateComponent`. Nothing in vanilla is multi-player aware. A co-op mod therefore needs, per
mechanic, (1) one authoritative spawn, (2) one authoritative claim, (3) an explicit rule for which players
receive the effect.

### 0.2 Findings that change the co-op design

1. **Nugget** pickup is Lua (`gold_pickup.lua`), but **gold powder** is collected natively by the player's
   `MaterialSuckerComponent.suck_gold`. R5 requires both paths; there is no verified Lua event for powder
   (§1.7-1.8).
2. Orbs have **native bookkeeping before/around a Lua reward callback**; wrapper-only suppression may be
   too late. Orb count, rewards and profile unlocks are three separate state layers (§2).
3. Hearts are flat max-HP effects; the temple heart mixes a +10-HP gain with a full heal (§3).
4. Most chests are position-seeded, but profile unlocks affect results. Great chests can yield Sampo (§4).
5. Boss loot, gold, native chest loot, progression and world effects are separate layers; never rerun a
   whole boss death N times (§5).
6. Perk deck/counters/removal, many perk effects, essence counters, key music credit and temple state are
   global. Personal perks require more than personal perk entities (§6, §8).
7. "Notes" includes instruments, note cards, songs and a distinct music-machine/key quest. Tower choice
   cleanup is global, experimental/Tower wands are frame-seeded, and NG+ updates only one player in Lua (§7).

All hooks and policy recommendations below are **[INFERENCE] design proposals**, not tested mod code.

## 1. Gold

### 1.1 Gold entities and denominations

All nugget entities share the same skeleton (shown for `entities/items/pickup/goldnugget_10.xml`):

- tags `item_physics,gold_nugget` (`:1`);
- `VariableStorageComponent name="gold_value"` (`:30-34`);
- `ItemComponent auto_pickup="1" stats_count_as_item_pick_up="0"` (`:39-50`), so the player's
  `ItemPickUpperComponent` collects it on contact without a key press [INFERENCE: engine pickup loop];
- `LifetimeComponent lifetime="900"` (`:60-63`): the nugget despawns after 900 frames (15 s at 60 fps)
  unless the timer is removed;
- `LuaComponent script_item_picked_up="data/scripts/items/gold_pickup.lua"` (`:65-68`);
- `LuaComponent script_source_file="data/scripts/perks/gold_explosion.lua" execute_on_added="1"
  remove_after_executed="1"` (`:70-76`). This runs **once on every nugget at creation**, whichever script
  spawned it. It is the most general "nugget created" hook in vanilla.

| File | `gold_value` | Lifetime | `hp_value` (blood money) | Notes |
|---|---|---|---|---|
| `goldnugget.xml` | 10 (`:30-34`) | 900 (`:60-63`) | - | sprite `goldnugget_01.png`; used by chests |
| `goldnugget_10.xml` | 10 (`:30-34`) | 900 (`:60-63`) | - | |
| `goldnugget_50.xml` | 50 (`:32-33`) | 900 (`:61-62`) | - | |
| `goldnugget_200.xml` | 200 (`:32-33`) | 900 (`:61-62`) | - | |
| `goldnugget_1000.xml` | 1000 (`:32-33`) | 900 (`:61-62`) | - | |
| `goldnugget_10000.xml` | 10000 (`:33-34`) | 900 (`:62-63`) | - | golden idol, `metal_prop` (`:27-28`) |
| `goldnugget_200000.xml` | 200000 (`:33-34`) | 900 (`:62-63`) | - | big idol |
| `goldnugget_x.xml` | 200 default (`:40-41`), overwritten at spawn | 900 (`:69-70`) | - | "gold statue" for huge drops |
| `bloodmoney_10.xml` | Base `goldnugget_10` (`:3`) | inherited | 0.16 (`:26-27`) | material `bloodgold_box2d` |
| `bloodmoney_50.xml` | Base `goldnugget_50` (`:3`) | inherited | 0.4 (`:24-25`) | |
| `bloodmoney_200.xml` | Base `goldnugget_200` | inherited | 0.6 (`:24-25`) | |
| `bloodmoney_1000.xml` | Base `goldnugget_1000` | inherited | 0.6 (`:24-25`) | |
| `bloodmoney_10000.xml` | Base `goldnugget_10000` | inherited | 0.6 (`:24-25`) | |
| `bloodmoney_200000.xml` | Base `goldnugget_200000` | inherited | 1.6 (`:24-25`) | |
| `bloodmoney_x.xml` | Base `goldnugget_x` | inherited | 100 (`:24-25`) | |

`WalletValuableComponent` (`CDOC:2637-2639`) is **not used** by any vanilla file (grep over `data/` finds no
user). Gold value lives only in the `gold_value` variable.

### 1.2 Pickup: `scripts/items/gold_pickup.lua`

`item_pickup(entity_item, entity_who_picked, item_name)` (`:5-63`):

1. Reads the picker's `WalletComponent.money` with `ComponentGetValue2` (int64) (`:15-18`).
2. Reads `gold_value` (default 10) and `hp_value` from the nugget (`:21-33`).
3. Spawns pickup FX by value: `>500` huge, `>40` large, else small (`:36-42`).
4. **Extra Money:** `value = value * 2` once per stack of the `EXTRA_MONEY` game effect **on the picker**
   (`GameGetGameEffectCount(entity_who_picked, "EXTRA_MONEY")`, `:44-49`).
5. `money = money + value`, written back to the picker's wallet (`:51-55`).
6. Blood money: `heal_entity(entity_who_picked, hp_value * 0.5)` (`:57-60`; helper
   `scripts/game_helpers.lua:10-26`).
7. `EntityKill(entity_item)` (`:62`).

No RNG. All effects target the picker only. The player starts with an empty `WalletComponent`
(`entities/player_base.xml:406-407`; default `money=0`, `CDOC:2632`). `WalletComponent` also has
`money_spent`, `mMoneyPrevFrame` ("HAX to give player towards infinite moneys") and `mHasReachedInf`
(`CDOC:2630-2635`).

### 1.3 Enemy death drop: `scripts/items/drop_money.lua`

**Attachment.** `entities/base_humanoid.xml:17-21` declares
`LuaComponent script_death="data/scripts/items/drop_money.lua" remove_after_executed="1"`.
`base_enemy_basic.xml:2`, `base_enemy_flying.xml`, `base_enemy_robot.xml` and
`base_enemy_robot_boss_limbs.xml` use `<Base file="data/entities/base_humanoid.xml">`, and about 170 entity
files (`rg -l` count) build on those four bases. Seven more reference `drop_money.lua` directly (six
`lukki*.xml`, `shooterflower.xml`). So practically every enemy drops gold. `base_helpless_animal.xml` (birds,
deer and so on) does not inherit `base_humanoid.xml` and drops nothing.

**Opt-out.** A `VariableStorageComponent` tagged `no_gold_drop` cancels the drop (`:12-17`). Static users:
`entities/animals/boss_centipede/boss_centipede_minion.xml:4`, `friend.xml:212`, `fungus_tiny.xml:172`,
`pebble_physics.xml:234`, `ultimate_killer.xml:132`, `misc/perks/plague_rats_rat.xml:121`, and others.
Dynamic users: `scripts/items/die_status.lua:26`, `scripts/items/greed_die_status.lua:34`,
`scripts/status_effects/necrobot_end.lua:15`, `scripts/buildings/failed_alchemist_orb.lua:11`.
Trailer mode also returns early (`:7`).

**Amount** (`do_money_drop(amount_multiplier, trick_kill)`, `:5-178`):

```
amount = floor(max_hp) if max_hp > 1 else 1                -- :19-28
if run flag greed_curse and not greed_curse_gone: amount *= 3   -- :30-32
amount *= amount_multiplier                               -- :34
money = 10 * amount                                       -- :39
```

`death()` calls `do_money_drop(1, false)` (`:180-182`). The variants `drop_money2x.lua` ... `drop_money64x.lua`
call `do_money_drop(N, true)` (e.g. `drop_money2x.lua:4-6`, `drop_money16x.lua:4-6`). **No data file
references the variants**, but their paths are string literals in `noita.exe` (`strings` output). The engine
therefore swaps the death script on trick kills [INFERENCE], scaled by
`WorldStateComponent.trick_kill_gold_multiplier` (`CDOC:2699`) and the `EXTRA_MONEY_TRICK_KILL` effect
(`GAMEEFFECT_EXTRA_MONEY_TRICK_KILL_MULTIPLIER="2"`, `magic_numbers.xml:51`;
`entities/misc/effect_extra_money_trick_kill.xml:5-8`, `frames="600"`) [INFERENCE on the exact formula].

**World-state reads** (`:48-72`), all global to the run:

| Field | Effect | Lines |
|---|---|---|
| `perk_gold_is_forever` | `remove_timer=true`: `load_gold_entity` removes every `LifetimeComponent` | `:50-52`; `scripts/game_helpers.lua:28-39` |
| `perk_trick_kills_blood_money` (only when `trick_kill`) | prefix becomes `bloodmoney_`, and the "first five 10s" step is skipped | `:54-60` |
| `perk_hp_drop_chance` | `SetRandomSeed(x + entity, y - entity)`; with probability `amount_multiplier * chance` % spawn `bloodmoney_50.xml` at `(x, y-8)` and subtract 50 | `:63-71` |

**Denomination split** (no RNG; everything spawns at `(x, y-8)` and physics scatters it):
1. Up to five `<prefix>10.xml` while `money >= 10` (`:75-84`).
2. If `money >= 2,200,000`: one `<prefix>x.xml` whose `gold_value` is set to `money - 2,000,000`, clamped
   to `2^31-1-512` (`:88-113`).
3. Greedy loops: `200000`, `10000`, `1000`, `200`, `50`, `10` (`:116-156`).
4. If run flag `exploding_gold`: each spawned nugget gets a `CollisionTriggerComponent required_tag="enemy"
   timer_for_destruction=5` and a `LuaComponent script_collision_trigger_hit=
   "data/scripts/perks/gold_explosion_collision_trigger.lua"` (`:158-177`).

Worked example: an enemy with `max_hp=4` (100 HP) drops 4 x `goldnugget_10`. One with `max_hp=30` drops
5 x 10, then 1 x 200 and 1 x 50 (300 gold).

**RNG:** only the blood-money roll, seeded from position plus entity id (`:63`). The entity id differs
between processes, so even this roll differs between clients [INFERENCE]. Everything else is a
deterministic function of `max_hp`, flags and world-state fields.

### 1.4 Every other gold source

| Source | Where | What spawns / changes | RNG seed |
|---|---|---|---|
| Kicked gold orb (`physics_gold_orb.xml`) | `scripts/items/gold_orb.lua:3-52` (`kick()` calls `drop()`) | Kick 1 always gives 3 x 10. Later kicks roll `Random(1,22)`: 2 x 10 or 3 x 10, 2 x 50 (outcome 15), 1 x 200 (outcome 20), or an explosion that destroys the orb (outcome 1, more likely after kick 4) | `SetRandomSeed(GameGetFrameNum(), x+y+entity_id)` (`:9`), then `SetRandomSeed(x+entity_id, y-GameGetFrameNum())` (`:16`): **frame-based** |
| Greed gold orb (`physics_gold_orb_greed.xml`) | `scripts/items/gold_orb_greed.lua:3-50` | Same with `bloodmoney_10/50/200/1000` and `explosion_giga` | same, frame-based (`:9,16`) |
| Gold orb placement | `scripts/item_spawnlists.lua:9-24` (potion spawn list, rolls 90-91 of 1..91) | greed variant if greed curse is active | caller's seed |
| Chest (`chest_random`) | `scripts/items/chest_random.lua:89-141` | gold branch (rolls 8-40): 7-20 x `goldnugget.xml` plus 50/200/1000 nuggets; `chest_load_gold_entity` delays pickup by 30 frames (`:46-56`). `drop_money()` is an empty stub (`:312-315`) | position-seeded, see §4 |
| Great chest | `scripts/items/chest_random_super.lua:47-48` | no nuggets; rolls 11-15 spawn the `rain_gold.xml` projectile (row below). `drop_money()` is empty (`:132-135`) | position-seeded, see §4 |
| Charmed mimic | `scripts/animals/mimic_charm.lua:44-59` | per "poop": `Random(1,1000)` → 1000: `heart.xml`; ≥985: potion; ≥925: `goldnugget_200`; ≥800: `goldnugget_50`; else `goldnugget_10` | `SetRandomSeed(pos_x+pos_y, -GameGetFrameNum())` (`:9`), frame-based |
| Wand dipped in charm ("golden wand") | `scripts/animals/wand_charm.lua:27-48` | an **empty** wand becomes `gun_level` x `goldnugget_200` (1..10) | none |
| Outhouse (`huussi`) | `scripts/buildings/huussi.lua:3-24` | first time: spell `TOUCH_PISS` + persistent flag; later (if no poopstone within 200 px): 1/3 one `goldnugget_200`, else `Random(5,19)+1` x `goldnugget_10` | `SetRandomSeed(x, y)` with **undefined globals** `x, y` (`:8`) [INFERENCE: nil → 0, fixed seed] |
| Die / greed die | `scripts/items/greed_die_status.lua:77-103` | results 4/5/6: circles of 16 x 10, 12 x 50, 8 x 200 gold; greed "good" result: 2 x `chest_random_super.xml` | `SetRandomSeed(GameGetFrameNum(), pos_x+pos_y+entity_id)` (`:6`), frame-based |
| Greed meteor | `scripts/magic/greed_curse/greed_meteor.lua:10-21` | 6 nuggets of 10/10/50 | `SetRandomSeed(x, y)` |
| Gold rain projectile | `scripts/projectiles/rain_gold.lua:6-59` (spawned by `chest_random_super.lua:48`, `all_spells_stage.lua:67`) | `Random(1, math.min(math.floor(math.abs(pos_y)/1024, 10)))` nuggets (`:8`). The `10` lands inside `math.floor`, so the intended cap of 10 never applies [INFERENCE: Lua semantics]. Mostly 10-gold nuggets, up to 200, scattered within ±256 px | `SetRandomSeed(GameGetFrameNum(), pos_x+pos_y+entity_id)` (`:6`), frame-based |
| Kolmi reward statues | `entities/animals/boss_centipede/rewards/gold_reward.lua:3-49` | **direct wallet write** on manual pickup: `8000 * (GameGetOrbCountThisRun()+1)` or `end_gold` (70 000 - 500 000; `reward_*.xml`) | particles only: `SetRandomSeed(pos_x, pos_y)` (`:40`) |
| Streaming event `RAIN_GOLD` | `scripts/streaming_integration/event_list.lua:987-998` | Twitch only | frame-based |
| `coin.lua` | `scripts/items/coin.lua:3-21` | +1 money; "legacy file", referenced by nothing | - |

Kolmi's reward selection (`entities/animals/boss_centipede/rewards/spawn_rewards.lua:2-140`) reads
**per-process stats** (`StatsGetValue("enemies_killed"|"playtime"|"projectiles_shot"|"damage_taken"|"kicks"
|"wands_edited"|"biomes_visited_with_wands")`, `:6-16`) and the wallet of the **closest** `player_unit`
(`:26-33`). In co-op the statues depend on which client runs the death script and who stands closest.

### 1.5 Money sinks (local spending, R5.2)

| Sink | Where |
|---|---|
| Shop purchase | `ItemCostComponent cost` (`entities/base_shop_item.xml:4`; `CDOC:1371-1376`). The deduction and steal check happen in the engine [INFERENCE]; `scripts/items/shop_effect.lua:3-7` only spawns FX. Details in §6. |
| Perk reroll | `ItemCostComponent` on `entities/items/pickup/perk_reroll.xml` (§6) |
| Spell `MONEY_MAGIC` | `scripts/gun/gun_actions.lua:4826-4856`: `d=min(floor(money*0.05),24000)`; only if `d>1` and `money>=10`, spend `max(d,10)` from the **caster's** wallet and add it to `money_spent` (`:4841-4856`). Below 40 gold the branch does nothing. |

### 1.6 Perks and curses that change gold

| Perk / item | Definition | Mechanism | Scope in vanilla |
|---|---|---|---|
| `EXTRA_MONEY` | `scripts/perks/perk_list.lua:79-87` | game effect on the picker; ×2 per stack at pickup (`gold_pickup.lua:44-49`) | picker only |
| `EXTRA_MONEY_TRICK_KILL` | `perk_list.lua:88-96` | game effect; trick kills use `drop_moneyNx.lua` [INFERENCE] | killer only [INFERENCE] |
| `GOLD_IS_FOREVER` | `perk_list.lua:97-124` | `WorldStateComponent.perk_gold_is_forever = 1` | **global**: every death drop (`drop_money.lua:50`) and chest gold (`chest_random.lua:95-100`) |
| `TRICK_BLOOD_MONEY` | `perk_list.lua:125-151` | `perk_trick_kills_blood_money = 1` | **global**: every trick kill by anyone |
| `EXPLODING_GOLD` | `perk_list.lua:152-169` | run flag `exploding_gold` | **global**: every nugget at creation (`scripts/perks/gold_explosion.lua:3-60`) and every death drop (`drop_money.lua:158-177`). The explosion is attributed to `EntityGetWithTag("player_unit")[1]` (`gold_explosion.lua:24-33`); its radius uses Globals `PERK_PICKED_EXPLODING_GOLD_PICKUP_COUNT` (`:35-40`) |
| `NO_WAND_EDITING` | `perk_list.lua:1040-1068` | `perk_hp_drop_chance += 20` | **global**: blood-money HP drops from every kill |
| `ATTRACT_ITEMS` | `perk_list.lua:2289-2326` | per-player LuaComponent pulls **all** `gold_nugget` entities within Globals `PERK_ATTRACT_ITEMS_RANGE` (`scripts/perks/attract_items.lua:6-31`) | range is global |
| Greed curse (`greed_curse.xml`, "essence of greed") | `scripts/magic/greed_curse/greed_curse_pickup.lua:4-31` | run flag `greed_curse`, persistent `greed_curse_picked`; ghost entity attached to the picker | **global**: ×3 gold (`drop_money.lua:30-32`), greed dice/gold orbs/chest variants; lifted by `greed_curse_gone` (`greed_crystal_death.lua:24`) |
| Inactive | `GOLD_NO_DISAPPEAR_WHILE_SEEN`, `GOLD_STAY_FOREVER`, `GOLD_ATTRACTION` (`perk_list.lua:3215-3266`) | inside the `--[[ ... ]]--` block `perk_list.lua:3137-3268` | not in the game |

### 1.7 Co-op analysis and the proposed hook "player X picked gold amount A" (R5, D4)

Facts that shape the design:
- Each nugget exists only in the process that ran the spawning script. Death drops run where the enemy died.
  Several spawners are frame-seeded, so two clients running them would produce different gold.
- The pickup amount depends on the **picker's** `EXTRA_MONEY` stack count. Blood money also heals the picker.
- Lifetime is 900 frames, unless `perk_gold_is_forever` (global) removes it.
- Kolmi statues bypass nuggets and write the wallet directly.

**Proposed hooks**

| Step | Hook | Detail |
|---|---|---|
| Nugget created (assign identity) | `ModLuaFileAppend("data/scripts/perks/gold_explosion.lua", ...)` | Runs once per nugget on creation for every source (`goldnugget_10.xml:70-76`). Only the spawning authority keeps the nugget and stamps a run-unique id into a `VariableStorageComponent` (e.g. `mp_uid`). Other clients receive it through entity replication. Alternative: wrap `load_gold_entity` (`scripts/game_helpers.lua:28`); it misses non-`drop_money` spawners. |
| (a) Detect pickup | `ModLuaFileAppend("data/scripts/items/gold_pickup.lua", ...)` that saves the vanilla `item_pickup` and redefines it. Same for `gold_reward.lua`. | Report only when `entity_who_picked` is the **local** player entity. Event `GoldPickup{nugget_uid, picker_slot, gold_value, hp_value, source}`. For statues `source="kolmi_reward"` and the amount is `end_gold` or `8000*(orbs+1)` read from the shared orb counter. |
| (b) Suppress vanilla effect | in the wrapper | Do not write `WalletComponent`. Disable the nugget's `ItemComponent` (or hide it) until the host confirms the claim, then `EntityKill`. The host rejects a second claim with the same uid (R1.4.2). |
| (c) Apply to all | host ledger `GoldIncome{id=nugget_uid, amount, source}` (R5.1) | Each player's client applies `money += f(amount)` exactly once (host keeps a per-player applied-sequence number). Offline players catch up on rejoin (R2.4). |

Open design point (R5/D4 with D5): **whose multiplier applies?** Option A: the amount
includes the picker's `EXTRA_MONEY` doublings, and everyone gets the same number. Option B: the ledger stores
the base `gold_value`, and each recipient multiplies by their own `EXTRA_MONEY` stacks. Option B matches D4
("everyone earns like a solo player") together with D5 (personal perks). Also undecided: is blood-money
healing personal (picker only) or shared like hearts (D6)?

Additional rules:
- Spending stays local (R5.2). Shop deductions happen in the engine; the host stores the resulting balance
  (R1.3.3), never as income.
- Global gold modifiers (`perk_gold_is_forever`, `exploding_gold`, `perk_hp_drop_chance`,
  `perk_trick_kills_blood_money`, greed curse) apply to **all** drops in vanilla. See §8 for whether they
  stay global.
- Trick-kill gold is chosen by the engine. The mod sees only the resulting `drop_moneyNx.lua` call. Appending
  to `drop_money.lua` and wrapping `do_money_drop` catches every variant, because each variant `dofile`s
  `drop_money.lua` (`drop_money2x.lua:1`).

### 1.8 Gold powder is a separate **native** income source

`entities/player_base.xml:163-166` enables:

```xml
<MaterialSuckerComponent suck_gold="1" suck_health="1" />
```

`CDOC:1661-1677` explicitly defines `suck_gold`: **"if set will just suck gold and update wallet"**
(`:1668`). This does **not** create a `gold_nugget` item or call `gold_pickup.lua`. Gold sand is defined in
`materials.xml:9924-9949` (`name="gold"`, tags include `[gold]`, `liquid_sand=1`); raw gold can come from veins,
gold biomes, gold spells, conversions and chest roll 96-98 (§4). `suck_health` similarly consumes healthium
and adds HP (`CDOC:1669`), another native personal heal.

The component exposes `mGoldAccumulator` ("amount of gold picked during consecutive frames") and
`mLastFramePickedGold` (`CDOC:1676-1677`), but the accumulator's reset/rounding/multiplier semantics and
relation to actual credited money are **unresolved**. Engine strings include
`AUDIO_PICK_GOLD_SAND_MIN_AMOUNT_FOR_SOUND` and `...AMOUNT_ACCUMULATION_FRAMES`; it may be sound bookkeeping,
not a transaction counter. Do not use its deltas as certified income.

**Required native hook [INFERENCE proposal]:** intercept the MaterialSucker **gold-cell removal / wallet
credit branch**, emit `(player_entity, exact_credited_amount, collected_cell_count, source_cell_ids)` into
the host journal, and suppress duplicate collection on replicas. The terrain authority removes those cells
once; income is applied to every wallet once. The exact native function/address, the cell→gold formula and
whether `EXTRA_MONEY` applies to powder have **not been reverse engineered here**. No corresponding Lua
callback is documented (`API`, `CDOC:1661-1677`).

Disabling `suck_gold` is a suppression point, **not a finished solution**: a replacement collector needs
native cell access/removal and exact vanilla amount semantics. Sampling `WalletComponent.money` each frame
still violates R5.3 and cannot distinguish this income from spending. The exact Lua nugget hook in §1.7
must not be advertised as covering all of R5.4.

## 2. Orbs

### 2.1 Entities

`entities/items/orbs/orb_base.xml` defines the common orb:

- `ItemComponent auto_pickup="0" enable_orb_hacks="1"` (`:24-30`): picked up with the interact key, and
  the engine treats it as an orb;
- `CameraBoundComponent max_count="20" distance="160000"` (`:32-35`);
- `OrbComponent orb_id` (`:37-39`);
- three sprites tagged `orb_undiscovered` / `orb_discovered` / `orb_picked` (`:41-94`), switched by the
  engine by profile and run state [INFERENCE];
- `VariableStorageComponent card_name="LIGHT_BULLET"` (`:105-108`): the spell given on the first-ever pickup;
- `LuaComponent execute_on_added script_source_file=orb_init.lua` (`:110-116`) and
  `LuaComponent script_item_picked_up=orb_pickup.lua` (`:118-120`).

| `orb_id` | File | `card_name` | Spawner (biome script, `spawn_orb`) | Location per `scripts/biomes/orbrooms/orb_plan.txt` |
|---|---|---|---|---|
| 0 | `orb_00.xml:5-17` | `SEA_LAVA` | `orbrooms/orbroom_00.lua:29-34`; `mountain/mountain_top.lua:14-15`; `mountain/mountain_floating_island.lua:13-14` | mountain top / floating island |
| 1 | `orb_01.xml` | `CRUMBLING_EARTH` | `orbroom_01.lua:29-30`; `pyramid_top.lua:37-38`; `lake_statue.lua:249-250` | top of the pyramid |
| 2 | `orb_02.xml` | `TENTACLE` | `orbroom_02.lua:32-33` | snow vault |
| 3 | `orb_03.xml` | `NUKE` | `orbroom_03.lua:29-30`; `lavalake.lua:31-32` | below the lava lake |
| 4 | `orb_04.xml` | `NECROMANCY` | `orbroom_04.lua:30-31` | below the pyramid |
| 5 | `orb_05.xml` | `BOMB_HOLY` | `orbroom_05.lua:32-33` | wand cave |
| 6 | `orb_06.xml` | `SPIRAL_SHOT` | `orbroom_06.lua:30-31` | vault |
| 7 | `orb_07.xml` | `CLOUD_THUNDER` | `orbroom_07.lua:40-41` | lava lake; pickup also runs `scripts/items/orb_07_trap.lua` (enemy ambush groups chosen by `ProceduralRandomf(seed,-463)`, `:12,20-33`) |
| 8 | `orb_08.xml` | `FIREWORK` | `orbroom_08.lua:32-33` | hell |
| 9 | `orb_09.xml` | `EXPLODING_DEER` | `orbroom_09.lua:32-33` | below the snow |
| 10 | `orb_10.xml` | `MATERIAL_CEMENT` | `orbroom_10.lua:32-33` | below the desert |
| 11 | `orb_11.xml:3-14` | none (base `LIGHT_BULLET`) | `orbroom_11.lua:29-30` | - |
| 13 | `orb_13.xml` | `DESTRUCTION` | only commented-out loads (`scripts/magic/moon_altar.lua:34`, `moon_altar_extra.lua:17`, `dark_moon_altar_extra.lua:17`) | unused |

Each orb room also loads a lore book and `music_energy_000.xml` (`orbroom_00.lua:32-33`). The commented
`heart_better` lines in every orb room (`orbroom_00.lua:31` and so on) show that hearts were removed from orb
rooms. `ORB_COUNT_IN_WORLD = 11` (`scripts/lib/utilities.lua:2`), `NUM_ORBS_TOTAL="12"`
(`magic_numbers.xml:98`).

**Parallel worlds.** `orb_init.lua:14-31`: if `orb_id >= 100`, the script clears `card_name` and paints the
sprite red (`orb_red_evil.xml`). IDs ≥ 100 are the "outside bounds" orbs of parallel worlds (comment at
`orb_pickup.lua:23`). The engine offsets the id by world index [INFERENCE]. This yields the 33/34-orb
counts used by the endings.

### 2.2 Pickup flow (`scripts/items/orb_pickup.lua:4-74`)

1. Read `orb_id` from `OrbComponent` (`:7-13`). `AddFlagPersistent("progress_orb_1")` (`:19`).
2. `orb_id >= 100`: message "evil orb", persistent `progress_orb_evil`, **spawn `heart_evil.xml`** at the orb
   (`:22-30`).
3. Otherwise, if `GameGetOrbCollectedAllTime(orb_id)` (profile) or daily run: **spawn `heart.xml`**
   (`:32-39`).
4. Otherwise (first pickup ever on this profile): persistent `progress_orb_pickup_<id>`, and **spawn the
   orb's spell** with `CreateItemActionEntity(card_name)` (`:40-52`).
5. `GameGetOrbCountAllTime() >= 11`: persistent `progress_orb_all` and achievement `ALL_ORBS` (`:56-60`).
6. Unless run flag `boss_centipede_is_dead`: attach `orb_boss_scream.xml` to the picker (sound and screen
   shake 100, `scripts/items/orb_boss_scream.lua:7-10`) (`:64-68`). No vanilla script sets
   `boss_centipede_is_dead`; the only `GameAddFlagRun` is commented out (`:63`).
7. Message, FX, `EntityKill(entity_item)` (`:70-73`).

**The run counter.** No Lua writes `WorldStateComponent.orbs_found_thisrun` (`CDOC:2694`). The engine
records the orb when an item with `enable_orb_hacks="1"` + `OrbComponent` is picked up [INFERENCE]. Lua only
reads it through `GameGetOrbCountThisRun()` / `GameGetOrbCollectedThisRun(id)` and resets it through
`GameClearOrbsFoundThisRun()` (`API:101-104`). The all-time variants (`GameGetOrbCountAllTime`,
`GameGetOrbCollectedAllTime`) read the **profile** (`orbs_new/` in the save, see
`docs/research/noita_capabilities.md` §5.1).

No RNG is involved in the pickup itself.

### 2.3 Who reads the orb count

| Reader | Lines | Effect |
|---|---|---|
| Kolmi init | `entities/animals/boss_centipede/boss_centipede_update.lua:43-111` | `orbcount = GameGetOrbCountThisRun() + NEW_GAME_PLUS_COUNT`, stored in the boss's `orbcount` variable "for savegames" (`:49`, re-read on load `:112-113`). HP `= 46 + 2^(orbcount+1.3) + 15.5*orbcount` (`:52-57`); weak shield at 0 orbs, else strong (`:60-64`); resistances at ≥3, 4, 5, 7, 9, 11 orbs (`:69-95`); material weaknesses at exactly 12..16 (`:99-107`); touch-spell immunity above 30 (`:110-111`) |
| Kolmi melee volley | `entities/animals/boss_centipede/melee_shots.lua:6-8` | `8 + floor(orbs/2)*2` projectile branches |
| Kolmi gold statue | `entities/animals/boss_centipede/rewards/gold_reward.lua:4-5` | `8000 * (orbs+1)` gold |
| Kolmi rewards | `entities/animals/boss_centipede/rewards/spawn_rewards.lua:11,106-109` | crown statue at ≥12 orbs |
| Sampo name | `entities/animals/boss_centipede/sampo_init.lua:3-16` | item name/description `$item_mcguffin_<0..13>`, `_33` above 33 |
| Ending / NG+ | `entities/animals/boss_centipede/ending/sampo_start_ending_sequence.lua:13,48-83,98-...` | NG+ if `orbs >= 5 + NG count` (rules at `:51-54`) and the Sampo is at the mountain top; calls `GameClearOrbsFoundThisRun()` (`:68`). ≥33 orbs: secret endings (`:98-140`). See §7 |
| Kolmi's "hearty" halving | `scripts/status_effects/hearty_start.lua:19` | not applied to Kolmi at ≥33 orbs |
| Temple fish tank | `scripts/biomes/temple_altar.lua:253-259` | one fish per **all-time** orb (profile) |
| Progress menu | `scripts/init.lua:217-236` | all-time count (profile) |

### 2.4 Co-op analysis (R2.1, R2.4, R1.4.1)

- **Count:** the shared counter is `orbs_found_thisrun`, which only the engine writes. Pickup on one machine
  updates only that machine's world state. Options: (i) every client performs a real orb pickup for the
  orb id when it applies the event (spawn the orb entity next to the local player and call
  `GamePickUpInventoryItem`, `API:124`; this keeps the engine's own bookkeeping) [INFERENCE]; (ii) keep the
  counter only on the host and override the readers (Kolmi, Sampo, rewards). Option (i) touches fewer
  scripts. Option (ii) is cleaner but has to patch every reader in §2.3.
- **Rewards are per-profile.** Whether the picker gets a spell or a heart depends on **the picker's**
  all-time orb history (`orb_pickup.lua:32`). Two players with different profiles would see different
  outcomes. Proposal: the host decides the reward kind once and records it with the pickup event. The spell
  falls under the unique-item policy (§7.5, N or 1). The heart becomes a normal heart event (D6).
- **Persistent flags** (`progress_orb_*`, achievement) are per profile. Apply them on every client
  (cosmetic) or only on the picker's (Q22).
- **Kolmi** freezes its orb count at first initialisation (`boss_centipede_update.lua:40-49`). The shared
  count must be in place before any client loads Kolmi.
- **Hooks:** (a) detect: `ModLuaFileAppend("data/scripts/items/orb_pickup.lua", ...)` wrapping
  `item_pickup`; also `orb_07_trap.lua` for orb 7. (b) **Native suppression cannot rely on that wrapper
  alone**: `enable_orb_hacks` may already have updated the counter when `item_pickup` runs [INFERENCE].
  Make replica orbs non-pickable **before contact** (or remove their native orb-hack path), use a custom
  interaction to send `OrbClaim{epoch,orb_id,world_index,picker}`, and only enable/execute the native pickup
  on accepted commit. (c) Host log `OrbPicked{id}`; each client marks the orb found (option i or ii) and
  removes its world copy. Gate reward/ambush callbacks separately so replaying native bookkeeping does not
  generate extra hearts, spells or enemy ambushes.

## 3. Hearts

### 3.1 Variants (exact effects)

All heart entities are tagged `drillable,hittable,teleportable_NOT,item_pickup`, have `auto_pickup="1"`,
`SimplePhysicsComponent`, and **no `LifetimeComponent`** (`entities/items/pickup/heart.xml:1-34`;
`heart_better.xml:1,25-33`; `heart_evil.xml:1,25-33`; `heart_fullhp.xml:1,26-34`).

| Entity | Script | Max HP change | Heal | Side effects |
|---|---|---|---|---|
| `heart.xml` | `scripts/items/heart.lua:3-44` | `max_hp += 1 * HEARTS_MORE_EXTRA_HP_MULTIPLIER` (`:10,18`), clamped to `max_hp_cap` if `> 0` (`:20-23`); writes `max_hp_old`, `max_hp`, `mLastMaxHpChangeFrame` (`:26-28`) | **none**; current `hp` is unchanged (`:25` is commented out) | log "+N" or "blocked" if capped (`:34-39`); music cue (`:40`) |
| `heart_better.xml` | `scripts/items/heart_better.lua:3-43` | `+2 * multiplier` (`:18`) | none | same messages (`:34-38`) |
| `heart_evil.xml` | `scripts/items/heart_evil.lua:3-47` | `+2 * multiplier` (`:18`) | none | if max HP grew, attaches `data/entities/misc/effect_poison_big.xml` to the picker (`:36-40`) |
| `heart_fullhp.xml` | `scripts/items/heart_fullhp.lua:3-28` | none | `hp = max_hp` (`:10-20`) | |
| `heart_fullhp_temple.xml` | Base `heart_fullhp.xml`, script overridden (`heart_fullhp_temple.xml:3-9`); `scripts/items/heart_fullhp_temple.lua:3-40` | `max_hp += 0.4` (+10 HP) (`:6,18`); if `max_hp_cap > 0`, `cap = max(max_hp, cap)` (`:20-22,27`) | `hp = max_hp` (`:29`) | It also has `CollisionTriggerComponent required_tag="polymorphed_player"` and `script_collision_trigger_hit` pointing to the same file (`heart_fullhp_temple.xml:11-24`). The file defines no `collision_trigger()`, so the trigger does nothing [INFERENCE]. |

`HEARTS_MORE_EXTRA_HP_MULTIPLIER` is a **Globals** value (world-global, not per player). The
`HEARTS_MORE_EXTRA_HP` perk sets it to 1 → 2 → 3, then +0.25 per pick up to 4 (`perk_list.lua:647-671`).
`max_hp_cap` is set by `GLASS_CANNON` (max_hp = cap = 2, `perk_list.lua:684-697`; reset `:718-731`). Temple
hearts are the only thing that raises the cap.

### 3.2 Spawn sites

| Source | Where | Seed |
|---|---|---|
| Biome `spawn_heart(x,y)` | `scripts/biome_scripts.lua:144-180`: `r = ProceduralRandom(x,y)`; heart if `r > 0.7` (0.35 on 14 February, `:149-150`); if `0.3 < r <= 0.7`: chest / great chest (1/1000) / mimic / leggy chest, seeded `SetRandomSeed(x+45, y-2123)` (`:155-178`) | position + world seed, deterministic |
| Biome spawn tables with `heart.xml` entries | `scripts/biomes/endgame.lua:152`, `hills.lua:119`, `lake.lua:157`, `lake_deep.lua:120`, `mountain/mountain_center.lua:147`, `mountain_tree.lua:155`, `pyramid.lua:146`, `winter.lua:211` | wang/biome spawn |
| Stash | `scripts/director_helpers.lua:137-145` (`entity_load_stash` → `heart.xml`) | position |
| Fixed | `scripts/biomes/watercave.lua:70,75` (`heart` + `heart_fullhp`); `scripts/biome_scripts.lua:211-213` (`spawn_fullhp`) | fixed |
| Holy Mountain | `heart_fullhp_temple.xml`: `scripts/biomes/temple_altar.lua:134`, `temple_altar_left.lua:166`, `temple_altar_right.lua:112`, `temple_altar_right_snowcastle.lua:112`, `temple_altar_right_snowcave.lua:113`, `temple_altar_secret.lua:58`, `shop_room.lua:54`, `boss_arena.lua:156` | fixed (§6) |
| Bosses | `boss_alchemist/death.lua:26`, `boss_fish/death.lua:8`, `boss_ghost/death.lua:7`, `boss_pit/boss_pit_death.lua:20` (`heart_fullhp`); `boss_limbs/boss_limbs_death.lua:10` (`heart`) and `:31` (`heart_fullhp`); `maggot_tiny/death.lua:9-10` (`heart_better` + `heart_fullhp`); `scripts/animals/boss_dragon_death.lua:9` (`heart`); `scripts/animals/worm_death.lua:11` (`heart`) | §5 |
| Orb pickup | `scripts/items/orb_pickup.lua:30` (`heart_evil` for `orb_id >= 100`), `:39` (`heart` if the orb was **already collected in any earlier run on this profile** or in a daily run) | per-profile |
| Chests | `scripts/items/chest_random.lua:261-267`, `chest_random_super.lua:89-93` | §4 |
| Charmed mimic | `scripts/animals/mimic_charm.lua:47-48` (1/1000) | frame-based |
| Not spawned | `the_end.lua:379` defines `spawn_heart` as a no-op. `temple_altar_left.lua:167` and `temple_altar_right.lua:113` load `heart_refresh.xml`, which **does not exist** under `entities/items/pickup/` [INFERENCE: dead code]. | |

### 3.3 Other HP sources (perk-based)

| Source | Effect |
|---|---|
| `EXTRA_HP` (`perk_list.lua:583-611`) | `max_hp *= 1.5` (cap-clamped); heals by the increase |
| `VAMPIRISM` (`perk_list.lua:556-581`) | `max_hp *= 0.75`; game effect `HEALING_BLOOD` |
| `NO_WAND_EDITING` (`perk_list.lua:1040-1068`) | +20 % global chance of `bloodmoney_50` per kill (heals 0.2 = 5 HP on pickup) |
| `TRICK_BLOOD_MONEY` (`perk_list.lua:125-151`) | blood money from trick kills; heals `hp_value*0.5` |
| `RESPAWN` (`perk_list.lua:754-767`) | engine `RESPAWN` effect, `do_not_remove` |

### 3.4 Co-op analysis (D6, Q12, R2.2)

- Max-HP hearts are **additive** (+1, +2, +0.4 internal units) and use the picker's cap and the global
  multiplier. "Shared effect" therefore means: one claim, then apply the same delta to each player's own
  `DamageModelComponent`, using that player's cap and multiplier. Order matters relative to multiplicative
  perks (`EXTRA_HP` ×1.5, `VAMPIRISM` ×0.75), so the ledger must be applied in event order, and an offline
  player must replay missed hearts before taking new perks.
- With personal perks (D5), `HEARTS_MORE_EXTRA_HP_MULTIPLIER` cannot stay in Globals. Store it per player
  (e.g. a `VariableStorageComponent` on the player) and read it in the wrapped heart scripts.
- `heart_evil` adds poison to the picker. Decide whether the poison is shared together with the +max HP.
- `heart_fullhp` (non-temple) is a pure heal. `heart_fullhp_temple` mixes +0.4 max HP with a full heal.
  Q12's recommendation (max HP shared, heal personal) splits the temple heart into two effects.
- Orb pickups turn into hearts based on the **picker's profile** (`orb_pickup.lua:32`). This
  per-machine input must be fixed by the host (§2).
- **Hooks:** (a) `ModLuaFileAppend` on each of the five heart scripts, wrapping `item_pickup`.
  (b) In the wrapper skip the `DamageModelComponent` writes and send `HeartPickup{uid, kind, picker}`.
  (c) Host ledger `HeartEffect{id, kind, delta_max_hp, heal}`; each client applies it to its own player
  once. Identity: a generated marker can use `(run_id,world_epoch,spawn_fn,marker_x,marker_y,spawn_index)`,
  not a position/time heuristic. Include NG+ epoch; runtime hearts (bosses, orbs, chests, mimics) receive a
  monotonic uid from the spawning authority. The heart's origin and the event id are not interchangeable.

## 4. Chests

### 4.1 Common mechanics (`chest_random`, `chest_random_super`, `utility_box`)

- **Entity.** `entities/items/pickup/chest_random.xml`: tags `teleportable_NOT,item_physics,chest,item_pickup,
  effectable_prop` (`:1`), `PositionSeedComponent` (`:7`), a physics body made of `wood_prop` (`:9-28`), and
  an `ItemComponent` without `auto_pickup` and with `custom_pickup_string="$itempickup_open"` (`:30-36`).
  It has two `LuaComponent`s: `script_physics_body_modified=chest_random.lua execute_times="1"` (`:38-42`)
  and `script_item_picked_up=chest_random.lua` (`:44-46`). `chest_random_super.xml` has the same layout
  (`:1-46`) plus a particle emitter (`:57`).
- **Opening.** Either the player "picks up" the chest with the interact key (`item_pickup`, then `on_open`,
  then `EntityKill`; `scripts/items/chest_random.lua:351-362`), or the physics body is damaged or broken
  (`physics_body_modified`, then `on_open`, the `ItemComponent` is disabled, then `EntityKill`; `:364-376`).
  Both paths kill the chest, so `on_open` runs once per process. Two processes simulating the same chest can
  both open it [INFERENCE].
- **Seed.** `on_open` takes the **spawn position** from `PositionSeedComponent` (falls back to the current
  position), adds `+509.7/+683.1` so the roll is decorrelated from `spawn_heart`'s 0.3-0.7 window, and calls
  `SetRandomSeed(rand_x, rand_y)` (`chest_random.lua:317-334`). Deferred items are loaded at `(rand_x, rand_y)`
  first and then moved, so their own `SetRandomSeed` calls are also position-deterministic (`:70-73, 289-296`).
  `chest_random_super.lua` seeds `SetRandomSeed(rand_x, rand_y)` **without** the offset (`:137-149`).
  `utility_box.lua` uses the offset (`:188-205`). The `set_rnd` flag that would seed from
  `GameGetFrameNum()` is passed `false` everywhere (`chest_random.lua:342`, `chest_random_super.lua:157`).
- **Hidden per-profile input.** `make_random_card` keeps drawing `Random(1, #actions)` until it hits a spell
  whose `spawn_requires_flag` is a persistent flag the **local profile** has (`chest_random.lua:7-44`;
  `utility_box.lua:8-48` for utility/modifier spells). The die option needs persistent
  `card_unlocked_duplicate` (`chest_random.lua:178-189`). So the same chest yields different spells, and
  shifts every later draw, on machines with different unlocks.
- **Gold delay.** Auto-pickup drops get `next_frame_pickable = now + 30` (`chest_random.lua:46-56, 298-305`).

### 4.2 Drop tables

`chest_random.lua` `drop_random_reward` (`:60-310`). One draw `Random(1,100)` per pass; `count` starts at 1:

| Roll | Result | Lines |
|---|---|---|
| 1-7 | `bomb_small.xml` ("bad" chest effect) | `:81-87` |
| 8-40 | gold: `amount` x `goldnugget.xml` (7 at ≤80, 10 at ≤95, 20 at ≤100; 5 is unreachable) plus one extra roll: 31-80 → 1 x 50; 81-95 → 1 x 200 (also 0-30 lands here because of the `elseif` order); 96-99 → 1 x 1000; 100 → 1-3 x 50, 50 % 1-3 x 200, 20 % 1-3 x 1000. Timer removed if `perk_gold_is_forever` | `:89-141` |
| 41-50 | potion 94 %, powder stash 4 %, `potion_secret` / `potion_random_material` 2 % | `:142-158` |
| 51-54 | `spell_refresh.xml` 98 %, else `illusions/shaman_wind.xml` | `:159-168` |
| 55-60 | one of `safe_haven`, `moon`, `thunderstone`, `evil_eye`, `brimstone`, random runestone, die (or greed die; potion without the unlock), gold orb (greed variant) | `:170-204` |
| 61-65 | 1-5 random spells (`make_random_card`) | `:206-229` |
| 66-84 | wand: `wand_level_01` 25 %, `unshuffle_01` 25 %, `level_02` 25 %, `unshuffle_02` 15 %, `level_03` 6 %, `unshuffle_03` 2 %, `level_04` 1 %, `unshuffle_04` 1 % | `:230-253` |
| 85-95 | `heart.xml` 89 % (rolls 0-88), `illusions/dark_alchemist.xml` 1 %, `heart_better` 10 %, `heart_fullhp` 1 % | `:254-268` |
| 96-98 | `EntityConvertToMaterial(chest, "gold")` | `:269-273` |
| 99 | roll twice more (`count += 2`) | `:274-281` |
| 100 | roll three more times | `:282-286` |

`chest_random_super.lua` `drop_random_reward` (`:5-130`):

| Roll | Result | Lines |
|---|---|---|
| first `Random(0,100000) >= 100000` (1 / 100 001) | **`EntityLoadEndGameItem(sampo.xml)`** and nothing else | `:22-26` |
| 1-10 | 2 x potion + `potion_secret` (30 %) or 2 x `potion_secret` + `potion_random_material` | `:33-46` |
| 11-15 | `projectiles/rain_gold.xml` (the only gold; `drop_money()` is empty, `:132-135`) | `:47-48` |
| 16-18 | `waterstone` (29/30) or `poopstone` (1/30) | `:49-56` |
| 19-39 | wand `level_04`/`unshuffle_04`/`level_05`/`unshuffle_05`/`level_06`/`unshuffle_06`/`level_06`/`level_10` with 25/25/25/15/6/2/1/1 % | `:57-81` |
| 40-60 | `heart` 90 %, `heart_better` 10 %, `heart_fullhp` 1 % | `:82-94` |
| 61-98 | roll twice more | `:95-102` |
| 99-100 | roll three more times | `:103-106` |

`utility_box.lua` (`entities/items/pickup/utility_box.xml`; spawned by `scripts/biomes/meat.lua:538`,
`vault_frozen.lua:701`, and the frame-seeded rain `scripts/buildings/spawn_utility_boxes.lua:8`):
1-2 bomb, 3-5 spell refresh, 6-11 misc item, 12-97 2-6 utility/modifier spells, 98-99 roll twice more,
100 roll three more times (`:52-186`).

### 4.3 Where chests come from

| Spawner | Lines | Seed |
|---|---|---|
| `spawn_heart` (all normal biomes) | `scripts/biome_scripts.lua:155-178`: when `0.3 < ProceduralRandom(x,y) <= 0.7`: 90 % (always above depth `512*3`) chest, with great chest at `Random(1,1000) == 1000`; else 95 % `chest_mimic.xml`, 5 % `chest_leggy.xml`; mimic sign 1/300 or 1/30 | `SetRandomSeed(x+45, y-2123)` |
| Mines start chest and similar | `scripts/biomes/coalmine.lua:1193-1206` (great chest if `Random(1,2000) >= 1999`, `>= 99` of 100 with greed); `tower.lua:798-800`; `the_end.lua:444-446` | `SetRandomSeed(x, y)` |
| Puzzles | `scripts/buildings/oiltank_puzzle_check.lua:7`, `swing_puzzle_target.lua:9`, temple stats check `workshop_trigger_check_stats.lua:32-33` (§6) | event-driven |
| Bosses / dice | `entities/animals/boss_fish/death.lua:9` (great chest); greed die "good" 2 x great chest (`scripts/items/greed_die_status.lua:101-102`); `scripts/projectiles/all_spells_stage.lua:69` | §5 / frame-based |
| Chest rain (altar tablet) | `scripts/buildings/spawn_chests.lua:6-18`: 1-3 chests 300 px above | `SetRandomSeed(GameGetFrameNum(), pos_x+pos_y+entity_id)`, frame-based |

### 4.4 Special chests

| Chest | Entity / script | Opening | Content | State |
|---|---|---|---|---|
| Mimic | `entities/animals/chest_mimic.xml` (enemy, `Base base_enemy_basic.xml`, `:2`) | it is a monster; dies like any enemy (gold via `drop_money.lua`) | when charmed: gifts via `scripts/animals/mimic_charm.lua` (§1.4) | - |
| Leggy chest | `entities/items/pickup/chest_leggy.xml` (`CollisionTriggerComponent required_tag="player_unit"` radius 30, `:46-53`) | pickup, physics damage **or a player within 30 px** → `scripts/items/chest_leggy.lua:3-24` replaces it with `entities/animals/chest_leggy.xml` | death: perk `LEGGY_FEET` with `perk_dont_remove_others` (`scripts/animals/chest_leggy_death.lua:4-17`) | - |
| Steel chest | `entities/buildings/chest_steel.xml` (`:38-44`), spawned at `scripts/biomes/robot_egg.lua:186` | pickup or physics | spell `ALL_SPELLS`; persistent `card_unlocked_everything` (`scripts/buildings/chest_steel.lua:3-13`) | no RNG |
| Dark chest | `entities/buildings/chest_dark.xml` (`script_source_file`, `:38`), at `scripts/biomes/alchemist_secret.lua:34` | polls for an `alchemist_key` entity within 24 px whose `status == 2` (`scripts/buildings/chest_dark.lua:6-26`) | first time on the profile: `ALL_ACID`, `ALL_NUKES`, `ALL_DISCS`, `ALL_ROCKETS`, `ALL_BLACKHOLES`, `ALL_DEATHCROSSES` and persistent `card_unlocked_alchemy`; later: 3 random of those (`:27-49`); persistent `secret_chest_dark`; key and chest killed (`:51-58`) | `SetRandomSeed(x, y * GameGetFrameNum())` (`:41`), frame-based |
| Light chest | `entities/buildings/chest_light.xml` (`:37-40`), at `scripts/biomes/song_room.lua:34` | `alchemist_key` within 24 px with `music_machine == 4`, i.e. the key "heard" all four music machines (§7.1) (`scripts/buildings/chest_light.lua:6-26`) | first time: `DIVIDE_2`, `DIVIDE_3`, `DIVIDE_4`, `BURST_8`, `BURST_X` and persistent `card_unlocked_musicbox`; later 3 random of those (`:27-48`); persistent `secret_chest_light` (`:50`) | `SetRandomSeed(x, y * GameGetFrameNum())` (`:40`) |
| Darkness chest | `biome_impl/static_tile/chest_darkness.xml:38-39`, spawned by `biome_impl/static_tile/puzzle_logic_darkness.lua:9` | pickup or physics | spell `TOUCH_GRASS`, persistent `card_unlocked_touch_grass` (`chest_darkness.lua:4-9`) | none |

### 4.5 Co-op analysis (R4 "chests", Q11)

- Ordinary and great chests are **position-seeded**, so N copies "as if each player opened their own chest"
  can be produced deterministically by re-running `drop_random_reward` with seeds derived from the base seed
  and the player index, e.g. `SetRandomSeed(rand_x + k*7919, rand_y)`. Q11 recommends "different seeds".
  Per-profile spell unlocks must be fixed first: either run the roll on the host's profile only, or treat
  every spell as unlocked. Otherwise the result is not reproducible.
- **Hooks.** (a) Detect: `ModLuaFileAppend` on `chest_random.lua`, `chest_random_super.lua`,
  `utility_box.lua` wrapping `on_open(entity_item)` (covers both opening paths).
  (b) Suppress: replicas send `ChestOpened{chest_uid}`; only the accepted host transaction produces drops.
  (c) N-fold: **roll a reward manifest, not N whole callbacks**. The base manifest's gold/material-gold,
  hearts and orbs are shared entries, counted **once**. Independent slot rolls may create extra
  equipment/spells/potions, but must not add gold, hearts or orbs again. Deduplicate shared outputs across
  all batches. Great-chest Sampo is also one quest object, not N.
- Deterministic special-chest rewards (steel, dark/light first-unlock sets, darkness) retain their exact
  spell ids for every copy. A slot-derived seed may vary random stats/choices where vanilla is random, but
  must not reroll a guaranteed unique spell out of existence.
- Dark/light chest scripts are **file-scope polling**, so an append after execution cannot suppress the
  first event. Replace their `LuaComponent.script_source_file` or rewrite the source at init, keeping the
  key/progression consumption once and only multiplying the reward manifest.
- A chest converted to gold (`EntityConvertToMaterial`, roll 96-98) creates terrain pixels once. The
  subsequent native powder pickup follows §1.8, not the nugget Lua hook.

## 5. Bosses and their drops

Internal filenames are authoritative. In particular, **`boss_centipede` is Kolmisilmä** (main boss);
`boss_limbs` is the pyramid boss, **not** Kolmi; `boss_wizard` is the master-wizard encounter
(`book_mestari` drop). `boss_spirit/islandspirit` is a different encounter. UI strings are
`$animal_boss_centipede`, `$animal_boss_limbs`, `$animal_boss_wizard`, `$animal_islandspirit`
(`entities/animals/boss_centipede/boss_centipede.xml:1`, `boss_limbs/boss_limbs.xml:1`,
`boss_wizard/boss_wizard.xml:1`, `boss_spirit/islandspirit.xml:1`).

### 5.1 Spawn sites and repeat guards

| Boss | Spawn site | Explicit guard |
|---|---|---|
| Dragon (`entities/animals/boss_dragon.xml`) | `scripts/biomes/rainforest.lua:967-968`, `dragoncave.lua:618-619` load `buildings/dragonspot.xml`; its source `scripts/buildings/dragonspot.lua:6-13` loads dragon and attaches the reward death callback. Other egg paths: `egg.lua:6-9`, `egg_damage.lua:34-37`, `end_egg.lua:48-50` | `egg_damage.lua:8-15`, `end_egg.lua:8,48` use Globals `boss_dragon_spawned` and spawn x/y. `egg.lua` has a frame/entity seed and 60/101 chance; its reward attachment is not established. |
| Alchemist (`boss_alchemist/boss_alchemist.xml`) | `scripts/biomes/secret_lab.lua:30` | none in the spawn callback; relies on chunk/entity save |
| Pyramid (`boss_limbs/boss_limbs.xml`) | `scripts/biomes/pyramid.lua:687-688` creates trigger; `boss_limbs_arena.lua:29-34` loads boss only if `floor(x/512)==1` | position guard |
| Pit (`boss_pit/boss_pit.xml`) | `scripts/buildings/orb_07_pitcheck_b.lua:9`; optional `boss_pit_spawner.lua:3-12` | optional spawner: `SetRandomSeed(x,y)`, `Random(1,10)==5` |
| Ghost (`boss_ghost/boss_ghost.xml`) | `scripts/biomes/ghost_secret.lua:30` creates `ghost_spawn_check.xml`; every 36 frames it runs `spawn_ghost.lua`, which spawns at player distance <300 px then kills itself (`spawn_ghost.lua:7-11`) | one-shot trigger entity |
| Robot (`boss_robot/boss_robot.xml`) | `scripts/biomes/roboroom.lua:34` | no Lua guard |
| Meat (`boss_meat/boss_meat.xml`) | `scripts/biomes/meatroom.lua:34` | no Lua guard |
| Wizard (`boss_wizard/boss_wizard.xml`) | `scripts/biomes/mestari_secret.lua:30` | no Lua guard |
| Fish (`boss_fish/fish_giga.xml`) | fixed `(-14000,10000)` (`biome/_pixel_scenes.xml:248`, NG+ `..._newgame_plus.xml:57`) | scene generation |
| Tiny (`maggot_tiny/maggot_tiny.xml`) | fixed maggotspot `(14941,16454)` (`_pixel_scenes.xml:246`); `scripts/buildings/maggotspot.lua:6-12` triggers within 220 px then kills itself | one-shot trigger |
| Gate monsters A-D (`boss_gate/gate_monster_*.xml`) | `scripts/buildings/wizardcave_gate.lua:91` loads spawner; four `LoadEntitiesComponent`s name A/B/C/D (`entities/buildings/wizardcave_gate_monster_spawner.xml:27,42,57,72`) | gate state + spawner entity |
| Island spirit (`boss_spirit/islandspirit.xml`) | `scripts/biomes/lake_statue.lua:281` loads spawner; `boss_spirit/spawner.lua:11-15` requires player within 200, `HELPLESS_KILLS>=30`, `ISLANDSPIRIT_SPAWNED==0`, sets that Global then loads spawn portal | explicit global once-per-run guard |
| Sky (`boss_sky/boss_sky.xml`) | `biome_impl/static_tile/temples_common.lua:387`; phase-2 markers `:394` | scene generation |
| Kolmi (`boss_centipede/boss_centipede.xml`) | `scripts/biomes/boss_arena.lua:133-140`: boss and reference always; Sampo only without run `ending_game_completed` | Sampo flag, not a boss spawn guard |

Most `miniboss_*` flags below are **persistent profile unlocks**, not run spawn guards. Do not assume that
writing `miniboss_alchemist` means a later biome callback will skip spawning it; the callback does not check it.

### 5.2 Death and reward table

`pw = check_parallel_pos(x)` is the parallel-world x index from `GetParallelWorldPosition(x,0)`
(`scripts/lib/utilities.lua:906-914`). Seeds `(pw, constant)` are therefore the same for that boss everywhere
in the **same parallel world**, not based on exact death position. Wands have their own later seed.

| Boss / callback wiring | Exact scripted drops | RNG / state |
|---|---|---|
| **Dragon**: attached `script_death=scripts/animals/boss_dragon_death.lua` (`dragonspot.lua:6-13`) | `heart.xml`, `wand_unshuffle_06.xml`, orbit spells from `ORBIT_DISCS/FIREBALLS/NUKES/LASERS/LARPA` (`boss_dragon_death.lua:9-30`) | `SetRandomSeed(pw,540)` (`:15-16`). Initially count 3, but persistent `card_unlocked_dragon` is written (`:13`) **before** checking it (`:18`), so the already-unlocked branch (one card, no nukes) appears unconditional [INFERENCE]. Persistent `miniboss_dragon`. |
| **Alchemist**: `boss_alchemist/death.lua`, XML `:114-117` | 4 cards without replacement from `ALPHA,OMEGA,GAMMA,MU,ZETA,PHI,TAU,SIGMA`; full-heal heart only if profile `card_unlocked_duplicate` absent; always one `boss_alchemist/key.xml` (`death.lua:6-29`) | `SetRandomSeed(pw,60)`, one discarded draw before the four actual draws (`:8-24`); persistent `card_unlocked_duplicate`, `miniboss_alchemist` (`:31-32`) |
| **Pyramid / limbs**: `boss_limbs/boss_limbs_death.lua`, XML `:48-53`; wait-for-kill flag (`:129`) | `heart.xml`, `wand_unshuffle_04.xml`, 4 without replacement from `NOLLA,DAMAGE_RANDOM,RANDOM_SPELL,RANDOM_PROJECTILE,RANDOM_MODIFIER,RANDOM_STATIC_PROJECTILE,DRAW_RANDOM,DRAW_RANDOM_X3,DRAW_3_RANDOM`; full heal only if `card_unlocked_pyramid` absent (`death.lua:7-34`) | `SetRandomSeed(pw,44)` (`:13-29`); persistent `card_unlocked_pyramid`; update-side death sets `miniboss_limbs` and `kill_now` (`boss_limbs_update.lua:268-311`); choreography frame-seeded (`:289`) |
| **Pit**: `boss_pit/boss_pit_death.lua`, XML `:63` | **both** `WORM_RAIN`, `METEOR_RAIN`; `wand_unshuffle_05.xml` and `wand_level_06.xml`; full heal if profile `card_unlocked_rain` absent (`:6-24`) | `SetRandomSeed(pw,120)` (`:8-20`); persistent `card_unlocked_rain`, `miniboss_pit` (`:26-27`) |
| **Ghost**: `boss_ghost/death.lua`, XML `:409` | `heart_fullhp.xml`, `sun/sunseed.xml` (`death.lua:7-8`) | no direct RNG; persistent `miniboss_ghost` (`:10`). XML also configures `ItemChestComponent level=4 enemy_drop=1` (`boss_ghost.xml:2`), a separate native loot layer [INFERENCE on enabled state]. |
| **Robot**: `boss_robot/death.lua`, XML `:209` | perk `MAP` (Spatial Awareness) (`death.lua:10`) | no direct RNG; one `explosion_giga.xml`, persistent `miniboss_robot` (`:8-11`) |
| **Meat**: `boss_meat/death.lua`, XML `:149` | `wands/experimental/experimental_wand_4.xml` (`death.lua:10`) | no direct RNG, but wand seeds `(x,y+GameGetFrameNum())` (`experimental_wand_4.lua:31-33`); persistent `miniboss_meat`, Globals `BOSS_MEAT_DEAD=1`, disables every `effect_no_heal_in_meat_biome` (`death.lua:8-19`) |
| **Wizard**: `boss_wizard/death.lua`, XML `:205` | all five `ADD_TRIGGER,ADD_TIMER,ADD_DEATH_TRIGGER,RESET,DUPLICATE`; `book_mestari.xml`, `wandstone.xml`; bonus `HOMING_WAND` only if `damage_message=="$damage_drowning"` and liquid is `"swamp"` (`death.lua:12-35`) | `SetRandomSeed(pw,30)` (`:9-15`), main cards fixed; persistent `card_unlocked_mestari`, `miniboss_wizard`, conditional `card_unlocked_homing_wand` |
| **Fish**: `boss_fish/death.lua`, `fish_giga.xml:285` | `heart_fullhp.xml`, `chest_random_super.xml` (`death.lua:8-9`) | no direct RNG; **water→smoke globally**, one `teleport_teleroom.xml`, run + persistent `miniboss_fish` (`:7-13`) |
| **Tiny**: `maggot_tiny/death.lua`, XML `:275` | `heart_better.xml`, `heart_fullhp.xml`, `wand_unshuffle_06.xml`, `wand_unshuffle_10.xml` (`death.lua:9-12`) | no direct RNG; run + persistent `miniboss_maggot`, persistent `card_unlocked_maggot` (`:14-18`) |
| **Gate A-D**: `boss_gate/gate_monster_death.lua`, A XML `:223` (B/C/D inherit A, their XML `:1-2`) | one `make_random_card` each (`death.lua:22`) | `SetRandomSeed(offset,10+pw)`, offset A=0/B=1/C=2/D=3 (`:8-20`); run `$animal_gate_monster_<a/b/c/d>_killed`, persistent `miniboss_gate_monsters` only when all four set (`:25-32`) |
| **Island spirit**: same `islandspirit.lua` serves update / damage / death (`islandspirit.xml:164-170`) | one `MASS_POLYMORPH` (`islandspirit.lua:20`) | no direct RNG; persistent `card_unlocked_polymorph`, `miniboss_islandspirit`; Globals `BOSS_SPIRIT_DEAD=1`; persistent `miniboss_threelk` at `HELPLESS_KILLS>=300` (`:3-29`). Damage callback retaliates against **all** players (`:3-13`). |
| **Sky**: same `boss_sky.lua` source / damage / death (`boss_sky.xml:221-227`) | no explicit item drop: creates `apparition_spawn_fx.xml` at each phase-2 marker (`boss_sky.lua:44-54`); after 160 frames/removal calls native `SpawnApparition(x,y,0,true)` (`:85-104`; FX XML `:31-39`) | persistent `miniboss_sky`, kill log; boss/apparition HP uses **`players[1].max_hp`** (`:6-13,89-104`); native apparition RNG/contents unverified; native chest disabled/zeroed and inherited gold callback removed (`boss_sky.xml:48-53,161-164`) |
| **Kolmi**: update-side `check_death`, not the XML death callback; see below | body chunks, victory portal; native `ItemChestComponent level=4 enemy_drop=1` (`boss_centipede.xml:2`) [INFERENCE on enabled state]. **Sampo is not a death drop.** | see §5.3 |

**Independent gold/native layers.** Most bosses inherit `base_enemy_basic/flying` → `base_humanoid`'s
`drop_money.lua` in addition to their custom death callback (§1.3; `base_humanoid.xml:9-21`). Therefore
suppressing custom boss loot alone does not suppress gold. Conversely, N-fold custom loot must **not** run
`drop_money` N times (R5). `ItemChestComponent` is another independent native layer (exact native drop table,
seed and callback ordering unverified). Disable it on replicas or replicate the host's actual produced items;
do not infer a loot list from `level=4`.

Dropped procedural wands reseed `SetRandomSeed(x,y)` in
`scripts/gun/procedural/gun_procedural.lua:968-971`:
unshuffle04=`generate_gun(80,4,true)`, unshuffle05=`(100,5,true)`, unshuffle06=`(120,6,true)`,
unshuffle10=`(180,11,true)` (**11**), level06=`(120,6,false)`
(`wand_unshuffle_04/05/06/10.lua:1-3`, `wand_level_06.lua:1-3`).

### 5.3 Kolmi death, Sampo and reward statues

- Init orb scaling is in §2.3: HP, shield and resistances are based on shared orbs + NG count and frozen
  into its `orbcount` storage. Phase RNG uses `SetRandomSeed(xe+xp+subphase,ye+yp)`
  (`boss_centipede_update.lua:373`).
- `check_death` (`boss_centipede_update.lua:672-766`) seeds
  `SetRandomSeed(GameGetFrameNum(),GameGetFrameNum())` (`:676`), waits for HP≤0, runs death choreography
  (`:712-749`), loads `teleport_ending_victory_delay.xml` (`:749`), sets `kill_now`,
  `FINAL_BOSS_ACTIVE=0`, increments `GLOBAL_BOSS_KILL_COUNT`, persistent `boss_centipede`, kill log
  (`:751-766`).
- XML `script_death=death_check.lua` (`boss_centipede.xml:225-229`) only checks whether a sun/dark sun
  (`seed_e` / `seed_f`) is within 540 px and sets run `sun_kill` (`death_check.lua:3-12`).
- Victory-room script imports `rewards/spawn_rewards.lua` (`scripts/biomes/boss_victoryroom.lua:7`).
  Reward eligibility is per-process stats and closest player's wallet (§1.4), while shared orbs determine
  the ordinary statue value. No reward selection RNG.
- Statue conditions: pacifist / almost-pacifist (`spawn_rewards.lua:58-67`), no projectiles but kicked
  (`:69-73`), no damage (`:75-81`), no wands / no edits (`:83-90`), ≤1 minute / ≤5 minutes (`:92-103`),
  ≥12 orbs (`:105-109`), zero lifetime gold (`:111-117`), ≥1M gold giant dollar / ≥500K gold statue
  (`:119-126`), sun kill (`:134-138`). If none, ordinary `gold_reward.xml` (`:128-130`).
  Statue pickup adds money directly (§1.4): default `8000*(orbs+1)`, explicit `end_gold` overrides
  70K..500K (`gold_reward.lua:4-19`; e.g. `reward_crown.xml:9-12` 350K).
- Sampo pickup and the ending transition are separate events (§7.4). `ending_game_completed` is a **run**
  flag; persistent `boss_centipede` only records profile progress.

### 5.4 Other boss-like encounters / internal names

| Entity | Behaviour and rewards |
|---|---|
| `friend.xml` (`big_friend`) | spawned by `scripts/biomes/friend_1..6.lua:52`; `BossHealthBarComponent` (`friend.xml:208-209`), `no_gold_drop` (`:211-213`); `friend_death.lua:3-13` only unlocks `final_secret_orb3` when `ULTIMATE_KILLER_KILLS>=9`. No item drop. Bringing it to the summit unlocks a spell (§7.5), not killing it. |
| `ultimate_killer.xml` (`small_friend`) | `no_gold_drop` (`:132`); `ultimate_killer_death.lua:3-12` creates explosion and increments Globals `ULTIMATE_KILLER_KILLS` (clamped 25); no item drop. |
| `worm_big.xml` | boss health bar (`:266-269`); `script_death=worm_death.lua` (`:280`). With `SetRandomSeed(frame,x+y+entity_id)`, 50 % `heart.xml` (`scripts/animals/worm_death.lua:8-12`). |
| `worm_end.xml` / `the_end/worm_end.xml` | no death callback or item-chest in the inspected XML; the_end variant has 250 internal HP (`:2-7`). Treat as ordinary mob until a separate source demonstrates unique drops. |
| `boss_book/book_physics.xml` | secret-altar entity (`scripts/biomes/secret_altar.lua:77-79`), only 0.4 HP, inherits robot base (`book_physics.xml:84-110`); periodic `scripts/animals/book_physics.lua` spawns up to 4 wand ghosts, frame/entity seeded (`:3-30`); no custom death reward. Internal `boss_` folder is not proof of a full boss encounter. |
| `ending_placeholder/*` | legacy placeholder assets; no active unique-drop path established [INFERENCE]. |

### 5.5 Co-op hooks (R1.2.1, R4, Q11)

1. **Detect:** append/wrap each exact `death()` above; for limbs and Kolmi also hook their update-side
   death functions. The host/encounter authority issues one `BossDefeated{boss_uid, killer_slot, loot_manifest}`.
2. **Suppress:** replicas must not run death rewards, inherited gold, native chest, portal creation or
   persistent counters. Remove/disable those components **before** lethal damage; a `death()` append is
   too late to stop an earlier independent gold component. Preserve ordinary visual death as a separate path.
3. **N-fold:** resolve first-kill/profile conditions **once before writing unlock flags**; create N total
   equipment/spell bundles (R4.1). Do not literally rerun the entire death callback N times: it changes
   flags, counters, materials, portal state, explosions and the eligibility of subsequent calls.
4. **Separate S outputs:** chest/boss multiplication excludes gold (D4/R5), and shared hearts/orbs are
   deduplicated across every personal reward batch. Equipment/spell copies retain guaranteed fixed ids.
   N-fold equipment never implies N conversions, portals, Sampo or quest keys. Fish's water→smoke and portal
   happen once; robot's explosion once; meat's healing restriction lift once; spirit retaliation once.
5. **Per-profile unlocks:** `card_unlocked_*`/`miniboss_*` writes are not authoritative run state. Decide Q22
   for granting discoveries to everyone; host eligibility must not change with which client happened to
   simulate a boss death.

## 6. Holy Mountain (temple)

### 6.1 Biome construction and variants

Each `biome/temple_altar*.xml:9` names its matching `scripts/biomes/temple_altar*.lua`. Scripts register
pixel-colour callbacks (`temple_altar.lua:11-28`) then `init(x,y,w,h)` loads the altar scene at
`(x, y-40+300)` (`temple_altar.lua:128-131`). `spawn_altar_top` seeds `SetRandomSeed(x,y)` and chooses water,
blood, oil, radioactive, lava or ordinary top (specific rolls 5/8/11/13/15 of 1..50); below `y>12000` it
uses the boss-arena top (`temple_altar_top_shared.lua:1-27`).

| Script | Relevant active functions |
|---|---|
| `temple_altar.lua` | heart/refresher (`:133-138`), combined shop (`:150-184`), workshop (`:186-192`), doors (`:194-200`), leak checkers (`:210-215`), worm deflector (`:217-221`), perks (`:223-239`), reroll (`:241-243`), pacifist-chest check (`:245-251`), spell visualizer + workshop AABB (`:265-268`) |
| `temple_altar_left.lua` | loads `altar_left.png` (`:159-163`); heart + obsolete `heart_refresh.xml` (`:165-168`); individual spell-shop spawns (`:170-178`); duplicator (`:208-210`); leak checkers (`:212-218`); statue is green with greed curse and spawns `greed_crystal.xml` (`:229-238`) |
| `temple_altar_right.lua` | `altar_right.png` + `altar_right_extra.png` (`:104-109`); same heart/shop (`:111-128`); exit trigger (`:156-158`); reroll (`:160-162`); statue (`:185-187`) |
| `temple_altar_right_snowcave.lua` / `...snowcastle.lua` | heart (`:112-114` / `:111-113`), exit, reroll (`:179-181` / `:180-182`); `spawn_all_perks` only prints an error — the apparent perk/guardian code is **commented out** (`:160-177` / `:160-178`). The real spawn uses `wang_scripts.csv:25` (`ffffff81,spawn_all_perks,...`) |
| `temple_altar_secret.lua` | heart, shop and workshop (`:49-85`); no perks |
| `*_empty.lua` | same scene with empty/no-op reward functions (e.g. `temple_altar_empty.lua:153-201`, `temple_altar_right_empty.lua:117-156`); some still spawn a reroll machine and visualizer (`temple_altar_empty.lua:201-202,221-223`, `...snowcave_empty.lua:154-155`) |
| `temple_wall.lua` | no rewards (`:10-27`) |
| `temple_wall_ending.lua` | ending top + solid scene (`:27-32`), ending portal (`:35-36`) |
| `boss_arena.lua` | temple-like shop, heart/refresher, workshop, perks before Kolmi (`:156,181,265-278`) |

`temple_random(x,y)` always returns `"1","1"` (shop and perks on), even when
`DESIGN_TEMPLE_CHECK_FOR_LEAKS` is enabled (`temple_shared.lua:74-86`). There is no random "missing shop"
or "missing perk" in the active code.

### 6.2 Perk spawning, selection, removal, reroll

- `spawn_all_perks` checks `TEMPLE_SPAWN_GUARDIAN`, spawning Steve or leaving a
  `workshop_guardian_spawn_pos.xml` marker (`temple_altar.lua:223-238`), then calls `perk_spawn_many`.
- `perk_spawn_many(x,y)` uses `TEMPLE_PERK_COUNT` (default 3), lays them out over width 60, and draws from the
  **run-wide deck** (`scripts/perks/perk.lua:485-535`). The deck uses `SetRandomSeed(1,2)` (world seed),
  shuffles all active perks, duplicates stackable perks and spaces duplicates by ≥4 slots, then replaces
  already-picked, non-stackable or maxed perks with `""` (`perk.lua:53-167`).
- `TEMPLE_NEXT_PERK_INDEX` advances from the front for normal spawns (`:499-530`);
  `TEMPLE_REROLL_PERK_INDEX` advances backwards from the deck end for rerolls (`:573-591`).
- `perk_spawn(x,y,id)` loads `perk.xml`, adds sprite/UI/`ItemComponent` and `VariableStorageComponent
  perk_id` (`:367-435`). The XML is only tags `perk,item_perk` and the pickup script
  (`entities/items/pickup/perk.xml:1-5`). `perk_pickup.lua:5-22` calls `perk_pickup`, unless the perk has
  `perk_dont_remove_others=true` (e.g. boss/leggy drops).
- Pickup bookkeeping and the global leakage are detailed in §8.1. At default
  `TEMPLE_PERK_DESTROY_CHANCE=100`, picking one deletes **all** loaded perk entities (`perk.lua:321-339`),
  not just the adjacent two, and disables reroll machines within 200 px (`:309-319,341-358`).
- Reroll machine: `entities/items/pickup/perk_reroll.xml` has `ItemCostComponent` (`:3`), manual pickup
  (`:17-24`), pickup Lua (`:41-44`), and init Lua (`:46-51`).
  `perk_reroll_init.lua:6-7` computes `cost = 200 * 2^TEMPLE_PERK_REROLL_COUNT`; the engine charges this
  via `ItemCostComponent` [INFERENCE]. On pickup `perk_reroll.lua:3-10` rerolls, kills the machine, loads FX
  and a fresh machine. `perk_reroll_perks` kills every loaded perk, increments the run-wide reroll count,
  then spawns a replacement at each old location (`perk.lua:539-596`). No frame-based RNG.
- `perk_spawn_random`, used by GAMBLE and runtime perk spawners, draws from the **same** front cursor
  (`perk.lua:437-467`; `scripts/perks/perk_gamble_spawn.lua:7-25` gives two perks to the closest player).

### 6.3 Shops

`temple_altar.lua:150-184`:

1. Create a `shop_hitbox.xml` (shop area) (`:156`).
2. `SetRandomSeed(x,y)`. `count = TEMPLE_SHOP_ITEM_COUNT` (default 5), width 132 (`:158-162`).
3. One `sale_item_i = Random(1,count)`.
4. `Random(0,100) <= 50`: two rows of `count` spells (10 total by default), lower row has one sale item;
   the upper row is not on sale (`:164-174`).
5. Otherwise `count` wands with one sale item (`:175-183`).

`scripts/items/generate_shop_item.lua`:

| Function | Seed and selection | Price |
|---|---|---|
| `generate_shop_item(x,y,cheap,biomeid,is_stealable)` (`:7-179`) | `SetRandomSeed(x,y)` (`:10`); depth `floor(y/512)` maps to level 0..6 (7 below depth 35), optional override (`:12-63`); `GetRandomAction(x,y,level,0)` (`:76`) | `max(floor((action.price*0.30 + 70*level²)/10)*10, 10)` (`:81`); half on sale (`:94-96`); ×5 when level² ≥10 (`:98-101`) |
| `generate_shop_wand(x,y,cheap,biomeid)` (`:183-306`) | `SetRandomSeed(x,y)` (`:185`); level 1..6; `Random(0,100)<=50` normal `wand_level_0L`, else `wand_unshuffle_0L` (`:235-247`) | `50 + 210*(0.5*L + 0.5*L²) + Random(-15,15)*10` (`:251-252`); half on sale |

Both add a `SpriteComponent` for price, an `ItemCostComponent` (`:145-149`, `:294-298`) and
`LuaComponent script_item_picked_up=shop_effect.lua` (`:151-153`, `:300-302`). The sale indicator is a
separate entity (`:105-107`, `:258-260`). Wands always `stealable=1`; cards only when passed
`is_stealable=true` (the combined temple shop does, `temple_altar.lua:167-172`). `ItemCostComponent.stealable`
means "will check that it's within an area called shop" (`CDOC:1374`). The engine performs the wallet
deduction and removes the cost after a purchase [INFERENCE]; `shop_effect.lua:3-7` only loads FX.
Moving an item outside the shop area bypasses its cost [INFERENCE: engine-side steal logic]. No script
connects stealing to Steve anger: **temple damage**, not shoplifting, spawns Steve.

Legacy entities `entities/items/shop_item.xml`, `shop_wand_level_01.xml`, `shop_potion.xml`, `shop_cape.xml`
inherit `base_shop_item.xml` (`ItemCostComponent cost=100`, `:4`). Main temples use the generator functions,
not those templates (`temple_altar.lua:140-148`).

`temple_duplicator.lua:4-74` (left-altar variant) creates a copy of the closest `card_action` spell and puts
it up for sale at `floor(action.price*1.2)` (`:23-29`), then kills the duplicator (`:73`). No RNG.

### 6.4 Heart, refresher, workshop, pacifist chest

- `spawn_hp` in the main altar spawns one `heart_fullhp_temple` at x−16, one `spell_refresh` at x+16, a music
  trigger, and `coop_respawn.xml` (`temple_altar.lua:133-138`). The `coop_respawn` entity is only an empty
  marker (`entities/buildings/coop_respawn.xml:1-3`), not a respawn system.
- Temple heart: +0.4 max HP **and** full heal (§3). `spell_refresh` calls
  `GameRegenItemActionsInPlayer(entity_who_picked)` then kills itself (`scripts/items/spell_refresh.lua:3-12`).
  Q12 recommends personal heal and refresh; the +0.4 max-HP portion remains shared.
- `workshop.xml` loads `workshop_allow_mods.xml` (`entities/buildings/workshop.xml:10-16`). The latter is an
  entity tagged `workshop` with a large hitbox (`:1-10`), which lets the engine allow wand editing inside it
  [INFERENCE].
- **Pacifist, not no-hit:** `workshop_trigger_check_stats.lua:3-38` checks
  `StatsBiomeGetValue("enemies_killed")==0` and creates `chest_random.xml` at the closest
  `workshop_reference`, renamed `$item_chest_treasure_pacifist` (`:18-34`). The biome stats are reset by the
  previous mountain's exit (`workshop_exit.lua:8-9`). These are **per-process stats**, so co-op needs a
  team/individual rule rather than taking the triggering client's statistic.

### 6.5 Collapse and anger (Steve / Skoude)

| Event | Hook / state / result |
|---|---|
| A player crosses the exit | `entities/buildings/workshop_exit.xml`: `CollisionTriggerComponent required_tag="player_unit" destroy_this_entity_when_triggered=1` (`:2-8`), Lua `workshop_exit.lua` (`:10-13`). The first player triggers it for all [INFERENCE]. |
| Collapse | `scripts/buildings/workshop_exit.lua:4-53`: resets biome stats, loads `workshop_collapse.xml` and two area-damage fields (`:9-13`), kills the closest three `workshop` entities and hint (`:17-30`), kills leak checkers within 2048 px at the same depth (`:34-41`), sets `TEMPLE_COLLAPSED_<id>=1` (`:46-47`), sets temple active=0 (`:49`). |
| Timed destruction | `workshop_collapse.lua:4-22`: coroutine: wait 40 frames, remove joints, wait 20 frames, load `loose_chunks_workshop.xml`. `altar_collapse.lua:4-15`: wait 60 frames, load `loose_chunks.xml`. |
| Final exit | `workshop_exit_final.lua:4-49`: similar, but sets `FINAL_BOSS_ARENA_ENTERED=1` (`:46`); no `TEMPLE_COLLAPSED` key. |
| Temple wall broken / leaks | `temple_areacheck_*` entities invoke `scripts/buildings/temple_check_for_leaks.lua:4-53` via `script_material_area_checker_failed`. Unless peace: spawn guardian once if `TEMPLE_SPAWN_GUARDIAN != 1`, set it to 1 (`:15-19`); set `TEMPLE_LEAKED_<id>=1` (`:51`); shake/message/achievement if this is the first leak in this temple (`:22-30`). |
| Guardian spawner | `temple_shared.lua:19-51` aborts when `TEMPLE_PEACE_WITH_GODS==1`; uses and kills the closest `guardian_spawn_pos` marker, then loads `spawn_necromancer_shop.xml` (`:26-45`). The spawner executes after 180 frames (`entities/misc/spawn_necromancer_shop.xml:50-54`). |
| Steve → Skoude | `scripts/animals/necromancer_shop_spawn.lua:6-10`: `STEVARI_DEATHS < 3` → `necromancer_shop.xml`, otherwise `necromancer_super.xml`. `necromancer_shop_death.lua:1-14` increments `STEVARI_DEATHS`. |
| Peace with Gods | `perk_list.lua:2952-2982`: sets `TEMPLE_PEACE_WITH_GODS=1`, clears `TEMPLE_SPAWN_GUARDIAN`, charms **all** `necromancer_shop`; new Steve entities also charm themselves (`necromancer_shop_init.lua:1-7`). |

There is no RNG in collapse/anger/Steve selection. All these are **shared world events** (R1.4.1).

### 6.6 State across visits

- `temple_pos_to_id(x,y)` returns `floor((x+32*512)/(64*512)) .. "_" .. floor(y/512)`:
  (parallel-world index, depth cell) (`temple_shared.lua:2-8`).
- Globals `TEMPLE_LEAKED_<id>` and `TEMPLE_COLLAPSED_<id>` disable future checkers
  (`temple_should_we_spawn_checkers`, `temple_shared.lua:54-71`).
- Globals `TEMPLE_ACTIVE_<floor(workshop_aabb.x)>_<floor(workshop_aabb.y)>` is set by
  `temple_set_active_flag` (`temple_shared.lua:10-17`), read by the engine's music system [INFERENCE:
  `noita.exe` has `TEMPLE_ACTIVE_` among music strings].
- Run-global deck cursors, picked flags, reroll count and world-field modifiers (§8).
- Globals `HOLY_MOUNTAIN_VISITS`, `HOLY_MOUNTAIN_DEPTH`, `visited_biomes`, `visited_biomes_count` are updated
  by `scripts/magic/player_biome_check.lua:32-49`.
- The remaining state is in **the entities and terrain themselves**: purchased item/perk removed, destroyed
  walls, missing refresh, collapsed workshop, Steve state. Spawn functions do not guard themselves against
  repeat calls; normal chunk-generation/save mechanics must prevent re-running them [INFERENCE].
- Serialization trap: `workshop_trigger_check_stats_reference.xml:1-4` includes a sprite with the comment
  **"empty entities don't get serialized"**. Empty markers such as `coop_respawn.xml` may disappear on
  unload/reload [INFERENCE]. A mod's durable spawn/claim state cannot rely on an empty entity alone.
- Guardian lookup has no live distance bound: the closest `guardian_spawn_pos` anywhere loaded is consumed
  (`temple_shared.lua:26-45`; the distance check is commented out). Repeated N-fold calls could steal another
  mountain's marker. Spawn N **loot instances**, not N world transitions or N guardian invocations.

### 6.7 Co-op hooks (D5, Q10, Q12, Q13)

- **Perks (P):** append `perk.lua` and wrap `perk_spawn_many` / `perk_spawn_random` / `perk_pickup` /
  `perk_reroll_perks`. Keep each player's deck/counters, perk entities, removal search and reroll machine
  separate (no raw `EntityGetWithTag("perk")`). Each personal entity has `(temple_id, player_slot, index)`
  identity, invisible and non-pickable to other slots, persisted by the host.
- **Shop (P or N, Q10):** wrap `generate_shop_item` / `generate_shop_wand`; they return the entity id
  (`:178,305`), so stamp owner and identity, hide other players' goods, preserve price and generated wand
  stats. A shop is generated only once per `(temple_id,slot)`, not on reconnect.
- **Heal/refresh (P recommended):** wrap `spawn_hp` or the pickup scripts. Separate a shared max-HP heart
  event from a personal heal, and store one used flag per `(temple_id,slot)`.
- **Collapse (S):** wrap `workshop_exit.lua`'s `collision_trigger`; host accepts `TempleCollapsed{id}` once
  then all clients apply it. Policy decision needed: preserve vanilla "first one out collapses it" or wait
  until every player has left? The latter changes vanilla; this is an additional policy gap for Main.
- **Anger (S):** append `temple_check_for_leaks.lua`; send host one `TempleAngered{id}`, host alone spawns
  Steve and updates shared `TEMPLE_*` / `STEVARI_DEATHS`. Replicas must not run the death counter script.

## 7. Music, secret items, essences, Sampo and NG+

### 7.1 Music machines (gramophones)

These are **real physical props**, not UI/music-only entities:
`entities/props/music_machines/base_music_machine.xml` has `PhysicsBody2Component` (`:3-8`), physics
material `metal_prop_low_restitution` (`:10-15`), `DamageModelComponent hp=8` (`:17-38`) and an
`ExplodeOnDamageComponent` (`:40-74`). The four variants are tagged `musicmachine,musicmachine1..4`
(`music_machine_00.xml:1`; similarly 01..03) and bind `script_kick` to their own Lua file (`:4-6`).

| Variant | Kick script | What kick does | Fixed placements |
|---|---|---|---|
| `music_machine_00.xml` | `music_machine_00.lua:3-18` | if `AudioComponent.m_latest_source==-1`, play sound `"00"`, enable `fx`, add tag `fish_attractor`, set run flag `musicmachine1` | `(14698,-90)`, `(51050,-90)`, `(-21654,-90)` (`biome/_pixel_scenes.xml:172-174`) |
| `...01.xml` | `...01.lua:3-18` | sound `"01"`, flag `musicmachine2` | `(-1905,-1412)`, `(34447,-300)`, `(-38257,-300)` (`:176-178`) |
| `...02.xml` | `...02.lua:3-18` | sound `"02"`, flag `musicmachine3` | `(24166,-421)`, `(-48538,-421)` (`:180-181`) |
| `...03.xml` | `...03.lua:3-18` | sound `"03"`, flag `musicmachine4` | `(2800,250)`, `(39252,-150)`, `(-33652,-150)` (`:183-185`) |

The audio bank/root is `data/audio/Desktop/music.bank`, `music/ancient_tracks` (`base_music_machine.xml:
167-173`). No RNG decides the track. `script_audio_event_dead` disables `fx` and removes `fish_attractor`
(`music_machine_audio_dead.lua:1-5`). On entity load, `music_machine_reset.lua:3-6` **resets playback FX**,
explicitly "in case game was closed while the machine was active" (`base_music_machine.xml:149-153`).
Playback does not resume automatically on reload.

`fx` includes `music_machine_animal_effects.lua` every 60 frames (`base_music_machine.xml:159-165`):
for helpless animals within 700 px except advanced fish, `SetRandomSeed(px,py)` then a 30 % cosmetic
question/charm effect (`music_machine_animal_effects.lua:8-23`).

**Alchemist key music quest.** `entities/animals/boss_alchemist/key_music.lua:22-64` polls machines within
96 px. For each type with its run flag set and **without** `<musicmachineN>_done`, it sets that global done
flag, increments the **key's** `VariableStorageComponent music_machine`, updates description/particles.
At 4, the key opens the light chest (§4.4). **N keys are not independent:** the first key consumes global
`musicmachineN_done`, so later keys cannot count the same machines. This is an important reason not to
multiply quest keys unless their progress is rewritten.

**Hooks (R1.2.3, Q16):** replicate one physics prop with durable id, transform, HP, destroyed state, and
playback state (track + start/stop event). Append each `music_machine_0N.lua` to gate the `kick` event through
the host; on replicas, play the accepted track and FX but do not set quest flags independently.
Keep global quest flags and the key's counter on the host; sync key inventory/position. The reset script's
intent is explicit, so after reconnect restore *chosen policy* (stopped, or resume the track) rather than
mistaking stopped audio for lost prop state.

### 7.2 Kantele, Ocarina and "notes"

**These are three different things the user might mean by "notes":** (1) Kantele/Ocarina wand entities,
(2) note spell cards lying next to them, (3) the music-machine/key quest above. They need separate R4 rows.

- Kantele: `entities/items/kantele.xml`, an unshuffle wand with deck capacity 20 (`:31-52`), **empty** (no
  note cards inside). Ocarina is `entities/items/flute.xml`, same config (`:31-52`).
- World tree: `scripts/biomes/mountain_tree.lua:254-271` spawns **two copies of each** `KANTELE_A`,
  `KANTELE_D`, `KANTELE_DIS`, `KANTELE_E`, `KANTELE_G` card, then one Kantele. No RNG.
- Ocarina room: `scripts/biomes/ocarina.lua:33-50` spawns **two copies of each** `OCARINA_A`, `_B`, `_C`,
  `_D`, `_E`, `_F`, `_GSHARP`, `_A2`, then one flute. No RNG.
- Actions: Ocarina definitions `scripts/gun/gun_actions.lua:8575-8710`, Kantele `:8711-8795`:
  mana 1, price 10, `spawn_probability="0"`, persistent unlock `card_unlocked_ocarina`/`card_unlocked_kantele`,
  add a note projectile and +15 fire-rate wait. They are not normal random shop spells.
- Note projectiles carry `VariableStorageComponent kantele_note` / `ocarina_note` (e.g.
  `entities/projectiles/deck/kantele/kantele_a.xml:94`, `.../ocarina/ocarina_a.xml:94`) and a sound.
  The puzzle scripts keep per-player song progress in the player's `kantele_song`, `kantele_song_pos`,
  `ocarina_song`, `ocarina_song_pos` variables (`entities/player_base.xml:416-431`).

Song recognisers (`scripts/magic/kantele.lua`, `ocarina.lua`) choose
`EntityGetInRadiusWithTag(x,y,24,"player_unit")[1]` (`:13-16`), **not the projectile's shooter**. Two
players playing near one another can therefore feed notes into the wrong player's song.

| Song | Kantele notes (`kantele.lua:3-8`) | Ocarina notes (`ocarina.lua:3-8`) | Reward |
|---|---|---|---|
| Portal | A D♯ E G | A F D E A2 | `mystery_teleport_back.xml` (`:125-136`) |
| Bomb | G D E D | F C D C | live `projectiles/bomb_holy.xml` (`:138-149`) |
| Worm | D E A E D♯ | G♯ F E B D | `animals/worm_big.xml` (`:151-162`) |
| Alchemy | raw sequence F G F A2 C maps through `alt_notes` to **G D♯ G E A** | raw A2 D D♯ E A maps to **E C B G♯ F** | increments alchemist key `status` within 48 px, once per instrument per key (`:164-207`); `status==2` opens the dark chest |

`alt_notes` is fixed in `scripts/lib/utilities.lua:919-930`. Portal/bomb/worm are gated by tags on the
**player** (`kantele_secret_00/01/02` or `ocarina_secret_00/01/02`), plus corresponding run flags
(`kantele.lua:126-158`; `ocarina.lua:126-158`). Alchemy uses a tag on **each key**
(`alchemy_kantele`/`alchemy_ocarina`) and also run flags (`kantele.lua:170-182`, `ocarina.lua:170-181`).
No RNG in song matching.

**Hooks / N-fold policy:** wrap the fixed spawn callbacks (`mountain_tree.lua` `spawn_ocarina`,
`ocarina.lua` `spawn_secret`) so the host creates N instruments and N sets of notes. Do not multiply the
projectiles or the song reward N times; those are ordinary casts/world events. Attribute the note to its
shooter rather than the first nearby player; keep song progress and per-player "already played" tags in the
host's player state. World portals, worms and explosions need one authority.

### 7.3 Essences and essence eaters

| Essence id | Pickup file (all under `entities/items/pickup/`) | Fixed spawning script | Continuous effect |
|---|---|---|---|
| `laser` (Earth) | `essence_laser.xml` | `scripts/biomes/essenceroom.lua:34` | 8 laser bullets, random initial angle (`scripts/essences/laser.lua:6-22`), every 120 frames (`entities/misc/essences/laser.xml:6-7`) |
| `air` | `essence_air.xml` | `essenceroom_air.lua:34` | 16 air bullets (`air.lua:6-22`), every 40 frames (`air.xml:6-7`) |
| `water` | `essence_water.xml` | `essenceroom_hell.lua:34` | one water bullet (`water.lua:6-18`), every 100 frames (`water.xml:6-7`) |
| `fire` | `essence_fire.xml` | `lake_statue.lua:277` | fire explosion (`fire.lua:6-10`) every 120 frames (`fire.xml:6-7`); pickup also multiplies projectile/explosion damage received by 1.3 |
| `alcohol` (Spirits) | `essence_alcohol.xml` | `essenceroom_alc.lua:34` | `DRUNK frames=-1` (`entities/misc/essences/alcohol.xml:17-21`) |

All pickup XMLs use `VariableStorageComponent essence_id` (`:19-23`) and
`script_item_picked_up=data/scripts/essences/essence_pickup.lua` (`:25-28`).
`essence_pickup.lua:5-81` loads the corresponding `misc/essences/<id>.xml` child on the picker
(`:37-65`), adds UI child `essence_effect`, sets **run and persistent** flag `essence_<id>` (`:67-68`),
increments **Globals** `ESSENCE_<ID>_PICKUP_COUNT` (`:70-73`), and kills the pickup. The bullet scripts
seed `SetRandomSeed(pos_x,pos_y)`; no frame input (`air/laser/water.lua:6`).

**Essence eater.** Fixed entities `entities/buildings/essence_eater.xml` at the snow/desert surface and
parallel copies (`biome/_pixel_scenes.xml:197-204`; NG+ `..._newgame_plus.xml:54-55`).
Its `script_death=data/scripts/essences/away.lua` (`essence_eater.xml:83`):

1. Iterate **all** `player_unit`s and kill every child tagged `essence_effect` (`away.lua:7-23`).
2. Read all five **global** pickup counts; spawn one corresponding stone per count:
   laser→`stonestone`, fire→`brimstone`, water→`waterstone`, air→`thunderstone`,
   alcohol→`poopstone` (`:25-83`).
3. Zero all counts and remove all five run flags (`:46-89`).
4. It **does not undo fire's 1.3 damage multiplier** [INFERENCE: no such writes in the full function].

Thus vanilla's eater is already "remove from everyone" in a process containing multiple players, but the
stones are counted globally. Applying the essence S to N players would produce N stones at the eater unless
the counter policy is changed. Q15 must decide essence S/N/P/1 and eater output policy.

**Hooks:** append `essence_pickup.lua` (detect claim / suppress local writes / apply to selected players);
append `away.lua` `death()` (one host `EssencesRemoved` event plus explicit per-player removals and explicit
stone output count). Store both personal effect children and shared run flags: the flags alone do not say
which player has an essence.

Other stones relevant to the N/1 list: `musicstone` from `scripts/biomes/rock_room.lua:35`
(`MusicEnergyAffectorComponent` while held, `musicstone.xml:37-45`); `wandstone` gives
`EDIT_WANDS_EVERYWHERE` while held (`wandstone.xml:37-41`); `thunderstone` electricity source and lightning
immunity (`thunderstone.xml:42-61`); `brimstone` fire immunity (`brimstone.xml:42-52`);
`waterstone` underwater breathing + cleaner stains (`waterstone.xml:37-47`) and wets nearby hittables
(`scripts/items/waterstone.lua:4-8`); `poopstone` poisons enemies within 160 px
(`scripts/items/poopstone.lua:4-25`); `evil_eye` toggles hidden platforms within 200 px
(`scripts/items/evil_eye_state.lua:3-29`); `beamstone` has a kick callback (`beamstone.xml:37-41`).
Most can also come from ordinary chests (§4), so "unique world version" and "random loot version" need distinct
spawn-origin policies, not a policy based solely on filename.

#### Sun-seed chain and moon state (quest objects, Q15)

| Stage | Event / state / seed |
|---|---|
| Sunseed | ghost boss drop (`boss_ghost/death.lua:7-8`), tag `seed_a` (`entities/items/pickup/sun/sunseed.xml:1`) |
| Sunstone | pyramid spot scans root `seed_a` within 64 px and attaches one processing entity (`scripts/buildings/sun/spot_1.lua:8-18`); at 80 frames `spot_1_finish.lua:10-17` replaces it with `sunstone.xml` |
| Sunegg | `spot_2.lua:27-43`: within 120 px, >5 `big_explosion`, or `sea_of_lava`, `beam_from_sky`, `nuke_giga` converts stone into egg |
| Sunbaby | egg every 20 frames tags `homing_target`s within 260 and attaches death callback; at 100 kills replace egg (`spot_3.lua:6-36`). **Death credit goes to `EntityGetWithTag("seed_c")[1]`, not nearest egg** (`spot_3_death.lua:3-23`), frame/entity seeded. |
| Sun / dark sun | baby every 10 frames consumes stones within 56, stores `sunbaby_essences_list`; four core stones produce `newsun.xml`, with poopstone also fed produce `newsun_dark.xml`; persistent `progress_sun` / `progress_darksun` (`spot_4.lua:5-151`) |
| Supernova | sun/dark-sun encounter within 180: destroys both, attaches `eradicate.xml` to every player, inflicts 500 curse damage on mortals, converts 13 static materials→fire globally, persistent `secret_supernova` (`sun_collision.lua:6-52`) |

These scripts live in `scripts/buildings/sun/`; entities in `entities/items/pickup/sun/`.
Normal/dark suns have physics/material-conversion/cell-eating components (`newsun.xml:8-107`,
`newsun_dark.xml:8-96`); not ordinary inventory loot. Baby-air/dark-sun lightning seeds
`SetRandomSeed(GameGetFrameNum()+GetUpdatedComponentID(),x+y+entity_id)`
(`sunbaby_air_effect.lua:3-14`, `sun_dark_effect.lua:3-14`).

Celestial moon altars run `scripts/magic/moon_altar.lua` / `dark_moon_altar.lua` every 90 frames
(`entities/buildings/moon_altar.xml:1-21`, `dark_moon_altar.xml:1-21`). Eligibility uses four **global**
essence flags, a player within 48 px, and nearby sun/encoded-tag objects. Normal altar gives `DESTRUCTION`
(`moon_altar.lua:19-48`); extra upper-moon branch gives six `TOUCH_*` spells (`moon_altar_extra.lua:5-29`);
dark extra produces ten full-heal hearts (`dark_moon_altar_extra.lua:5-28`), each requiring a distinct
shared-effect event rather than one dedupe key for all ten.

Persistent flags `moon_is_sun`, `moon_is_darksun`, `darkmoon_is_sun`, `darkmoon_is_darksun`,
`secret_sun_collision`, `secret_darksun_collision`, `secret_hat` are written by
`moon_altar.lua:52-96`, `dark_moon_altar.lua:43-87`. **Profile state can regenerate world objects**:
`moon_altar_init.lua:3-7` respawns sun for `moon_is_sun`; `dark_moon_altar_init.lua:3-7` respawns dark sun for
`darkmoon_is_darksun`. These local profile flags must not overwrite the host's run state on reconnect.

Hook the stage polling components **before execution**, run consumption/counters/conversions once on the
host and replicate the result. Do not N-fold sun stages, their kill counters or moon world transitions.
Portable seeds/stones and guaranteed moon spell rewards can be selected separately for N-fold policy.

### 7.4 Sampo, endings and New Game+

**Sampo** (`entities/animals/boss_centipede/sampo.xml`) is a real inventory item tagged `this_is_sampo`
(`:1-22`), with one-time pickup script (`:24-28`) and a disabled-in-world `EndingMcGuffinComponent`
enabled in inventory (`:51-54`). The latter's native logic triggers the ending script
`sampo_start_ending_sequence.lua` (LuaComponent `execute_every_n_frame=-1`, `:43-48`) [INFERENCE].
A great chest can also spawn Sampo with `EntityLoadEndGameItem` (§4.2).

On pickup, `sampo_pickup.lua:3-102` sets `FINAL_BOSS_ACTIVE=1` (`:11`), enables `disabled_at_start`
components on all `sampo_or_boss` entities, disables `enabled_at_start`, unfreezes physics, removes Kolmi's
protection child and adds `boss_centipede_active` (`:52-76`), then drops the lava ceiling/bridge
(`:78-81`). The pacifist bypass is overridden by `fight_player=true` (`:48`).
`sampo_init.lua:3-26` picks the displayed name from orb count (§2.3).

The Work / mountain altar: `ending_sampo_spot_underground.xml` and `ending_sampo_spot_mountain.xml`
are location markers (the mountain also runs `altar_tablet_magic.lua` every 240 frames). The ending script
uses Sampo's position, marker distance <32, shared orb count, and `NEW_GAME_PLUS_COUNT`.

`entities/animals/boss_centipede/ending/sampo_start_ending_sequence.lua`:

| Branch | Exact condition | Effect |
|---|---|---|
| NG+ | `<33` orbs, at mountain (<32 Manhattan distance), and either `5+NG <= orbs < 11`, or `orbs > 11`, `5+NG >= 11`, `orbs >= 5+NG` (`:48-64`) | kill Sampo, `GameClearOrbsFoundThisRun()`, move closest player up 25 px, persistent `progress_ngplus`, call `do_newgame_plus()` (`:66-80`) |
| All non-NG+ | `doing_newgame_plus==false` (`:85-90`) | run flag `ending_game_completed`, `GameOnCompleted()` |
| ≥33 orbs | (`:98-140`) | persistent `secret_amulet`; >33 sets run flag `ending_game_completed_with_34_orbs`; at either mountain or underground marker: `GameDoEnding2()`, persistent `progress_ending2`, kill Sampo |
| Underground normal Work | underground marker present and distance <32 (`:190-223`) | persistent `progress_ending0`; load `midas_sand`, `midas_chunks`, `midas_walls`, `sampo_working`; unless exactly 11 orbs load lethal `gold_effect.xml`; animate ending mechanism (`:225-234`) |
| Mountain secret | else if mountain marker present and distance <32 (`:235-285`) | exactly 11 → `midas.xml`, persistent `progress_ending1_gold`; otherwise `midas_radioactive.xml`, `progress_ending1_toxic`; attach effect to `player_unit[1]`, proportional material damage, persistent `progress_ending1`, set `INFINITE_GOLD_HAPPENING=1` |

The apparent separate 12-orb branch (`:141-189`) is **commented out**. `ORB_COUNT_IN_WORLD` is 11, not 12.
Beware the active `elseif #endpoint_underground > 0`: it checks marker existence before marker distance;
if both endpoint types are loaded, the mountain branch is skipped [INFERENCE from control flow].

`midas_convert_everything_to_gold.lua:1` calls `ConvertEverythingToGold()`; radioactive variant calls it
with `"gold_radioactive","gold_static_radioactive"` (`...radioactive_gold.lua:1`).
This is an engine routine (`API:341`) that sets global conversion/ending state [INFERENCE]:
`WorldStateComponent.EVERYTHING_TO_GOLD`, `material_everything_to_gold`, `..._static`,
`INFINITE_GOLD_HAPPENING`, and the engine happy-ending fields (`CDOC:2678-2684`).
The normal ending's `sampo_normal_ending.lua:6-10` sets world `damage_flash_multiplier=0` and loads
`midas_entities.xml`. **Do not** treat infinite-gold animation wallet changes as `GoldIncome`.

**NG+** (`scripts/newgame_plus.lua:1-93`) is a world transition, not a new co-op run:

- `NEW_GAME_PLUS_COUNT += 1`, save session numbers (`:5-8,73`).
- Enemy HP scales min `7+(n−1)*2.5`, max `25+(n−1)*10`; attack speed `0.5^n`; session
  `DESIGN_SCALE_ENEMIES=1`, `DESIGN_NEW_GAME_PLUS_HP_SCALE_MIN/MAX`, `...ATTACK_SPEED` (`:10-19`).
- Multiplies damage intake on **the closest player to (0,0)** only: melee and poison ×3, projectile,
  explosion, electricity, fire, drill, slice, ice, radioactive ×2; healing and physics unchanged
  (`:25-68`). Must be applied to every co-op player.
- `BiomeMapLoad_KeepPlayer("data/biome_impl/biome_map_newgame_plus.lua",
  "data/biome/_pixel_scenes_newgame_plus.xml")` (`:72`), then four entrance cleanup scenes (`:77-80`).
  The function's name implies inventory/perks/HP survive [INFERENCE]; there is no explicit player
  inventory/HP/wallet reset in the script.
- NG+ map is 64×48 (`biome_map_newgame_plus.lua:4-5`), with **constant seed arguments**
  `SetRandomSeed(4573,4621)` (`:118`), reads NG count (`:137`), calls `orb_map_update()` (`:379-384`).
  That helper stores `ORB_MAP_STRING` (`scripts/lib/utilities.lua:947-961`). Counts divisible by
  2/3/5/7 modify biome assignment/walls/surface (`biome_map_newgame_plus.lua:137-205`); most orb rooms
  relocate (`:289-362`). No explicit reset of picked-perk Globals, fungal counters or world flags appears
  here; native map-load reset semantics remain unverified [INFERENCE].

**Hooks (R2.3, R1.4.1, Q15):** keep exactly one Sampo/key per run unless Q15 says otherwise. Wrap
`sampo_pickup.lua` `item_pickup` (host `BossActivated` event) and replace/gate the **file-scope**
`sampo_start_ending_sequence.lua` (appending is too late to suppress file-scope side effects).
Only the host chooses an ending/NG+; persist an epoch transition event before any client unloads its world.
Every client must enter the same map epoch, preserve its own player state, and apply per-player damage
scaling exactly once. N-fold Sampo or N-fold endings would destroy the run.

### 7.5 Unique world wands and fixed spell rewards: selection list for R4/Q11/Q14

This list separates fixed/puzzle rewards from random wands on ordinary biome floors or carried by mobs.
The latter remain **1** under R4. XML filenames are stable identities; a mod should stamp a *spawn origin*
alongside them because the same filename can also appear as random loot.

| Place / trigger | Reward(s) | Spawning code | Randomness / guard |
|---|---|---|---|
| World tree / Ocarina room | Kantele or flute + **2 of each** note spell | §7.2 | fixed; scene persistence only |
| Tower end | three "great" wands: `wands/wand_good/wand_good_1/2/3.xml`, plus exit portal | `scripts/biomes/tower_end.lua:33-38` | Each generator seeds **`SetRandomSeed(x,y+GameGetFrameNum())`** (`wand_good_1/2/3.lua:33`). Taking one globally kills the other in-world `wand_good`s (`wand_good_pickup.lua:4-25`). Need N choice sets, not 3N freely collectible wands. |
| Wand room | `wands/experimental/experimental_wand_3.xml` | `scripts/biomes/gun_room.lua:34` | stats seed `(x,y+GameGetFrameNum())` (`experimental_wand_3.lua:33`) |
| Lake hut A → bunker | `experimental_wand_2.xml`; `COLOUR_RED/ORANGE/YELLOW/GREEN/BLUE/PURPLE/RAINBOW/INVIS` | `scripts/buildings/bunker_check.lua:8-24` | run `fishing_hut_a`, player within 200 px; kills trigger; wand seed `(x,y+frame)` (`experimental_wand_2.lua:33`) |
| Lake hut B → second bunker | `experimental_wand_1.xml`; `IF_HALF`, `IF_HP`, `IF_PROJECTILE`, `IF_ENEMY`, 2×`IF_ELSE`, 2×`IF_END` | `bunker2_check.lua:8-24` | `fishing_hut_b`; kills trigger; wand seed `(x,y+frame)` (`experimental_wand_1.lua:33`) |
| Meat boss | `experimental_wand_4.xml` | `entities/animals/boss_meat/death.lua:10` | boss drop (§5) |
| Snowcastle sauna | `wand_vasta.xml` if x>190, else `wand_vihta.xml`; brimstone | `scripts/biomes/snowcastle.lua:1373-1383` | side-dependent, no RNG |
| Oil receptacle | `wand_ruusu.xml` | `scripts/buildings/receptacle_oil.lua:3-12` | `material_area_checker_success`; trigger killed |
| Water receptacle | `wand_valtikka.xml` | `receptacle_water.lua:3-10` | same |
| Steam receptacle | `wand_kiekurakeppi.xml` | `receptacle_steam.lua:3-10` | same |
| Vault laboratory puzzle | `wand_arpaluu.xml` (70 %) or `wand_varpuluuta.xml` | `vault_lab_puzzle_check.lua:3-23` | `ProceduralRandomf(x,y)>0.3`; kills nearby puzzle entities |
| Skull puzzle | `wand_leukaluu.xml` at mountain altar if loaded, else trigger position | `spawn_leukaluu.lua:1-11` | no RNG |
| Forge | `leukaluu_kantele.xml` when skull wand + kantele; `FISH` when skull without kantele; repaired `SUMMON_PORTAL`; `wand_level_05_better.xml` from broken wand; forged tablets | `forge_item_convert.lua:11-118` | inputs consumed; no RNG except generated wand internals |
| Lake rainbow | `RAINBOW_TRAIL` | `scripts/biomes/lake.lua:251` | fixed |
| Sun/dark-sun scales | `BLACK_HOLE_GIGA` | `scripts/biomes/scale.lua:53-56` | profile `progress_sun` and `progress_darksun`, and not `card_unlocked_black_hole` |
| Moon room | perk `MOON_RADAR` | `scripts/biomes/moon_room.lua:35` | fixed |
| Spatial Awareness | perk `MAP` (not a wand) | `entities/animals/boss_robot/death.lua:10` | boss drop (§5); choosing it sets up map child (`perk_list.lua:915-929`) |
| Orbs | unlocked spell on first profile pickup, else shared max-HP heart | §2 | per-profile |
| Steel / light / dark / darkness chests | fixed unique spell sets | §4.4 | first-open profile flags / music/key progress |
| Upper moon | `DESTRUCTION` | `scripts/magic/moon_altar.lua:19-48` | four core run essence flags + player within 48 + no nearby sun/encoded-tag entity; altar consumed |
| Upper moon extra condition | `TOUCH_GOLD/WATER/OIL/ALCOHOL/BLOOD/SMOKE` | `moon_altar_extra.lua:5-29` | four essence flags + encoded-tag entity + player proximity; altar consumed |
| Summit / tower-check marker | `NUKE_GIGA` for `small_friend`; `BOMB_HOLY_GIGA` for `big_friend`; `DIVIDE_10` for active greed + player; else `AIR_BULLET` | `scripts/biomes/hills.lua:273`, `winter.lua:406`; `scripts/magic/towercheck.lua:6-32` | radius 16, kills detector, sets unlock flags |
| Sky/hell shops | random level-10 spell, not a fixed item | `scripts/biomes/the_end.lua:410-429` | `SetRandomSeed(x,y)`: normal marker outside central y band has 1/50 chance; `spawn_specialshop` guaranteed |
| `wand_petri.xml` | defined special wand, **no active fixed spawn located** [INFERENCE] | `entities/items/wand_petri.xml:55-60`; `scripts/gun/procedural/wand_petri.lua:625-628` | position-seeded generator definition; do not call it a guaranteed world reward |

Named wand **identity** is fixed but its inherited stats/cards are usually seeded:
`wand_vasta/vihta` inherit `wand_level_03_better` with permanent `HITFX_CRITICAL_WATER` /
`HITFX_EXPLOSION_ALCOHOL`; `wand_valtikka` inherits level03 with `PIERCING_SHOT`;
`wand_kiekurakeppi` unshuffle02 with `KNOCKBACK`; `wand_arpaluu` unshuffle05 with `EXPLOSION_REMOVE`;
`wand_varpuluuta` level05_better; `wand_ruusu` level02; `wand_leukaluu` level03
(`entities/items/wand_vasta.xml:2,18-26`, `wand_vihta.xml:2,18-26`, `wand_valtikka.xml:2,18-26`,
`wand_kiekurakeppi.xml:2,18-26`, `wand_arpaluu.xml:2,18-26`, `wand_varpuluuta.xml:2-14`,
`wand_ruusu.xml:2-14`, `wand_leukaluu.xml:1-14`).
The ordinary/better generators seed `(x,y)` (`gun_procedural.lua:968-977`,
`gun_procedural_better.lua:415-418`); visual scripts only change name/sprite. Preserve the fixed type and
permanent actions for every N-copy even when varying the generated stats.

The jawbone wand's preceding event is **fish sacrifice**, not a separate skull spawn:
`altar_tablet_magic.lua:323-357` consumes a nearby advanced fish, writes Globals `MISC_FISH_RAIN=1` and
persistent `misc_fish_rain`, attaches fish rain to `players[1]`. The rain lasts 300 frames and spawns fish
every 20; its **on-removal** callback creates the jawbone (`entities/misc/fish_rain.xml:4-23`,
`scripts/buildings/spawn_leukaluu.lua:1-11`). Keep one rain/consumption event and multiply the final reward,
not the rain entity.

**N-fold hook:** wrap the specific spawn or reward callback, run **world/puzzle state change once**, and
create N real items with unique ids. For frame-seeded wands, generate on the authority and replicate the
resulting complete wand (stats, actions, always-casts), not `EntityLoad` on each client. For Tower wands,
rewrite `wand_good_pickup.lua` to delete only the alternatives in the picker's choice set. A fixed seed
does not make `EntityLoad` safely repeatable after a crash: persist both the claimed puzzle and the spawned
item ids atomically.

## 8. Perks that write global state (D5 vs. `WorldStateComponent`)

### 8.1 Generic perk bookkeeping is global

Every perk pickup (`scripts/perks/perk.lua:169-364`) writes run-global state, even when the perk itself is
personal:

| State | Where written | Who reads it |
|---|---|---|
| Globals `PERK_PICKED_<ID>_PICKUP_COUNT` (+1) | `perk.lua:205-207` | deck filtering (`perk.lua:144-157`), stack limits (`perk.lua:13-29`), `perk_list` functions that scale with pickup count (e.g. `gold_explosion.lua:35-40`), null room (`perk.lua:668-818`) |
| run flag `PERK_PICKED_<ID>` | `perk.lua:219` | `get_perk_stackable_status` (`perk.lua:598-616`), null room. The engine also contains the string `PERK_PICKED_` near the progress-menu strings [INFERENCE: progress UI] |
| persistent flag `perk_picked_<id>` and run flag `new_perk_picked_<id>` (unless run flag `no_progress_flags_perk`) | `perk.lua:210-218` | profile progress |
| run flag `PERK_<ID>` when a perk is **spawned** | `perk.lua:463,477,532,593` | [INFERENCE: progress/statistics] |
| `remove_other_perks` marks other perks as picked | `perk.lua:260-271` | deck filtering |
| Globals deck cursors `TEMPLE_NEXT_PERK_INDEX`, `TEMPLE_REROLL_PERK_INDEX`, `TEMPLE_PERK_REROLL_COUNT` | `perk.lua:442-459, 499-530, 559-591` | every temple; the deck itself is seeded `SetRandomSeed(1, 2)` and is identical for the whole run (`perk.lua:53-117`) |
| Picking one perk with `kill_other_perks` kills **every entity tagged `perk` in the loaded world**, not only the ones in this temple; it also disables reroll machines within 200 px | `perk.lua:309-359` (`EntityGetWithTag("perk")` at `:328`) | |
| Rerolling kills and respawns **every** `perk` entity in the loaded world | `perk.lua:539-596` (`:545`) | |

Under D5, all of the above must become per-player: a per-player deck cursor and pickup counters, and kill or
reroll scoped to the perks spawned for that player. Otherwise player A's pickup removes player B's perk
choices, and A's stack counts filter B's deck.

### 8.2 Perks whose effect is global

Active perks only. Perks inside `--[[ ]]--` blocks (`perk_list.lua:219-344, 534-551, 613-645, 980-990,
1922-1939, 2722-2737, 2761-2814, 3095-3111, 3112-3132, 3137-3268, 3269-3321`) are not in the game.

| Perk | Lines in `perk_list.lua` | Global write | Readers / consequence in co-op |
|---|---|---|---|
| `GOLD_IS_FOREVER` | 97-124 | `WorldStateComponent.perk_gold_is_forever` | all death and chest gold has no lifetime (§1) |
| `TRICK_BLOOD_MONEY` | 125-151 | `perk_trick_kills_blood_money` | all trick kills drop blood money |
| `EXPLODING_GOLD` | 152-169 | run flag `exploding_gold` | all nuggets explode; credit goes to `player_unit[1]` (`gold_explosion.lua:24-33`) |
| `HEARTS_MORE_EXTRA_HP` | 647-671 | Globals `HEARTS_MORE_EXTRA_HP_MULTIPLIER` | every player's heart pickups (§3) |
| `NO_WAND_EDITING` | 1040-1068 | `perk_hp_drop_chance += 20` | all kills may drop blood money |
| `UNLIMITED_SPELLS` | 1178-1223 | `perk_infinite_spells` | the engine makes "almost all spells" unlimited (`CDOC:2673`); applies to all wands [INFERENCE: engine-global]. `GameRegenItemActionsInPlayer` touches the picker only (`:1195`) |
| `SHIELD` | 1409-1465 | Globals `PERK_SHIELD_COUNT` | shield radius and recharge for the next pick (`:1423-1427`); a second player's shield would be sized from the first player's count |
| `REVENGE_RATS`, `PLAGUE_RATS`, `VOMIT_RATS` | 1538-1570, 1777-1813, 1814-1846 | `perk_rats_player_friendly` (via `component_write`) | rats and the player herd stop attacking each other, "a persistent change" (`CDOC:2677`); affects every player (same herd) |
| `ATTACK_FOOT`, `LEGGY_FEET` | 1612-1692, 1693-1776 | run flag `ATTACK_FOOT_CLIMBER` | no data reader found (grep) |
| `CORDYCEPS`, `MOLD`, `FUNGAL_DISEASE`, `ANGRY_GHOST`, `HUNGRY_GHOST`, `DEATH_GHOST`, `LUKKI_MINION` | 1847-1903, 1999-2026, 2133-2269 | run flags `player_status_*` | no data reader found (grep) |
| (any of the above via `perk_utilities.lua`) | `scripts/perks/perk_utilities.lua:24-210` | Globals `PLAYER_RATTINESS/FUNGAL/GHOSTNESS/LUKKINESS/HALO_LEVEL`, `<STYLE>_PERK_TOTAL_COUNT`; persistent `player_status_*` at level 3 | transformation thresholds are summed over all players |
| `ATTRACT_ITEMS` | 2289-2326 | Globals `PERK_ATTRACT_ITEMS_RANGE` | the second picker extends the first one's range |
| `NO_MORE_SHUFFLE` | 2561-2587 | Globals `PERK_NO_MORE_SHUFFLE_WANDS=1`; sets `shuffle_deck_when_empty=0` on **every** `wand` entity in the world (`:2571-2582`) | all future generated wands are non-shuffle (`scripts/gun/procedural/gun_procedural.lua:720`, `gun_procedural_better.lua:535`), including other players' wands, shop wands and world wands |
| `EXTRA_PERK` | 2816-2835 | Globals `TEMPLE_PERK_COUNT` | perk count in every temple (`perk.lua:487`) |
| `PERKS_LOTTERY` | 2836-2856 | Globals `TEMPLE_PERK_DESTROY_CHANCE` | `perk.lua:323-326` |
| `GAMBLE` | 2857-2869 | spawns `perk_gamble_spawner.xml` | `scripts/perks/perk_gamble_spawn.lua:11-16` draws from the shared deck (`perk_spawn_random`) and advances `TEMPLE_NEXT_PERK_INDEX` |
| `EXTRA_SHOP_ITEM` | 2870-2888 | Globals `TEMPLE_SHOP_ITEM_COUNT` | items per temple shop (`temple_altar.lua:159`, `boss_arena.lua:181`) |
| `GENOME_MORE_HATRED` / `GENOME_MORE_LOVE` | 2889-2920 / 2921-2951 | `global_genome_relations_modifier` ∓25 | engine adds it to every herd relation (`CDOC:2669`) |
| `PEACE_WITH_GODS` | 2952-2982 | Globals `TEMPLE_PEACE_WITH_GODS=1`, `TEMPLE_SPAWN_GUARDIAN=0`; charms every `necromancer_shop` | Steve no longer spawns or attacks (§6) for everyone |
| `MEGA_BEAM_STONE`, `FOOD_CLOCK`, `VOMIT_RATS`, `MOLD` | 3080-3093, 798-853, 1826, 1888 | spawn world items (`beamstone`, `potion_porridge`, `potion_vomit`, `potion_slime`) next to the picker | anyone can pick these up |
| `GLOBAL_GORE`, `REMOVE_FOG_OF_WAR` | 495-507, 508-516 | engine game effects on the picker (`GAMEEFFECT_GLOBAL_GORE_GORE_MULTIPLIER=5`, `magic_numbers.xml:49`) | probably world-visual only for the picker's process [INFERENCE] |
| `EXTRA_POTION_CAPACITY` | 2722-2737 (commented out) | would set Globals `EXTRA_POTION_CAPACITY_LEVEL` | readers still exist (`scripts/items/potion.lua:158`, `init_potion.lua:17`, `powder_stash.lua:84`) |

The **null room** (`scripts/magic/null_room/check.lua:94-149`) calls `create_all_player_perks` and
`IMPL_remove_all_perks` (`perk.lua:668-818`). Both read the global `PERK_PICKED_*` flags and counters, so in
co-op the room would rebuild the perks of **all** players around the triggering player.

### 8.3 Recommendation (feeds Q13)

| Class | Perks | Recommendation |
|---|---|---|
| Truly personal | game-effect perks, stat perks | keep personal; move the bookkeeping (§8.1) to per-player storage |
| Personal, but implemented via a global | `HEARTS_MORE_EXTRA_HP`, `SHIELD`, `ATTRACT_ITEMS`, transformation levels, `EXPLODING_GOLD` pickup count | re-implement as per-player state (wrap the `perk_list` functions with `ModLuaFileAppend` on `perk_list.lua`) |
| Global by nature | `GOLD_IS_FOREVER`, `TRICK_BLOOD_MONEY`, `NO_WAND_EDITING` hp drops, `EXPLODING_GOLD`, `UNLIMITED_SPELLS`, rat friendship, genome modifiers, `PEACE_WITH_GODS`, `NO_MORE_SHUFFLE` | Q13: decide per perk whether (i) global team effect or (ii) owner-scoped (e.g. drop-time checks the killer's perks). Removal must recompute surviving owners' contributions, not set the field to zero. |
| Temple structure | `EXTRA_PERK`, `PERKS_LOTTERY`, `EXTRA_SHOP_ITEM`, `GAMBLE` | personal, applied only to that player's temple instance (D5, Q10) |

## 9. Fungal shift

### 9.1 Trigger

1. Ingesting materials whose ingestion list contains `<StatusEffect type="TRIP">` builds up the TRIP status
   (e.g. `materials.xml:7365, 8915, 9646, 9665, 11058-11163, 11553`).
2. At `min_threshold_normalized=3.0` the status list loads `data/entities/misc/effect_trip_03.xml`
   (`scripts/status_effects/status_list.lua:273-279`). The effect entity has 600-frame `TRIP_00` and
   `FOOD_POISONING` effects and a `LuaComponent execute_every_n_frame="600"
   script_source_file="data/scripts/status_effects/effect_trip_03.lua"` (`entities/misc/effect_trip_03.xml:6-29`).
3. `effect_trip_03.lua` spawns cosmetic eyes with `SetRandomSeed(pos_x + GameGetFrameNum(), pos_y)`
   (`:7-21`) and calls `fungal_shift(entity_id, pos_x, pos_y, false)` (`:24`). `fungal_shift` re-targets the
   parent of the effect entity, i.e. the tripping creature (`scripts/magic/fungal_shift.lua:114-117`).
   The only other caller is the debug menu (`entities/_debug/debug_menu.lua:390`).

The trigger is **per creature**. Any player, or a polymorphed or charmed creature with the effect, can
shift the whole world.

### 9.2 Algorithm (`scripts/magic/fungal_shift.lua:103-250`)

| Step | Lines | Detail |
|---|---|---|
| Cooldown | `:119-123` | Return unless `GameGetFrameNum() >= fungal_shift_last_frame + 60*60*5` (18 000 frames). The frame counter's epoch/save continuity is unverified; use one authoritative cooldown clock rather than assuming all peers have the same frame number. |
| Gold ending lock | `:125-128` | No shift if `WorldStateComponent.EVERYTHING_TO_GOLD`. |
| Hard cap | `:130-133` | At most 20 shifts per run: Globals `fungal_shift_iteration`. |
| Choose pair | `:140-145` | Up to 20 tries. `seed2 = 42345 + iter + 1000*tries`; `SetRandomSeed(89346, seed2)`; `rnd = random_create(9123, seed2)`, a `ProceduralRandomf`/`ProceduralRandomi` stream (`scripts/lib/utilities.lua:25-42`); weighted picks from `materials_from` (`:3-23`) and `materials_to` (`:25-59`). |
| Held flask override | `:146-165` | If the shifting creature's **active item** is a potion or powder pouch (`get_held_item_material`, `:85-100`), with 75 % chance its main material replaces `from` (50 %) or `to` (50 %). `to=gold` becomes a random "greedy" material unless a 1/1000 roll succeeds (`:157-159`); `grass_holy` becomes `grass` the same way (`:161-163`). |
| Ban list | `:168-174` | A `to` material tagged `[NO_FUNGAL_SHIFT]` is re-rolled once. |
| Apply | `:177-195` | `ConvertMaterialEverywhere(from_i, to)` for each material in the `from` group. |
| Bookkeeping | `:201-249` | `fungal_shift_iteration += 1` (`:203`); remove the TRIP ingestion (`:206`); music; eye particle; log; `fungal_shift_last_frame = frame` (`:225`); UI icon child `fungal_shift_ui_icon` on the creature (`:228-248`). |

**Determinism.** The pair depends only on (world seed, `iter`, try number) **plus the held item of the
tripping creature**. Two clients with the same iteration counter and the same held material compute the same
pair. The particles and cosmetic eyes are frame-seeded.

### 9.3 Persistence

- `ConvertMaterialEverywhere` "will add a new entry to WorldStateComponent which serializes these changes"
  (`API:342`) into `changed_materials` ("pairs of materials changed ... restored when loading a save",
  `CDOC:2696`).
- The counters `fungal_shift_iteration` and `fungal_shift_last_frame` are Globals, stored in
  `WorldStateComponent.lua_globals` (`CDOC:2690`) [INFERENCE: Globals storage location].
- Other world-wide conversions that follow the same persistence path: `entities/animals/boss_fish/death.lua:7`
  (water → smoke), the supernova `scripts/buildings/sun/sun_collision.lua:36-48` (13 static materials → fire),
  the trip secret `scripts/buildings/funroom_check.lua:13-14`.

### 9.4 Co-op hook (R1.1.5)

- (a) **Detect:** `ModLuaFileAppend("data/scripts/magic/fungal_shift.lua", ...)` that wraps
  `fungal_shift(entity, x, y, debug)`. This covers the only gameplay caller. `ConvertMaterialEverywhere`
  itself is an engine global and would have to be wrapped in every Lua file that calls it, because Lua VMs
  are per script or per component (`CDOC:1558`, `vm_type`) [INFERENCE].
- (b) **Suppress:** on any process that is not authoritative for the run, the wrapper returns before the
  cooldown/iteration checks and sends `FungalShiftRequest{player_slot, held_material, x, y}`.
- (c) **Apply to all:** the host owns `iteration` and the cooldown (on a host clock, not `GameGetFrameNum`),
  computes the pair with the vanilla algorithm and the requester's held material, records
  `FungalShift{id, iter, pairs[]}` in the run log, and every client calls `ConvertMaterialEverywhere` per pair
  and updates both Globals. A rejoining client replays the missing shifts in order. The host's authoritative
  `changed_materials` list must win over a client's cached save (R6.7).
- Risk: if a client's cached save already contains some pairs, replaying from the log must not apply a pair
  twice. Conversions chain, e.g. A→B then B→C, so apply them in sequence and dedupe by shift id.

## 10. Boundaries not established by this source audit

These are **unresolved**, not assumed vanilla contracts:

- Native gold-powder credit formula and exact collection hook/address; `mGoldAccumulator` is not a
  certified income counter (§1.8).
- Ordering of native orb bookkeeping relative to Lua callbacks; supported direct writing of
  `orbs_found_thisrun`; native parallel-world id remapping (§2).
- Native `ItemChestComponent` drop table/seed/enable inheritance; apparition selection and carried
  inventory (§5).
- Exact native `EndingMcGuffinComponent` dispatch, `ConvertEverythingToGold` and `GameDoEnding2`
  multi-player semantics; which world fields and player effects they mutate (§7.4).
- `BiomeMapLoad_KeepPlayer` reset/preservation of run flags, Globals, changed-material mappings and
  multiple player entities; frame-number continuity after reload (§7.4, §9).

No game was launched and no builds/tests were run. Claims above are static source/API observations.

## 11. Policy gaps to resolve before implementation

Existing decisions remain intact; these are recommendations/questions, not new decisions:

- **Q11/Q14:** select rows in §7.5; identical complete wand copies or varying random stats; loot reservation;
  N Tower *choices* versus freely collectible 3N wands. Guaranteed fixed spells/actions stay guaranteed.
- **R5/D5:** picker multiplier or recipient multiplier for shared base gold; blood-money healing scope.
  Gold never multiplies with N boss/chest rewards.
- **Q12/D6:** split temple +10 max HP (S) from full heal (P recommended); ordinary full-heal and evil-heart
  poison scope. Shared hearts/orbs are counted once across personal loot batches.
- **Q13:** classify each global perk in §8 individually; removing one owner's perk must not clear another's
  world effect. Per-player deck and counters are mandatory regardless of the world-effect choice.
- **Q15:** essence scope; eater's all-player stripping and stone output count; one shared alchemist key
  versus independently progressing N keys; sunseed/stone replication; all-player ending/NG+ consent/transition.
- **Q22:** profile unlock policy (per player or team discovery). Profile flags must never restore a stale
  sun, chest eligibility or other world object over host state.
- Additional temple rule: collapse on first exit (vanilla) or last player exit. Additional music rule:
  stopped playback after reload (vanilla reset) or explicit track resumption.

## 12. Final multiplicity / hook matrix

"Today" means **one vanilla game world**, not EW. Every recommendation is a proposal except D4/D5/D6 and
the explicitly decided R2 rows. N means **N total reward instances**, never N extra copies, and never
N invocations of a destructive world transition.

| mechanic | object multiplicity today | recommended policy (S/N/P/1) | hook point | risks |
|---|---|---|---|---|
| Enemy/drop gold nuggets, all denominations/blood money | one physical drop set per death | **1 object / S income** (D4) | `drop_money.lua:5`, `gold_pickup.lua:5`, nugget init `gold_explosion.lua:3` | extra-money owner; blood heal; death callbacks on replicas; lifetime |
| Gold powder / material gold | terrain cells, natively consumed | **S income**, cells removed once | `player_base.xml:163-166`, native MaterialSucker gold credit (`CDOC:1668`) | **native hook/formula unresolved**; no Lua pickup callback |
| Kolmi reward money statues | one statue per qualifying condition | **1 object / S income** | `rewards/gold_reward.lua:3` | closest player's stats/wallet determine eligibility; no double income |
| Shop/reroll/MONEY_MAGIC spending | one wallet per player | **P** | `ItemCostComponent`; `gun_actions.lua:4841-4856` | must not enter income journal |
| Orb | one per orb id/location/world | **S** (R2.1) | `orb_pickup.lua:4`, native Orb/Item component | native counter order; per-profile heart/spell choice; orb07 ambush |
| Max-HP heart / better heart | one consumed pickup | **S** (D6) | `heart.lua:3`, `heart_better.lua:3` | per-player cap/multiplier; event replay order |
| Evil heart | one +50-HP/poison pickup | **S max HP**, poison pending Q12 | `heart_evil.lua:3` | poison scope; no effect if capped |
| Ordinary full-heal heart | one consumed pickup | **S or P**, Q12 | `heart_fullhp.lua:3` | remote full healing changes balance |
| Temple heart | one +10-HP/full-heal pickup | **S max HP + P heal** recommended | `heart_fullhp_temple.lua:3`, `temple_altar.lua:133` | split compound effect; polymorph callback mismatch |
| Spell refresher | one consumed pickup | **P** recommended | `spell_refresh.lua:3` | personal charges/used state survive rejoin |
| Holy Mountain perks | one 3-choice set | **P** (D5) | `perk.lua:486,169` | global deck/counters and global removal searches |
| Perk reroll | one machine, global escalating price | **P** recommended (Q13) | `perk_reroll.lua:3`, `perk_reroll_init.lua:6` | all-world perk destruction; price global in vanilla |
| Holy Mountain shop | one 10-spell or 5-wand shop | **P** recommended, Q10 | `temple_altar.lua:150`, `generate_shop_item.lua:7,183` | purchase/steal native; personal stock/price persistence |
| Temple collapse/anger/guardian | one world transition/guardian | **S** | `workshop_exit.lua:4`, `temple_check_for_leaks.lua:4` | first exit; world-key identity; no distance bound on guardian marker |
| Pacifist chest reward | one chest if process biome kills=0 | **N equipment**, S gold/hearts | `workshop_trigger_check_stats.lua:3` | team-vs-personal kill accounting |
| Normal/great chest | one chest, one recursive reward manifest | **1 container / N equipment**, shared outputs once | `chest_random.lua:317`, `chest_random_super.lua:137` | profile unlocks; gold/heart duplication; rare Sampo |
| Utility box | one box, utility/modifier reward manifest | **N** (candidate) | `utility_box.lua:188` | separate table, profile unlocks |
| Mimic / leggy mimic | one monster; leggy perk after death | **1 monster**, N special perk if chosen | `chest_leggy.lua:3`, `chest_leggy_death.lua:4` | proximity transformation; replica deaths |
| Steel/dark/light/darkness chest spells | one fixed set on first unlock, random repeats where coded | **N** guaranteed fixed spells | `chest_steel.lua:3`, file-scope `chest_dark/light.lua`, `chest_darkness.lua:4` | consume key/progression once; do not slot-reroll fixed ids |
| Boss equipment/spell drops | one bundle per boss death | **N**; gold/hearts/orbs once | exact callbacks in §5.2 | independent gold/native chest; profile first-kill state |
| Robot Spatial Awareness / moon radar | one perk pickup each | **N** personal grant/selection | `boss_robot/death.lua:10`, `moon_room.lua:35` | ordinary perk pickup globally deletes other perk choices |
| Alchemist key | one portable key per alchemist death | **1** recommended, Q15 | `boss_alchemist/death.lua:29`, `key_music.lua:40` | N keys contend for global machine-done flags |
| Music machines | one physical prop per fixed scene placement | **1** synchronized prop | `music_machine_00..03.lua:3` | PhysicsBody2; playback reset; quest credit separate from audio |
| Kantele / Ocarina | one instrument + two copies of each note | **N** stock sets | `mountain_tree.lua:254`, `ocarina.lua:33` | spells not inside instrument; don't accidentally multiply stock twice |
| Instrument song rewards | once per player-tagged song; alchemy per key | **1 world event** per accepted cast | `magic/kantele.lua`, `magic/ocarina.lua` | detector uses first nearby player, not shooter |
| Tower "great" wands | three offers; take one, delete alternatives | **N choice sets** | `tower_end.lua:33`, `wand_good_pickup.lua:4` | global `wand_good` cleanup; frame-seeded stats |
| Fixed/puzzle named and experimental wands | one reward at each listed origin | **N** selected rows of §7.5 | origin spawn/checker/forge callbacks | fixed identities/always-casts preserved; stats may vary |
| Fixed secret spell sets / moon / summit rewards | one guaranteed set per completed puzzle | **N** selected rows | `towercheck.lua:6`, `moon_altar*.lua`, forge/reward callbacks | global qualification; consume puzzle once; keep fixed ids |
| Sky/hell shops | seeded level-10 stock | **P or N**, Q10/Q14 | `the_end.lua:410-429`, shop generators | random stock, not a guaranteed special spell |
| Ordinary floor/mob wands/spells, potions/pouches | one real item | **1** (R4) | authoritative ordinary spawn/pickup | do not classify by filename alone |
| Essence | one pickup → picker child, global count/flags | **S or N/P**, Q15 unresolved | `essence_pickup.lua:5` | global counters cannot represent owner; fire multiplier |
| Essence eater | one destructible converter | **S event**, explicit stone policy | `essences/away.lua:4` | strips all players, pooled stone counts |
| Portable stones / sunseed / quest tablets | one physical object per origin | **1** default; selected N, Q15 | origin spawns, forge/altar consumption | same filename can be random loot; consumed input ids |
| Sun stages / moon arrangement | one chain per physical seed; persistent profile respawn | **S world state / 1 chain** | replace stage polling; `spot_3_death.lua` | first global egg credit; profile flags spawn suns; supernova global |
| Sampo / The Work | one inventory quest item and ending transition | **1 item / S transition** | `sampo_pickup.lua:3`, replace ending source | native dispatch; rare extra Sampo; first-player effects |
| NG+ | one whole-world regeneration, one player scaled by vanilla Lua | **S transition**, per-player carry-over | `newgame_plus.lua:1`, ending branch | native map-reset semantics; epoch/counter reset; never N-fold |
| Global-world perk effects | one WorldStateComponent | **S** where chosen; personal bookkeeping **P** | `perk_list.lua` functions, §8 | owner contributions/removal; not all effects can simply stay personal |
| Fungal shift / global material conversion | one ordered conversion sequence | **S** | `fungal_shift.lua:103`, other conversion callers | held flask, iteration/cooldown, native conversion persistence |
