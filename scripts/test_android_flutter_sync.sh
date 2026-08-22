#!/usr/bin/env bash
# Android Flutter integration tests
# Sets up a serve process on the host, then runs all Flutter integration
# suites (UI, FRB smoke, sync) on the connected Android device or emulator.
set -euo pipefail

PROJECT_ROOT="/home/mr/Projects/FerriSync"
ANDROID_TARGETS=("x86_64-linux-android" "aarch64-linux-android")
AVD_NAME="${AVD_NAME:-test_phone}"
HOST_BINARY="${PROJECT_ROOT}/target/debug/ferrisync-cli"
SERVE_PORT=9847
SERVE_DIR="/tmp/test_flutter_sync_serve"
DATA_DIR="/tmp/test_flutter_sync_data"

# Integration suites to run, in order (files in ferrisync-flutter/integration_test/).
# UI-driven suites go last: they pair the device for real, which would
# otherwise change the empty-state assertions of app_test.dart.
INTEGRATION_SUITES=("app_test.dart" "frb_smoke_test.dart" "sync_test.dart" "sync_incremental_test.dart" "pairing_ui_test.dart" "folders_flow_ui_test.dart")

PASS="PASS"
FAIL="FAIL"

# Map Rust target → Android ABI
target_to_abi() {
  case "$1" in
    aarch64-linux-android) echo "arm64-v8a" ;;
    x86_64-linux-android)  echo "x86_64" ;;
    *) echo "unknown"; return 1 ;;
  esac
}

get_device_serial() {
  # Explicit override wins (e.g. DEVICE_SERIAL=emulator-5556 ./script.sh)
  if [ -n "${DEVICE_SERIAL:-}" ]; then
    echo "${DEVICE_SERIAL}"
    return
  fi
  # Only devices in 'device' state — never pick offline/unauthorized ones,
  # and prefer an emulator over an attached physical phone.
  local online emu
  online=$(adb devices 2>/dev/null | awk 'NR>1 && $2 == "device" {print $1}')
  emu=$(grep -E '^emulator-' <<<"$online" | head -1)
  if [ -n "$emu" ]; then
    echo "$emu"
  else
    echo "$online" | head -1
  fi
}

cleanup() {
  echo ""
  echo "=== Cleaning up ==="
  kill ${SERVE_PID:-} 2>/dev/null || true
  wait ${SERVE_PID:-} 2>/dev/null || true
  adb reverse --remove tcp:${SERVE_PORT} 2>/dev/null || true
  rm -rf "${SERVE_DIR}" "${DATA_DIR}" 2>/dev/null || true
  echo "Cleanup done."
}

trap cleanup EXIT

check_adb_device() {
  local serial
  serial=$(get_device_serial)
  if [ -n "$serial" ]; then
    echo "Device connected: $serial"
    return
  fi
  if [ -n "${DEVICE_SERIAL:-}" ]; then
    echo "ERROR: DEVICE_SERIAL='${DEVICE_SERIAL}' is not connected." >&2
    return 1
  fi
  echo "No device connected. Starting emulator (${AVD_NAME})..."
  ANDROID_SDK_ROOT=/home/mr/Android/Sdk \
    nohup /home/mr/Android/Sdk/emulator/emulator \
      -avd "${AVD_NAME}" \
      -no-snapshot-load \
      > /tmp/emu.log 2>&1 &
  echo "Waiting for emulator to boot..."
  wait_for_boot
  echo "Emulator ready."
}

# adb wait-for-device returns before Android is actually up; also require
# sys.boot_completed=1. Cold-boot only (-no-snapshot-load) so a wedged
# snapshot can never come back.
wait_for_boot() {
  adb wait-for-device
  local serial
  serial=$(get_device_serial)
  for _ in $(seq 1 60); do
    if [ "$(adb -s "$serial" shell getprop sys.boot_completed 2>/dev/null | tr -d '\r')" = "1" ]; then
      return 0
    fi
    sleep 2
  done
  echo "ERROR: emulator did not finish booting within timeout." >&2
  return 1
}

