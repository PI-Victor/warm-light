#!/usr/bin/env bash
set -euo pipefail

# Capture and compare read-only VCP snapshots for Samsung monitor feature discovery.
#
# Typical flow:
#   ./scripts/diff-vcp-snapshot.sh capture before --display=1
#   # toggle one monitor OSD feature manually
#   ./scripts/diff-vcp-snapshot.sh capture after --display=1
#   ./scripts/diff-vcp-snapshot.sh compare .ddc-probes/before-...txt .ddc-probes/after-...txt
#
# This script does not write VCP values.

SNAPSHOT_DIR=".ddc-probes"
COMMON_ARGS=()
TARGET_DISPLAY=""

declare -a SNAPSHOT_CODES=(
  "10:Brightness"
  "12:Contrast"
  "14:Color preset"
  "16:Video gain red"
  "18:Video gain green"
  "1A:Video gain blue"
  "6C:Black level red"
  "6E:Black level green"
  "70:Black level blue"
  "60:Input source"
  "62:Speaker volume"
  "8D:Audio mute"
  "B0:Settings"
  "C6:Application enable key"
  "C8:Display controller type"
  "C9:Display firmware level"
  "CA:OSD"
  "CC:OSD language"
  "D6:Power mode"
  "DC:Display mode"
  "E0:Samsung candidate"
  "E1:Samsung candidate"
  "E2:Samsung candidate"
  "ED:Samsung candidate"
  "EE:Samsung candidate"
  "EF:Samsung candidate"
  "FF:Manufacturer specific"
)

usage() {
  cat <<'EOF'
Usage:
  ./scripts/diff-vcp-snapshot.sh capture <label> [--display=N] [ddcutil args...]
  ./scripts/diff-vcp-snapshot.sh compare <before-file> <after-file>
  ./scripts/diff-vcp-snapshot.sh list

Examples:
  ./scripts/diff-vcp-snapshot.sh capture baseline --display=1
  ./scripts/diff-vcp-snapshot.sh capture eye-saver-on --display=1
  ./scripts/diff-vcp-snapshot.sh compare .ddc-probes/baseline-display1-*.txt .ddc-probes/eye-saver-on-display1-*.txt

Notes:
  - capture is read-only
  - compare shows a unified diff between two snapshot files
EOF
}

headline() {
  printf '\n== %s ==\n' "$1"
}

print_command() {
  printf '$'
  printf ' %q' "$@"
  printf '\n'
}

timestamp() {
  date +%Y%m%d-%H%M%S
}

sanitize_label() {
  printf '%s' "$1" | tr -cs '[:alnum:]._-' '-'
}

run_getvcp() {
  local display=$1
  local code=$2

  ddcutil "${COMMON_ARGS[@]}" --display="$display" --brief getvcp "$code" 2>&1 || true
}

capture_display_snapshot() {
  local display=$1
  local label=$2
  local stamp=$3
  local outfile="${SNAPSHOT_DIR}/${label}-display${display}-${stamp}.txt"

  {
    printf 'snapshot_label=%s\n' "$label"
    printf 'snapshot_timestamp=%s\n' "$(date --iso-8601=seconds)"
    printf 'display=%s\n' "$display"
    printf 'ddcutil=%s\n' "$(command -v ddcutil)"
    printf 'ddcutil_version=%s\n' "$(ddcutil --version | head -n 1)"
    printf '\n'
    printf '[capabilities]\n'
    ddcutil "${COMMON_ARGS[@]}" --display="$display" capabilities 2>&1 || true
    printf '\n'
    printf '[vcp]\n'

    local entry code desc output
    for entry in "${SNAPSHOT_CODES[@]}"; do
      code=${entry%%:*}
      desc=${entry#*:}
      output=$(run_getvcp "$display" "$code")
      printf '%s\t%s\t%s\n' "$code" "$desc" "$output"
    done
  } >"$outfile"

  printf '%s\n' "$outfile"
}

list_snapshots() {
  mkdir -p "$SNAPSHOT_DIR"
  if ! compgen -G "${SNAPSHOT_DIR}/*.txt" >/dev/null; then
    printf 'No snapshots found in %s\n' "$SNAPSHOT_DIR"
    return 0
  fi

  ls -1t "${SNAPSHOT_DIR}"/*.txt
}

if ! command -v ddcutil >/dev/null 2>&1; then
  printf 'ddcutil is not installed or not on PATH.\n' >&2
  exit 1
fi

if [[ $# -lt 1 ]]; then
  usage
  exit 1
fi

COMMAND=$1
shift

case "$COMMAND" in
  capture)
    if [[ $# -lt 1 ]]; then
      usage
      exit 1
    fi
    LABEL=$(sanitize_label "$1")
    shift

    while [[ $# -gt 0 ]]; do
      case "$1" in
        --display=*)
          TARGET_DISPLAY=${1#*=}
          shift
          ;;
        --display)
          TARGET_DISPLAY=${2:-}
          shift 2
          ;;
        *)
          COMMON_ARGS+=("$1")
          shift
          ;;
      esac
    done

    mkdir -p "$SNAPSHOT_DIR"
    stamp=$(timestamp)

    headline "Environment"
    printf 'label: %s\n' "$LABEL"
    printf 'cwd: %s\n' "$(pwd)"
    printf 'date: %s\n' "$(date --iso-8601=seconds)"
    printf 'target display: %s\n' "${TARGET_DISPLAY:-all detected displays}"
    printf 'snapshot dir: %s\n' "$SNAPSHOT_DIR"
    print_command ddcutil "${COMMON_ARGS[@]}" detect
    ddcutil "${COMMON_ARGS[@]}" detect 2>&1 || true

    mapfile -t displays < <(
      if [[ -n "$TARGET_DISPLAY" ]]; then
        printf '%s\n' "$TARGET_DISPLAY"
      else
        ddcutil "${COMMON_ARGS[@]}" detect 2>/dev/null |
          awk '/^Display [0-9]+$/ { print $2 }'
      fi
    )

    if [[ ${#displays[@]} -eq 0 ]]; then
      headline "Capture summary"
      printf 'No displays available to snapshot.\n'
      exit 0
    fi

    headline "Capture summary"
    for display in "${displays[@]}"; do
      outfile=$(capture_display_snapshot "$display" "$LABEL" "$stamp")
      printf 'saved: %s\n' "$outfile"
    done
    ;;

  compare)
    if [[ $# -ne 2 ]]; then
      usage
      exit 1
    fi
    BEFORE=$1
    AFTER=$2

    if [[ ! -f "$BEFORE" ]]; then
      printf 'Missing before snapshot: %s\n' "$BEFORE" >&2
      exit 1
    fi

    if [[ ! -f "$AFTER" ]]; then
      printf 'Missing after snapshot: %s\n' "$AFTER" >&2
      exit 1
    fi

    headline "Compare"
    printf 'before: %s\n' "$BEFORE"
    printf 'after:  %s\n' "$AFTER"
    printf '\n'
    diff -u "$BEFORE" "$AFTER" || true
    ;;

  list)
    list_snapshots
    ;;

  *)
    usage
    exit 1
    ;;
esac
