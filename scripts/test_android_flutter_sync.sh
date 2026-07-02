#!/usr/bin/env bash
# Android Flutter sync integration test
# Sets up a serve process on the host, then runs the Flutter integration
# test on the Android emulator which connects via adb reverse.
set -euo pipefail

PROJECT_ROOT="/home/mr/Projects/FerriSync"
AVD_NAME="${AVD_NAME:-test_phone}"
HOST_BINARY="${PROJECT_ROOT}/target/debug/ferrisync-cli"
SERVE_PORT=9847
SERVE_DIR="/tmp/test_flutter_sync_serve"
DATA_DIR="/tmp/test_flutter_sync_data"

PASS="PASS"
FAIL="FAIL"

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
  local state
  state=$(adb devices 2>/dev/null | awk 'NR==2{print $2}')
  if [ "$state" = "device" ]; then
    echo "Device connected."
    return
  fi
  echo "No device connected. Starting emulator (${AVD_NAME})..."
  ANDROID_SDK_ROOT=/home/mr/Android/Sdk \
    nohup /home/mr/Android/Sdk/emulator/emulator \
      -avd "${AVD_NAME}" -no-window -no-audio \
      > /tmp/emu.log 2>&1 &
  echo "Waiting for emulator to boot..."
  adb wait-for-device
  echo "Emulator ready."
}

build_binaries() {
  if [ ! -f "${HOST_BINARY}" ]; then
    echo "Building host binary..."
    cd "${PROJECT_ROOT}" && cargo build -p ferrisync-cli
  fi
}

build_flutter_apk() {
  echo "Building Flutter APK..."
  cd "${PROJECT_ROOT}/ferrisync-flutter" && flutter build apk --debug
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
  sleep 2
}

run_integration_test() {
  echo "=== Running Flutter integration test ==="

  adb reverse tcp:${SERVE_PORT} tcp:${SERVE_PORT}

  cd "${PROJECT_ROOT}/ferrisync-flutter" && \
    flutter test integration_test/sync_test.dart -d "emulator-5554" 2>&1

  local exit_code=$?
  if [ $exit_code -eq 0 ]; then
    echo "${PASS} Flutter integration test passed"
  else
    echo "${FAIL} Flutter integration test failed (exit code $exit_code)"
    exit $exit_code
  fi
}

main() {
  check_adb_device
  build_binaries
  build_flutter_apk
  prepare_serve_dir
  start_serve
  run_integration_test
}

main "$@"
