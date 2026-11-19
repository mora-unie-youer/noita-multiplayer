# Stage 1 technical spikes — offline findings

Date: **2026-11-19**. Scope: source inspection, independent dependency provenance, read-only analysis of the pinned executable, and preparation of an isolated test rig. **No Noita launch; no in-game experiment, native hook implementation or performance measurement. None of G1–G9 is passed.**

This is the stage-1 result index requested by [ROADMAP](../ROADMAP.md), not a replacement for the gate criteria in [ACCEPTANCE](../ACCEPTANCE.md). Product decisions remain D1–D41. Findings do not silently authorize a new dependency, loss of gameplay state or a game launch.

## Build and evidence boundary

- Game branch `master`, version hash `9dbd52ced019a643169a2db02f46c77f8766c6e5`.
- `noita.exe` SHA-256 `808d2a0ab51ea0b46e9ad2aeb3327a4b0ce3feae04f32ba26326bf585b5779bd`, PE32 i386, preferred image base `0x00400000`.
- Vanilla sources are in ignored `.cache/noita-data/data/`; game binaries/assets/save data are not committed.
- Dependency clones, release downloads and analysis scratch are under ignored `.cache/spikes/`. Exact source revisions, hashes, file/line references and native VA/RVA evidence are in the individual reports.
- Source inspection establishes code paths, not callback timing or live safety. Decompiled branches are distinguished from checked instructions. `[INFERENCE]` means an unverified interpretation/design. Frame timings, FPS and network sizes remain unmeasured.

## Gate-by-gate result

| Gate | Offline finding | What remains open | Report |
|---|---|---|---|
| G1 — dependency and pixel access | Independent NoitaPatcher releases/revisions and 37-function API identified; world-access patterns match this executable. Original NSEW has MIT ancestry, current NP additions lack general licensing. A scene-dedupe pattern does not match | Strict licensed provenance and safe live pixel write/read-back; matching patterns do not validate every struct offset | [G1 provenance/ABI](spikes/g1_noitapatcher.md) |
| G2 — Linux↔Win32 | Runtime/hardware/storage surveyed; fresh separate prefixes **and** copied game directories proposed; read-only planner delivered | Steam-off startup, Cloud isolation, unsafe-mod activation, two concurrent real game↔agent↔server round trips | [G2 test rig](spikes/g2_testrig.md) |
| G3 — world snapshot | Stock Lua lacks a complete cell snapshot API. NP codec has dimensions/run-count limits and only material/liquid-static state, not complete cell metadata | Capture/install fidelity, loaded-area/thread ordering, dense data, bytes/frame costs and omitted metadata | [G1 codec](spikes/g1_noitapatcher.md), [G3 cell state](spikes/g5_g9_static.md) |
| G4 — player persistence | Player/item/effect fields, child trees, five perk stores, Lua read/write gaps and native/XML restore candidates mapped | Full cross-process restoration of inventory/perks/statuses/polymorph; runtime behaviour, not only matching saved XML | [G4/G7](spikes/g4_g7_static.md) |
| G5 — damaged bodies | Gramophones use PhysicsBody2 plus an image shape. Documented physics/NP transform APIs do not export surviving masks, fixture topology, joints or all detached pieces | Native capture/rebuild path and persistent fragment identity; serialization must prove actual geometry coverage | [G5/G9](spikes/g5_g9_static.md) |
| G6 — powder and orbs | Native powder-removal/credit candidate identified; wallet arithmetic, purchases, saturation and native orb bookkeeping traced separately from Lua rewards | Version-pinned safe interception/receipts, exactly-once terrain removal and credit, native/Lua order, remote orb application | [G6 economy/orbs](spikes/g6_economy_static.md) |
| G7 — death/bot | Stock game-over callback and wait-for-kill flag are candidates; Controls-driven inventory-wand control has source precedents | Final-death/second-life/polymorph order, bypasses, real-wand costs and authority transfer; no generic death veto proved | [G4/G7](spikes/g4_g7_static.md) |
| G8 — casts | Frame/deck/entity-ID RNG and nested shots prevent an unconditional replay guarantee. Zero damage does not remove teleport/material/summon/physics side effects | Per-class visual fidelity and single authoritative effect path under actual collisions/latency | [G8 cast classes](spikes/g8_casts_static.md) |
| G9 — remote generation | Wang/biome/native-scene/deferred paths mapped; a far client's generation lease plus persisted manifests is a candidate | Hook coverage/timing, queued generation barriers, simultaneous visitation, crash/reload reconciliation | [G5/G9](spikes/g5_g9_static.md) |

## Findings that change the next implementation step

### Native dependency is not a complete engine integration

