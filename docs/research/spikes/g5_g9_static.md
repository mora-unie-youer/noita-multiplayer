# G5 / G9 static spike: physics geometry, remote generation and cell state

## Conclusion

- **G5: needs native access; high confidence for the documented-Lua limitation.** Lua exposes body identities, motion and pixel counts, but not the surviving pixel mask, fixture polygons or a complete fragment set. Whether independent NoitaPatcher is sufficient remains **unknown**.
- **G9: feasible design candidate, not a passed gate.** A remote client must be allowed to generate under a host-server lease; “only the host's Noita spawns” is insufficient. Callback timing, coverage and duplicate suppression need live evidence.
- **G3 world-state side: needs NoitaPatcher or another native implementation.** Stock Lua has terrain operations and probes, not a live per-cell snapshot API. A material-only codec is not a complete cell snapshot.
- No game, Steam, Wine or Proton process was launched. No runtime result, performance measurement or multiplayer verification is claimed; G3/G5/G9 remain open.

## What was checked

Target game hash: `9dbd52ced019a643169a2db02f46c77f8766c6e5` (`_version_hash.txt`); stock modding API version 12.

Source notation throughout this report:

| Prefix | Source |
|---|---|
| `API:N` | `/data/Games/steamapps/common/Noita/tools_modding/lua_api_documentation.txt:N` |
| `CD:N` | Same directory, `component_documentation.txt:N` |
| `V/path:N` | `.cache/noita-data/data/path:N`, already unpacked vanilla data; no game files copied into this report |
| `MM/path:N` | `V/entities/props/music_machines/path:N` |
| `EW/path:N` | `~/noita_entangled_worlds_fix/path:N`, reference-only local fork; no implementation copied |
| `NPF:N`, `NPW:N` | Independent NoitaPatcher revision `bbcc587d2f1ffe82905431b4518ba43c8f56e075`, `nsew/nsew/world_ffi.lua` / `world.lua` |

