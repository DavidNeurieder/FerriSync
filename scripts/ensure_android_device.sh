#!/usr/bin/env bash
# Ensure an Android device is online for the Android test targets.
#
# If no device is online, start the configured AVD with a visible emulator
# window (i.e. not -no-window), wait for it to boot, and leave it running for
# the remaining tests. The chosen serial is printed on stdout and also written
# to ANDROID_DEVICE_SERIAL_FILE.
set -euo pipefail

PROJECT_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
ANDROID_SDK_ROOT="${ANDROID_HOME:-${ANDROID_SDK_ROOT:-/home/mr/Android/Sdk}}"
EMULATOR_BIN="${EMULATOR_BIN:-${ANDROID_SDK_ROOT}/emulator/emulator}"
AVD_NAME="${AVD_NAME:-test_phone}"
EMU_LOG="${EMU_LOG:-/tmp/ferrisync-emulator.log}"
SERIAL_FILE="${ANDROID_DEVICE_SERIAL_FILE:-/tmp/ferrisync-android-device-serial}"
BOOT_TIMEOUT="${EMU_BOOT_TIMEOUT:-240}"
EMU_HEADLESS="${EMU_HEADLESS:-0}"

die() {
  echo "ERROR: $*" >&2
  exit 1
}

command -v adb >/dev/null 2>&1 || die "adb not found in PATH."

online_devices() {
  adb devices 2>/dev/null | awk 'NR > 1 && $2 == "device" && $1 != "" {print $1}'
}

pick_serial() {
  local online emu
  online="$(online_devices)"

  emu="$(grep -E '^emulator-' <<<"$online" | head -1 || true)"
  if [ -n "$emu" ]; then
    echo "$emu"
  else
    echo "$online" | head -1
  fi
}

serial=""
if [ -n "${DEVICE_SERIAL:-}" ]; then
  serial="$DEVICE_SERIAL"
  online_devices | grep -Fxq "$serial" || die "DEVICE_SERIAL='$serial' is not online."
else
  serial="$(pick_serial)"
fi

if [ -z "$serial" ]; then
  [ -x "$EMULATOR_BIN" ] || die "emulator binary not found: $EMULATOR_BIN (set ANDROID_HOME or EMULATOR_BIN)"

  if ! "$EMULATOR_BIN" -list-avds 2>/dev/null | grep -Fxq "$AVD_NAME"; then
    echo "ERROR: AVD '$AVD_NAME' not found. Available AVDs:" >&2
    "$EMULATOR_BIN" -list-avds >&2 || true
    exit 1
  fi

  local_args=(-avd "$AVD_NAME" -no-snapshot-load)
  # Visible window by default. Set EMU_HEADLESS=1 for CI/headless runs.
  [ "$EMU_HEADLESS" = "1" ] && local_args+=(-no-window -no-audio -gpu swiftshader_indirect)

  echo "No Android device online. Starting '$AVD_NAME' with a visible emulator window..." >&2
  echo "Emulator log: $EMU_LOG" >&2
  # setsid keeps the emulator alive even if the invoking shell/test runner
  # is terminated. It still opens a visible window (DISPLAY is inherited).
  setsid env ANDROID_HOME="$ANDROID_SDK_ROOT" ANDROID_SDK_ROOT="$ANDROID_SDK_ROOT" \
    "$EMULATOR_BIN" "${local_args[@]}" </dev/null >"$EMU_LOG" 2>&1 &

  deadline=$((SECONDS + BOOT_TIMEOUT))
  while :; do
    serial="$(pick_serial)"
    if [ -n "$serial" ]; then
      boot_completed="$(adb -s "$serial" shell getprop sys.boot_completed 2>/dev/null | tr -d '\r')"
      [ "$boot_completed" = "1" ] && break
    fi
    if [ "$SECONDS" -ge "$deadline" ]; then
      echo "ERROR: emulator did not finish booting within ${BOOT_TIMEOUT}s." >&2
      tail -40 "$EMU_LOG" >&2 2>/dev/null || true
      exit 1
    fi
    sleep 2
  done
fi

echo "$serial" >"$SERIAL_FILE"
echo "Android device ready: $serial" >&2
printf '%s\n' "$serial"
