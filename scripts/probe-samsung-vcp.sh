#!/usr/bin/env bash
set -euo pipefail

# Read-only probe for Samsung-specific DDC/CI controls.
# This does not write any VCP values. It only calls ddcutil detect/getvcp/capabilities.
#
# Candidate codes come from:
# - the current monitor capability dump in output.log
# - older Samsung reverse-engineering notes from ddcci-tool
#
# Usage:
#   ./scripts/probe-samsung-vcp.sh
#   ./scripts/probe-samsung-vcp.sh --display=1
#   ./scripts/probe-samsung-vcp.sh --sleep-multiplier=2

COMMON_ARGS=()
TARGET_DISPLAY=""

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

headline() {
  printf '\n== %s ==\n' "$1"
}

print_command() {
  printf '$'
  printf ' %q' "$@"
  printf '\n'
}

run_capture() {
  local description=$1
  shift

  headline "$description"
  print_command "$@"

  local output
  if output=$("$@" 2>&1); then
    printf '%s\n' "$output"
    return 0
  fi

  local status=$?
  printf '%s\n' "$output"
  printf 'exit status: %s\n' "$status"
  return 0
}

if ! command -v ddcutil >/dev/null 2>&1; then
  printf 'ddcutil is not installed or not on PATH.\n' >&2
  exit 1
fi

# Known Samsung-relevant candidates. We split these into:
# - standard controls worth re-reading verbosely
# - vendor/legacy Samsung candidates that may or may not respond
declare -a STANDARD_CODES=(
  "10:Brightness"
  "12:Contrast"
  "14:Color preset"
  "16:Video gain red"
  "18:Video gain green"
  "1A:Video gain blue"
  "60:Input source"
  "62:Speaker volume"
  "8D:Audio mute"
  "CA:OSD"
  "CC:OSD language"
  "D6:Power mode"
  "DC:Display mode"
  "FF:Manufacturer specific"
)

declare -a SAMSUNG_VENDOR_CODES=(
  "B0:Settings / preset bank"
  "C6:Application enable key"
  "C8:Display controller type"
  "C9:Display firmware level"
  "E0:Samsung color preset candidate"
  "E1:Samsung power candidate"
  "E2:Samsung vendor candidate"
  "ED:Samsung black level red candidate"
  "EE:Samsung black level green candidate"
  "EF:Samsung black level blue candidate"
)

headline "Environment"
printf 'script: %s\n' "$0"
printf 'cwd: %s\n' "$(pwd)"
printf 'date: %s\n' "$(date --iso-8601=seconds)"
printf 'ddcutil: %s\n' "$(command -v ddcutil)"
printf 'version: %s\n' "$(ddcutil --version | head -n 1)"
printf 'target display: %s\n' "${TARGET_DISPLAY:-all detected displays}"

run_capture "ddcutil detect" ddcutil "${COMMON_ARGS[@]}" detect

mapfile -t displays < <(
  if [[ -n "$TARGET_DISPLAY" ]]; then
    printf '%s\n' "$TARGET_DISPLAY"
  else
    ddcutil "${COMMON_ARGS[@]}" detect 2>/dev/null |
      awk '/^Display [0-9]+$/ { print $2 }'
  fi
)

if [[ ${#displays[@]} -eq 0 ]]; then
  headline "Probe summary"
  printf 'No displays available to probe.\n'
  exit 0
fi

for display in "${displays[@]}"; do
  run_capture \
    "Display ${display}: capabilities" \
    ddcutil "${COMMON_ARGS[@]}" --display="$display" capabilities

  headline "Display ${display}: standard Samsung-relevant codes"
  for entry in "${STANDARD_CODES[@]}"; do
    code=${entry%%:*}
    label=${entry#*:}
    run_capture \
      "Display ${display}: ${label} (${code})" \
      ddcutil "${COMMON_ARGS[@]}" --display="$display" --brief getvcp "$code"
  done

  headline "Display ${display}: Samsung vendor candidates"
  for entry in "${SAMSUNG_VENDOR_CODES[@]}"; do
    code=${entry%%:*}
    label=${entry#*:}
    run_capture \
      "Display ${display}: ${label} (${code})" \
      ddcutil "${COMMON_ARGS[@]}" --display="$display" --brief getvcp "$code"
  done
done

headline "Next step"
printf '%s\n' \
  "Look for vendor codes that return stable values instead of unsupported/invalid." \
  "If E0/E1/E2/ED/EE/EF/FF respond, those are the best Samsung-specific candidates to map into the app."
