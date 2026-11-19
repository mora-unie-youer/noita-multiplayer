# G6: economy and orb accounting — static spike

## Conclusion

**Verdict: needs native DLL** for a provable G6 implementation with receipts for removed cells; Lua interception of nuggets and scripted awards is **feasible**. Confidence: high in the identified arithmetic of this pinned binary, medium in the proposed interception design. No mod/DLL was implemented and no game process was started.
The native powder path credits **one money unit per removed `gold`/`gold_radioactive` cell**, without `money_spent` or the nugget's `EXTRA_MONEY` multiplier. `mGoldAccumulator` serves sound accumulation, not a monotonic income counter.
`Δmoney + Δmoney_spent` separates aggregate income and spending under explicit assumptions; it cannot establish provenance or prevent two clients collecting duplicated terrain. Native orb bookkeeping is distinct from Lua rewards; native/Lua callback ordering remains **unknown** until a live experiment.

## What was checked

- Required project sources: `AGENTS.md`, `DECISIONS.md` D4/D7–D9/D17–D19/D22/D32/D33, `ACCEPTANCE.md` G1–G9/A08/A14/A15, `EVIDENCE.md`; background: `research/vanilla_pickups.md` §1/2/6.3, `SYNC_CONTRACTS.md` §5/6, `LOOT_CATALOG.md`. D32/D33 supersede background questions about recipient multipliers, blood-money healing and profile-dependent orb rewards.
- Paths `scripts/…`, `entities/…`, `materials.xml` below are relative to `.cache/noita-data/data/`. The existing unpacked data was used, not unpacked again. `API:N` / `CDOC:N` mean line numbers in the installed `tools_modding/lua_api_documentation.txt` / `component_documentation.txt`.
- `_version_hash.txt`: `9dbd52ced019a643169a2db02f46c77f8766c6e5`. Binary: `/data/Games/steamapps/common/Noita/noita.exe`, SHA256 **`808d2a0ab51ea0b46e9ad2aeb3327a4b0ce3feae04f32ba26326bf585b5779bd`**, PE i386, image base **`0x00400000`**.
- Read-only Ghidra analysis was opened with eval `await dsm.open("/data/Games/steamapps/common/Noita/noita.exe")`. The managed database incarnation handle was `dsm://ghidra/808d2a0ab51ea0b4-noita.exe/89ef73ef-62fd-4a1e-81c3-16d614d6eea0` (session-scoped, not a durable artifact). Reproduce using that original binary and the opening command, then `await db.view("0x00bbc330:asm")` or `await db.exec("pseudocode(0xbbc330)")`. Source binaries, installation, profiles and EW repositories were not modified. No Noita/Steam/Proton process was launched.
- Automatically generated `FUN_…` labels are not engine source symbols. Function identity was inferred from recovered RTTI/vtables, instructions, strings and component documentation. All addresses below are **VA**; **RVA = VA − 0x00400000**, never the executable's file offset.
- A whole-data search found `WalletComponent` only in `player_base.xml`, `gold_pickup.lua`, `coin.lua`, `gold_reward.lua`, `spawn_rewards.lua`, and `gun_actions.lua`. No vanilla Lua `EntityAddComponent[2](…, "WalletComponent", …)` or additional direct money setter was found. `WalletValuableComponent` is documented (CDOC:2637–2639) but has no found vanilla user.
- `GameGetMoney` / `GameSetMoney` do not occur in the supplied API or unpacked scripts. Use WalletComponent (`API:59–63`); `StatsGetValue` is a statistics reader (`API:212–215`), not a documented credit API.

## Findings

### 1. Wallet writers and statistics