# Ensure the device is still online and the port forward is intact.
# ADB drops reverse rules when a device disconnects/reconnects, so this
# must be re-run before every suite.
ensure_device_ready() {
  local serial="$1"
  for _ in $(seq 1 30); do
    [ "$(adb -s "$serial" get-state 2>/dev/null)" = "device" ] && break
    echo "  Device $serial offline, waiting..."
    sleep 2
  done
  if [ "$(adb -s "$serial" get-state 2>/dev/null)" != "device" ]; then
    echo "ERROR: device $serial is offline; aborting remaining suites." >&2
    return 1
  fi
  adb reverse tcp:${SERVE_PORT} tcp:${SERVE_PORT}
}

build_binaries() {
  echo "Building host binary..."
  cd "${PROJECT_ROOT}" && cargo build -p ferrisync-cli
}

build_flutter_apk() {
  echo "Building universal Flutter APK..."
  local flutter_dir="${PROJECT_ROOT}/ferrisync-flutter"
  local jnilib_base="${flutter_dir}/android/app/src/main/jniLibs"

  for target in "${ANDROID_TARGETS[@]}"; do
    local abi
    abi=$(target_to_abi "$target")
    echo "  Building .so for $target ($abi)..."
    cd "${flutter_dir}/rust" && cargo build --target "$target" --release
    mkdir -p "${jnilib_base}/${abi}"
    cp "${flutter_dir}/rust/target/${target}/release/libferrisync_core.so" \
       "${jnilib_base}/${abi}/"
  done

  cd "${flutter_dir}" && flutter build apk --debug
}

prepare_serve_dir() {
  echo "=== Preparing serve directory ==="
  rm -rf "${SERVE_DIR}" "${DATA_DIR}"
  mkdir -p "${SERVE_DIR}"

  # Files for remote (host) side — use printf (no trailing newline)
  printf 'remote_content' > "${SERVE_DIR}/from_remote.txt"

  # Conflict file created AFTER the emulator creates its version,
  # so remote has a newer mtime and wins the conflict.
  sleep 1
  printf 'remote_version' > "${SERVE_DIR}/conflict.txt"

  # Seed for sync_incremental_test.dart (see that suite for the contract).
  printf 'v1' > "${SERVE_DIR}/base.txt"

  # Seed for folders_flow_ui_test.dart (see that suite for the contract).
  printf 'host_content' > "${SERVE_DIR}/from_host.txt"
}

start_serve() {
  echo "=== Starting serve on host (port ${SERVE_PORT}) ==="
  SERVE_LOG="/tmp/test_flutter_sync_serve.log"
  rm -f "${SERVE_LOG}"
  # stdin from /dev/null: non-TTY => auto-accept pairing, which is what
  # pairing_ui_test.dart expects; the transcript still logs every request.
  ${HOST_BINARY} --data-dir "${DATA_DIR}" serve --port ${SERVE_PORT} "${SERVE_DIR}" \
    < /dev/null > "${SERVE_LOG}" 2>&1 &
  SERVE_PID=$!

  # Wait until the port actually accepts connections (max ~10s).
  local ready=0
  for _ in $(seq 1 50); do
    if ! kill -0 "${SERVE_PID}" 2>/dev/null; then
      echo "ERROR: serve process exited during startup:" >&2
      return 1
    fi
    if (exec 3<>"/dev/tcp/127.0.0.1/${SERVE_PORT}") 2>/dev/null; then
      ready=1
      break
    fi
    sleep 0.2
  done
  if [ "$ready" -ne 1 ]; then
    echo "ERROR: serve never became ready on port ${SERVE_PORT}." >&2
    return 1
  fi
  echo "Serve is listening (pid ${SERVE_PID})."
}

