# G2 preparation: isolated dual-Noita test rig

## Verdict

**Feasible in principle; confidence: medium; G2 remains unpassed.** This machine has the runtime, GPU, RAM and filesystem features needed for a two-instance experiment. Separate Proton prefixes plus separate game working directories are the safest candidate. Concurrent startup, Steam-off operation, focus behavior and Cloud isolation have **not** been demonstrated. `tools/testrig/plan.sh` is therefore a completed **read-only planner**, not an executable launcher; it has no execute branch.

No game, server, agent, network probe mod, prefix clone or save copy was started/created by this spike. No game installation, real prefix, save directory, EW checkout or switcher was modified. D17 still requires an explicit request in the current session before any live launch.

## What was checked

### Scope and source notation

Paths below are shorthand, not additional directories created by this spike:

```text
G = /data/Games/steamapps/common/Noita
S = /home/mora/.local/share/Steam
C = /data/Games/steamapps/compatdata/881100
P = S/compatibilitytools.d/GE-Proton11-12-x86_64
R = /data/Games/steamapps/common/SteamLinuxRuntime_4
N = C/pfx/drive_c/users/steamuser/AppData/LocalLow/Nolla_Games_Noita
REPO = /data/Development/Projects/noita-multiplayer
```

Repository constraints read first: `AGENTS.md`, `DECISIONS.md` D7–D9/D17–D19/D22/D35, `ACCEPTANCE.md` G1–G9, `EVIDENCE.md`, and the roadmap. Existing save-layout research was cross-checked at `docs/research/vanilla_world_and_saves.md:369–395,543–555`; its description of the *then-active* profile is historical, not the current profile.

### Machine and storage: commands and observed output

| Command used | Observed output / limit |
|---|---|
| `uname -a` | `Linux sapphire 7.3.0-4-cachyos ... x86_64 GNU/Linux`. |
| `id`; `nproc` | uid/gid 1000 (`mora`); 16 logical CPUs. |
| `free -h` | 62 GiB RAM, 48 GiB available; 94 GiB swap, 2.3 MiB used at inspection time. Not a Noita memory measurement. |
| `df -h /data /home /tmp` | `/data`: 1.6T total, **24G available, 99% used**; `/`: 5.1G available, 98% used; `/tmp`: 32G tmpfs available. |
| `findmnt -T /data/Games` | `/data`, `/dev/nvme0n1p1`, `btrfs`, `rw,relatime,ssd,...`. |
| `stat -f -c '%n fsid=%i type=%T' REPO/.cache C G S /tmp` | Repository, game and real prefix have the same btrfs fsid `a44f1acd8daa51c8`; Steam client under home is on a different btrfs filesystem; `/tmp` is tmpfs. |
| `du -sh C`; `du -sh --apparent-size C` | 382M allocated; 374M apparent. This includes `pfx`, registry and Proton metadata. No copy made. |
| `du -sh N/*`; same with `--apparent-size` | `save00`: **2.4M allocated / 1.7M apparent**; `save01`–`save06`: 4K / 53 bytes each; `save_rec`: 1.7M; `save_shared`: 12K. |
| `du -sh N` | 4.1M total. |
| `ls -la N/save00`; `find N/save00 -type f \| wc -l` | 194 files; current `world/` empty; profile archives occupy much of this tiny save00. This is not a representative long-run size. |
| `du -sh G`; `du -sh G/data/*` | Game installation 1.7G; audio 1.2G, fonts 165M, `data.wak` 41M, schemas 21M. User mods/proxy/backups are also present in G and must be excluded. |
| `cat REPO/.gitignore` | `/.cache/` already ignores `.cache/testrig/`; no ignore change required. |
| Scratch-only `cp --reflink=always` of a newly generated 4 MiB file; `btrfs filesystem du -s` | Copy succeeded; total 8 MiB, exclusive 0, shared set 4 MiB. Both scratch files/directories were removed. **No game or prefix file was copied.** |

24G currently allows a modest rig, but 99% usage is a real risk: btrfs allocation pressure and logs/world growth still matter. Use `/data`, not home or a long-lived `/tmp` tree; recheck space before the live run. The planner proposes a conservative 6 GiB free-space floor, not a measured engine requirement. Reflinks save blocks, **not** future write growth; never hard-link mutable files.

