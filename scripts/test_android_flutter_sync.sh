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

# Integration suites to run, in order (files in ferrisync-flutter/integration_test/)
INTEGRATION_SUITES=("app_test.dart" "frb_smoke_test.dart" "sync_test.dart")

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
  adb devices 2>/dev/null | awk 'NR==2{print $1}'
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
  echo "No device connected. Starting emulator (${AVD_NAME})..."
  ANDROID_SDK_ROOT=/home/mr/Android/Sdk \
    nohup /home/mr/Android/Sdk/emulator/emulator \
      -avd "${AVD_NAME}" \
      > /tmp/emu.log 2>&1 &
  echo "Waiting for emulator to boot..."
  adb wait-for-device
  echo "Emulator ready."
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
    cp "${flutter_dir}/rust/target/${target}/release/libferrisync_flutter.so" \
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
}

start_serve() {
  echo "=== Starting serve on host (port ${SERVE_PORT}) ==="
  ${HOST_BINARY} --data-dir "${DATA_DIR}" serve --port ${SERVE_PORT} "${SERVE_DIR}" &
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

run_integration_tests() {
  echo "=== Running Flutter integration tests ==="

  local serial
  serial=$(get_device_serial)
  echo "  Device serial: $serial"

  ensure_device_ready "$serial"
  adb reverse tcp:${SERVE_PORT} tcp:${SERVE_PORT}

  cd "${PROJECT_ROOT}/ferrisync-flutter" || exit 1

  local failed=()
  for suite in "${INTEGRATION_SUITES[@]}"; do
    echo ""
    echo "--- Suite: ${suite} ---"
    if ! ensure_device_ready "$serial"; then
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