| Path | Fields / arithmetic | Evidence |
|---|---|---|
| Every nugget, including blood money | `money += gold_value × 2^EXTRA_MONEY_stacks`; no spent write | `scripts/items/gold_pickup.lua:15–18,21–33,44–60` |
| Kolmi reward statue | `money += end_gold`, otherwise `8000 × (GameGetOrbCountThisRun()+1)`; no EXTRA_MONEY check | `entities/animals/boss_centipede/rewards/gold_reward.lua:3–20,29–38` |
| Legacy coin | +1 to each found wallet; no calling entity/script found | `scripts/items/coin.lua:3–20`; not evidence of an active playthrough source |
| `MONEY_MAGIC` | `money -= d`, `money_spent += d`; `d=min(floor(money×0.05),24000)`, only if `d>1 && money>=10`, then `d=max(d,10)` | `scripts/gun/gun_actions.lua:4826–4856`; 39 money does nothing, 40 spends 10 |
| Native ItemCost purchase | int64 `money -= cost`, `money_spent += cost` | CDOC:1371–1376; VA `0x00b8ff20`, RVA `0x0078ff20`; explicit paired SUB/SBB and ADD/ADC at VA `0x00b8ff95–0x00b8ffad` |
| Native MaterialSucker gold | `money += removed_gold_cells`; no spent write | CDOC:1668; `player_base.xml:163–166`; VA `0x00bbc330`, RVA `0x007bc330`; credit VA `0x00bbd189–0x00bbd18c` |
| Native WalletSystem | saturation/infinite animation writes money, previous-frame money, infinite flag | CDOC:2630–2635; updater VA `0x00c35270`, RVA `0x00835270` |
| Player creation / restoration | XML WalletComponent is empty, default money=0; load/save/entity replacement can restore state | `entities/player_base.xml:406–407`, CDOC:2632; restore implementation and polymorph transfer were not established here |

The recovered native WalletComponent getter, VA `0x00d1ca00` / RVA `0x0091ca00`, has six found direct caller functions: sucker, affordability, purchase, HUD (`0x00b750b0`) and two statistics snapshots (`0x00b184c0`, `0x00b19d80`). The snapshots compute `gold_all = money + money_spent`, rather than granting money. This strengthens coverage of arithmetic paths, but does **not** exclude generic setters, load/restore or another/inlined getter.

#### Nugget sizes and spawns

There is one `gold_pickup.lua`, not a distinct callback per denomination. All gold nugget XMLs reference it; blood variants inherit the matching base.

| XML under `entities/items/pickup/` | Value / healing data | Found producer coverage |
|---|---|---|
| `goldnugget.xml`, `goldnugget_10.xml` | 10 (`:32–33` each) | Chest/rain/meteor; death/orb/mimic/outhouse/die, detailed below |
| `goldnugget_50.xml`, `_200.xml`, `_1000.xml` | 50/200/1000 (`:32–33`) | Death; chest (`chest_random.lua:114–140`); orb/mimic/rain where supported |
| `_10000.xml`, `_200000.xml` | 10000/200000 (`:33–34`) | `drop_money.lua:116–156`; no other active Lua producer found |
| `_x.xml` | Default 200 (`:40–41`), overwritten after spawn | `drop_money.lua:88–113`: large remainder from 2,200,000; value capped at `2147483647-512` |
| `bloodmoney_10/50/200/1000/10000/200000/x.xml` | Same gold denominations; hp_value=0.16/0.4/0.6/0.6/0.6/1.6/100 (`_10:26–27`, others `:24–25`) | Trick prefix `drop_money.lua:54–60`; hp-drop `_50` `:63–71`; greed orb 10/50/200/1000 (`gold_orb_greed.lua:33–43`) |

Healing is `hp_value×0.5`, capped at max HP (`gold_pickup.lua:57–60`, `scripts/game_helpers.lua:10–18`); log internal HP units. D32 makes this picker-only, while money is shared using each **recipient's** multiplier, not the picker's multiplier for everyone.

`drop_money.lua:5–39` derives gold from max HP, greed curse and amount_multiplier. Normal death calls `do_money_drop(1,false)` (`:180–182`). The `drop_money2x/4x/8x/16x/32x/64x.lua:1–6` variants load the same file and call with their multiplier and `trick_kill=true`; they are not extra wallet writers. The engine's dynamic choice of variant remains a live-test item. `no_gold_drop` cancels generation (`:12–17`); the callback does not inspect `drop_items`.

#### Shops, modifiers and flags