verify_host_incremental_results() {
  local failed_list="$1" offline="$2"
  if [[ "$failed_list" == *"sync_incremental_test"* ]] || [ "$offline" -eq 1 ]; then
    echo "Skipping host-side incremental verification (suite did not pass)."
    return 0
  fi
  echo ""
  echo "=== Host-side verification of app-pushed changes ==="
  local ok=1
  check_host_file() {
    local desc="$1" expected="$2" path="$3"
    local actual
    actual=$(cat "$path" 2>/dev/null)
    if [ "$actual" = "$expected" ]; then
      echo "  ${PASS} ${desc}"
    else
      echo "  ${FAIL} ${desc} — expected '${expected}', got '${actual}'"
      ok=0
    fi
  }
  check_host_file "host base.txt updated by app"   "v2-from-app"  "${SERVE_DIR}/base.txt"
  check_host_file "host base.txt.bak holds old v1" "v1"           "${SERVE_DIR}/base.txt.bak"
  check_host_file "host received app_new.txt"      "made-by-app"  "${SERVE_DIR}/app_new.txt"
  if [ "$ok" -ne 1 ]; then
    failed+=("host-side incremental verification")
    return 1
  fi
}

# Cross-checks for the UI-driven suites: the app sandbox can only verify
# its own side, so pull/push directions are confirmed from host state.
verify_host_ui_results() {
  local failed_list="$1" offline="$2"
  if [[ "$failed_list" == *"pairing_ui_test"* ]] || [[ "$failed_list" == *"folders_flow_ui_test"* ]] || [ "$offline" -eq 1 ]; then
    echo "Skipping host-side UI-suite verification (suites did not pass)."
    return 0
  fi
  echo ""
  echo "=== Host-side verification of UI-driven suites ==="
  local ok=1
  if grep -q "Pair request from" "${SERVE_LOG}" 2>/dev/null; then
    echo "  ${PASS} serve log shows app's pair request over TLS"
  else
    echo "  ${FAIL} no 'Pair request from' in ${SERVE_LOG}"
    ok=0
  fi
  local actual
  actual=$(cat "${SERVE_DIR}/from_app.txt" 2>/dev/null)
  if [ "$actual" = "app_content" ]; then
    echo "  ${PASS} host received from_app.txt pushed via Sync-now button"
  else
    echo "  ${FAIL} from_app.txt on host — expected 'app_content', got '${actual}'"
    ok=0
  fi
  if [ "$ok" -ne 1 ]; then
    failed+=("host-side UI-suite verification")
    return 1
  fi
}

run_integration_tests() {
  echo "=== Running Flutter integration tests ==="

  local serial
  serial=$(get_device_serial)
  echo "  Device serial: $serial"

  # Crashed runs leave zombie forward/reverse rules that hijack later
  # runs' harness connections. Start clean.
  adb forward --remove-all 2>/dev/null || true
  adb reverse --remove-all 2>/dev/null || true

  cd "${PROJECT_ROOT}/ferrisync-flutter" || exit 1

  local failed=()
  local offline=0
  for suite in "${INTEGRATION_SUITES[@]}"; do
    echo ""
    echo "--- Suite: ${suite} ---"
    if [ "$offline" -eq 1 ]; then
      failed+=("${suite} (skipped: device offline)")
      continue
    fi
    if ! ensure_device_ready "$serial"; then
      echo "${FAIL} ${suite} (device offline — skipping remaining suites)"
      offline=1
      failed+=("${suite} (device offline)")
      continue
    fi
    if flutter test "integration_test/${suite}" -d "$serial" > /tmp/flutter_test_${suite%.dart}.log 2>&1; then
      echo "${PASS} ${suite}"
    else
      echo "${FAIL} ${suite} (see /tmp/flutter_test_${suite%.dart}.log)"
      failed+=("${suite}")
    fi
  done

  verify_host_incremental_results "${failed[*]}" "$offline"
  verify_host_ui_results "${failed[*]}" "$offline"

  echo ""
  if [ ${#failed[@]} -eq 0 ]; then
    echo "${PASS} all ${#INTEGRATION_SUITES[@]} integration suites passed"
  else
    echo "${FAIL} ${#failed[@]} of ${#INTEGRATION_SUITES[@]} integration suites failed: ${failed[*]}"
    exit 1
  fi
}

main() {
  check_adb_device
  build_binaries
  build_flutter_apk
  prepare_serve_dir
  start_serve
  run_integration_tests
}

main "$@"
