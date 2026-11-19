# G1 — independent NoitaPatcher provenance, ABI and API audit

## Conclusion

**Verdict: technically feasible, but the strict licensed-provenance part of G1 remains blocked; confidence: high.** An independent upstream and exact release bytes are established. NoitaPatcher itself has no general license, including for personal use; D18 does not create one. Public documentation and an existing author reply support the interpretation that ordinary use and bundling are intended, but are not a license covering every file/contributor. Do not publish a NoitaPatcher-inclusive distribution as “MIT”. A self-written native layer, potentially using the separately licensed original NSEW as a starting point, is the fallback.

This is static research only: no DLL was loaded and no game, Steam, Wine or Proton process was started. Pixel write/read-back and all runtime parts of G1/G3–G9 remain unverified.

## What was checked

Inspection date: **2026-11-19**; initial GitHub API capture: `2026-11-19T19:58:19Z`. Clones/downloads are under ignored `.cache/spikes/`; nothing was copied from EW into this project. Game installation, prefixes and both EW checkouts were read-only. No build, test or formatter was run.

Source prefixes below refer to these immutable revisions:

| Prefix | Upstream / revision inspected | Commit date |
|---|---|---|
| `NP` | [dextercd/NoitaPatcher](https://github.com/dextercd/NoitaPatcher/tree/bbcc587d2f1ffe82905431b4518ba43c8f56e075), `main`, tag `release-1.36.2`, **`bbcc587d2f1ffe82905431b4518ba43c8f56e075`** | 2025-08-29 23:29:36 +0200 |
| `NP135` | tag [`release-1.35.3`](https://github.com/dextercd/NoitaPatcher/tree/309be76e74462606a5b939f4e5699e7ac27e38aa), **`309be76e74462606a5b939f4e5699e7ac27e38aa`** | 2025-01-25 18:16:05 +0100 |
| `NS` | [dextercd/Noita-Synchronise-Expansive-Worlds](https://github.com/dextercd/Noita-Synchronise-Expansive-Worlds/tree/b458d57e7a9bce2ed79fecb3a0c0c27b057b8ef6), **`b458d57e7a9bce2ed79fecb3a0c0c27b057b8ef6`** | 2023-12-18 22:44:22 +0100 |
| `GAME` | `/data/Games/steamapps/common/Noita`, `_branch.txt:1` = `master`; `_version_hash.txt:1` = **`9dbd52ced019a643169a2db02f46c77f8766c6e5`** | Embedded build: `Noita - Build Jan 25 2025 - 15:55:41` |
| `EW` | `~/noita_entangled_worlds_fix`, visible HEAD **`236fe9a89b2464bd4349ff73484ac129f1b2d26d`**, with unrelated local changes | 2026-11-17 23:55:39 +0300 |

`NP:path:line` means a file at the pinned `NP` revision above, not an installed EW copy. `VANILLA` means `.cache/noita-data/data/`, extracted from this installation. `API` and `COMP` mean `GAME/tools_modding/lua_api_documentation.txt` and `component_documentation.txt`.

## Findings

### 1. License and file provenance

#### NoitaPatcher itself

The complete tracked `main` tree has **110 files**. A filename inventory and content search found:

- No `LICENSE`, `LICENCE`, `COPYING`, `NOTICE`, `COPYRIGHT` file, SPDX header or README license grant in `main`.
- The only actual copyright declarations in its source tree are `NP:Documentation/conf.py:9–11` (`2023, Dexter Castor Döpping`) and `NP:src/noitapatcher.rc.in:21` (`Copyright Dexter Castor Döpping`). Neither grants reuse rights.
- `NP:README.md:5–7` points to releases/docs. `NP:Documentation/Usage.rst:4–6` instructs placing NoitaPatcher inside a mod. `NP:packaging/README.txt:23–25` explicitly describes an “included dependency”; these are usage instructions, not a general license text.
- `git rev-list --all --objects` over the cloned branches/tags found **no historical license-like filename**. License-grant/SPDX searches found none. The historical copyright additions are `2ce5029` (2023-04-03, documentation) and `24f1d42` (2023-05-11, DLL version resource). Old `gh-pages` license-word matches are generated Sphinx JavaScript BSD comments, not a license for NP.
- [GitHub `/license`](https://api.github.com/repos/dextercd/NoitaPatcher/license): **HTTP 404**, body `{"message":"Not Found",...,"status":"404"}`. [Repository metadata](https://api.github.com/repos/dextercd/NoitaPatcher) has `license: null`; [community profile](https://api.github.com/repos/dextercd/NoitaPatcher/community/profile) also has `license: null`. A 404 alone would not establish absence of terms; the source/history inspection is the additional evidence.
- No `vcpkg.json`, license manifest or submodule exists in NP. Dependencies are declared in `dependencies.cmake`, `Examples/client/CMakeLists.txt`, `packaging/CMakeLists.txt` and CI, not vendored as source files in its 110-file tree.

**Important existing author statement, separate from licensing:** [issue #5](https://github.com/dextercd/NoitaPatcher/issues/5) asks to include NP in a free “Metamorph: Creative Menu” archive, with credit and upstream link. On **2026-08-18T20:02:34Z**, owner `dextercd` [replied](https://github.com/dextercd/NoitaPatcher/issues/5#issuecomment-5333391161): **“Yeah that's fine. That's actually the intended way to use it.”** No contact with the author was made for this investigation. The reply is strong evidence of bundling intent, but addresses that request; it is not an SPDX license, does not define modification/sublicensing scope and cannot silently license other contributors' work.

#### `nsew/` really is upstream, with a separate MIT ancestor

`NP:nsew/nsew/{world_ffi.lua,world.lua,rect.lua,native_dll.lua}` and its native rectangle optimiser are in the independent repository. They are **not EW-only modules**. The import commit is [`32268b96a7ac42c8c28bcbbdf77126394bf8d011`](https://github.com/dextercd/NoitaPatcher/commit/32268b96a7ac42c8c28bcbbdf77126394bf8d011), 2023-12-17, “Merge with NSEW project”; integration followed in `1b4cbca716b082088a81b87f024c64197f2f1d7e`, 2023-12-18.

The original [NS `LICENSE.md:1–21`](https://github.com/dextercd/Noita-Synchronise-Expansive-Worlds/blob/b458d57e7a9bce2ed79fecb3a0c0c27b057b8ef6/LICENSE.md) is **MIT**, copyright 2022 Dexter Castor Döpping; added by `4ee2f1b`, 2022-06-14. `NS:README.md:1–5` says it was integrated into NP for multi-version compatibility. At the NP import, git blob IDs of `world_ffi.lua`, `world.lua`, `rect.lua`, `native_dll.lua`, the rectangle optimiser and its headers match NS's last pre-archive revision **`74e0095`**, 2023-04-12, byte-for-byte. Thus the imported material has a concrete MIT source; retain its notice if using it.

**Do not extrapolate this to all current NP files.** Later edits have no explicit license in NP. Current `world_ffi.lua` blame attributes 207/427 lines to the import, 184 to later Dexter commits and 36 to NathanSnail; `world.lua`: 213/272 imported, 12 later Dexter, 47 NathanSnail; `rect.lua`: 80/167 imported, 4 later Dexter, 83 NathanSnail. Line counts measure ancestry, not legal separability. [`PR #1`](https://github.com/dextercd/NoitaPatcher/pull/1) and [`PR #2`](https://github.com/dextercd/NoitaPatcher/pull/2) show external contributions. No CLA/general grant was found. The original MIT grant survives for original material; it does not automatically cover all post-import additions.

NS itself is **not a drop-in current-build solution**: `NS:nsew/world_ffi.lua:187,196,205,216,233–260` uses absolute addresses from April 2023; NP replaced discovery with patterns. NS's old material layout also needs independent updating.

#### Dependency inventory and license-bearing files

All following source repositories were cloned independently under `.cache/spikes/dep_git/`. Pinned dependency revisions were inspected with `git show`, without changing branches. Dates are commit dates. Root license files and source headers carrying the same terms were checked; no runtime dependency license was inferred from NP's missing license. Full-build/doc-tool dependency closure is **not reproducibly pinned** by upstream, as noted below.

| Dependency / use | Exact inspected revision and date | License-bearing files / terms / history |
|---|---|---|
| [MinHook](https://github.com/TsudaKageyu/minhook/tree/49d03ad118cf7f6768c79a8f187e14b8f2a07f94), statically linked into `noitapatcher.dll` | `49d03ad118cf7f6768c79a8f187e14b8f2a07f94`, 2022-11-12 | `LICENSE.txt:1–25` = BSD-2-Clause; `:27–83` also includes HDE32/HDE64 copyright/terms (Vyacheslav Patkov). Source/header notices, no SPDX. `eeaecad`, 2014-07-16, renamed COPYING and added HDE terms. Binary redistribution must reproduce notices. |
| [Zydis](https://github.com/zyantific/zydis/tree/4a661c659edaa51c3a459a885add92a216c87612), decoder in DLL | `4a661c659edaa51c3a459a885add92a216c87612`, 2023-04-06 | `LICENSE:1–25` = MIT, Bernd/Höner; source/header notices, no SPDX. License exists since initial `6632e73`, 2014-10-24. |
| [Zycore-C](https://github.com/zyantific/zycore-c/tree/c6b1d5d6ffd89a2f0d5c240f8ad3e8e262a8a1ed), Zydis submodule | gitlink `c6b1d5d6ffd89a2f0d5c240f8ad3e8e262a8a1ed`, 2023-04-04 | `LICENSE:1–25` = MIT; source/header notices, no SPDX. Initial `8d858d6`, 2018-04-09. Zydis `.gitmodules:1–3` identifies this repository. |
| [Abseil](https://github.com/abseil/abseil-cpp/tree/a6a88630c7d0613aa8d4d8bd55455d1530326d40), `absl::btree` in `nsew_native.dll`; hash/map in example server | Upstream asks for floating `master`; inspected `a6a88630c7d0613aa8d4d8bd55455d1530326d40`, 2026-11-19 | `LICENSE` = Apache-2.0; source/header copyright/Apache notices, no SPDX. Initial license `c2e75482`, 2017-09-19. Exact Abseil used in 2025 release binaries is **unknown**, not today's HEAD. |
| [googletest](https://github.com/google/googletest/tree/a46a6012a133c7c0929fdae55b321c2fe65f3c5f), tests/build configuration | Floating `main`; inspected `a46a6012a133c7c0929fdae55b321c2fe65f3c5f`, 2026-11-16 | `LICENSE` = BSD-3-Clause; source notices, no SPDX. Root file rename `27615dbc`, 2011-09-09. Not required inside a runtime mod; exact 2025 build revision unknown. |
| [LuaSocket](https://github.com/lunarmodules/luasocket/tree/88c8a85cb6528232288ec590910b7a966aace52b), **example client only** | `88c8a85cb6528232288ec590910b7a966aace52b`, v3.0.0, 2022-03-25 | `LICENSE:1–22` = MIT, Diego Nehab; source/header notices. Root license history reaches `6789b83`, 2003-08-31 (not a claim that its earliest text was identical). |
| [LuaJIT 2.0.4](https://github.com/LuaJIT/LuaJIT/tree/69e5342eb893815b18a1ec84ba74b0e0d1cc9beb), Lua headers/import library in CI, runtime supplied by game | Release commit `69e5342eb893815b18a1ec84ba74b0e0d1cc9beb`, 2015-05-14 | `COPYRIGHT:4–24` = Mike Pall MIT; `:27–47` Lua.org/PUC-Rio MIT; `:50–54` dlmalloc public-domain statement. Root COPYRIGHT history reaches `1d1fed48`, 2009-12-08. Do not redistribute the game's DLL as a dependency. |
| [Boost](https://github.com/boostorg/boost/tree/ea95c164044bbbe8bdc485c41b6add7a0fd9b106), `system`/Asio for example server | Requires ≥1.78, CI installs latest via vcpkg; inspected superproject `ea95c164044bbbe8bdc485c41b6add7a0fd9b106`, 2026-11-18 | Root `LICENSE_1_0.txt` = BSL-1.0, filename history `7dd85823c4`, 2003-09-30. No exact release-build Boost/component dependency versions are recorded; entire Boost submodule license closure was not audited. Not part of the minimal NP DLL. |
| [Handtokening-CMake](https://github.com/dextercd/Handtokening-CMake/tree/b492761e64dffba932a5c51a8123869a1149ed49), packaging/signing | Pinned `b492761e64dffba932a5c51a8123869a1149ed49`, 2025-08-24 | **No license at pinned revision** (three CMake files, no headers/SPDX). `LICENSE` (Apache-2.0) was added **later**, in `a06a201f7f224df8335c6c6b6d5796a04c836f48`, 2025-09-25. Current GitHub `/license` saying Apache does not establish the pinned revision's terms. No runtime code from this helper is needed. |
| [vcpkg](https://github.com/microsoft/vcpkg/tree/f4c425bd7750eea5f9b59d7df2b818d3298a5e58), CI package manager | Unpinned shallow clone; inspected `f4c425bd7750eea5f9b59d7df2b818d3298a5e58`, 2026-11-19 | Root `LICENSE.txt` = MIT (initial `ccca198c1b`, 2016-09-18); `NOTICE.txt` / `NOTICE_pt.txt` and many port-specific copyright/licenses. They are not all incorporated dependencies. Exact CI baseline and fetched Boost port closure are unknown. |

Declarations/call sites: `NP:dependencies.cmake:4–25,28–52`; `NP:src/CMakeLists.txt:6–12,38–43`; `NP:nsew/native/CMakeLists.txt:9–12`; `NP:Examples/client/CMakeLists.txt:1–6,8–35`; `NP:Examples/server/CMakeLists.txt:5–9`; `NP:packaging/CMakeLists.txt:37–50`; `NP:.github/workflows/build.yml:19–45`.

`doc_requirements.txt:1–2` names unpinned `sphinxcontrib-luadomain` and `sphinx-lua`; their transitive Python/Sphinx/theme assets were not reconstructed. They build documentation, not the mod. NP's in-tree `vs2013/` is its own compatibility implementation, not a separately licensed Microsoft STL distribution (`vs2013/src/init.cpp:12–21`). This lack of complete pinned build closure is another reason **not** to claim every file in a fresh full package is license-cleared.

The downloaded **1.35.3 and 1.36.2 ZIPs contain no license/notice-named files**, including for statically linked MinHook/Zydis, Abseil and example LuaSocket. A distributor must supply applicable notices independently; “upstream zip” is not a substitute for compliance. The GitHub license endpoint for MinHook/LuaJIT returns `NOASSERTION` despite readable license text, illustrating why endpoint classification alone is insufficient.

#### Personal use and publication

Not legal advice: copyright is exclusive by default; “public on GitHub”, “non-commercial”, “personal only” and “the author did not object” are **not** an automatic copyright license. [GitHub's no-license explanation](https://choosealicense.com/no-permission/) and [Terms D.5–D.6](https://docs.github.com/en/site-policy/github-terms/github-terms-of-service#d-user-generated-content) distinguish viewing/forking through GitHub from wider reuse, and contributor licensing from an existing repository license.

For D18, downloading and running the unmodified author release for its documented mod purpose is strongly supported by the author's public intent **[INFERENCE: practical permission scope, not a legal clearance]**. Each player can obtain the release directly from upstream; this project need not redistribute it. Sharing a private archive with a friend is still making/providing copies, not automatically exempt. D18 accepts personal-use risk, but **G1's condition “license of each used file found” is not met**. Do not silently turn D18 into permission to relicense or publish NP.

Publication of **our original code** is distinct from publication of NP source/binaries. A source-only project that documents a separately obtained dependency does not thereby publish NP's code; that does not resolve all integration/notice questions. Shipping NP/current NSEW additions, modified NP code, or a full copied release remains uncleared. The user declined author contact: retain this block, use the licensed old NSEW subset where appropriate, or replace the native functionality. The game's own modding agreement (`GAME/tools_modding/Noita-ModdingAgreement-v100.rtf`, “Mods”, “Permission to Modify”, “Software Assets”) is a separate consideration; do not distribute game binaries/assets or assume a dependency's license overrides it.

### 2. Target builds, discovery and update fragility

There is **no supported-hash list or `_version_hash` guard in NP**. `GetVersionString()` only reads the embedded `Noita - Build ` string (`NP:src/version_string.cpp:11–24`). It detects release versus dev layout by filename containing `noita_dev.exe` (`src/executable_info.cpp:39–41`; `src/noita.hpp:119–126`). Both are Win32; this is not a native Linux engine library.

Discovery is a **hybrid**, not just fixed addresses:

1. `GetModuleHandleA(nullptr)` identifies the loaded executable; PE headers locate `.text`, `.rdata` and imports (`src/executable_info.cpp:11–49`).
2. Byte patterns, wildcard captures and relative-call decoding locate internal functions/globals, including entity manager, game globals, serializer and RNG (`src/main.cpp:70–123,241–468`; `src/memory_pattern.hpp:73–88,117–134`; `src/memory_pattern.cpp:97–138`). String references such as `item_physics`, `OnPausePreUpdate` and a damage error identify other functions. Some searches start at stock Lua C bindings, obtained from the active Lua state (`src/lua_util.cpp:6–11`).
3. `find_function_start/end` uses alignment/prologue/return heuristics, not symbols (`src/x86.cpp:27–82`). Engine functions are **not exported-symbol APIs**. Export lookup/IAT hooking is used for `lua51.dll`/MSVCR120, not for stable engine entry points (`src/main.cpp:949–956`; `src/extended_logs.cpp:44–64`; `vs2013/src/init.cpp:12–21`).
4. **Structure offsets remain hard-coded:** `GameGlobal`/`DeathMatch` and system vtables (`src/noita.hpp:28–78,143–152`); `CellData` stride `0x290`, `game_global+0xc → world_data`, `world_data+0x44 → GridWorld`, `game_global+0x18 → CellFactory`, factory `+0x18 → material array` (`nsew/nsew/world_ffi.lua:388–424`). Physics reads component `+0x48`, then body doubles at `+0x58,+0x60,+0x70,+0x80,+0x88,+0x90` (`src/physics.cpp:22–32,46–56`).

Known maintenance breakage: `32ff618` (2024-05-02) changed material stride `0x28c → 0x290`; `c2df20b458757a094484f3ee1514d089bbfdaabd` (2025-01-25) changed ItemPickUpper pattern offsets `0x50/0x54 → 0x70/0x74` for “new Noita beta”. Earlier fixes include `737c488`/`34947ab` (May 2023), `59740c4` (October 2023 damage layout), `6ab03c9`/`4e955b4` (February 2024 patterns/dev build). Signature scanning tolerates relocation, **not arbitrary compiler/layout changes**. Missing patterns are inconsistently guarded; a nil function is not a safe fallback for all APIs.

#### Static check against this machine's executable

Read-only PE/header/disassembly and byte-pattern matching were performed against `GAME/noita.exe`; this is **not runtime validation**. Its PE machine is `0x014c` (i386), image base `0x00400000`. SHA-256: **`808d2a0ab51ea0b46e9ad2aeb3327a4b0ce3feae04f32ba26326bf585b5779bd`**. `lua51.dll` SHA-256: `8a5ca62b77bb5708a1b45c85808cff1191299be34b49a36168354f3bb87620ec`.

| NP discovery target | Static result / preferred VA on this exact executable |
|---|---|
| `GetWorldInfo`: get/remove/construct cell, chunk-loaded | Each byte pattern uniquely matches; inferred function starts **`0x0089c2d0`, `0x0071b480`, `0x007048c0`, `0x0089c1b0`** (`src/world_info.cpp:26–29`). |
| `GetGameGlobal` | Two call-site matches, both target **`0x00439bb0`**; NP chooses first. |
| Entity lookup, manager, serializer/deserializer | Entity lookup **`0x0056eba0`**, manager pointer slot `0x01204b98`; serialise start **`0x00a59d50`**; first deserialise match after serializer's end gives start **`0x00a5a190`**. |
| Player slots / wand / component access | Deathmatch pointer slot `0x01204bc0`; fire function `0x00c0d290`, RNG global `0x0120741c`; updated-entity variable `0x01204ff0`; component lookup `0x008a8a00`. Discovery patterns match. |
| Pausing, GUI, damage, materials, magic numbers | Current ItemPickUpper pattern matches once; old offsets match zero times. Inventory GUI, damage-string xref, material clear/load, UI grid patterns match. Magic-number traversal finds **403** names, including all six NP type probes (`src/magic_nums.cpp:136–147`). |
| `ForceLoadPixelScene` duplicate check | **Zero matches** for NP's exact pattern (`src/main.cpp:365–370`). A relaxed register variant exists at `0x0087f574`, but was not patched or executed. NP therefore falls back to stock `LoadPixelScene` (`:712–714`). |

Disassembly of `get_cell` confirms a chunk grid indexed using `>>9` and `&0x1ff` (512-pixel chunks), `ChunkMap+8` and a per-cell pointer slot. **An unloaded chunk returns a shared dummy slot** (`noita.exe:0x0089c30a–0x0089c312`); never write without `chunk_loaded`. The removal path uses the chunk map at `GridWorld+0x500` (`0x0071b4b6`), consistent with `world_ffi.lua:265–272`. `construct_cell` reads material `cell_type` at `+0x38` and the texture-hole flag at `+0x8c` (`0x007048f2–0x00704912`); transparent texture pixels can return nil. These observations support, but do not prove, safe live access.

**Useful stock API correction:** current `API:131` already has `LoadPixelScene(..., load_even_if_duplicate:bool=false)` as its **10th argument**. Prefer evaluating that documented argument over relying on NP's broken duplicate-check pattern. No code change was made.

### 3. Complete relevant Lua-visible API surface

`np = require("noitapatcher")` registers **37 functions**, exhaustive in `NP:src/main.cpp:862–900`. `Documentation/noitapatcher.lua` documents 35; **`GetWorldInfo` and `EnablePCallErrorFilter` are omitted** there. Below `int` means Lua integer-compatible number; pointers are process-local address numbers, not transferable IDs.

| Function signature | Actual purpose / source |
|---|---|
| `InstallShootProjectileFiredCallbacks()` | Installs native pre/post fire hook (`main.cpp:470–488`). Must precede spread RNG writes. |
| `InstallDamageDetailsPatch()` | Hooks `TakeDamage_Impl`; adds global `GetDamageDetails` to **new** Lua states (`main.cpp:490–524`). Not a death-cancellation hook. |
| `SetProjectileSpreadRNG(value:int)` | Writes engine fire/spread RNG global (`main.cpp:526–530`); not the entire Lua/world RNG state. |
| `RegisterPlayerEntityId(id:int)` | Filters red damage flash to one player; `-1` restores default (`main.cpp:194–212,533–538`; doc `:38–41`). Does **not** choose camera/game-over player. |
| `SetActiveHeldEntity(entity:int,item:int,unknown:bool,make_noise:bool)` | Calls native inventory-held-item change (`main.cpp:540–560`). `unknown` semantics remain unknown. |
| `SetPlayerEntity(entity:int,slot:int=0)` | Replaces entry in `DeathMatch.player_entities` (`main.cpp:562–576`); does not create inventory or player components. |
| `GetPlayerEntity(slot:int=0) -> int?` | Reads that player slot (`main.cpp:578–594`). |
| `EnableGameSimulatePausing(enabled:bool)` | Installs simulate/GUI/pickup hooks; false keeps simulation advancing through menu pause (`main.cpp:596–606`; `game_pause.cpp:115–154`). |
| `EnableInventoryGuiUpdate(enabled:bool)` | Suppresses GUI updates without disabling component (`main.cpp:608–614`); menu workaround, not general time control. |
| `EnablePlayerItemPickUpper(enabled:bool)` | Suppresses pickup updates for registered player (`main.cpp:616–622`; `game_pause.cpp:36–40`). Not a gold-powder event callback. |
| `UseItem(responsible:int,item:int,ignore_reload:bool,charge:bool,started_this_frame:bool,x:number,y:number,target_x:number,target_y:number)` | Sends native `Message_UseItem`, temporarily setting updated-entity ID (`main.cpp:624–654`). Responsible argument is read but not inserted into the constructed message. |
| `SilenceLogs(literal:string) -> bool` | Finds a literal and replaces a native log call sequence with NOPs (`main.cpp:656–708`). |
| `ForceLoadPixelScene(materials:string,colours:string,x:number,y:number,background:string,skip_biome:bool=false,skip_edges:bool=false,map:table={},z:int=50)` | Temporarily bypasses duplicate check; if target not found, simply invokes stock binding (`main.cpp:710–735`). Current stock 10th argument is separate. |
| `EnableExtendedLogging(enabled:bool)` | Hooks print and Lua pcall import for source/stack locations (`main.cpp:737–747`; `extended_logs.cpp:44–64`). |
| `EnableLogFiltering(enabled:bool)` | Enables `FilterLog` callback in logging hook (`main.cpp:749–753`). |
| `EnablePCallErrorFilter(enabled:bool)` | Routes enhanced pcall error text through filter (`main.cpp:755–759`; `extended_logs.cpp:126–160`). Undocumented in signature file. |
| `ComponentUpdatesSetEnabled(system:string,enabled:bool) -> bool` | Changes update vtable entry for matching `class <name>` system, globally (`main.cpp:765–805`). Not per-entity freeze. |
| `SerializeEntity(entity:int) -> string?` | Engine entity serializer with NP `SerialSaver` (`entity_serialisation.cpp:21–37`). Opaque build-specific format. |
| `DeserializeEntity(into_entity:int,blob:string,x:number?,y:number?) -> int?` | Engine loader into existing entity, optional forced position (`entity_serialisation.cpp:39–78`); create target with stock `EntityCreateNew()` (`API:8`). |
| `PhysBodySetTransform(component:int,x:number,y:number,angle:number,vx:number,vy:number,angular_v:number)` | Writes six Box2D transform/velocity doubles at fixed offsets (`physics.cpp:7–35`). **No geometry.** |
| `PhysBodyGetTransform(component:int) -> x,y,angle,vx,vy,angular_v` | Reads same six doubles (`physics.cpp:37–58`). Errors if no component/body. |
| `SetGameModeDeterministic(enabled:bool)` | Sets current mode's daily/deterministic flag; docs say affects unlocks/all-spells availability (`game_mode.cpp:8–17`; doc `:155–157`). Not deterministic multiplayer simulation. |
| `GetGameModeNr() -> int` | Reads current mode index (`game_mode.cpp:20–27`). |
| `GetGameModeName(index:int=current) -> string` | Mode name lookup with bounds check (`game_mode.cpp:35–49`). |
| `GetGameModeCount() -> int` | Size of mode vector (`game_mode.cpp:29–33`). |
| `SetPauseState(value:int) -> previous:int` | Writes game pause bitfield (`main.cpp:807–823`). Docs list 0,1,4,≥32 as safe; 4 = Esc menu (`Documentation/noitapatcher.lua:159–164`). |
| `GetPauseState() -> int` | Reads bitfield (`main.cpp:825–837`). |
| `GetWorldInfo() -> {get_cell:int,remove_cell:int,construct_cell:int,chunk_loaded:int,game_global:int}?` | Returns process pointers after pattern discovery (`main.cpp:839–860`; `world_info.cpp:21–44`), no ready snapshot object. |
| `SetInventoryCursorEnabled(enabled:bool)` | Hooks UI grid to disable interactions (`noita_ui.cpp:102–112`). |
| `CrossCallAdd(name:string,f:function)` | Registers/replaces main-state callback (`crosscall.cpp:20–41`). |
| `CrossCall(name:string,...primitive) -> up to 10 primitives` | Calls registered main-state function across Lua states. Supports nil/bool/number/string/lightuserdata; tables/functions become nil (`crosscall.cpp:43–113`). Also installed globally in newly created states. |
| `GetVersionString() -> string` | Finds embedded engine build text (`version_string.cpp:11–24`), not dependency version. |
| `MagicNumbersSetValue(name:string,value:bool\|int\|number\|string)` | Writes discovered typed magic-number address; implementation supports strings despite narrower annotation (`magic_nums.cpp:200–245`). |
| `MagicNumbersGetList() -> {name:string,address:int,type:string}[]` | Discovers names/addresses/types, not their current values (`magic_nums.cpp:248–270`). |
| `ReloadMaterials()` | Native clear/reload CellFactory (`material.cpp:72–87`); docs require clearing world state's `changed_materials` afterwards (`Documentation/noitapatcher.lua:226–228`). |
| `GetComponentAddress(id:int) -> address:number` | Raw component pointer, possibly zero (`ecs.cpp:14–19`). No safe field schema. |
| `GetEntityAddress(id:int) -> address:number` | Raw entity pointer, possibly zero (`ecs.cpp:21–26`). |

Additional globals/callbacks, not extra module-table functions:

- `GetDamageDetails() -> table?` during hooked damage: `damage_types`, `ragdoll_fx`, `impulse={x,y}`, `world_pos={x,y}`, `knockback_force`, `blood_multiplier`, `projectile_thats_responsible`, `description` (`damage_detail.cpp:93–133`). Does not expose/cancel final death. Some docs annotate vectors as integers, but implementation pushes numbers; knockback is pushed as integer.
- `OnProjectileFired(shooter?,projectile?,rng,x,y,target_x,target_y,send_message,unknown1,multicast_index,unknown3)` and identical `OnProjectileFiredPost(...)`; pre/post native fire, no return-based cancellation (`main.cpp:127–192`; `Documentation/callbacks.rst:9–63`). RNG setter in pre hook can alter spread; post extracts realized state. Unknown arguments are not promises of a stable ABI.
- `FilterLog(source:string,function_name:string,line:int,...strings) -> bool` (`Documentation/callbacks.rst:66–85`).
- NP hooks `luaL_newstate` to grant globals to **new states**, and stores the latest module-loading state in `current_lua_state` (`main.cpp:907–958`). Do not load independent copies from multiple mods. [Issue #4 author response](https://github.com/dextercd/NoitaPatcher/issues/4) explicitly says multiple copies are not handled well; losing CrossCalls is a known failure.

#### Pixel/world/FFI modules

`require("noitapatcher.nsew.world_ffi")` obtains function pointers through `GetWorldInfo` and exposes the following (`nsew/nsew/world_ffi.lua:319–424`):

| Lua-visible entry / exact C-ABI signature | Meaning |
|---|---|
| `get_grid_world() -> GridWorld*` | Follows hard-coded globals/world offsets. |
| `get_cell(ChunkMap*,int x,int y) -> Cell**` (`__thiscall`) | Returns writable cell-pointer **slot**, not `Cell*`; Lua annotation at `:354` is inaccurate. Check loaded chunk first. |
| `chunk_loaded(ChunkMap*,int x,int y) -> bool` (`__thiscall`) | World-coordinate loaded-slot test, not a generation request. |
| `remove_cell(GridWorld*,void* cell,int x,int y,bool) -> void` (`__thiscall`) | Native remove/destruct path. Lua annotation claims boolean; cdef says void (`:322,:357–359`). |
| `construct_cell(GridWorld*,int x,int y,void* material,void* memory) -> Cell*` (`__thiscall`) | Allocates if memory=nil. Remove old cell first, then assign returned pointer into slot; can return nil. |
| `get_material_ptr(id:int) -> CellData*`, `get_material_id(CellData*) -> number` | Converts using CellFactory base/`0x290` stride. Material order must agree between clients. |

Exposed FFI types: `Position`, `Colour`, `AABB`, `CellType`, `Cell`, `CLiquidCell`, `ChunkMap`, `GridWorld`, `GridWorldThreaded`, `WorldUpdateParams`, `GridWorldThreadImpl` (`:374–384`). Vtable methods include destroy, cell type, colour get/set, material, position, burning check/stop and remove (`:181–223`). Cell fields include `hp`, `is_burning`, material pointer; liquid adds x/y, `is_static`, unknown floats, `colour` and `original_colour` (`:225–249`). Exposing fields is **not** proof they can all be restored safely.

`require("noitapatcher.nsew.world")` exposes `EncodedAreaHeader`, `PixelRun`, `EncodedArea`, `encoded_size(area)`, `encode_area(chunk_map,x0,y0,x1,y1,buffer?) -> area?`, `decode(grid_world,header,runs)` (`world.lua:14–75,87–270`). Encoding caps dimensions at 256×256 and RLE at 4096 runs; `PixelRun={uint16 length_minus_one,int16 material,uint8 flags}`; header is x/y int32, dimensions-minus-one uint8 and run count uint16. Only material and liquid `is_static` are encoded. `CELL_TYPE_SOLID` is omitted (`:136–145`), and unloaded destinations are skipped (`:215`). No hp, burning state, colour/original colour, unknown liquid dynamics, gas/fire-specific state or body geometry is preserved. Current material equality also skips flag-only changes (`:228–245`). This codec is **not a complete G3 snapshot format**. A noisy 128×128 tile can exceed 4096 runs; 64×64 bounds the worst case to 4096.

Terrain is not synonymous with `CELL_TYPE_SOLID`: `VANILLA:materials.xml:793–806` defines `rock_static` as `cell_type="liquid"`, `liquid_static="1"`; `steel_static` likewise at `:3116`, while `rock_box2d` at `:13468` and wood at `:11857` are solid. Original NSEW commit `ce6b330`, 2022-08-25, explains excluding SOLID: “Doesn't actually work and maybe makes the game less stable to try this.”

`require("noitapatcher.nsew.rect")`: `Rectangle(left,top,right,bottom)` with `area()/height()/width()`, `parts(iterator,size)`, `Optimiser_new()` with `submit(rect)`, `scan()`, `reset()`, `size()`, `get(index)`, `iterate()` (`rect.lua:53–167`). Its C ABI is `rectangle_optimiser_{new,delete,reset,submit,scan,size,get}` (`rect.lua:22–28`; `rectangle_optimiser.cpp:6–45`). `native_dll.lib = ffi.load(__nsew_path.."nsew_native.dll")` (`native_dll.lua:4–9`). This DLL merges rectangles; **it does not implement pixel access**. World access itself uses LuaJIT FFI into engine functions located by `noitapatcher.dll`. Dirty-update rectangles can be read from `GridWorld.mThreadImpl`/`updated_grid_worlds`/`world_update_params`; the upstream example does so in `Examples/client/nsew_client/init.lua:154–205`.

**No built-in network/socket, mod-file API, file serializer, general clock setter, physics fixture serializer, powder callback, chunk-generation-authority hook or death-veto callback exists in the 37-function NP table.** Unsafe Lua provides `ffi/io/os`; stock mod-file functions remain `API:369–371`. LuaSocket belongs only to the example client (`Examples/client/nsew_client/init.lua:1–6`). NP's `CrossCall` is intra-process Lua-state bridging, not networking. `SetRandomSeed`/`SetWorldSeed` are stock (`API:207,233–237`); spreading RNG control is not a full RNG snapshot.

#### Mapping to our gates

| Gate | Useful NP surface | Missing proof / missing capability |
|---|---|---|
| G3 world snapshot | `GetWorldInfo`, `world_ffi`, dirty rectangles, optional rectangle optimiser | Complete per-cell metadata, static/physics-cell handling, loading order, thread safety and a codec without silent losses/cap failure. |
| G4 player persistence | `SerializeEntity`/`DeserializeEntity`, `Set/GetPlayerEntity`, held-item/UseItem | Perks, children, statuses, material inventories and cross-process rebind semantics are not proven. NP's format differs from Poro's file format (`serializer.hpp:10–11`); do not treat it as a save-file blob. |
| G5 damaged gramophone | Raw component/entity addresses, entity blob, transform functions | NP only exposes six transform values, no fixtures/shapes/pixel masks/joints/fragments snapshot. Stock already has body-ID transforms (`API:254–267`); NP transforms do not solve geometry. |
| G6 gold powder | Cell material access, system toggles, raw ECS pointers | No native collection/quantity/Wallet transaction hook. Observing wallet deltas or removed cells alone does not establish income causality. |
| G7 death/bot | Player slot change, damage details, real held-item/use-item calls; pause control | No final-death veto, no guarantee for polymorph. Stock `COMP:759,1532,1540` supplies wait/death/damage callbacks, but ordering still needs live proof. |
| G9 remote generation | `chunk_loaded`, raw grid, pixel-scene helper, mode/seed information | No force-stream-region or spawn-authority API. `API:172` supplies streamed-in area check; stock scene duplicate flag exists. Exactly-once generation needs higher-level ownership/manifests. |

### 4. Loading, Proton and building

Canonical load sequence (upstream instructions, **not created/executed here**): mod.xml requests `request_no_api_restrictions="1"`; `dofile_once("mods/<our_mod>/NoitaPatcher/load.lua")`; then `local np = require("noitapatcher")` (`Documentation/Usage.rst:4–25`). `load.lua:5–24` briefly wraps `do_mod_appends`, derives its own path, sets `__nsew_path`, appends `?.dll`/`?.lua` to package paths. `require` loads exported `luaopen_noitapatcher`, not `ffi.load` (`main.cpp:916–930`). The NSEW **rectangle helper** is the separate `ffi.load` path above.

This is a **Windows i686 DLL under Proton**, not a `.so`. `noita.hpp:41` asserts 4-byte pointers; calling conventions include `__thiscall`, `__fastcall`, stdcall and explicit stack/register adjustment (`calling_convention.hpp:4–29`). The `vs2013` shim uses engine-compatible string/vector layouts and allocates through `MSVCR120.DLL` exports (`vs2013/src/init.cpp:12–21`). Runtime imports of both inspected patcher releases: `KERNEL32.dll`, `dbghelp.dll`, `lua51.dll`, `MSVCP140.dll`, `VCRUNTIME140.dll`, UCRT heap/math/runtime API sets. Noita's own `lua51.dll` and MSVCR120 plus the x86 VC runtime/Wine equivalents must resolve. A Win64 DLL cannot load in this process.

Native Windows is **not required for a possible source build**: `TC-mingw.cmake:4–25` explicitly selects `i686-w64-mingw32-gcc/g++`, Windows system and MinGW sysroot. It contains an author-specific `/home/dexter/.local/...` path and `CMAKE_CROSSCOMPILING_EMULATOR=wine`; do not run configure/tests unchanged under this assignment. Requirements: CMake ≥3.26, C++20, MinHook/Zydis, Lua 5.1-compatible headers/import library, Abseil/gtest, Boost ≥1.78 (configure requires it for examples), Windows SDK-equivalent headers/libraries (`CMakeLists.txt:1–14`; `dependencies.cmake`). Docs additionally need Python/Sphinx tooling; packaging fetches Handtokening.

Official CI uses Windows 2022 + Visual Studio 2022, x86 `vcvarsall ... -vcvars_ver=14`, Ninja, `/MD`, LuaJIT 2.0.4 and vcpkg `x86-windows-static` Boost (`.github/workflows/build.yml:14–59`). MSVC is the **release-build route**, not an intrinsic requirement for the provided MinGW route. On this machine GCC/G++ **16.2.0** for i686, MinGW headers/CRT/binutils, dbghelp/ws2_32 import libraries, CMake **4.4.5** and Ninja **1.13.2** exist; host LuaJIT 2.1 is installed, but no i686 Lua headers/library or MinGW Boost was found in the target sysroot. `cl` was not found; `clang-cl` alone is not an installed MSVC SDK/runtime toolchain. **No source build was attempted.**

Reproducibility blockers: Abseil/gtest/vcpkg float; Boost lacks an exact version; Handtokening's pinned revision has no license; and the CI's LuaJIT `v2.0.4.zip` URL now returns **404**, with no remote `v2.0.4` tag. The old release commit remains in git (`69e5342...` above). Fetching the tag URL unchanged will fail today; use an independently pinned source revision/import-library strategy if building. These are preparation findings, not a request to run or repair upstream CI now.

Unsafe-mode official boundaries: `GAME/mods/example/mod.xml:12–14` explicitly mentions full `os/io` access; `GAME/tools_modding/READ_ME_FIRST.txt:65` says unrestricted mods do not work through Steam Workshop. Manual installation is expected. Proton loader success for a fresh independently obtained NP package remains a live experiment, even though EW on this machine uses the same upstream 1.35.3 DLL bytes.

### 5. EW bundle versus independent releases

Comparison was **hash/name/size/EOL-normalization only for provenance**, no EW content copied or transplanted. Both local EW checkouts have the same DLL/PDB/world_ffi hashes. The `EW:quant.ew/NoitaPatcher` bundle corresponds to **upstream 1.35.3**, not latest 1.36.2:

| File | SHA-256 / result |
|---|---|
| `noitapatcher.dll`, 617984 bytes | `b2292ec67b0bbabfc07ec9447fb4bd9c030b5669c17a11b90c5768cd5c69d76c` — byte-identical upstream 1.35.3 |
| `noitapatcher.pdb`, 4722688 bytes | `d2f8bd64c8f124acf1c4064f4991aa90e9c20daf61411273e960059eb5a223e1` — byte-identical upstream 1.35.3 |
| `noitapatcher/nsew/nsew_native.dll`, 49152 bytes | `4128109f9ddb1f1893f91d4143c46da066ba1cbc3584fbfc56ea26bb9aac0779` — byte-identical upstream 1.35.3 |
| `native_dll.lua`, `rect.lua` | Same as upstream 1.35.3 after CRLF→LF normalization only. |
| `load.lua`, `world.lua`, `world_ffi.lua` | Still differ after EOL normalization: respectively 2 lines removed; 5 removed/6 added; 5 removed/5 added relative to 1.35.3. Thus these are **not untouched release Lua files**. No changed code is reproduced here. |

The DLL matches **only 1.35.3 among the 81 distinct upstream release ZIPs downloaded** (API lists 82 assets, with a repeated filename). `world_ffi.lua` matches no release byte-for-byte. DLL PE resource reports `1.35.3.0`, machine `0x014c`, timestamp `2025-01-25T17:24:31Z`. EW update commit **`97ad02800c482f69140b4d436f14d1bd1e23f0a1`**, 2025-01-25, “Update NoitaPatcher”, records DLL size 618496→617984 and native DLL 49664→49152. This supports the hash attribution, not a transfer permission.

Independent release pin candidates:

- [1.35.3 ZIP](https://github.com/dextercd/NoitaPatcher/releases/tag/release-1.35.3), published **2025-01-25T17:24:42Z**, 1610939 bytes, SHA-256 **`232fae99a4d08843c85ed2bcdceec1ec7c51cf38e8bfefcedc4ade2c5c992e34`**. Above DLL/PDB/native hashes apply.
- [1.36.2 ZIP](https://github.com/dextercd/NoitaPatcher/releases/tag/release-1.36.2), published **2025-08-29T22:16:18Z**, 2131904 bytes, SHA-256 **`9637632b107390e6b8c4ae50525e2b2a96171f127825ed4af41bdccfa5ff987d`**, matching GitHub asset `digest`. DLL hash **`a6e257c54b69797abce0198015ea9da46d8d408021df84a94c687337e7eed968`**, native DLL **`3243e9f696dd4083bf2a8cec578b716fbaf45fb6ab0ea34c49f892be48e977f2`**.
- `git diff release-1.35.3 release-1.36.2 -- src/` is empty: native source is unchanged. The differences are version, CI/signing and `world_ffi.lua` colour-field descriptions. Different binary hashes/sizes therefore do not establish different engine functionality or reproducibility.

Use **one coherent independent release**, preferably the current 1.36.2 candidate for the later isolated smoke experiment, and record its full hashes. Do not mix EW-edited Lua into it or assume its different build tool/dependency versions are harmless.

### 6. Minimal alternative: our own native layer

This is an interface/engine-work inventory, **not implemented functionality**. A small Win32 DLL loaded through LuaJIT FFI can expose a plain C ABI; it need not clone NP's 37-function surface, bundled examples, logger, mode browser or signing machinery. Build-pinned addresses can be simpler than a scanner under D19, but every layout/calling convention must be independently verified. Do not translate unlicensed NP implementation line-by-line. Old MIT NSEW is a licensed reference for its original material; keep its notice and independently update offsets.

| Required engine access/hook | Why / minimum state to expose |
|---|---|
| Executable identity + game-global/world/CellFactory access | Fail closed before any write on another hash. Return safe bulk operations, not arbitrary pointer ownership to network inputs. |
| Chunk presence, cell-slot lookup, construct/remove, cell vtable/material layouts; dirty-region access | G1/G3: capture/install cells in a safe frame phase, preserve audited metadata, handle unloaded chunks and static materials. Our static lookup targets above indicate feasibility, not a stable public ABI. |
| Entity manager/component lookup + native entity serializer/deserializer and compatible serializer interface | G4: entity trees/inventory with a versioned blob envelope. The sixteenth-byte serializer vtable/string ABI and MSVC2013 heap/layout are part of the work, not a memcpy shortcut. |
| Player slot assignment; held-item/use-item message and updated-entity scope | G4/G7: restore controller/camera identity and let the bot fire the **real current wand**. Native final-death interception is needed only if stock wait/damage/death callbacks cannot cover every death/polymorph path. |
| Full physics-body/fixture/shape/joint/material-mask capture/rebuild and fragment identity | G5: transforms alone are insufficient. **Exact geometry hook locations/layouts unknown**; inspect native physics representation and test partial destruction. |
| Native gold-material absorption → wallet credit hook | G6: capture quantity/material/entity and commit exactly once at the removal/credit boundary; do not classify purchases as negative income. **Exact hook unknown**, NP does not supply it. |
| Simulation/menu pause bypass, with inventory/pickup behavior | D25/G7: keep simulation running under Esc without GUI/pickup corruption. No need for a global game-time rewrite just to disable pause. |
| Fire pre/post hooks + spread RNG state, if replay needs them | G8: tie authoritative cast and replica to realized projectiles, not a claim that identical seeds make all casts deterministic. |
| Generation/load/spawn admission callbacks only if stock Lua points are insufficient | G9: host-approved chunk manifests and shared spawn IDs. `chunk_loaded` does not generate a biome; exact missing native hook is still to be determined. |

Networking need not be an engine hook: a separately licensed nonblocking socket library or a tiny independent Winsock C-ABI wrapper can connect to the Linux agent. File/protocol/SQLite logic belongs outside the game. MinHook (BSD-2-Clause) and Zydis (MIT) can be used independently with notices; NP is not required to make those libraries usable.

## Risks and options

1. **Legal/provenance:** keep independent download/hash provenance and personal-use risk explicit; no false MIT attribution. Strict license clearance remains blocked for NP/current additions and exact full-build closure. Publication alternatives are source-only separation or replacement, not copying an EW DLL.
2. **Build/layout fragility:** enforce D19 in our loader, not merely `GetVersionString`. Pattern success does not prove struct validity; the pixel-scene miss already demonstrates selective breakage on the pinned game.
3. **Persistence:** NP serializer is unversioned and not a trusted-input parser. `entity_serialisation.cpp:56–60` only rejects blobs shorter than 40 bytes; `serializer.cpp:135–145` uses `< data_end`, so an exact-end read is marked overflow. Whether the last-field behavior matters is **[INFERENCE]** pending round-trip experiments. Validate transport envelopes/identity outside the engine before deserializing.
4. **State lifetime:** raw pointers cannot cross process/reconnect boundaries; one NP instance and one callback-owning Lua state are required. No live proof of restart restoration or full physics geometry exists.
5. **Scope:** a complete G3/G5/G6 solution likely needs native work even if NP is retained **[INFERENCE]**; existing getters are candidate access paths, not acceptance evidence.

## What still needs in-game verification (Что остаётся проверить в игре)

Run only after an explicit launch request, on the future isolated test profile, never the user's real save. Log exact game/dependency/package hashes and input/output, not just “did not crash”.

| Experiment / input | Expected observable; pass/fail |
|---|---|
| Fresh independent 1.36.2 package, unsafe minimal mod, no EW, exact pinned game; query version, 37 exports and `GetWorldInfo` | Version/addresses agree with static derivation and API calls are usable. Fail on nil target, loader/runtime import error or crash. This is not yet a pixel-access pass. |
| G1: loaded 8×8 air patch, construct chosen water/sand/static-rock cells, read their slots/materials back, remove and re-read | Exact expected materials/count at all written coordinates, no neighboring corruption; nil texture-hole cases logged explicitly. Fail on missing cells, wrong material or crash. |
| Negative build identity input to our eventual loader (without patching the real installation) | Loader refuses **before** NP/native memory writes. Fail if native hooks install first. |
| G3: bounded patch with water, sand, fire, gas and static terrain; snapshot/install in second isolated process, including static-flag-only change and >4096-run case | Material plus every claimed metadata field agrees; omitted fields enumerated and dense patches handled, not silently dropped. Measure bytes/frame time later. Stock upstream codec alone is expected to fail completeness. |
| G4: player with Always Cast/trigger wand, mixed flask, perks, statuses; serialize, restart process, deserialize, select restored player | Inventory trees, resources/charges, statuses and player identity survive; compare final-field-sensitive data and re-serialized blobs. Fail on stale IDs, loss, duplicated child or game over. |
| G5: partially destroyed gramophone, detached fragments and jointed body, process restart | Fixture/mask/fragment equality, not merely matching six transforms. If entity serialization omits shape, record that failure and locate native geometry path. |
| G6: measured gold-powder patch, two collectors plus an intervening shop purchase | Native event accounts removed gold once and credits both exactly once; purchase never appears as income. NP alone currently offers no proving hook. |
| G7: lethal damage, curse/fire, polymorph, entity replacement; then bot fires current real wand | No game over while another slot is alive; all final-death paths observed, wand consumes actual state. SetPlayerEntity example is not a death-coverage proof. |
| G9: client enters unloaded biome far from host; repeat/meet/reload; also use documented stock 10th scene argument | Exactly one host-approved shared spawn set and consistent terrain; `LoadPixelScene` duplicate flag effect visible. Fail on missing/duplicate spawns or false assumptions that NP forces chunk generation. |