- `base_shop_item.xml:4`: cost=100. Legacy `shop_potion.xml:7` / `shop_cape.xml:7`: 150; `shop_wand_level_01.xml:7`: 350. Holy Mountain instead calls `generate_shop_item` / `generate_shop_wand` (`scripts/biomes/temple_altar.lua:150–184`), attaching ItemCost and pickup FX (`generate_shop_item.lua:145–153,294–302`). `shop_effect.lua:3–7` creates FX, **not** a debit.
- Other shops use the same generator (`scripts/biomes/shop_room.lua:59–65`, `scripts/biome_scripts.lua:249`). The duplicator sells a copied card at `floor(action.price×1.2)` (`scripts/buildings/temple_duplicator.lua:23–29,43–50`). Personal/shared scope does not change the native debit mechanism.
- Reroll XML cost=400 (`entities/items/pickup/perk_reroll.xml:2–3`) is overwritten with `200×2^TEMPLE_PERK_REROLL_COUNT` (`scripts/perks/perk_reroll_init.lua:3–7,34`). Pickup regenerates perks and the machine (`perk_reroll.lua:3–10`), increasing the count (`perk.lua:559–561`). The new machine's cost is not the price just paid.
- `stealable` checks an area called shop (CDOC:1374). Exact cost removal/disable and callback ordering during theft remain unmeasured. A zero debit is not evidence of missed income.
- `GameAddFlagRun` changes run flags (`API:278–280`, CDOC:2695), not a wallet or income counter. `exploding_gold` (`perk_list.lua:152–169`), `greed_curse` (`scripts/magic/greed_curse/greed_curse_pickup.lua:22`) and `greed_curse_gone` (`greed_crystal_death.lua:24`) change future drop/effect behavior.
- `GOLD_IS_FOREVER` writes `perk_gold_is_forever` (`perk_list.lua:97–124`, CDOC:2676). `drop_money.lua:48–52` and `chest_random.lua:95–100` read it; `load_gold_entity` removes LifetimeComponents (`game_helpers.lua:28–39`). This is not proof that every unrelated producer makes permanent gold.
- `EXPLODING_GOLD` init (`scripts/perks/gold_explosion.lua:3–60`) creates an explosive child and reads gold_value but never writes a wallet; `drop_money.lua:158–177` adds collision triggers. Replica explosions can create repeat deaths/income even with idempotent pickup claims.
- `StatsGetValue("gold"/"gold_all")` is read in `spawn_rewards.lua:9–10`, then replaced by the **closest player's** money / money+spent (`:26–33`). A `progress_nogold` profile achievement is not an income transaction or live gold counter.

### 2. Native powder collection

CDOC:1661–1677 documents `material_type=0` liquid, 1 sand, 2 gas; `barrel_size=50`; `num_cells_sucked_per_frame=1`; `mAmountUsed` as fullness; `suck_gold` as “just suck gold and update wallet”; `suck_health`; `suck_static_materials`; `suck_tag`; private `mGoldAccumulator` and `mLastFramePickedGold`.
Bracketed documentation ranges are not reliable hard runtime limits: powder stash sets 10 cells/frame and vacuum powder 100 (`powder_stash.xml:62–71`, `projectiles/deck/vacuum_powder.xml:91–100`).

#### Reproducible binary evidence and its limits

| Link | VA / RVA | What was actually observed |
|---|---|---|
| Recovered RTTI updater lambda vtable | `0x0102af08` / `0x00c2af08` | `_Func_impl<…,void,MaterialSuckerComponent*,unsigned int>`; slot 2 points to `0x00d91410` |
| Lambda call | `0x00d91410` / `0x00991410` | **Instructions:** component enabled/entity checks, then CALL `0x00bbc330` |
| Sucker update | `0x00bbc330` / `0x007bc330` | **Decompiler reconstruction:** special gold/health branch, material-name resolution, scan window, cell-removal indirect call and count |
| Wallet credit | `0x00bbd173–0x00bbd18f` / `0x007bd173–0x007bd18f` | **Instructions:** wallet lookup CALL `0x00d1ca00`; skip if null; `MOV EAX,ESI; CDQ; ADD [ECX+0x48],EAX; ADC [ECX+0x4c],EDX`; then accumulator `ADD [EBX+0x8c],ESI` |
| Sound threshold | `0x00bbd195–0x00bbd1bf` / `0x007bd195–0x007bd1bf` | **Instructions:** accumulator compare, branch, `"sound_pick_gold_sand"` string and call |
| Wallet update | `0x00c35270` / `0x00835270` | **Instructions and constants:** growth comparison, saturation, flag and previous-frame writes; details below |
| Native purchase | `0x00b8ff20` / `0x0078ff20` | **Instructions:** subtract cost at wallet +0x48/+0x4c; add cost at +0x50/+0x54 |

For reproduction, obtain complete `asm()` and `pseudocode()` for these function addresses; `asm()` accepts an interior address but returns the containing function. The exact material match, loop bounds and virtual cell-removal semantics below are **decompiler-derived static conclusions**, not separately instruction-by-instruction proofs or runtime observations. The +count wallet instruction sequence is directly verified. Names of private fields and the inferred object offsets are corroborated by CDOC/XML but are **not a supported ABI**.

The update's reconstructed control flow:

