# G8 static spike: cast classes and effect ownership

## Conclusion

**G8 remains open.** The pinned vanilla scripts already disprove the assumption that an identical wand plus one spread seed is sufficient for faithful remote replay. They also show why setting projectile damage to zero does not make a replica cosmetic. The class matrix below defines the minimum live experiments; it is not a working replication implementation.

No Noita, Steam, Wine or Proton process was launched. No FPS, latency, RNG-equivalence or damage measurement is claimed.

## Sources and method

Inspected 2026-11-19 for game build `9dbd52ced019a643169a2db02f46c77f8766c6e5`. `V/` below means `.cache/noita-data/data/`; `API` and `CD` mean the installed `tools_modding/lua_api_documentation.txt` and `component_documentation.txt`. These files were read, not executed. This extends [the earlier casting inventory](../vanilla_world_and_saves.md#6-projectiles).

Stock Lua has no configured language server in this workspace; source locations below were found by direct file inspection. No EW implementation was used or copied for this report.

## Confirmed boundaries

1. **The gun VM carries state between shots.** `_start_shot` copies gun configuration into the root shot and orders the deck only on the first shot (`V/scripts/gun/gun.lua:454–483`). Drawing can reload; reload moves hand/discarded cards back to the deck and shuffles (`:486–535`). `order_deck` seeds shuffle with `GameGetFrameNum()` (`:170–187`). Inventory XML alone does not describe this running VM state.
2. **A trigger is a nested shot, not just a projectile filename.** `add_projectile_trigger_timer`, `_hit_world` and `_death` call `draw_shot(create_shot(action_draw_count), true)` inside `BeginTrigger*`/`EndTrigger` (`gun.lua:335–371`). The API exposes those builders (`API:386–392`), not a documented export of a running trigger tree.
3. **Even an ordinary spark has variable flight parameters.** `light_bullet.xml:12–29` sets speed 750–850, lifetime 40 with randomness 7 and explosion-on-death/lifetime. Its base velocity affects physics bodies (`V/entities/base_projectile.xml:3–5`). A damage-free copy may still push bodies or explode unless those effects are separately excluded.
4. **Random choices occur at several layers.** `SUMMON_EGG` uses the current frame (`gun_actions.lua:2338–2343`); `RANDOM_PROJECTILE` uses frame plus deck length and filters on persistent unlock flags (`:8850–8874`). `random_explosion.lua:3–19` seeds from the local projectile entity ID and chooses a collision-spawn entity. `teleport_cast.lua:3–16` reads nearby `homing_target` entities and frame-dependent RNG. A spread seed does not replace any of these inputs.
5. **Stock callbacks have narrower contracts than a complete cast capture.** `LuaComponent.script_shot` receives a projectile entity ID; `script_wand_fired` receives the gun entity ID (`CD:1534,1549`). `GameShootProjectile` accepts an already-created projectile and warns that throwing physics items disables components without `enabled_in_world` (`API:138`). Neither contract by itself promises a serialized nested-shot tree or correct restoration of gun-VM state.
6. **Attribution is not network identity.** `ProjectileComponent.mWhoShot` and `mShooterHerdId` are local shooter metadata; `mEntityThatShot` describes a trigger parent but is explicitly documented as not properly tested (`CD:2289–2293`). Remap references to network identities instead of sending process-local entity IDs as durable IDs.

## Representative class matrix

These are representative vanilla paths, **not an exhaustive spell-support list**. A class label must consider the whole cast, modifiers and descendants; a simple root projectile does not make a trigger carrying water or a summon harmless.

| Class / example | Static evidence | Candidate replication boundary `[INFERENCE]` | Failure to detect in G8 |
|---|---|---|---|
| Ordinary projectile: `LIGHT_BULLET` | `gun_actions.lua:44–62` adds `light_bullet.xml`; variable flight/explosion fields above | Capture resolved launch parameters; visual flight can be corrected separately from the target-authority damage path | Missing/double hits, divergent speed/lifetime, replicated impulses |
| Hit trigger: `LIGHT_BULLET_TRIGGER` | `gun_actions.lua:65–82`, `gun.lua:348–358`; one child draw on world hit | Preserve parent/child identity and resolve a single authoritative child/effect result; compare replay with captured outcome | Different collision point produces different child shot or duplicates terrain damage |
| Timer trigger: `LIGHT_BULLET_TIMER` | `gun_actions.lua:104–121`; timer 10 frames, one child draw | Carry relative remaining timer and child payload; distinguish shot creation from delayed activation | Late packets restart timer, omit payload or execute it twice |
| Physics projectile: `SUMMON_ROCK` | `gun_actions.lua:2308–2323`; `rock.xml:5–22` has a physics body and image shape | Treat the physical result as an authoritative body/entity, not an additional independently colliding cosmetic object | Duplicate body, wrong mask or impulse; G5 dependency |
| Material projectile: `MATERIAL_WATER` | `gun_actions.lua:3191–3208` also adds a wet-effect entity; `material_water.xml:8–29` has zero damage but emits water on death | One terrain/effect authority creates water; cosmetic path must not emit additional cells or independently apply statuses | Water doubles even though numeric projectile damage is zero; G3 dependency |
| Teleport: `TELEPORT_PROJECTILE` | `gun_actions.lua:3422–3439`; `teleport_projectile.xml:12–14,16–33` combines zero damage with `TeleportProjectileComponent`; `CD:2493–2499` links shooter and reset velocity | Commit one player movement/effect, then render its projectile; do not leave an active teleport component on a cosmetic replica | Wrong player moves, player moves twice, replicas disagree about destination |
| Summon/random spawn: `SUMMON_EGG` | `gun_actions.lua:2326–2344` chooses one of four egg entities using frame RNG | Replicate the resolved egg type and authoritative spawned lineage, not a second RNG draw | Different creature, duplicate creature/drop or ownership gap |
| Modifiers/random actions | `TRANSMUTATION` appends extra entities and draws a further action (`gun_actions.lua:6259–6275`); random-action and projectile scripts above | Capture resolved decisions/side effects or use a measured class adapter; preserve the full composition | Modifier introduces hidden material/spawn effects into a supposedly cosmetic class |

## Design constraints, not new product decisions

D31 accepts a hybrid and records the user's preference for visual fidelity. These findings do not change that decision or accept dishonest damage. The live comparison must measure both visible trajectories and authoritative outcomes.

- Use `(world_epoch, cast_id, projectile/child_index)` and a separate hit index where repeated hits are legal. A cast-level dedup key alone loses piercing/DoT/trigger events (`SYNC_CONTRACTS.md`, section 9).
- Choose exactly one damage path per attack/target: local collision on the target authority **or** an accepted damage request, never both.
- Separate actor ownership from terrain ownership. A cast may hit several regions; the owner of its shooter is not automatically authorized to replace every affected area's state.
- Cosmetic replication must exclude material emission/conversion, teleport, status application, physical impulses, summons, secondary gameplay shots and death drops, not only `ProjectileComponent.damage`.
- Do not silently drop an unclassified cast or substitute a harmless placeholder. Before claiming coverage, give each class a demonstrated authoritative result path; unknown compositions remain an explicit implementation blocker.

## What remains to verify in-game

Only after explicit launch permission, on disposable isolated profiles (D17). Capture game/native/mod hashes, both frame counters, cast/child IDs, resolved parameters, target HP/statuses, body counts, terrain revisions and reward IDs.

| Experiment | Pass/fail criterion |
|---|---|
| Same wand at different local frame counters: no-shuffle, shuffle, then `RANDOM_PROJECTILE` | Record actual selected cards/children and trajectories. Pass only if the chosen capture/replay mechanism explains divergence and produces the intended shared result; no equivalence inferred from one matching screenshot. |
| Spark + hit trigger carrying another projectile; timer trigger carrying water; nested triggers | Each logical child/effect is produced once with correct parent and relative timer despite delayed/reordered delivery. Missing/extra child, extra water or double damage fails. |
| Material-only zero-damage spray and teleport, with cosmetic copies enabled | Exactly one water creation and one player teleport; cosmetic replicas create no cells, move no player and apply no extra status. Numeric zero damage is not a pass condition. |
| Rock impact near two players and across an area boundary | One authoritative physical body/result, no second impulse, correct later state after transfer. Verify changed masks through G5 rather than transform equality. |
| Egg spawn and random-explosion modifier at different entity IDs | The resolved egg/child type and spawn lineage agree; one creature and one eventual drop, including after reconnect. |
| Repeated/piercing hits and friendly-fire damage/states on/off | Intended repeated hits remain valid, duplicate deliveries do not add hits, one selected damage path applies. Record exceptions to best-effort status suppression under D30. |
| Repeat at 0 and approximately 30 ms link latency (D20) | Compare trajectory/timing error separately from HP/status/material results and capture frame cost. Select per-class strategy from these observations, not from this static table. |

No G8 experiment above has been run. There is no headless claim of faithful simulation, and no synthetic benchmark is presented as game performance.