### Steam library, installed tools and actual launch path

| Command used | Observed output / limit |
|---|---|
| Read selected fields in `S/steamapps/libraryfolders.vdf` | Library 0 = S; library 1 = `/data/Games`; app 881100 belongs to library 1. |
| `cat /data/Games/steamapps/appmanifest_881100.acf` | App 881100, `Noita`, installdir `Noita`, buildid **17130612**, StateFlags 4. `AutoUpdateBehavior="0"`; this is not evidence that the D19 freeze has already been configured. |
| `ls -la C`; `cat C/version C/config_info` | Real prefix C exists; `pfx`, `pfx.lock`, `tracked_files`, `version`, `config_info`. Prefix version **11.0-100**; config points at P. |
| Read only the 881100 entry in `S/config/config.vdf` | Forced compatibility tool **GE-Proton11-12-x86_64**, priority 250. |
| `cat P/version P/toolmanifest.vdf` | `1793687165 GE-Proton11-12`; required runtime appid **4183110**. |
| `ls /data/Games/steamapps/common` filtered for Proton/runtime | Proton Experimental, 10.0, 9.0 (Beta), Hotfix; directories also named Proton 5.0, 8.0, BattlEye Runtime and EasyAntiCheat Runtime. The presence of the old directories alone does not establish a complete runnable old tool. |
| Read `version` files of Experimental / 10.0 / Hotfix | `experimental-11.0-20261112-x86_64`, `proton-10.0-4b`, `hotfix-20261007-x86_64`. |
| Read selected fields of app manifests 4183110 / 1628350 / 1391110 / 1070560 | Steam Linux Runtime **4.0** build 26116036; **3.0 (sniper)** build 26116003; **2.0 (soldier)** build 26115965; **1.0 (scout)** build 26115262. All have corresponding common/ directories. |
| Read only the Noita block in `S/userdata/*/config/localconfig.vdf` | Launch Options: `WINE_CPU_TOPOLOGY=8:0,1,2,3,4,5,6,7 gamemoderun taskset -c 0-7 /home/mora/.local/bin/noita-switcher launch -- %command%`. |
| `pgrep -a wineserver`; process listing filtered for game/Proton | No wineserver or actual Noita game at inspection; Steam client was already running. No process was stopped. |
| Read EW fork `proxy.log:1` | Captured real `%command%`: `steam-launch-wrapper -- reaper SteamLaunch AppId=881100 -- R/_v2-entry-point --verb=waitforexitandrun -- P/proton waitforexitandrun G/noita.exe`. |
| Read `S/logs/console-linux.txt` near lines 1144–1180 | Existing launch changed cwd to G, used GE-Proton11-12, and ran Noita via the user's solo profile wrapper. These are historical user-session logs, not a new test. |
| `grep -n 881100 S/logs/cloud_log.txt` | Existing **Upload complete, result OK** at 22:04:40 after the 22:03:36 game exit, including `Nolla_Games_Noita/save00/stats/...`; Cloud is genuinely active. |

D9 removes Steam Networking from our architecture; it does **not** prove that this Steam-distributed game has no Steam startup behavior. Do not run Steam Play, `steam://run/881100`, the user's wrapper, or EW's mod manager for the rig: those lead back to the real installation/profile.

### Executable and modding facts