1. If `suck_gold || suck_health`, use a **special branch**, not ordinary MaterialInventory filling. It scans x=-5…5 and y=1…4 relative to the entity's integer position: at most **44 cells/update**. This branch does not use material_type, barrel_size, num_cells_sucked_per_frame, randomized_position, mAmountUsed or suck_tag.
2. Gold matching resolves exact strings **`gold`** (bytes at VA `0x0100148c`, RVA `0x00c0148c`) and **`gold_radioactive`**; it does not match `[gold]`. Each matching cell goes through an indirect world cell-removal call, increments the local count and sets last-picked frame. No match for gold_molten, gold_static or gold_box2d was seen in this branch.
3. After the scan, count>0 credits the owner's wallet **one unit per cell**. There is no EXTRA_MONEY lookup, spent write, capacity check or MaterialInventory write in the recovered gold-credit branch. This is bounded evidence about this function, not a global proof that no other system can modify those fields later.
4. Cells have already been removed when wallet lookup happens. A sucker proxy without a wallet can therefore destroy gold without a credit. The count is also added to mGoldAccumulator and used for sound; after a sufficiently long gap it resets to zero. Binary strings include `AUDIO_PICK_GOLD_SAND_MIN_AMOUNT_FOR_SOUND` / `…AMOUNT_ACCUMULATION_FRAMES`; this is not a fractional-money remainder.
5. The **ordinary branch**, selected when both special flags are false, checks material type/tag and capacity, fills inventory and increments mAmountUsed. Flask barrel=1000 (`potion.xml:68–80`), pouch barrel=1500 and sand type (`powder_stash.xml:62–74`). Player_base has a sucker and wallet but **no MaterialInventoryComponent**. Gold held in a pouch is not money already earned.

This corrects any proposal to collect every `[gold]` material: `gold` is `cell_type="liquid", liquid_sand=1` (`materials.xml:9924–9949`); radioactive gold inherits it (`:9951–9959`). `gold_molten` is liquid (`:7785–7817`), and even molten steel/other molten metals carry `[gold]` (`:7819–8134`). Static gold (`:4732–4748`) and gold_box2d/bloodgold_box2d (`:13803–13837`) have that tag too. Tag-wide collection would change the economy.

**Wallet saturation:** recovered updater RTTI vtable `0x01041e20` → `0x00d93400` → `0x00c35270`. On growth relative to mMoneyPrevFrame, it compares against double **2147483135** (constant VA `0x01053d78`), sets money=**2147483647** and mHasReachedInf=true at/above it. While true, it restores money to 2147483647 each update. Finally it copies money into mMoneyPrevFrame. These conditions and constants were inspected in asm, not only the decompiler.

A world-state byte at +0x1a4 additionally enables growth toward cap: `v=(money+1)+(2147483647-(money+1))×0.05` before the threshold test (constants 1.0/0.05/2147483647 inspected). **[INFERENCE]** this byte belongs to the EVERYTHING_TO_GOLD/infinite-ending path; the field identity was not established from an ABI definition. CDOC:2634–2635 documents special wallet handling; CDOC:2681 and `sampo_start_ending_sequence.lua:275–280` document INFINITE_GOLD_HAPPENING; Midas calls `ConvertEverythingToGold()` (`API:341`). These animations/clamps must not become GoldIncome. A purchase under infinite state can produce `Δmoney=0, Δspent=cost` after the next update.

**Native-hook limitations:** addresses are for this hash only. Decompiled parameters/calling conventions are incomplete (register parameters and indirect calls are not fully typed); private offsets must not become a public struct definition. No detour boundaries, instruction relocation, control-flow coverage under mods, reentrancy, ABI compatibility or actual callback ordering were validated. All failure/error paths, generic restore writes and other engine systems remain outside the bounded claim.

### 3. Income-source matrix

“Interception” is a design **[INFERENCE]**, not an implemented mod. `GoldPickup` means suppress the original wallet write, claim one NID, then apply one server income under D32. Producer events are not themselves credit events.

