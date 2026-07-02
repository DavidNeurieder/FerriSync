#!/usr/bin/env bash
# Android Flutter sync integration test
# Sets up a serve process on the host, then runs the Flutter integration
# test on the connected Android device (physical or emulator).
set -euo pipefail

PROJECT_ROOT="/home/mr/Projects/FerriSync"
ANDROID_TARGETS=("x86_64-linux-android" "aarch64-linux-android")
AVD_NAME="${AVD_NAME:-test_phone}"
HOST_BINARY="${PROJECT_ROOT}/target/debug/ferrisync-cli"
SERVE_PORT=9847
SERVE_DIR="/tmp/test_flutter_sync_serve"
DATA_DIR="/tmp/test_flutter_sync_data"

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
  sleep 2
}

run_integration_test() {
  echo "=== Running Flutter integration test ==="

  local serial
  serial=$(get_device_serial)
  echo "  Device serial: $serial"

  adb reverse tcp:${SERVE_PORT} tcp:${SERVE_PORT}

  cd "${PROJECT_ROOT}/ferrisync-flutter" && \
    flutter test integration_test/sync_test.dart -d "$serial" 2>&1

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