| Command / source used | Observed output / conclusion |
|---|---|
| `cat G/_version_hash.txt G/_branch.txt` | **9dbd52ced019a643169a2db02f46c77f8766c6e5**, branch `master`. |
| `sha256sum G/noita.exe`; `ls -l`; `file` | SHA-256 **808d2a0ab51ea0b46e9ad2aeb3327a4b0ce3feae04f32ba26326bf585b5779bd**; 15,460,864 bytes; **PE32 GUI Intel 80386**. |
| Read-only Python `struct` PE-header/import parsing; `objdump -p` | Machine 0x014c, OptionalHeader 0x10b, ImageBase 0x400000, GUI subsystem 2, LARGE_ADDRESS_AWARE set, NX set, ASLR flag absent. PE timestamp 2025-01-25 13:59:24 UTC. Win32 native DLLs are required, not x64 DLLs. |
| PE imports | `lua51.dll`, `SDL2.dll`, `fmod.dll`, `fmodstudio.dll`, `MSVCP120.dll`, `MSVCR120.dll`, **steam_api.dll**, Windows system DLLs including WS2_32 and WININET. Keep the vanilla steam_api.dll even in the Steam-off experiment: loader dependency and successful API initialization are different things. |
| `cat G/steam_appid.txt` / byte dump | Six bytes `881100` with no newline. File already ships in this installation. |
| `strings -a -t x -n 3 G/noita.exe` | `SteamAPI init` at file offset 0xbe757c; `Init failed` 0xbe758c; `SteamAPI initialized` 0xbe7598. Imports include SteamAPI_Init and SteamAPI_RestartAppIfNecessary. |
| `objdump -d -Mintel --start-address=0x84782d --stop-address=0x84789e G/noita.exe` | At VA **0x84783a**, calls SteamAPI_Init via IAT 0xf07a30. False result reaches a logging call and **returns at 0x847872**; this function does not terminate there. On success, **0x847889** calls RestartAppIfNecessary via IAT 0xf07a18 with 0xd71cc = 881100. True result calls an imported function at 0x847898; no claim about the whole startup path follows from this bounded window. |
| `strings ...` with flag filtering | Real literals: `-always_store_userdata_in_appdata` (0xc28510), `-always_store_userdata_in_workdir` (0xc28534), `-save_slot` (0xc28910), `-config`, `-no_extra_config`, `-windowed`, `-fullscreen`, `-no_logo_splashes`, `-gamemode`, `-debug_lua`, `-enable_unsafe_paths`, `-clean_save`, `-clean_config`, `-clean_stats`, `-clean_everything`. Presence is not an end-to-end test of their semantics. |
| G/README.txt, “Some arcane secret spells” | Documents `-windowed` / `-fullscreen` and destructive `-clean_*`; explicitly warns that `-clean_everything` destroys stats/progress, including with Cloud enabled. No clean flags in the rig. |
| `G/tools_modding/lua_debugging.txt:4–11` | `-debug_lua` enables sandbox-escape exploits; instructions target noita_dev.exe. It is unnecessary for G2. |
| `G/tools_modding/READ_ME_FIRST.txt:3–4,65` | Modding Agreement applies; unsafe mods requesting unrestricted APIs do not work through Workshop. |
| `G/mods/example/mod.xml:4,9,12–15` | `request_no_api_restrictions="1"` requests full Lua API including os/io. Game-mode mods can opt into multiple save slots. This is a request, not proof of permission granted at runtime. |
| Read real `N/save00/mod_config.xml` | `<Mods>` with ordered `<Mod enabled="0\|1" name="folder_id" settings_fold_open="0" workshop_item_id="0\|ID">...</Mod>` entries. Currently all entries were disabled; no enabling performed. |
| Read selected root attributes in `N/save_shared/config.xml` | `mods_sandbox_enabled="1"`, disclaimer/warning fields 1; fullscreen 1; window/backbuffer 1920×1080; internal 1280×720; `application_pause_when_unfocused="1"`; sounds enabled. |
| `G/data/translations/common.csv:1985–1999` | English UI text says topmost enabled mod loads first; extra privileges require unsafe mods; Workshop mods cannot be enabled while unsafe mods are on; the Unsafe mods toggle requires an explicit warning confirmation. |

**Steam requirement remains unknown in practice.** [INFERENCE] The early-return path suggests the game may run when SteamAPI_Init fails. That is not proof that all later engine paths tolerate it. The first candidate intentionally omits `steam_appid.txt`, SteamAppId and SteamGameId from the **test copy**, so no deliberate Steam API session is requested. If this fails, stop and inspect logs; do not automatically retry through Steam or touch the real profile. A client-connected candidate retains the appid file and sets both IDs, as the existing launcher does, but is a separate supervised experiment because of Cloud/relaunch risks.