| Source | Mechanism and file:line evidence | Interception | N-player double-count risk |
|---|---|---|---|
| Normal/trick death, bosses inheriting death script | Lua nuggets 10/50/200/1000/10000/200000/x; `drop_money.lua:5–182`, `base_humanoid.xml:17–21` | Death authority + GoldPickup | N callbacks create N physical batches; repeated NID claims must not pay |
| Blood money | Lua same pickup; `drop_money.lua:54–71`, `gold_orb_greed.lua:33–43` | GoldPickup + separate picker-only heal | Do not make N copies then share each; do not heal all |
| Ordinary chest | Lua 50/200/1000 and ordinary10; `chest_random.lua:89–141`, delay `:46–56` | One shared gold roll + GoldPickup | N personal gold rolls violate D36/D32 |
| Great chest / all-spells rain | `chest_random_super.lua:47–48`, `scripts/projectiles/all_spells_stage.lua:67` → `rain_gold.lua:6–59`, 10/50/200 | One rain producer + GoldPickup | N projectile producers or reopening create more gold |
| Kicked gold orb / greed orb | `gold_orb.lua:3–52`, 10/50/200; `gold_orb_greed.lua:3–50`, up to1000 | One kick outcome + GoldPickup | Frame/entity-seeded RNG; replay outcome, not kick |
| Die/greed die | `greed_die_status.lua:77–103`:16×10,12×50,8×200 or two great chests | One outcome + GoldPickup | Replica rolls must not run independently |
| Greed meteor | `scripts/magic/greed_curse/greed_meteor.lua:10–21`: six10/10/50 variants | One producer + GoldPickup | Repeated projectile/update multiplies spawns |
| Charmed mimic | `scripts/animals/mimic_charm.lua:44–59`, 10/50/200 | Gift ordinal + GoldPickup | Repeatable source needs ordinal, not entity NID alone |
| Empty charmed wand | `scripts/animals/wand_charm.lua:27–48`:gun_level×200 | Atomic consume-wand/drop + GoldPickup | Repeating conversion of one wand repeats income |
| Outhouse | `scripts/buildings/huussi.lua:3–24`:200 or batch10 after first unlock | Activation ordinal + GoldPickup | Profile differences alter branch; no N shared monetary outcomes |
| Kolmi statues | `rewards/gold_reward.lua:3–38`; selection `spawn_rewards.lua:6–33,58–140` | Separate suppress/replace callback | N statues or differing orb counts produce N income events |
| Legacy coin / debug nugget | `coin.lua:3–20`; `entities/_debug/debug_menu.lua:574` | Ledger if debug is enabled; not active production evidence | Debug must not bypass accounting |
| Veins / gold biome | Gold material, native sucker; `biome/coalmine.xml:182`, `biome/gold.xml:21`; modifier `scripts/biome_modifiers.lua:190–214` | Cell-removal commit | Same cell in N terrain replicas can pay N times or return after rollback |
| Flask/pouch gold | `potion_secret.lua:14`, `powder_stash.lua:49`, `potion_starting.lua:28`; inventory→world→native sucker | Inventory consumption + terrain revision + removal | Cannot credit contents and preserve them in inventory snapshot |
| Chest dust, tablet/hourglass, Touch Gold, fungal shift | Chest branch `chest_random.lua:59–310` → `entities/misc/chest_random_gold_dust.xml:2–10`; `scripts/magic/altar_tablet_magic.lua:270`; `entities/buildings/hourglass_blood.xml:2–12`; `entities/projectiles/deck/touch_gold.xml:2–18`; `fungal_shift.lua:22,157–158` | One material conversion, then native removal | Repeated conversion or different material tables changes available income |
| Real-gold emitters / Midas / Sampo / curses | E.g. `entities/misc/custom_cards/midas.xml:24–44`, `money_magic.xml:20–44`, `entities/animals/boss_centipede/sampo.xml:116`, ending/midas.xml:9 | Authoritative real cells, then removal | Replica visuals must not create real particles; emitter ≠ credited income |
| Gold visual FX | `entities/particles/gold_sparks.xml:3,25–26`:real=0,cosmetic=1; CDOC:1714–1718 | No credit | Counting drawn particles fabricates income |
| Streaming rain | `scripts/streaming_integration/event_list.lua:987–998` → special rain entity | If enabled: one external event ID + GoldPickup | Do not execute external event independently N times |
| Infinite ending | Native WalletSystem; `ending/midas_convert_everything_to_gold.lua:1` | Explicit infinite state, not GoldIncome | Never share cap-approach frame deltas as income |

### 4. Wallet-delta classifier: diagnostic, not authoritative

Proposed **[INFERENCE]** sample on fixed pre/post-frame boundaries: `(entity_generation, wallet_component_id, frame, money=M, spent=S, native_prev, infinite)`, plus sucker flags/accumulator/lastFrame, source-hook events and **our own ledger writes**. API:185 supplies frame number, not its ordering relative to native systems; measure that ordering first.

For a stable entity without cap/restore and with the listed writers:

`ΔM = income - expense + known_adjustment`, `ΔS = expense`.

