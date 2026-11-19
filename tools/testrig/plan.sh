#!/usr/bin/env bash
# NOT YET RUN: launch/preparation commands below have never been executed.
# Read-only planner, not a launcher. --dry-run is the only supported mode.
# There is deliberately no execute branch until live isolation is established.
set -euo pipefail

fail() { printf 'error: %s\n' "$*" >&2; exit 2; }
usage() {
    cat <<'HELP'
Usage: bash tools/testrig/plan.sh [--dry-run] [options]
  --phase all|prepare|launch|network|cleanup  Select the printed plan (default: all)
  --root PATH             Destination, restricted to this repo's .cache/testrig/
  --mod-dir PATH          OUR mod source inside this repo (default: mod/)
  --mod-id ID             Local mod folder ID (default: nmp)
  --mod-mode copy|symlink Install into each test game only (default: copy)
  --steam-mode off|client Candidate launch mode (default: off; NOT yet verified)
  --game-source PATH     Read-only game source (default: local Steam installation)
  --proton-dir PATH      Read-only Proton source (default: installed GE-Proton11-12)
  --runtime-dir PATH     Read-only runtime (default: SteamLinuxRuntime_4)
  --steam-root PATH      Read-only Steam client files
  --help                 Print this help

All output is an UNEXECUTED HUMAN-REVIEW PLAN. Never pipe it into a shell.
No mkdir, copy, log file, Steam/Proton/Wine/game invocation, or process termination
is performed. --execute is rejected. Printed launch commands are not permission:
D17 requires an explicit request from the user in the current live-test session.
HELP
}

SCRIPT_DIR=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)
REPO=$(cd -- "$SCRIPT_DIR/../.." && pwd -P)
ROOT="$REPO/.cache/testrig"
MOD_DIR="$REPO/mod"
MOD_ID=nmp
MOD_MODE=copy
STEAM_MODE=off
PHASE=all
GAME_SOURCE=/data/Games/steamapps/common/Noita
PROTON_DIR="$HOME/.local/share/Steam/compatibilitytools.d/GE-Proton11-12-x86_64"
RUNTIME_DIR=/data/Games/steamapps/common/SteamLinuxRuntime_4
STEAM_ROOT="$HOME/.local/share/Steam"
EXPECTED_HASH=9dbd52ced019a643169a2db02f46c77f8766c6e5

while (($#)); do
    case "$1" in
        --dry-run) shift ;;
        --execute) fail 'No execute mode exists: Steam Cloud and live isolation remain unverified.' ;;
        --help|-h) usage; exit 0 ;;
        --phase|--root|--mod-dir|--mod-id|--mod-mode|--steam-mode|--game-source|--proton-dir|--runtime-dir|--steam-root)
            (($# >= 2)) || fail "Missing value for $1"
            case "$1" in
                --phase) PHASE=$2 ;;
                --root) ROOT=$2 ;;
                --mod-dir) MOD_DIR=$2 ;;
                --mod-id) MOD_ID=$2 ;;
                --mod-mode) MOD_MODE=$2 ;;
                --steam-mode) STEAM_MODE=$2 ;;
                --game-source) GAME_SOURCE=$2 ;;
                --proton-dir) PROTON_DIR=$2 ;;
                --runtime-dir) RUNTIME_DIR=$2 ;;
                --steam-root) STEAM_ROOT=$2 ;;
            esac
            shift 2 ;;
        *) fail "Unknown argument: $1" ;;
    esac
done

case "$PHASE" in all|prepare|launch|network|cleanup) ;; *) fail 'Invalid phase' ;; esac
case "$MOD_MODE" in copy|symlink) ;; *) fail 'Invalid mod mode' ;; esac
case "$STEAM_MODE" in off|client) ;; *) fail 'Invalid Steam mode' ;; esac
[[ "$MOD_ID" =~ ^[A-Za-z0-9][A-Za-z0-9_.-]*$ ]] || fail 'Invalid mod ID'
# realpath only reads path metadata; -m permits not-yet-created destinations.
ROOT=$(realpath -m -- "$ROOT")
MOD_DIR=$(realpath -m -- "$MOD_DIR")
GAME_SOURCE=$(realpath -m -- "$GAME_SOURCE")
PROTON_DIR=$(realpath -m -- "$PROTON_DIR")
RUNTIME_DIR=$(realpath -m -- "$RUNTIME_DIR")
STEAM_ROOT=$(realpath -m -- "$STEAM_ROOT")
[[ "$ROOT" == "$REPO/.cache/testrig" || "$ROOT" == "$REPO/.cache/testrig/"* ]] \
    || fail 'Destination must resolve inside this repo .cache/testrig (no external symlink targets)'
[[ "$MOD_DIR" == "$REPO/"* && "$MOD_DIR" != "$REPO/.cache" && "$MOD_DIR" != "$REPO/.cache/"* ]] \
    || fail 'Mod source must resolve inside this repo, outside cached third-party/game files'