The existing [author reply about bundling](https://github.com/dextercd/NoitaPatcher/issues/5#issuecomment-5333391161) is evidence of intended use, **not** a general license for every contributor/file. D18's personal-use restriction and refusal to contact the author remain unchanged. Original MIT NSEW is not a drop-in current-build substitute: its old hard-coded addresses/layouts need independent updating. The project's strict G1 provenance condition remains unsatisfied.

NoitaPatcher exposes useful engine access but does not supply a geometry serializer, powder income callback or generation authority mechanism. Its upstream area encoder is capped at 256×256 and 4096 runs, preserves only material plus one liquid-static flag, and skips flag-only changes in the same-material decode branch. Do not call that a full 512×512 world snapshot, damaged-body snapshot or a safe codec for arbitrary input.

The pinned game's stock `LoadPixelScene` already documents a tenth `load_even_if_duplicate` argument. Evaluate that stock path rather than depending on the nonmatching NP scene patch.

### Powder income must be tied to removal, not wallet deltas

On this exact binary, the recovered gold branch checks `gold`/`gold_radioactive`, scans an 11×4 window and credits one unit per removed cell. The arithmetic credit instructions were inspected directly; detailed match/scan/removal control flow is a decompiler-derived static result. It is not an observed gameplay result or a ready detour ABI.

The branch bypasses ordinary flask-capacity fields and does not apply the nugget Extra Money multiplier. `mGoldAccumulator` is sound bookkeeping. Wallet infinity/restore paths invalidate treating every positive `Δmoney + Δmoney_spent` as income; even a correctly observed delta cannot identify the cell or prevent two replicas paying for it.

[Q62](../OPEN_QUESTIONS.md) records the remaining product ambiguity: should D32 extend the personal Extra Money multiplier to powder and direct statue payouts, or retain vanilla source-specific eligibility? No choice was silently made. Any native implementation must preserve source kind, base quantity and event-time perk policy.

### Persistence and gameplay equivalence need separate proof

Perk effects are not just a list of perk IDs: entity children/components, already-modified fields, Globals/run flags and WorldState all matter. Replaying a perk function can award one-off items again or multiply already-restored attributes. Absence of Private-field names in two saved XMLs does not prove that a Lua schema snapshot captures all gameplay state or is equivalent to a live checkpoint. Native entity serialization is another candidate to test, not a guarantee by its name.

Likewise, neither setting `wait_for_kill_flag_on_death` nor overriding the found Lua game-over call establishes coverage of polymorph/entity-destruction/native completion paths. Bot shots must use the **real inventory wand** with real mana, charges and cast state; spawning approximate projectiles is not an acceptable shortcut.

## Delivered preparation tool

```bash
bash tools/testrig/plan.sh
```

The tool prints a labeled **unexecuted human-review plan**. It performs no mkdir/copy/log writes, game/runtime launches or process termination; `--execute` is rejected. Never pipe its output into a shell. It is not a launcher, network probe, installed mod or completed G2 fixture.

The candidate uses fresh test prefixes without copying real userdata, private game working directories and distinct local ports. A second save slot alone is not isolation: persistent data, mod lists and shared config still overlap. Steam Cloud is active in existing logs; Cloud isolation remains unproved. No Steam setting, real prefix, switcher or save was changed. Research invoked `wine --version` once accidentally; it printed metadata, not a game launch, and the deviation is recorded in G2.

## Next experiments

The individual reports contain concrete inputs, required logs and pass/fail criteria for every gate.

1. **Without launching:** resolve the strict G1 dependency path; investigate independently licensed/build-pinned native interfaces for complete cell capture, body geometry and powder-removal receipts. The reports identify entry points and gaps, not implemented APIs. Do not turn a byte-pattern match into permission to write memory.
2. **Only after a separate explicit launch request (D17):** prepare one fresh Steam-off instance, prove actual save/cwd/mod/Cloud isolation, then add the second instance. Do not use real Steam Play, the user's switcher, inherited save archives or EW's mod manager. Check free space immediately before preparation: the recorded /data volume was 99% used despite about 24 GiB free.
3. **Live native smoke, after provenance:** actual pixel write/read-back and snapshot field matrix; damaged gramophone with detached pieces; powder removal tied to one durable region operation; process-restart player restoration. Failure of any fidelity path is a blocker for its requirement, not a reason to narrow the requirement silently.
4. **Live integration:** death/second-life/polymorph paths, remote generation and simultaneous visits, then cast-class comparisons at the expected roughly 30 ms peer latency. Record outcomes separately from visual similarity and timing.

No synthetic offline benchmark is presented as game performance. Stage 1 remains in progress; stage 2 implementation was not started by this batch.

## Offline verification performed

- `bash -n tools/testrig/plan.sh` and ShellCheck 0.11.0 passed.
- Nine actual planner CLI cases passed: five successful display modes and four intended exit-2 refusals.
  The printed launch plan was observed; none of its commands was executed. `.cache/testrig/` remained absent.
- Eleven changed/new Markdown files passed local-link-target existence checks (52 targets), table-width checks
  and fenced-block balance. This is documentation structure validation, not verification of every engine claim.
- No live gate, native source build or game-performance test was run.