Secondary reference: [Noita Wiki launch options](https://noita.wiki.gg/wiki/Launch_Options) describes `-gamemode 0` as starting the default mode, `-windowed` as writing that setting, and `-no_logo_splashes` as skipping logo screens. The planner makes autostart an optional later step; no clean/debug flag or undocumented save redirection is needed. [Noita folders guide](https://noita.wiki.gg/wiki/Guide:_Noita_folders) corroborates the directory categories, not concurrency safety.

### EW and noita-switcher: knowledge only, no code reused

- `~/noita_entangled_worlds_fix/contributing.md:11–22` explicitly documents several local instances with different localhost ports, and separate native proxy / Wine launches. It does **not** document save isolation. `Justfile:42–49` also uses multiple ports and `-gamemode 0` for development.
- `noita_proxy/src/bookkeeping/noita_launcher.rs:106–169,176–209` locates Proton/runtime information and starts the game with explicit compatdata, cwd, localhost endpoint and Steam IDs. These establish useful environment variables, not a reusable launcher for us.
- `noita_proxy/src/bookkeeping/mod_manager.rs:305–335,481–508` confirms the prefix-relative save location and the two distinct files: enabled-mod list in `save00/mod_config.xml`, sandbox policy in `save_shared/config.xml`. EW changes both; the rig must never invoke this manager.
- `/data/Development/Projects/noita-switcher/README.md:94–99,114–124,145–170` explains that a switch replaces live save slots, leaves save_shared global, and stores inactive profiles as archives in save00. Modes/profiles currently include a live solo profile and archived co-op runs; copying save00 wholesale would copy those archives too.
- `noita_switcher/launch.py:151–184` resolves a profile then performs a real switch before exec; coop mode starts the proxy with Steam's command. It is **not** an isolation wrapper.
- `noita_switcher/profiles.py:92–127` scans all process cmdlines for noita.exe, not per-prefix. [INFERENCE] A running rig game may make the real switcher refuse a profile switch; never bypass that with `--force`.

## Findings

### Isolation choices

| Candidate | What is isolated | Verdict |
|---|---|---|
| Two separate test compat directories / Wine prefixes, with a separate game directory **inside each test C:** | Registry, wineserver namespace, LocalLow root, saves, config, mods and cwd logs | **Recommended candidate.** Fresh prefixes avoid copying any real progress. Same Wine user name is safe when C: differs. |
| Reflink-copy the closed real compatdata into two test directories | Similar runtime state, but also real saves, profile archives, Cloud markers and external drive mappings | D17 permits a copy, but it is unnecessary here. Must remove only copied userdata and review mappings before use; no such copy was made and the planner never reads C. |
| Same prefix with `-save_slot 0/1` | At most active run files (`??SAV`) | **No-go.** `??S00` remains save00 for persistent data/mod list; `??STA` is save00/stats and `??USR` is save_shared. Does not isolate registry, wineserver or shared config. Source: existing save research:376–381; executable strings `??S00/mod_config.xml`, `??S00/persistent/...`, `save00/stats`, `save_shared`. |
| Same prefix, two game cwds with `-always_store_userdata_in_workdir` | [INFERENCE] Potentially separate user-data roots | Unknown exact layout; still shares Wine runtime/registry. Not worth depending on for the first gate. |
| Same prefix with different USERPROFILE/HOME, registry Shell Folders, symlink swaps, or the real switcher | Ambiguous | **No-go initially.** Real prefix Shell Folders / known-folder GUID point at `C:\\users\\steamuser\\AppData\\LocalLow`; executable imports SHGetKnownFolderPath. A Linux env override alone is not a proven per-process Windows-profile redirect. |

The real prefix has `dosdevices/c: -> ../drive_c`, `z: -> /`, and `s: -> /data/Games`; its registry points at the standard steamuser folders. Prefix separation is **file/runtime separation, not a sandbox**. An unsafe mod still has full user access through Wine/native APIs. All dependencies and source symlinks need review. Kernel-enforced filesystem isolation would be a separate design if needed; none is asserted here.

### Proposed layout and lifecycle

```text
.cache/testrig/                      # already ignored
  instances/i1/                     # i2 equivalent
    compat/                         # STEAM_COMPAT_DATA_PATH
      pfx/                          # WINEPREFIX
        drive_c/nmp/Noita/          # PRIVATE game copy + cwd
          mods/nmp/                 # copy of OUR mod, optional dev symlink
          logger.txt                # engine cwd output when produced
        drive_c/users/steamuser/AppData/LocalLow/Nolla_Games_Noita/
          save00/{mod_config.xml,mod_settings.bin,world/,...}
          save_shared/config.xml
    xdg/{cache,config,data,state}/
    logs/{launcher.log,game/,steam-881100.log?}
  artifacts/<unique-live-run-id>/    # actual run evidence; never overwrite
```

1. **Read-only preflight:** verify pinned game hash, disk, no actual game, free ports, current-session permission, owned mod provenance. Archive original environment/commands without secrets.
2. **Preparation after review, not performed now:** create fresh instance trees; copy only listed vanilla assets into each test C: game directory with `cp -a --reflink=always`. Never copy G/mods, EW proxy/native bundles, libsteam_api.so, logs or backups. Do not hard-link files.
3. No real prefix clone is necessary: let the selected Proton initialize each new prefix on first approved launch. Static support: `P/proton:880–881` accepts no previous version; `1070–1089` fills missing template files; `1273–1306` initializes the prefix and its C:/Z: mappings. Preseeded game/config files are outside the template's normal files. Preservation and Noita's acceptance of a partial config remain live checks.
4. Copy our mod directory into `test-prefix/drive_c/nmp/Noita/mods/nmp/`. A source symlink is an optional later iteration aid; the runtime must see its target read-only. Enabled-mod state belongs in that prefix's LocalLow/save00, **not** the real game directory. Do not inherit the user's mod list.
5. Keep sandbox enabled initially and do not pre-accept legal/security prompts. During the approved first run, use **that instance's** Mods menu to allow Unsafe mods, confirm the warning, enable the mod and restart with mods active. Verify unrestricted os/io/FFI availability with a real probe before assuming it.
6. Launch candidates run P via R with explicit compat/data/install paths and cwd. A clean environment prevents inherited WINEPREFIX, SteamDeck/COPYPREFIX, overlays, Wine overrides and shared shader-cache paths from pointing at real state. Preserve only required display/runtime/DBus variables; direct caches/config writes under the instance. Proton itself sets WINEPREFIX from compatdata (`P/proton:2302,2999–3008`).
7. One native server on **127.0.0.1:24700**, two native agents listening on **24711 / 24712**, proposed mod env `NMP_INSTANCE_ID=i1|i2`, `NMP_AGENT_ADDR=127.0.0.1:24711|24712`. These ports were absent from `ss -lntu`; ephemeral range was 32768–60999. Recheck immediately before use; there is no port reservation yet.
8. Keep terminal/runtime stdout+stderr, per-mod logs, each agent/server log, process exit codes, exact build hashes, mod order, seed and tagged message counts. The current G/logger.txt only contained a header and FMOD errors; do not assume it includes every stdout/Lua message. In client mode PROTON_LOG writes `steam-881100.log` only if SteamGameId exists (`P/proton:2206–2223`); keep that in the instance logs. Verbose Proton logs may grow quickly.
9. Use per-instance user scopes `nmp-testrig-i1.scope`, `...i2.scope`; analogous server/agent scopes. Prefer normal game exit. Stop **only the exact test scope** for fault injection; never global pkill or an unqualified wineserver kill. Collect evidence before deleting an instance. Cleanup has no real-prefix restoration/switching step.

No Rust server/agent CLI or socket mod exists in this repository yet. The planner prints a **proposed integration contract**, not imaginary commands or a fake successful echo test. Two windows alone do not pass G2.

### Windows, focus, audio and GPU on this machine

| Command used | Observed output / implication |
|---|---|
| Relevant `env` keys; `pgrep -a -f 'xwayland\|niri'`; `niri --version` | Wayland/niri **26.04 (2516a83b-modified)**, WAYLAND_DISPLAY `wayland-1`, DISPLAY `:0`, xwayland-satellite running. No Noita-specific window rule was found in the read-only config inspection. |
| `niri msg --json outputs` | DP-4 Acer H236HL lists preferred 1920×1080 at 60 Hz. This is display capability, not a dual-window test. |
| `lspci` filtered for display | Intel Arc A380 plus AMD Navi 10 card. |
| `glxinfo -B` | Accelerated **AMD Radeon RX 5700 XT**, 8192 MiB VRAM, Mesa 26.2.5; OpenGL 4.6. |
| `vulkaninfo --summary` filtered for devices | Only GPU0 AMD RX 5700 XT / RADV, vendor 0x1002, device 0x731f. Do not promise the Arc as a usable second render GPU. |
| `gamescope --version`; `gamescope --help` | **3.17.0**; nested wayland/sdl and headless backends, independent XWayland count, resolution and focused/unfocused refresh controls. Help only, no compositor/game session started. |
| `pactl info` | PulseAudio compatibility on **PipeWire 1.6.10**; BEHRINGER UMC404HD default output. |
| `systemd-run --user --scope --collect --quiet --unit=nmp-testrig-capprobe -- /usr/bin/true` | Exit 0; user manager reports running. Only `/usr/bin/true` ran; no game/Proton. |

[INFERENCE] Two native/nested windows should be manageable, but only the focused one receives keyboard/mouse input; newly opened windows may steal focus. Avoid synthetic input and avoid fullscreen. The planned visible windows are 960×540 with internal size 1280×720; actual compositor resizing must be checked. The **real config pauses when unfocused**, so each copied config proposes `application_pause_when_unfocused="0"`; actual frame advance must be observed. Esc/menu pause is a separate issue and this setting is not evidence of the D25 multiplayer pause policy.

Keep FMOD enabled with both volume sliders at zero for the initial networking test; this avoids changing engine audio initialization while preventing doubled sound. Two Noita GPU/CPU loads, FPS, audio-device contention, minimization throttling and gamepad focus remain unmeasured. Do not inherit the user's identical CPU 0–7 affinity for both instances. After baseline measurements, optional separate CPU sets or nested gamescope windows may help. A headless gamescope run still **launches Noita and requires D17 permission**; availability is not proof of compatibility.

## Risks and options

### Real-state touch matrix and permission boundary

| Planned operation | Real prefix writes | Permission / remaining risk |
|---|---|---|
| Planner default / --dry-run / --help | **None**; no copy/mkdir/log file/launch | Read-only path normalization and printed text only. |
| Future fresh test-directory preparation | **None**; reads only whitelisted G assets | Already allowed in principle by D17, but not executed in this spike. No real userdata source. |
| Future Proton/game launch, restart, second instance, gamescope/headless launch | Intended **none**; explicit test compat path/cwd | **Explicit current-session user request required.** Initial live experiment must establish it really stays in those paths. A flag is not consent. |
| Future normal game exit / exact test-scope stop | Test state only | Must belong to the approved test session; no global process matching. |
| Future artifact collection / test-instance deletion | **None**; only validated .cache/testrig subtree | Close processes and retain evidence first; do not follow external symlink destinations during purge. |
| Steam-connected experiment | Unknown external Steam client activity | Separate supervised experiment. Cloud can operate in the real client outside a game's filesystem namespace; copied prefix alone does not prove Cloud safety. |
| Switcher launch/use, EW manager, Steam Play/settings | **Forbidden in this plan** | They can write/sync real state; no rig step invokes them. |

There are **zero planned write destinations in the real prefix**. Research did read its layout/config and used `du`; that is not a claim that the prefix was never inspected. Do not disable Cloud or change Steam settings automatically. Q22-cloud remains open; first prove the isolated Steam-off run, then design the Cloud experiment if it is still necessary.

### First live run: go/no-go checklist

- [ ] User explicitly requests Noita launch in **this** session and is ready for visible windows/focus changes.
- [ ] Source/copy hashes match D19; Steam update procedure is not silently changed.
- [ ] No real Noita/EW session; do not run the real switcher while the rig is active.
- [ ] At least the agreed disk reserve remains on /data; output is under the ignored rig tree, never home or real compatdata.
- [ ] Two distinct canonical test prefixes, game directories and LocalLow paths; no copied real save/profile archives or Cloud markers.
- [ ] Only our reviewed mod plus approved independent dependencies; no EW mod manager or bundled NoitaPatcher files.
- [ ] Supervisor first validates **one** fresh Steam-off instance. Unexpected Steam relaunch/API session or Cloud activity is a **stop**, not an automatic fallback.
- [ ] Read-only baselines for real prefix registry/save config metadata and current Steam Cloud log position recorded before launch; inspect after shutdown. No unrequested restore/copy-back.
- [ ] Captured runtime logs confirm test cwd, prefix, version, mod ID and actual save location; harmless distinct test-save markers remain isolated.
- [ ] Unsafe privilege prompt accepted only inside the test instance; real mod code proves os/io/FFI and endpoint configuration.
- [ ] Both instances advance frames when unfocused and use distinct agent ports; actual server/agent implementations exist and log tagged request/reply sequences.
- [ ] Exact test scopes and cleanup/evidence paths known before starting. Do not call G2 passed until the concurrent round trips and save isolation are observed.

### Delivered planner and offline checks

`tools/testrig/plan.sh` uses `set -euo pipefail` and a `NOT YET RUN` header. Default and `--dry-run` only print a labeled human-review plan. `--execute` is deliberately rejected. Destination canonicalization restricts every proposed write to this repository's `.cache/testrig/`; mod sources must be inside the repository and outside cached game/third-party files. No process launch/kill or filesystem-mutating command is executed by the planner.

Final parent verification (2026-11-19): `bash -n` and ShellCheck **0.11.0** passed. The scratch-only ShellCheck executable is `.cache/spikes/g2_testrig/venv-sc/bin/shellcheck`. The read-only CLI surface was exercised; no printed launch/preparation command was executed.

```bash
bash -n tools/testrig/plan.sh
.cache/spikes/g2_testrig/venv-sc/bin/shellcheck tools/testrig/plan.sh
bash tools/testrig/plan.sh --help
bash tools/testrig/plan.sh --dry-run --phase launch
bash tools/testrig/plan.sh --dry-run --phase prepare --mod-mode symlink
# Expected refusal (exit 2), no side effects:
bash tools/testrig/plan.sh --execute
bash tools/testrig/plan.sh --root /data/Games/steamapps/compatdata/881100
```

Observed CLI results: default, help, launch dry-run, preparation/symlink dry-run and network-plan mode returned 0.
`--execute`, a destination in the real compatdata, missing `--phase` value and an invalid phase each returned 2 as intended.
The `.cache/testrig/` destination was absent before and after all nine invocations. This verifies the planner's
observed read-only/refusal behaviour, not game startup, save isolation or G2 networking.

**Procedure deviation:** an unnecessary `wine --version` metadata query was inadvertently invoked and printed `wine-11.20`. This violated the blanket no-Wine-invocation instruction even though no game, prefix initialization or wineserver was started by that command. It must not be repeated; the intended checks were static metadata only. The game itself was never launched.

## What remains to verify in-game — Что остаётся проверить в игре

| Input / experiment after D17 permission | Expected observable | Pass / fail |
|---|---|---|
| One new Steam-off test prefix + private C: game copy; first launch with sanitized env | Game opens, stays in copied prefix/cwd; no Steam reroute or real Cloud changes | Pass only with actual paths/logs and unchanged real-state baseline; otherwise stop and record the failure. |
| First-prefix initialization with preseeded mod/config files | Proton creates its runtime files without discarding our seed; Noita accepts settings | Fail on lost seeds, another save root, warnings/errors preventing startup or stale build. |
| In that copy, allow Unsafe mods and restart with our probe active | Intended mod loaded; os/io/FFI and per-instance endpoint genuinely usable | Fail if blocked or an inherited real mod list/path is used. |
| Start i2 concurrently; write distinguishable harmless progress/test markers; save+quit+restart both | Different prefixes, user-data roots, mod lists/logs; markers never cross | Fail on shared/missing progress, cross-writes or single-instance rejection. |
| Keep i1 unfocused while observing both frame counters for 30–60 seconds | Both continue advancing; no input replay to the wrong window | Fail on hidden/minimized/unfocused pause; measure before choosing gamescope as an alternative. |
| Two games → two agents → one server; exchange tagged `(instance, sequence, nonce)` payloads | Simultaneous independent round trips through Win32↔Linux localhost | G2 passes only with both real paths and correctly matched responses; two windows or native-only traffic does not count. |
| Exit/stop only i1's test scope while i2 and native peers run | i2 continues; logs are retained; no real prefix/Steam state altered | Fail on group-wide kill, shared wineserver effects or missing evidence. |
| Optional client-connected or nested/headless gamescope variant, separately approved | No Steam reroute/Cloud interference; equivalent save isolation and sustained frames | Independent variant result; never infer success from installed tools or the Steam-off result. |