for source_path in "$MOD_DIR" "$GAME_SOURCE" "$PROTON_DIR" "$RUNTIME_DIR" "$STEAM_ROOT"; do
    [[ "$source_path" != "$ROOT" && "$source_path" != "$ROOT/"* && "$ROOT" != "$source_path/"* ]] \
        || fail "Source and destination overlap: $source_path"
done

section() { printf '\n## %s\n' "$*"; }
show() { printf '[UNEXECUTED]'; printf ' %q' "$@"; printf '\n'; }
selected() { [[ "$PHASE" == all || "$PHASE" == "$1" ]]; }
instance_paths() {
    INSTANCE="$ROOT/instances/i$1"
    COMPAT="$INSTANCE/compat"
    PREFIX="$COMPAT/pfx"
    GAME="$PREFIX/drive_c/nmp/Noita"
    USERDATA="$PREFIX/drive_c/users/steamuser/AppData/LocalLow/Nolla_Games_Noita"
    LOGS="$INSTANCE/logs"
}

printf 'READ-ONLY G2 PLAN; NOT YET RUN; NO EXECUTE BRANCH\n'
printf 'repo=%s\nroot=%s\nmod=%s (%s)\nSteam mode=%s\n' "$REPO" "$ROOT" "$MOD_DIR" "$MOD_MODE" "$STEAM_MODE"
printf 'Review prerequisites: pinned hash; mod source exists and contains only our code/approved dependencies; ports free; no real Noita; current-session D17 permission.\n'
printf 'No path in the real compatdata/881100 is a source or destination of this plan.\n'
printf 'The shared game installation is a read-only SOURCE of whitelisted vanilla files.\n'
printf 'Prefix separation is not a security sandbox; Wine Z: and an unsafe mod can access host files.\n'
printf 'All planned copies require fresh, nonexistent instance directories; never merge into a used run.\n'

if selected prepare; then
    section 'Prepare fresh test prefixes; do NOT copy the real prefix/save00'
    show df -h -- "$REPO/.cache"
    show cat -- "$GAME_SOURCE/_version_hash.txt"
    printf '[REQUIRE] hash equals %s; retain at least 6 GiB free and stop on ENOSPC.\n' "$EXPECTED_HASH"
    for id in 1 2; do
        instance_paths "$id"
        printf '\nInstance i%s: Proton will initialize the new prefix on the first approved launch.\n' "$id"
        show test ! -e "$INSTANCE"
        show mkdir -p -- "$GAME/mods" "$USERDATA/save00" "$USERDATA/save_shared" "$LOGS" \
            "$INSTANCE/xdg/cache" "$INSTANCE/xdg/config" "$INSTANCE/xdg/data" "$INSTANCE/xdg/state" "$ROOT/artifacts"
        # Deliberately exclude mods/, proxy binaries, libsteam_api.so, logs and backups.
        for asset in noita.exe SDL2.dll fmod.dll fmodstudio.dll lua51.dll msvcp120.dll msvcr120.dll \
            steam_api.dll config.xml _version_hash.txt _branch.txt _release_notes.txt README.txt screenshot_paths.txt data licenses; do
            show cp -a --reflink=always -- "$GAME_SOURCE/$asset" "$GAME/$asset"
        done
        if [[ "$STEAM_MODE" == client ]]; then
            show cp -a --reflink=always -- "$GAME_SOURCE/steam_appid.txt" "$GAME/steam_appid.txt"
        else
            printf '[REQUIRE] No steam_appid.txt in the test game dir; no inherited SteamAppId/SteamGameId.\n'
        fi
        if [[ "$MOD_MODE" == copy ]]; then
            show mkdir -p -- "$GAME/mods/$MOD_ID"
            show cp -a -- "$MOD_DIR/." "$GAME/mods/$MOD_ID/"
        else
            show ln -s -- "$MOD_DIR" "$GAME/mods/$MOD_ID"
            printf '[REQUIRE] Make the resolved mod source visible READ-ONLY inside the runtime; do not use symlink mode until checked.\n'
        fi
        printf '[PLANNED FILE, not written] %s/save00/mod_config.xml\n' "$USERDATA"
        printf '<Mods><Mod enabled="1" name="%s" settings_fold_open="0" workshop_item_id="0"></Mod></Mods>\n' "$MOD_ID"
        printf '[PLANNED FILE, not written] %s/save_shared/config.xml\n' "$USERDATA"
        cat <<CONFIG
<Config config_format_version="14" is_default_config="0" fullscreen="0"
 window_w="960" window_h="540" backbuffer_width="960" backbuffer_height="540"
 internal_size_w="1280" internal_size_h="720" framerate="60" vsync="2"
 application_pause_when_unfocused="0" audio_music_volume="0" audio_effects_volume="0"
 sounds="1" replay_recorder_enabled="0" online_features="0" check_for_updates="0"
 mods_sandbox_enabled="1" mods_disclaimer_accepted="0" mods_sandbox_warning_done="0"
 last_started_game_version_hash="$EXPECTED_HASH"></Config>
CONFIG
        printf '[MANUAL LIVE STEP] In this COPIED game only: review disclaimer, allow Unsafe mods, enable %s, restart with mods active. Do not pre-accept warnings by script.\n' "$MOD_ID"
    done
fi