Thus `expense_observed=ΔS`, `income_candidate=ΔM+ΔS-known_adjustment`. Subtract precisely known ACK/replay/restore writes, including any S adjustments. Otherwise applying shared income is rediscovered as new income: a feedback loop.

| Signature without our writes | Classification | What it cannot prove |
|---|---|---|
| ΔM=+10,ΔS=0 | income10 | Nugget, powder10, scripted+10 are indistinguishable |
| ΔM=-6,ΔS=+6 | expense6,income0 | Which item/spell was paid |
| ΔM=+4,ΔS=+6 | income10,expense6 | Sign-only classifier loses6 income |
| ΔM=-6,ΔS=+10 | income4,expense10 | Sign-only classifier misses income completely |
| ΔM=0,ΔS=+10 | income10,expense10 **or infinite clamp** | Net zero is not “no operations” |
| ΔS<0, negative candidate, entity replacement | ResetOrUnknown, not income | Do not use abs/max(0,delta) to conceal it |

Source attribution without engine hooks is possible for **known Lua writers**: nugget/statue/coin item_pickup wrappers, MONEY_MAGIC action wrapper, shop/reroll pickup callback plus previously captured cost/NID. Diagnostic wrappers sample M/S immediately around the callback; production wrappers suppress vanilla credit and submit claims. The shop_effect callback is not guaranteed to run before native debit/cost removal.

After subtracting known scripted grants, the residual is **UnattributedNativeIncome**, not automatically powder. In clean vanilla without ending/restore it can be correlated with sucker state. But accumulator resets, carries previous frames, and exists for sound; lastFrame gives a frame, not coordinates/revisions. Neither it nor Δ(M+S) proves two clients did not remove the same duplicated cell.

**Cancellation/replacement options:**

- `ModLuaFileAppend` / `ModTextFileSetContent` (`API:366,370`) intercept Lua writers, not native credit. Do not invoke the original wallet write and then also apply the server reward.
- `ComponentSetValue2(sucker,"suck_gold",false)` suppresses gold matching while health remains enabled; alternatively disable the whole component. With **both** special flags false it falls into ordinary collection, not a no-op. Suppression alone is not a replacement collector.
- Own DLL: hook special gold-cell removal inside `0x00bbc330`, collect exact `(x,y,material_id,region_epoch,revision,collector_slot)`, commit removals once under terrain authority, suppress the original wallet credit at `0x00bbd189`, journal `PowderCollected{op_id,cells,count}` → GoldIncome. Preserve sound bookkeeping separately. Pin hash and verify hook signature; do not use blind addresses.
- Alternative **[INFERENCE]**: replacement collector with native cell API and suck_gold=false, reproducing the scan/removal/1:1 semantics. `ConvertMaterialOnAreaInstantly` (`API:343`) returns no removed count/list and is not a sufficient receipt interface.
- Post-frame `money -= candidate` cannot undo cells, earlier shop-afford checks, infinity or another client's duplicate collection. It is a measurement technique, not a G6 solution.

Do not accidentally extend the nugget multiplier to powder/statues: vanilla multiplies only the nugget callback. Under D32 retain base_amount/source_kind/explicit multiplier policy and each recipient's perks **at event time**, not at offline replay time. Applying personal multipliers to powder/statues may be the chosen D32 interpretation, but is a deliberate departure from these vanilla paths, not something inferred from wallet deltas.

### 5. Orb accounting and remote application

| Layer | Writer / evidence | Required D33 result |
|---|---|---|
| Run identity/set | `WorldStateComponent.orbs_found_thisrun` VECTOR_INT32 (CDOC:2694); native OrbSystem handler VA `0x00bc6700` / RVA `0x007c6700` | Same **set**, not count alone; idempotent OrbPicked |
| Native run insertion | Handler asm checks ID via `0x00bc61e0`, then VA `0x00bc6795–0x00bc67a4` loads world-state pointer, LEA vector+0xe4 / orb_id+0x48, CALL `0x0090f210` | Native event must update the run set once |
| Run getters/reset | API:101–104; `GameGetOrbCountThisRun` → VA `0x00bc6850` (vector length); CollectedThisRun→`0x00bc68e0` (ID lookup); Clear→`0x00bc6980` (end=begin) | Same values before Kolmi/Sampo/reward; one NG+ reset |
| All-time profile | API:100,103; getter wrappers `0x007aacd0`, `0x007ab560` read separate profile map DAT_01207404; native handler invokes profile insertion path `0x0083cfe0` | Real per-profile native state, not only run vector |
| Lua progress/reward | `orb_pickup.lua:19,22–60`:progress_orb_1/evil; first pickup progress_orb_pickup_XX+card, otherwise heart; ≥11 all-time progress_orb_all+ALL_ORBS | Reward from each recipient's **pre-event profile**, not host profile |
| Scream/FX/trap | `orb_pickup.lua:64–73`; orb7 `orb_07_trap.lua:12–33` | Accounting replay must not spawn another reward/ambush |

