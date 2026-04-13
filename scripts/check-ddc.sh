#!/usr/bin/env bash
set -euo pipefail

COMMON_ARGS=()

if [[ $# -gt 0 ]]; then
  COMMON_ARGS+=("$@")
fi

headline() {
  printf '\n== %s ==\n' "$1"
}

run_capture() {
  local description=$1
  shift

  headline "$description"
  printf '$'
  printf ' %q' "$@"
  printf '\n'

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

headline "Environment"
printf 'script: %s\n' "$0"
printf 'cwd: %s\n' "$(pwd)"
printf 'date: %s\n' "$(date --iso-8601=seconds)"
printf 'user: %s\n' "${USER:-unknown}"
printf 'ddcutil: %s\n' "$(command -v ddcutil)"
printf 'version: %s\n' "$(ddcutil --version | head -n 1)"

headline "I2C Devices"
if compgen -G '/dev/i2c-*' >/dev/null; then
  ls -l /dev/i2c-*
else
  printf 'No /dev/i2c-* devices found.\n'
fi

run_capture "ddcutil detect" ddcutil "${COMMON_ARGS[@]}" detect

mapfile -t displays < <(
  ddcutil "${COMMON_ARGS[@]}" detect 2>/dev/null |
    awk '/^Display [0-9]+$/ { print $2 }'
)

if [[ ${#displays[@]} -eq 0 ]]; then
  headline "Per-display checks"
  printf 'No displays parsed from ddcutil detect output.\n'
  exit 0
fi

for display in "${displays[@]}"; do
  run_capture \
    "Display ${display}: brightness" \
    ddcutil "${COMMON_ARGS[@]}" --display="$display" --brief getvcp 10

  run_capture \
    "Display ${display}: contrast" \
    ddcutil "${COMMON_ARGS[@]}" --display="$display" --brief getvcp 12

  run_capture \
    "Display ${display}: input source" \
    ddcutil "${COMMON_ARGS[@]}" --display="$display" --brief getvcp 60

  run_capture \
    "Display ${display}: capabilities" \
    ddcutil "${COMMON_ARGS[@]}" --display="$display" capabilities
done