if selected launch; then
    section 'Candidate launch; explicit current-session permission is mandatory'
    printf '[UNKNOWN] Steam-off startup, prefix first-initialization, runtime mounts, window focus and Cloud isolation must be proved by a supervised single-instance run first.\n'
    printf '[MANUAL] Open two dedicated terminals; each command remains attached to its own scope. Do not use Steam Play/noita-switcher/steam://run.\n'
    for id in 1 2; do
        instance_paths "$id"
        env_args=(env -i "HOME=$HOME" "USER=${USER:-mora}" "LOGNAME=${LOGNAME:-mora}"
            'PATH=/usr/local/bin:/usr/bin:/bin' 'LANG=C.UTF-8' 'PYTHONDONTWRITEBYTECODE=1'
            "XDG_CACHE_HOME=$INSTANCE/xdg/cache" "XDG_CONFIG_HOME=$INSTANCE/xdg/config"
            "XDG_DATA_HOME=$INSTANCE/xdg/data" "XDG_STATE_HOME=$INSTANCE/xdg/state"
            "STEAM_COMPAT_CLIENT_INSTALL_PATH=$STEAM_ROOT" "STEAM_COMPAT_DATA_PATH=$COMPAT"
            "STEAM_COMPAT_INSTALL_PATH=$GAME" "WINEPREFIX=$PREFIX" "PRESSURE_VESSEL_FILESYSTEMS_RW=$ROOT"
            "NMP_INSTANCE_ID=i$id" "NMP_AGENT_ADDR=127.0.0.1:$((24710 + id))" 'NMP_TESTRIG=1')
        for key in DISPLAY WAYLAND_DISPLAY XDG_RUNTIME_DIR DBUS_SESSION_BUS_ADDRESS XAUTHORITY; do
            if [[ -v "$key" ]]; then env_args+=("$key=${!key}"); fi
        done
        if [[ "$STEAM_MODE" == client ]]; then
            env_args+=('SteamAppId=881100' 'SteamGameId=881100' 'PROTON_LOG=1' "PROTON_LOG_DIR=$LOGS")
        else
            env_args+=('PROTON_LOG=0')
        fi
        printf '\n[REQUIRE] Compare source and copied version hashes before each launch; preserve the original launch log before reusing i%s.\n' "$id"
        # Positional parameters belong to the printed child shell, not this planner.
        # shellcheck disable=SC2016
        show bash -c 'cd -- "$1"; log=$2; shift 2; exec "$@" >"$log" 2>&1' bash "$GAME" "$LOGS/launcher.log" \
            systemd-run --user --scope --collect --unit="nmp-testrig-i$id" -- \
            "${env_args[@]}" "$RUNTIME_DIR/_v2-entry-point" --verb=run -- "$PROTON_DIR/proton" run \
            "$GAME/noita.exe" -windowed -no_logo_splashes
        printf '[OPTIONAL AFTER VALIDATION] -gamemode 0 starts the default game mode; add it only to a fresh disposable run. No -clean_* or -debug_lua flags.\n'
    done
fi

if selected network; then
    section 'G2 network contract (not an implemented server/agent API)'
    printf 'server: 127.0.0.1:24700; agent i1: 127.0.0.1:24711; agent i2: 127.0.0.1:24712\n'
    printf 'Mod reads NMP_INSTANCE_ID / NMP_AGENT_ADDR only after unsafe privileges are granted.\n'
    printf 'Start one real native Linux server and two real native agents in nmp-testrig-server / nmp-testrig-agent-iN user scopes, with explicit ports and independent logs.\n'
    printf 'Their CLI is not yet implemented; no invented binary invocation is printed. Environment/port names here are the proposed integration contract.\n'
    printf 'Gate passes only with both Win32 games concurrently sending tagged sequence/nonce messages through their own agents to that one server and receiving verified responses.\n'
    show ss -lntu
fi

if selected cleanup; then
    section 'Cleanup plan: close games normally first; collect before purge'
    for id in 1 2; do
        instance_paths "$id"
        show systemctl --user status "nmp-testrig-i$id.scope"
        printf '[FAULT-TEST ONLY] Stop this exact scope, never pkill wine/noita or a global wineserver -k:\n'
        show systemctl --user stop "nmp-testrig-i$id.scope"
        show mkdir -p -- "$LOGS/game"
        show rsync -a '--include=*/' '--include=*.log' '--include=logger*.txt' '--exclude=*' -- "$GAME/" "$LOGS/game/"
        show cp -a -- "$GAME/_version_hash.txt" "$LOGS/_version_hash.txt"
        printf '[MANUAL] Retain mod/agent/server logs, exact commands, versions, ports, seed, frame/message counts and observed save paths in a unique artifact directory.\n'
        printf '[DESTRUCTIVE OPTIONAL PLAN] Only after no processes use this prefix and artifacts are retained:\n'
        show rm -rf --one-file-system -- "$INSTANCE"
    done
    printf '[MANUAL] Stop the exact native server/agent scopes too. Never invoke the real switcher or change Steam settings for cleanup.\n'
fi

printf '\nEnd of read-only plan. No listed command or file creation was performed.\n'