Reproduce with `asm(0xbc6700)`, `pseudocode(0xbc61e0)`, `pseudocode(0xbc6280)`, `pseudocode(0xbc6850)`, `pseudocode(0xbc68e0)`, `pseudocode(0xbc6980)`. Subtract image base for RVA, e.g. getters RVA `0x007c6850`, `0x007c68e0`, `0x007c6980`. Handler identity is supported by recovered `MessageListener<OrbComponent,Message_ItemPickUp,27>` and OrbSystem vtables; template dispatch and listener ordering were not instrumented.

`orb_base.xml:24–39` contains ItemComponent.enable_orb_hacks=1 and OrbComponent, with Lua callback `:118–120`; orb IDs must be unique (CDOC:1710). Neither `GameAddFlagRun("orb_…")` nor `AddFlagPersistent("progress_orb_pickup_00")` updates that native run vector/profile map. The boss-dead GameAddFlagRun at `orb_pickup.lua:63` is **commented out**; its active flag read controls the scream. No Lua writer of orbs_found_thisrun was found. API:60 does not promise vector writes; specialized vector APIs:64–66 are readers.

Decompiler reconstruction of native `0x00bc6280` normalizes out-of-main-bounds IDs below100 by adding **128 west / 256 east**. This is not world_index×11; distant parallel worlds must not automatically become new orb accounting IDs. `orb_init.lua:14–31` / `orb_pickup.lua:22–30` treat IDs≥100 as evil. Exact bounds, load order and effects of injecting an already-canonical ID remain live-test subjects.

**Remote apply [INFERENCE]:** commit `OrbPicked{run_epoch,canonical_orb_id,source_nid}` on the server. Make real replica orbs non-pickable before contact; a Lua reward wrapper is not a proven veto of native effects. If the local run set lacks the ID, reproduce native pickup using a synthetic OrbComponent with the exact ID and enable_orb_hacks, **without replaying reward/trap callbacks**. `GamePickUpInventoryItem(local_player,orb,true)` (`API:124`) is a candidate, not a demonstrated guarantee. Queue while offline or in incompatible polymorph and fence dependent boss/ending initialization until accounting agrees.

Apply personal Lua rewards separately once from a saved pre-event profile outcome, including progress flags/achievement. The resulting heart is a **personal orb reward** under D33, not another shared-heart event: N personal hearts must not each grant all players HP. Handle evil effects personally too. Query native set/count/profile after apply before acknowledging the cursor; crash-after-effect-before-cursor requires query-before-replay, not blind repeat pickup.

EW reference-only: `~/noita_entangled_worlds_fix/quant.ew/files/system/orb_sync/orb_sync.lua:9–11,16–36,48–50` reads the vector, constructs synthetic auto-pickup orbs and defers while polymorphed. This shows a community technique, not our implementation, not evidence of callback ordering and not permission to copy its code (D7).

## Risks and options

- Δ(M+S) provenance is ambiguous even with perfect sampling. A sufficient income ID is a nugget/statue NID or committed cell-removal operation, not `(frame,player,delta)`.
- Removal receipts must reference region revisions/authority epochs. Reconnect/snapshot must not resurrect paid cells; two operations against the same source revision cannot each pay them.
- `_x` receives its final gold_value **after** EntityLoad. A gold_explosion creation hook may see default200. Assign NID at creation, but freeze value after producer completion or at accepted pickup.
- Native infinity is below int64 capacity. int64 storage does not justify unbounded money writes. Store exact ledger integers and model native infinity separately.
- Polymorph/death/reload change entity generation. Restored balance is not new income; dead/offline slots accrue server income, bots collect none (D27).
- Boss/chest callbacks combine items, material gold, native ItemChest and scripts. N personal item rewards do not mean N executions of the whole callback.

## What remains to verify in game

**None of these experiments was run.** Use only the disposable D17 rig after explicit launch permission. Log build hashes, pre/post/native-system frames, M/S/prev/infinite, sucker fields, orb getters, region revisions and server sequence. Expected native results below are static predictions, not observed gameplay. Mismatches fail the hypothesis; do not tune the classifier to hide them.