Pinned independent sources: [cell definitions](https://github.com/dextercd/NoitaPatcher/blob/bbcc587d2f1ffe82905431b4518ba43c8f56e075/nsew/nsew/world_ffi.lua#L181-L249), [world codec](https://github.com/dextercd/NoitaPatcher/blob/bbcc587d2f1ffe82905431b4518ba43c8f56e075/nsew/nsew/world.lua#L87-L270). Their existence is not a licence or ABI compatibility claim; see the G1 spike and D18/D19.

The entire 407-line stock API document was inspected, including an exhaustive case-insensitive `Physics` search; relevant component definitions, all four music-machine XML/Lua pairs, their base/scripts, scene entries and representative biome/helper paths were read. Searches and source inspection are static evidence, not proof of callback ordering or complete spawn coverage. `[INFERENCE]` marks a proposed design or unverified engine behaviour.

## Findings

### G5. Music-machine composition

All four concrete XMLs inherit `base_music_machine.xml`. The suffix is zero-based, while the entity tags and run flags are one-based:

| Entity | Tags beyond `mortal` | Kick file / sound / run flag |
|---|---|---|
| `music_machine_00.xml` | `musicmachine,musicmachine1` | `music_machine_00.lua` / `00` / `musicmachine1` |
| `music_machine_01.xml` | `musicmachine,musicmachine2` | `music_machine_01.lua` / `01` / `musicmachine2` |
| `music_machine_02.xml` | `musicmachine,musicmachine3` | `music_machine_02.lua` / `02` / `musicmachine3` |
| `music_machine_03.xml` | `musicmachine,musicmachine4` | `music_machine_03.lua` / `03` / `musicmachine4` |

Sources: each concrete XML `:1–8`; each corresponding Lua file `:3–18`.

| Component / absence | Actual configuration | Source |
|---|---|---|
| `PhysicsBody2Component` | `allow_sleep=1`, damping 0, `kill_entity_after_initialized=0` | `MM/base_music_machine.xml:3–8` |
| `PhysicsImageShapeComponent` | `is_root=1`, `centered=1`, `image_file=data/props_gfx/music_machine.png`, `material=metal_prop_low_restitution` | `:10–15` |
| `DamageModelComponent` | `hp=8`, no air/falling damage/item death drop; smoke blood; material damage from fire/lava/acid; melee multiplier 0.1, electricity 0 | `:17–38` |
| `ExplodeOnDamageComponent` | death probability 1, damage probability 0; body-modified probability 0.7; destruction threshold 0.32 | `:40–45` |
| Explosion subobject | radius 25, damage 1.2, `hole_enabled=1`, `ray_energy=700000`, `physics_throw_enabled=1`, delayed 1–4 frames; cell/particle effects | `:46–74` |
| `ParticleEmitterComponent` | disabled, tag `fx`; cosmetic sparks, not real material particles | `:76–106` |
| `SpriteParticleEmitterComponent` | disabled, tag `fx`; note sprites | `:108–141` |
| `LuaComponent.script_kick` | concrete XML selects its kick script; callback is `kick(who_kicked)` | concrete XML `:4–6`; `CD:1546` |
| Three other Lua components | reset on add; audio-dead callback; disabled `fx` animal effects every 60 frames | base `:149–165` |
| `AudioComponent` | `music.bank`, `music/ancient_tracks`, latest-event positioning/removal and dead-event notification | `:167–173` |
| **Not present in this inheritance chain** | no legacy `PhysicsBodyComponent`, `PhysicsShapeComponent`, `PhysicsThrowableComponent`, `CellEaterComponent`, `PixelSpriteComponent` or `SpriteComponent` | base `:1–175` and concrete XMLs |

The image file is the **initial** physics-image source, not a Lua-readable live mask (`CD:1971`). Static PNG inspection found a 58×50 image with 1,647 nontransparent source pixels; that is not a measured initialized-body count.

The material inherits `aluminium`, whose `cell_type=solid`; it has `[box2d]`, `hp=200000`, `solid_static_type=0`, restitution 0.1 (`V/materials.xml:13102–13120,13168–13188`). Material HP is distinct from the entity's `DamageModelComponent.hp`.

The base contains a `music_machine_00_kick.lua` reference (`:146`) absent from the unpacked directory; concrete `<Base>` overrides name existing `music_machine_0N.lua` files. Merge/callback behaviour was not executed; this is **not** evidence of the reported desync's cause.

#### Sound and quest state are separate from geometry

A kick reads `AudioComponent.m_latest_source`; only when it is -1 does it play the named sound, enable `fx`, add `fish_attractor` and set the run flag (`MM/music_machine_00.lua:6–16`, analogously 01–03). Audio completion disables FX/removes the tag (`music_machine_audio_dead.lua:1–5`); loading the entity also clears them (`music_machine_reset.lua:3–6`). This fits D39: replicate a sound event, not a persisted playback position.

The alchemist key searches for `musicmachine` within radius 96, checks `musicmachineN`, sets `musicmachineN_done` and increments `VariableStorageComponent[name=music_machine].value_int` (`V/entities/animals/boss_alchemist/key_music.lua:5–59`). Thus the key's counter and `_done` flags must commit together; syncing only the sound or the machine's transform loses quest state. The light chest reads that counter and branches at 4 (`V/scripts/buildings/chest_light.lua:6–38`).

#### Spawn locations and related props

| Path | Placement / entry point |
|---|---|
| Native buffered scenes, `music_machine_00` | `(14698,-90)`, `(51050,-90)`, `(-21654,-90)` |
| Native buffered scenes, `music_machine_01` | `(-1905,-1412)`, `(34447,-300)`, `(-38257,-300)` |
| Native buffered scenes, `music_machine_02` | `(24166,-421)`, `(-48538,-421)` |
| Native buffered scenes, `music_machine_03` | `(2800,250)`, `(39252,-150)`, `(-33652,-150)` |
| Biome marker for `music_machine_02` | colour `0xff235a15` → `spawn_music_machine(x,y)` → `EntityLoad` in hills, lake and winter |

Fixed placements and the explicit adjacent-parallel-world comment: `V/biome/_pixel_scenes.xml:169–185`; stand material/visual scenes: `:160–164`. Biome route: `hills.lua:10,276–278`, `lake.lua:12,254–256`, `winter.lua:9,409–411`, all under `V/scripts/biomes/`. Suppressing the biome hook alone misses the native placements; removing native placements alone misses the biome route.

Related props establish why “one physics entity = one body” is unsafe:

| Example | Relevant differences / source |
|---|---|
| `physics_cart.xml`, legacy minecart | multiple `PhysicsBodyComponent` / image-shape / joint components; `V/entities/props/physics_cart.xml:3–49`, `physics_minecart.xml:3–78` |
| `physics/minecart.xml` | one `PhysicsBody2Component`, body ids 1–3, two `PhysicsJoint2Component`s; `:3–52` |
| `physics/lantern_small.xml` | image body, surface joint broken by body modification, separate flame `SpriteComponent`, modified/death scripts; `:3–29,105–128` |
| `temple_statue_01.xml`, `buildings/statue_hand_1.xml` | image-based rock bodies; hand has both kick and body-modified callbacks; statue `:2–38`, hand `:2–23` |
| `physics_fungus.xml` beside machine 01 | six image bodies (100–105), joint mutators/joints, fluids and explosions; `:2–153,175–242`; native scene entries at `_pixel_scenes.xml:187–192` |
| `base_coalmine_structure.xml` | `PixelSpriteComponent.create_box2d_bodies=1`, plus SimplePhysics/Velocity; **not** the gramophone's representation; `:4–19` |
| `base_item_physics2.xml` | defaults `kill_entity_after_initialized=1`; a body can intentionally outlive its spawner; `:2–15` |
| `PhysicsThrowableComponent` | throw/torque/surface-attachment parameters and WIP `hp`, not a mask API; absent from the machine; `CD:2076–2089` |
| `CellEaterComponent` | can eat dynamic physics bodies (`CD:412–422`); vanilla black hole explicitly enables this at `V/entities/projectiles/deck/black_hole.xml:111–114` |

#### What damage does, and what is not established

Documented: body modifications trigger `physics_body_modified(is_destroyed)` (`CD:1537`); the explosion component reacts to modification and lost-body percentage (`CD:933–936`). `PhysicsBody2Component` tracks `mPixelCountOrig` and private `mPixelCount` in solid cells (`CD:1909,1915`). Joints can break when an attached body is modified (`CD:1982`); legacy grid joints explicitly support bodies being destroyed/chipped away (`CD:2009`).

Vanilla machine Lua contains no polygon recomputation or fragment-creation routine. `[INFERENCE]` Pixel removal, collision-shape rebuilding and separation are engine-side. Neither the exact polygon algorithm, its ordering relative to callbacks, nor a rule that each fragment becomes a new entity is documented. The bridge spawner constructs several shapes/joints then calls `PhysicsBody2InitFromComponents` and intentionally destroys its entity (`V/entities/props/physics/bridge_spawner.lua:224–229`); `CD:1901,1903` also separates entity and body lifetimes. **A fragment may need its own network body identity without having an entity.** Do not invent `EntityLoad` events for every body returned by an area query.

#### Exhaustive stock physics surface: capability and verdict

The `Physics` search covers `API:241–271`, plus the physics warning on `GameShootProjectile` (`:138`) and Box2D trimming on conversion (`:343`). The following groups cover every function in that physics block; none returns pixels or fixture vertices.

| API / field | Capability | Verdict for G5 |
|---|---|---|
| `PhysicsBodyIDGetFromEntity(entity,component=0)` (`:254`) | lists linked bodies, including links through components/joints | useful enumeration; no persistent ID, ownership or mask |
| `PhysicsBodyIDQueryBodies(...,include_static,are_these_box2d_units)` (`:255`) | area-wide Box2D body IDs | needed to discover bodies without assuming entity ownership; no provenance/shape |
| `PhysicsBodyIDGetTransform` / `SetTransform` (`:256–257`) | position, angle, linear/angular velocity | motion only; units are Box2D |
| `PhysicsComponentGetTransform` / `SetTransform` (`:252–253`) | same at component level | motion only |
| `PhysicsGetComponentVelocity` / `PhysicsGetComponentAngularVelocity` (`:250–251`) | velocities | no geometry |
| `PhysicsBodyIDGetWorldCenter` (`:261`) | centre | not a mask or polygon |
| `PhysicsBodyIDGetBodyAABB` (`:266`) | documentation literally says `-> nil` | return contract unclear; even a real AABB cannot describe a chipped shape |
| `PhysicsBodyIDGetDamping` / `SetDamping`, `GetGravityScale` / `SetGravityScale` (`:262–265`) | body parameters | preserve alongside motion, not geometry |
| `PhysicsApplyForce`, `PhysicsApplyTorque`, `PhysicsApplyTorqueToComponent` (`:244–246`) | force/torque application | cannot restore damage |
| `PhysicsBodyIDApplyForce`, `ApplyLinearImpulse`, `ApplyTorque` (`:258–260`) | per-body forces/impulses | cannot restore damage |
| `PhysicsApplyForceOnArea` (`:247`) | callback receives entity, mass, position and velocities | possible mass diagnostic; no cells/fixtures |
| `PhysicsSetStatic` (`:249`) | changes static state | a freeze tool, not geometry capture |
| `PhysicsAddBodyImage`, `PhysicsAddBodyCreateBox`, `PhysicsAddJoint` (`:241–243`) | construct image/box bodies and joints | explicitly **do not work with PhysicsBody2Component** |
| `PhysicsRemoveJoints` (`:248`) | removes joints in area | no enumeration/serialization of existing joints |
| `PhysicsBody2InitFromComponents` (`:267`) | initialises shapes/joints from components | construction input, not an export of damaged shapes |
| `PhysicsPosToGamePos`, `GamePosToPhysicsPos`, `PhysicsVecToGameVec`, `GameVecToPhysicsVec` (`:268–271`) | unit conversion | required for motion; no shape access |
| `PhysicsImageShapeComponent` (`CD:1961–1975`) | image file, material, offsets, root/body id; private body pointer | initial recipe, no mutable live mask/vertex array |
| `PhysicsShapeComponent.recreate`, box/circle/capsule fields (`CD:2057–2074`) | parameterised primitive shapes | not arbitrary surviving image geometry |
| PB2 `mBodyId`, counts, `mBody`, `mInitialized` (`CD:1885,1909–1916`) | identity/counts and internal pointers | counts cannot distinguish different holes; pointers are process-local, not snapshots |
| `PixelSpriteComponent.mPixelSprite` (`CD:2102–2113`) | opaque native pointer; optional new Box2D pixel sprites | not the machine and not a documented read/write interface |
| `PhysicsBodyIDGetBodyInfo`, `PhysicsBodyIDQueueForDeletion`, `PhysicsBodyIDMarkForDeletion` | not in the complete local API document, vanilla scripts or matching executable string search | **not available as documented APIs for this pinned build**; do not design against them |

Legacy `PhysicsBodyComponent.is_enabled=false` destroys its body (`CD:1930`); PB2 has `destroy_body_if_entity_destroyed` and `kill_entity_if_body_destroyed` (`CD:1900,1903`), with different defaults. Neither is a general documented deletion API for arbitrary entity-less fragments.

**Capture/restore decision:** a body image can be constructed from a known PNG, but no documented call exports the *surviving* PNG/mask, material cells, fixtures or split-body lineage. `ModImageGetPixel` reads a source/virtual image, not the mutated physics body. Creating/editing pre-registered virtual masks is at best a restore experiment: `ModImageMakeEditable` is init-only and explicitly warns about image caches and late edits not becoming visible (`API:372–375`). `EntitySave` is dev-only (`API:7`). Replaying an explosion is an input replay, not transfer of its authoritative result.

A native candidate therefore needs, at minimum: enumerate root and detached bodies; export local-space live material/colour mask and offsets; read/write or rebuild fixtures consistently; preserve body transforms, joints and per-body parameters; replace old bodies without leaving orphan copies; map the new body set to persistent network IDs. Whether a serializer includes any of this must be demonstrated, not inferred from its name. Without that surface G5 is **blocked**, not satisfied by transform sync or an indestructible replacement.

#### What EW establishes about the reported symptom

The user's observation is ground truth: **one client saw an intact gramophone; the other saw only a remaining part** (`docs/EVIDENCE.md:63–64`, `docs/OPEN_QUESTIONS.md`, Q16). Its exact cause remains unproved.

Read-only EW evidence:

- Machines/base are allowlisted (`EW/quant.ew/files/core/constants.lua:3–6,91–94`); the helper adds sync tags, sets `destroy_body_if_entity_destroyed=true`, keeps the entity after init and marks the four machines unique (`files/system/entity_sync_helper/entity_sync_helper.lua:88–122`).
- Its Lua physics payload is position/rotation and optional velocities (`files/core/util.lua:406–458`). Current Rust collection likewise reads transforms (`ewext/src/modules/entity_sync/diff_model.rs:1703–1735`); applying them zips remote and local body lists (`:2037–2053`). These paths do not export a pixel mask or fixture topology.
- Remote copies disable local explosion-on-death/damage and body-modified explosion probability (`diff_model.rs:2610–2617`). This is not a geometry repair mechanism.
- Unique-machine dedup uses filename plus `floor(x/512),floor(y/512)` (`files/system/uniq_flags/uniq_flags.lua:32–35`). Spawn dedup and geometry replication are different problems.

`[INFERENCE]` Independent cell destruction plus motion-only sync is a strong candidate for this symptom, and unequal body lists make index-wise transform application fragile. It is not a proven diagnosis of the user's particular incident; no EW fix is claimed here.

### G9. Generation and spawn-path map

The global map is a 70×48 image (`V/scripts/biome_map.lua:3–4`); `_biomes_all.xml:1–3,59–63` maps colours to biome XMLs. Coalmine selects Wang topology, a template and a Lua script (`V/biome/coalmine.xml:2–20`). These establish *what* generates; the engine streamer decides *when* an area is needed.

The camera is a real loading trigger: vanilla trailer scripts explicitly “move camera to force pixel scene loading”, set a free camera and wait between moves (`V/entities/trailer/trailer.lua:10–15`, `trailer_2.lua:10–15`). `[INFERENCE]` Each client's local camera can therefore drive its own generation without the host's camera visiting. The exact loading radius/scheduling is not documented.

| Generation path | Call chain / evidence | Duplicate-suppression boundary |
|---|---|---|
| Global Wang markers | `V/scripts/wang_scripts.csv:1–30` → colour/function lookup in current biome script; includes hearts, wands, enemies, props, perks and scene loaders | hook the global table **and** each final biome function definition |
| Biome-specific markers | `RegisterSpawnFunction(colour,name)` (`API:75`); coalmine registers nests, structures, chests, traps etc. (`coalmine.lua:10–28`) | record full callback arguments, not only entity filename |
| Biome `init` | e.g. `0xffffeedd` → `temple_altar.lua:init(x,y,w,h)` → altar `LoadPixelScene` (`:11,128–131`) | do not skip wholesale: it creates terrain and downstream marker callbacks |
| Weighted entity helpers | `spawn/spawn2` → `random_from_table` → `entity_load_camera_bound` → `EntityLoadCameraBound`; `V/scripts/director_helpers.lua:28–44,48–107,149–166` | claim the site before registering a camera-bound spawn; retain its chosen payload |
| Pixel-scene helpers | `load_random_pixel_scene` → material/visual scene and optional colour mappings; `director_helpers.lua:187–248` | terrain manifest and nested spawn sites, not just entities |
| Buffered native scenes | `_pixel_scenes.xml` material scenes and `just_load_an_entity`; the latter includes all eleven fixed machines (`:172–185`) | remove selected entity entries at mod init and replace with explicit sites; Lua biome wrappers cannot intercept native loads |
| Entity initialisation | loaded procedural wand/potion runs its LuaComponent; e.g. `V/entities/items/wand_level_01.xml:51–57`, `V/scripts/items/potion.lua:107–158` | materialize only on the generator; transmit the resulting state, not just the procedural filename |
| Deferred generation | bridge, pipe, chain/wire spawners wait for terrain and then raycast/create bodies; bridge `:15–19,224–229`, drain pipe `V/scripts/buildings/drain_pipe_spawn.lua:8–29`, chain `V/scripts/props/chain_to_ceiling.lua:35–46` | assign the spawner and resulting body group to one authority; avoid replay on replicas |
| Camera-bound activation/reload | `EntityLoadCameraBound` (`API:5`); `CameraBoundComponent` kills/freezes by camera distance/count (`CD:395–402`) | a queued spawn and a reloaded entity are not a fresh site; server manifest/tombstone wins |

Paths above are representative audited routes, **not a claim that all vanilla/modded spawning is covered**. Loading a scene can recursively produce marker callbacks. Dynamic death/interaction/projectile spawns are another event source, not generation reruns.

| API / component | Capability | G9 verdict |
|---|---|---|
| `GameGetCameraBounds` (`API:119`) | current viewport, explicitly not pixel-perfect; vanilla helper `utilities.lua:992–995` | interest/lookahead hint, not loaded-area or authority proof |
| `DoesWorldExistAt` (`API:172`) | box is streamed in **and** no pixel scenes are loading | readiness predicate; does not load/generate or report “first generation” |
| `BiomeMapGetName` / `DebugBiomeMapGetFilename` (`:232,339`) | biome name/config at coordinates | classification only; not a loading trigger or generated marker |
| `StreamingKeepAliveComponent` (`CD:2455–2458`) | only two unexplained TEMP fields documented; on player `V/entities/player_base.xml:409–410` and selected bosses/suns | **unknown** whether it keeps an entity, terrain or both; not a substitute for remote generation |
| `LoadPixelScene` (`API:131`) | material/colour/background files, biome/edge options, colour mapping, z index, `load_even_if_duplicate=false` | usable terrain command; engine duplicate handling is not a persistent multiplayer spawn ledger |
| `ModLuaFileAppend` / `ModTextFileSetContent` (`:366,370`) | initialization-time file hooks/patching | usable stock integration boundary; not a global override of every Lua VM |
| `SetRandomSeed`, `ProceduralRandom*` (`:233–240`) | seeded stream vs coordinate-addressed random calls | reproducibility aid, not shared world state or a transaction |
| `GameAddFlagRun` / `GameHasFlagRun`, `GlobalsSetValue` / `GetValue` (`:278–280,204–205`) | local run flags/string globals | must be replicated/classified; not intrinsically networked or atomic claims |

#### RNG and once-only state

- Weighted choices, counts and offsets use `ProceduralRandom(x,y)` (`director_helpers.lua:33,61,66–70,89,98–103`); potions and chests seed with coordinates (`biome_scripts.lua:86–95`, `coalmine.lua:1193–1205`). `[INFERENCE]` Matching seed/build/inputs makes these repeatable, not their later physics.
- `SetRandomSeed` changes the RNG globally across Lua scripts (vanilla warning `V/scripts/init.lua:183`). Moving a callback to a later frame can change other unseeded consumers. Do not promise arbitrary callback replay is harmless.
- Generation reads run state: greed curse changes chest odds (`coalmine.lua:1196–1197`); temple guardian/shop/checkers read `TEMPLE_*` globals (`temple_altar.lua:159,226`; `temple_shared.lua:19–45,54–70`).
- “Spawn once” state is not one universal flag: perk generation advances `TEMPLE_NEXT_PERK_INDEX` and sets a run flag (`V/scripts/perks/perk.lua:499–533`); apparition generation invokes native `SpawnApparition` with a multi-state result (`biome_scripts.lua:29–62`; `WorldStateComponent.apparitions_per_level`, `CD:2692`). Music flags record activation, **not placement**.
- Scene uniqueness can be VM-local mutable table state: `is_unique` sets a scene's probability to 0 (`director_helpers.lua:219–222`; shrine `V/scripts/biomes/coalmine_alt.lua:681–689`). Different visitation order can select different later scenes.
- Profile/date inputs differ: hearts read 14 February (`biome_scripts.lua:144–150`); hills `init` reads Halloween (`hills.lua:204–219`); the sun-scale scene reads persistent flags (`scale.lua:31–54`). Modifiers depend on death count (`V/scripts/biome_modifiers.lua:1192–1235`); UTC snow changes terrain modifiers (`V/scripts/init.lua:121–144,184–187`).

Pin world seed, epoch/NG+, build/material registry/mod order, generation-relevant profile intersection (D34), modifier mapping, date/weather inputs (D39), and a server state sequence **before** world initialization/generation. Share run-state updates, material conversions and cut-through-world history (`CD:2690–2697`) before generating affected new areas. A single seed is insufficient.

#### What host-only spawning would break

1. The host receives no ordinary biome callback for a far area it has not generated. If the remote callback simply returns because `not is_host`, enemies, objects, traps, portals and shared pickups can be missing permanently. Host presence on the server does not imply host Noita loaded the biome.
2. Functions named `spawn_*` are not entity-only: coalmine `spawn_items` loads a wand altar (`coalmine.lua:1049–1064`); `spawn_heart` can load a mimic-sign scene using raycasts (`biome_scripts.lua:125–180`); Holy Mountain shop code also loads supporting terrain (`temple_altar.lua:150–184`). Skipping them can remove terrain/nested sites, not just duplicates.
3. Forwarding all entity creation to an ungenerated host area can drop objects through missing ground. EW documents this exact failure mode and waits on `DoesWorldExistAt` before re-enabling SimplePhysics (`EW/ewext/src/modules/entity_sync/diff_model.rs:2755–2796`). KeepAlive alone is not a demonstrated remedy.
4. Allowing all clients to spawn independently avoids omissions but duplicates shared entities and advances counters separately. Personal shops/perks are deliberate exceptions (D5/D13), not permission to multiply hearts, props or income.

#### Proposed suppression and far-client generation design `[INFERENCE]`

1. **Server owns generation records, not necessarily the host camera.** Key areas by `(run_id, world_epoch, signed_area_x, signed_area_y)`. Grant one active client's generation lease with `authority_epoch` and an input-state sequence. The far client may hold it while the host remains elsewhere. No continuation without the host server (D3).
2. **Site identity precedes materialization.** Persist a site descriptor containing source kind (Wang marker / fixed XML entry / deferred child), script/data hash, function, exact marker coordinates, callback arguments and occurrence/child index. Derive output IDs from `(site_id, output_index)`; do not dedup by current entity position, filename alone or a local entity ID. Store empty outcomes too.
3. **No lease: queue the callback, not its already-created entities.** Intercept top-level dispatch before original shared side effects. A synchronous Lua callback cannot assume a network reply arrives inside it. Capture arguments and drain only after grant; an area needs an application-level `generation_complete` barrier in addition to `DoesWorldExistAt`. Callback deferral and special `init` handling are live-test risks, not solved API guarantees.
4. **Classify outputs, including mixed functions.** Terrain-only paths run locally with pinned inputs or an authoritative scene manifest. Shared entity paths run once on the leased generator. For mixed paths, record the authoritative scene commands with all `LoadPixelScene` arguments and make every nested marker pass the same site gate. Do not globally suppress `LoadPixelScene`, and do not fake a successful `EntityLoad` with a dummy ID: vanilla code immediately edits returned entities.
5. **Fixed native entries become explicit sites.** During mod init remove only classified `just_load_an_entity` entries from all used scene lists (normal/NG+), retaining material stands/scenes. The granted client creates those sites after terrain readiness; other clients receive the committed entities. Cover biome-spawned machine 02 separately. Deferred spawners execute once on the same regional authority.
6. **Commit the output, not merely “spawned=true”.** Atomically retain entity/body snapshots, chosen loot, scene commands, world-state side effects and the area's generation revision. A pending manifest survives crash/reconnect. Personal results have a slot in the key; shared hearts/chests/props do not. Replay restores payloads/tombstones, never rerolls the generator.
7. **Reconcile before gameplay.** A second visitor applies the committed terrain/entity/body revision and activates only after local terrain readiness. Fence late old-owner packets. A generator that disconnects before commit loses its lease; the server reissues the recorded operation/input version, not an unchecked merge of its local save. Two simultaneous first visitors receive one winning epoch.

These steps require adapters for special generators, persisted queues and an installation barrier. They are a concrete candidate to test, not a claim that file appends alone make all generation transactional.

EW's reference design already illustrates the distinctions: its Wang detours exclude special `init`, arbitrate spawn calls through a host uniqueness flag, and can broadcast pixel-scene commands (`files/system/wang_hooks/wang_hooks.lua:134–148,229–291`). Shop generation RPCs instead run on the host and contain far-camera floating/stealability workarounds (`files/system/gen_sync/gen_sync.lua:24–81`). Its camera-bound item prehook separately suppresses client item spawns (`files/system/spawn_hooks/spawn_hooks.lua:42–61`). These are knowledge sources, not a design to copy or evidence that G9 is solved.

### G3. What a world cell carries: documented boundary

**Stock `lua_api_documentation.txt` and `component_documentation.txt` do not define a world `Cell` struct or a generic `(x,y) → cell` getter.** `CellData` in a material definition is a shared material recipe, not a separate copy of every property for every pixel. Do not call material `lifetime`, `hp` or `temperature_of_fire` a proved per-instance layout.

| Cell property | What is actually exposed | Snapshot verdict |
|---|---|---|
| Material/type | stock `CellFactory_*` maps IDs/names/categories (`API:106–115`) but does not query a coordinate; NP vtable `get_material`, `get_cell_type`, material-pointer conversion (`NPF:183,194,395–424`) | live coordinate read **[needs NoitaPatcher]** or own native code |
| Colour | NP vtable `get_colour/set_colour`; `CLiquidCell.colour`, `original_colour` (`NPF:187–189,247–248`) | **[needs NoitaPatcher]**; source sprite/material colour is not current cell colour |
| HP / accumulated cell damage | NP `Cell.hp` (`NPF:225–234`); exact semantics by cell class not documented | **[needs NoitaPatcher]**; no stock per-cell damage getter/setter |
| Burning flag | NP `Cell.is_burning`, vtable `is_burning/stop_burning` (`NPF:211–215,231`) | **[needs NoitaPatcher]**; material `on_fire` alone does not capture it |
| Liquid position/static state | NP `CLiquidCell.x/y/is_static` (`NPF:236–246`) | **[needs NoitaPatcher]**; static flag affects simulation |
| Per-cell lifetime, temperature, velocity, reaction timers and other flags | no named complete per-instance layout in the two stock documents; NP still has unknown fields | **unknown**, not safely captured by guessing offsets or copying opaque memory |
| Null / empty cell | NP `get_cell` yields a pointer slot; encoder tests nil (`NPW:131–133`) | **[needs NoitaPatcher]**; distinguish unloaded area from loaded air |

The independent NP codec encodes only `(run length, material, flags)` (`NPW:14–38`); it stores the liquid-static bit but no colour, HP or burning state (`:128–145`). It skips `CELL_TYPE_SOLID` material extraction (`:136–139`), relevant to the machine's aluminium-derived Box2D material. **This is not “all solid terrain is skipped”:** ordinary `rock_static` is a liquid-type static sand cell in vanilla (`V/materials.xml:793–807`). Terrain snapshots do not replace G5 body snapshots.

Decode constructs cells only where chunks are loaded and materials differ (`NPW:215–245`); same-material cells keep their existing instance state, including not updating the static flag through that branch. Thus even “decode then re-encode matches” would prove only that narrow codec's fields, not full-state restoration. Unknown fields are explicit G3 losses until experimentally characterised.

#### Stock terrain read/write APIs and cost caveats

No `GetWorldPixel`/`SetWorldPixel`-style API appears in the complete stock list. The following operations are useful, but none gives arbitrary live cell-state readback:

| API / component | Capability | Verdict / per-call caveat |
|---|---|---|
| `Raytrace`, `RaytraceSurfaces`, `RaytraceSurfacesAndLiquiform`, `RaytracePlatforms` (`API:162–165`) | first-hit occupancy/surface probes with different material-class filters | no material ID/colour/state; repeated rays scale with probes and traversed space `[INFERENCE]` |
| `MaterialAreaCheckerComponent` (`CD:1604–1617`) | asynchronous presence/fullness/count-threshold check for specified materials | coarse assertion, not a pixel dump; per-pixel entity/component creation would add avoidable overhead |
| `GameCreateParticle` (`API:136`) | creates named material particles; `just_visual` and velocity options | can emit real material, but no exact destination/state/readback contract; per-pixel calls would be O(area pixels) API crossings |
| `GameCreateCosmeticParticle` (`:135`) | cosmetic particles with colour/lifetime | **not** terrain restoration |
| `LoadPixelScene`, `PixelSceneComponent` (`API:131`, `CD:2091–2100`) | material/colour-image stamp; biome/edge/duplicate options | bulk terrain input candidate; files/caches, overlap/air semantics and nested spawn callbacks need testing |
| `EntityLoad` + `PixelSpriteComponent` (`API:3`, `CD:2102–2113`) | image/material entity; can produce Box2D pixel sprites | not a cell getter or arbitrary current-state setter; entity setup, physics and cleanup have side effects |
| `ConvertMaterialOnAreaInstantly` (`API:343`) | replace one material by another in a rectangle; optional Box2D trim | batch conversion, not arbitrary payload import; work scales with area `[INFERENCE]` |
| `ConvertMaterialEverywhere` / `ConvertEverythingToGold` (`:341–342`) | global transformations | not local snapshots; former is spread over frames and appends to persistent WorldState history—docs say call sparingly |
| `EntityConvertToMaterial` (`:340`), `LoadRagdoll` (`:344`), `LooseChunk` (`:272`) | entity conversion, ragdoll material creation, terrain loosening from an image | specialised world effects, not readback or arbitrary body/cell snapshots |
| `CellEaterComponent`, `MagicConvertMaterialComponent` (`CD:412–422,1565–1593`) | eat/convert/ignite cells in an area | modifies simulation over time, not a snapshot API; ignored-material-tag filtering explicitly costs performance |
| `GameCutThroughWorldVertical` (`API:223`) | persistent vertical cut | docs explicitly warn every beam adds chunk-creation overhead; no readback |
| `ModImageGetPixel` / `SetPixel` (`:372–375`) | source/virtual image editing | not the world grid; late image caching caveats apply |
| `GameGetFogOfWar*` / `GameSetFogOfWar` (`:169–171`) | fog grid read/write | different data, not terrain; docs prefer components for performance |
| `GamePrintImportant` (`:182`) | UI message | irrelevant to world-state access |

A 512×512 area has 262,144 positions; any Lua per-pixel workaround can require that many crossings per pass before encoding or network costs. This is arithmetic, **not a timing measurement**. Prefer bulk operations/native traversal, reuse buffers, and measure capture/install separately. Writes via particles/effects cannot close G3 without an independent exact readback path.

## Risks and options

- **Primary G5 risk:** no demonstrated geometry serializer. Investigate native export/import of live body cells and topology; do not assume stock entity serialization or NP terrain encoding includes it. D7 forbids importing EW implementations/bundles; D18/D19 require independent provenance and the pinned ABI.
- **Primary G9 risk:** some generation runs before an asynchronous lease reply and some `init`/scene operations have engine-context assumptions. Validate the callback/installation barrier before investing in comprehensive adapters. Driving the host camera around or calling KeepAlive is not the proposed solution.
- **Primary G3 risk:** lost state can change the next simulation frame even when materials visually match. Define a field-complete schema only after measurements; do not claim temperature/lifetime/velocity coverage from unnamed padding or material defaults.
- **Cross-cutting risk:** body cells, terrain, spawned entities, flags and deferred work must share a coherent revision. Separately restoring them can duplicate fragments, restart quests, or make a later terrain scene overwrite the authoritative cut.

## Что остаётся проверить в игре (What remains to verify in-game)

Only after explicit launch permission, on disposable isolated profiles (D17). Record game/mod/native hashes, seed, server sequence, authority epoch, material/mask hashes and both clients' logs. Expected observations below are **test criteria, not results**.

| Experiment / input | Expected observation and pass/fail |
|---|---|
| **G5 baseline:** load each machine; log components, body IDs, counts, transforms and shape/mask export | Pass: initialized counts/masks reproducibly correspond to the loaded image and each concrete kick hook runs once. Fail: inaccessible live mask, duplicate kick or unexpected body ownership; do not substitute initial PNG for export. |
| **G5 chip and split:** controlled off-centre explosion plus a cell-eater cut through a narrow connector; repeat acid/material damage separately | Log modification/death callbacks, before/after masks and all area bodies, including body IDs absent from entity mapping. Pass: capture identifies both a changed root and every detached piece; fail on unmatched/lost bodies or assuming each piece has an entity. Also determine callback ordering and when collision polygons rebuild. |
| **G5 transfer/rejoin:** send a chipped machine and detached pieces from A to B; unload/reload; restart B, then transfer authority | Pass: masks/materials, fragment count, joints and collision silhouette match after install and reconnect, with no old intact body or orphan duplicate. Position-only equality or matching pixel counts with different holes fails. This is G5/A06. |
| **G5 restore mechanics:** independently test the selected serializer/native route; separately test PB2 rebuild from a new/pre-registered mask image and body deletion | Pass only if a known hole and body group survive real capture/restore and stale bodies vanish; intact reconstruction, stale cached image, lost fragments or silent deletion fails. Establish whether serialization includes geometry instead of inferring it. |
| **G5 quest/audio:** kick with key nearby, replicate/reorder event, restart B | Pass: one shared `musicmachineN_done` increment and key-counter update; sound occurs once per live event and does not restart on reconnect (D39). Geometry remains independent of this test. |
| **G9 remote first visit:** host stays near spawn; B enters an untouched distant biome and machine site; host area probe there remains false | Pass: B generates terrain plus one committed set of enemies/items/props, including native and biome machine sites, without host camera movement. Fail: missing spawns, entities falling through absent ground or forced host loading. |
| **G9 simultaneous visit:** both first-enter the same fresh area, delay/reorder lease replies, then meet; include two sites of the same filename | Pass: one winning epoch, distinct site IDs, one shared output set, personal outputs preserved. Wrongly collapsing two same-file sites, double entities or erased personal rewards fails. |
| **G9 crash/deferred paths:** disconnect generator before/after manifest commit; visit a bridge/pipe/chain site while scenes are still loading; later reload and reconnect | Pass: persisted pending work finishes once, body attachments use ready terrain, committed outcomes/tombstones survive, old-owner output is rejected. Missing/duplicate work or replayed loot fails. |
| **G9 input/order stress:** fixed seed with divergent profile death counts/date, differing HM visit order, greed/temple flags and a unique shrine | Pass: pinned generation inputs/manifests produce matching base and site results; a deliberate base-hash mismatch requests full state instead of trusting a delta. Same seed with divergent terrain is a fail. |
| **G9 KeepAlive/trigger probe:** move only a test entity with/without KeepAlive far from the camera; separately move camera and poll area bounds/readiness | Pass: actual entity/area retention and generation timing are measured and the chosen protocol does not rely on an unproved KeepAlive behaviour. Merely keeping the entity alive does not prove remote terrain generation. |
| **G3 field matrix:** water, sand, gas, fire, static rock; custom-colour cells, damaged/burning cells and static/dynamic liquids; capture/install into a fresh destination | Pass: independent readback matches every claimed field and subsequent settling/burning/lifetime behaviour; report each excluded field. Include same-material/different-static-state destinations and unloaded boundaries. Material-only or self-codec roundtrip equality is insufficient. |
| **G3 cost:** 512×512 capture/install, 128×128 deltas, homogeneous and fragmented material patterns | Pass: log capture/install frame time, encoded bytes and unsupported fields against G3; codec limit/failure must be explicit and a valid snapshot path must exist, not a silently dropped area. No performance gate closes from these static estimates. |