| # | Inputs / action | Expected observable; pass/fail |
|---|---|---|
| 1 | Isolate1 then44 gold cells exactly in x=-5…5,y=1…4 window; M=S=0,no perks | Sucker removes1/44 and M=1/44,S=0; outside cell remains. Fail if rate/conversion differs |
| 2 | Repeat with gold_radioactive, gold_molten, gold_static, gold_box2d, molten steel | Only first pays1/cell; others not removed by gold branch and pay0. Prevent cooling/destruction into another material |
| 3 | Window44gold; vary type0/1/2,barrel0/1/50,cells_per_frame0/1/5,mAmountUsed=barrel | Special branch still +44,S=0; mAmountUsed does not become+44. Measure native update, not HUD |
| 4 | Gold with EXTRA_MONEY stacks0/1/2; nugget10 with same | Powder +1/+1/+1 per cell; nugget +10/+20/+40. Shared nugget: A stacks0,B1 → +10/+20, not picker multiplier for everyone |
| 5 | Collect3 over frames, pause, collect2; read sound MagicNumbers | Total M+5; accumulator grows/resets by interval; lastFrame changes only on gold. It must not be treated as money lost at reset |
| 6 | Disable gold with health flag still true; gold+healthium; separately disable whole sucker | First leaves gold/money untouched and retains health path; second collects neither. Both-flags-false must not accidentally select ordinary collector |
| 7 | Put100gold in pouch, pour10 into player window; vacuum/container transfer | Storage/pouring alone do not pay; actual player removal10 pays10. No credit while merely transferring inventory |
| 8 | Buy cost6 at M10,S0; reroll cost200 at M1000; take stealable item outside shop | Purchase M4,S6; reroll M800,S200,new price400. For theft measure native free/cost decision and callback/cost-removal order |
| 9 | Same frame income10 and buy6 at M10,S0; then income4 and spend10; then income10/spend10 | First M14,S6 → income10/spend6; second ΔM=-6,ΔS10 → income4; net0 case must retain both events. Repeat reversed order |
| 10 | MONEY_MAGIC at M39/40/1000/1000000,S0 | ΔM/ΔS=0/0,-10/+10,-50/+50,-24000/+24000. Income0; remote wallets do not pay |
| 11 | Every nugget/blood variant; `_x` explicit1234567; HP belowmax; simultaneous/repeated claims N2/4 | Exact one credit, x not200; picker-only hp_value×0.5 healing; no second credit/heal on retry |
| 12 | M1000; default statue at0/1orbs; statue end_gold70000 | +8000/+16000/+70000,S unchanged; repeated NID does not pay; selection uses chosen authoritative stats |
| 13 | M2147483134→+1; separately infinite=true and buy6; ending conversion | After wallet update M2147483647/infinite=true at threshold. Determine clamp/order/S/stat semantics; never classify clamp restoration as new income |
| 14 | Two players/terrain replicas in same44-cell pile; repeat receipt; reconnect; boundary authority/crash before/after commit | Exactly44 physical removals and one base44 shared income, not88; no cells/payment return on reload. Fail otherwise |
| 15 | Nugget,shop debit,powder,server-ACK write same frame; then polymorph/reload with balance | Subtract own writes; no share-feedback; generation change is restore, not income; unknown writer yields Unknown |
| 16 | First Orb00 on fresh profile; log before/entry/exit Lua callback/nextframe | Run0→1/collected=true; determine all-time update moment; first reward SEA_LAVA and progress. Establish native/Lua ordering |
| 17 | A profile does not know Orb00,B knows; accepted shared pickup; offline B/repeated delivery | Same set{0}; A one SEA_LAVA,B one personal heart; count1 after apply; no duplicate reward/ambush. Also test11th all-time orb/achievement |
| 18 | Synthetic orb accounting without Lua reward; GamePickUpInventoryItem effects=true/false; repeatID | Identify path updating run+profile exactly once without extra reward; if false does not count, reject it for accounting injection |
| 19 | Main/east/west Orb00,distant east; canonical IDs128/256 injected near main-world player | Check IDs0/128/256,evil rewards/native set; canonical ID must not be offset twice; do not assume farther world adds identity |
| 20 | Accepted orbs before Kolmi load; offline replay; NG+ clear then sameID in newepoch | Kolmi orbcount matches (`boss_centipede_update.lua:43–57`), reward8000×(count+1); NG+ clears run once, keeps all-time, permits newepoch pickup |
