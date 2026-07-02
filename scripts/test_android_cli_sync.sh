#!/usr/bin/env bash
# Android CLI sync integration test
# Tests bidirectional sync between an Android emulator and Linux host
set -euo pipefail

AVD_NAME="${AVD_NAME:-test_phone}"
EMULATOR_BIN="/home/mr/Android/Sdk/emulator/emulator"
ANDROID_SDK_ROOT="/home/mr/Android/Sdk"
ANDROID_TARGET="x86_64-linux-android"
ANDROID_BINARY="target/${ANDROID_TARGET}/release/ferrisync-cli"
HOST_BINARY="target/debug/ferrisync-cli"
EMU_DIR="/data/local/tmp/test_android_cli"
HOST_DIR="/tmp/test_android_cli"
EMU_BIN="/data/local/tmp/ferrisync-cli"
EMU_DATA_DIR="/data/local/tmp/ferrisync-test-data"
HOST_DATA_DIR="${HOST_DIR}/data"
SYNC_PORT=9847  # hardcoded in CLI sync command

PASS="PASS"
FAIL="FAIL"
TESTS=0
PASSED=0

cleanup() {
  echo ""
  echo "=== Cleaning up ==="
  adb forward --remove-all 2>/dev/null || true
  adb reverse --remove-all 2>/dev/null || true
  adb shell "pkill -f 'ferrisync' 2>/dev/null; rm -rf ${EMU_DIR} ${EMU_DATA_DIR} ${EMU_BIN}" 2>/dev/null || true
  kill $(jobs -p) 2>/dev/null || true
  wait $(jobs -p) 2>/dev/null || true
  rm -rf "${HOST_DIR}" 2>/dev/null || true
  echo "Cleanup done."
}

trap cleanup EXIT

check_adb_device() {
  echo "=== Checking ADB device ==="
  local state
  state=$(adb devices 2>/dev/null | awk 'NR==2{print $2}')
  if [ "$state" = "device" ]; then
    echo "Device already connected."
    return 0
  fi
  echo "No device found. Starting emulator '${AVD_NAME}'..."
  ANDROID_SDK_ROOT="${ANDROID_SDK_ROOT}" \
    nohup "${EMULATOR_BIN}" -avd "${AVD_NAME}" -no-window -no-audio -gpu swiftshader_indirect \
    > /tmp/emulator.log 2>&1 &
  echo -n "Waiting for boot"
  adb wait-for-device
  while [ "$(adb shell getprop sys.boot_completed 2>/dev/null | tr -d '\r')" != "1" ]; do
    echo -n "."
    sleep 2
  done
  echo " booted."
}

build_binaries() {
  echo "=== Building binaries ==="
  if [ ! -f "${ANDROID_BINARY}" ]; then
    echo "Building Android binary..."
    cargo build -p ferrisync-cli --target "${ANDROID_TARGET}" --release
  else
    echo "Android binary already exists."
  fi
  if [ ! -f "${HOST_BINARY}" ]; then
    echo "Building host binary..."
    cargo build -p ferrisync-cli
  else
    echo "Host binary already exists."
  fi
}

push_binary() {
  echo "=== Pushing binary to emulator ==="
  adb push "${ANDROID_BINARY}" "${EMU_BIN}"
  adb shell "chmod 755 ${EMU_BIN}"
}

prepare_dirs() {
  echo "=== Preparing test directories ==="
  rm -rf "${HOST_DIR}"
  mkdir -p "${HOST_DIR}/host_only"
  mkdir -p "${HOST_DIR}/emu_only"
  mkdir -p "${HOST_DIR}/shared"

  echo "from_host" > "${HOST_DIR}/host_only/host_file.txt"
  echo "host_version" > "${HOST_DIR}/shared/both_sides.txt"

  adb shell "rm -rf ${EMU_DIR} ${EMU_DATA_DIR}"
  adb shell "mkdir -p ${EMU_DIR}/host_only ${EMU_DIR}/emu_only ${EMU_DIR}/shared"
  adb shell "mkdir -p ${EMU_DATA_DIR}"
  adb shell "echo 'from_emulator' > ${EMU_DIR}/emu_only/emu_file.txt"
  adb shell "echo 'emu_version' > ${EMU_DIR}/shared/both_sides.txt"
}

kill_emu_serve() {
  adb shell "pkill -f 'ferrisync.*serve' 2>/dev/null" || true
  sleep 1
}

# Helper: verify file content on host
check_file_host() {
  local desc="$1" expected="$2" path="$3"
  local actual
  actual=$(cat "$path" 2>/dev/null)
  if [ "$actual" = "$expected" ]; then
    echo "  ${PASS} ${desc}"
    return 0
  else
    echo "  ${FAIL} ${desc} — expected '${expected}', got '${actual}'"
    return 1
  fi
}

# Helper: verify file content on emulator
check_file_emu() {
  local desc="$1" expected="$2" path="$3"
  local actual
  actual=$(adb shell "cat '$path' 2>/dev/null" | tr -d '\r')
  if [ "$actual" = "$expected" ]; then
    echo "  ${PASS} ${desc}"
    return 0
  else
    echo "  ${FAIL} ${desc} — expected '${expected}', got '${actual}'"
    return 1
  fi
}

record() {
  local desc="$1" result="$2"
  TESTS=$((TESTS + 1))
  if [ "$result" = "ok" ] || [ "$result" = "0" ]; then
    PASSED=$((PASSED + 1))
  fi
}

run_test_a_serve_on_emu_sync_from_host() {
  echo ""
  echo "=== Test A: Serve on emulator → sync from host ==="

  kill_emu_serve

  adb shell "nohup ${EMU_BIN} --data-dir ${EMU_DATA_DIR} serve --port ${SYNC_PORT} ${EMU_DIR} > ${EMU_DATA_DIR}/serve.log 2>&1 &"
  sleep 2

  adb forward tcp:${SYNC_PORT} tcp:${SYNC_PORT}

  echo "  Running sync from host to emulator..."
  ${HOST_BINARY} --data-dir "${HOST_DATA_DIR}" sync "${HOST_DIR}" --device "127.0.0.1" 2>&1 | tail -5

  echo "  Verifying..."
  check_file_host "host has emu_file.txt" "from_emulator" "${HOST_DIR}/emu_only/emu_file.txt"
  record "emu_file on host" $?

  check_file_emu "emu has host_file.txt" "from_host" "${EMU_DIR}/host_only/host_file.txt"
  record "host_file on emu" $?

  check_file_host "host shared resolved" "host_version" "${HOST_DIR}/shared/both_sides.txt"
  record "shared on host" $?

  # Host creates both_sides.txt after emulator, so host's newer mtime wins
  check_file_emu "emu shared resolved (host's newer version wins)" "host_version" "${EMU_DIR}/shared/both_sides.txt"
  record "shared on emu" $?

  adb forward --remove tcp:${SYNC_PORT} 2>/dev/null || true
  kill_emu_serve
}

run_test_b_serve_on_host_sync_from_emu() {
  echo ""
  echo "=== Test B: Serve on host → sync from emulator ==="

  kill_emu_serve

  echo "  Starting serve on host (port ${SYNC_PORT})..."
  ${HOST_BINARY} --data-dir "${HOST_DATA_DIR}" serve --port ${SYNC_PORT} "${HOST_DIR}" &
  local serve_pid=$!
  sleep 2

  adb reverse tcp:${SYNC_PORT} tcp:${SYNC_PORT}

  echo "  Running sync on emulator to host..."
  adb shell "${EMU_BIN} --data-dir ${EMU_DATA_DIR} sync ${EMU_DIR} --device 127.0.0.1" 2>&1 | tail -5

  echo "  Verifying..."
  check_file_host "host still has emu_file.txt" "from_emulator" "${HOST_DIR}/emu_only/emu_file.txt"
  record "emu_file on host (B)" $?

  check_file_emu "emu still has host_file.txt" "from_host" "${EMU_DIR}/host_only/host_file.txt"
  record "host_file on emu (B)" $?

  adb reverse --remove tcp:${SYNC_PORT} 2>/dev/null || true
  kill ${serve_pid} 2>/dev/null || true
  wait ${serve_pid} 2>/dev/null || true
}

run_test_c_conflict() {
  echo ""
  echo "=== Test C: Conflict resolution ==="

  kill_emu_serve

  # Fresh data dirs
  adb shell "rm -rf ${EMU_DATA_DIR} && mkdir -p ${EMU_DATA_DIR}"
  rm -rf "${HOST_DATA_DIR}"
  rm -f "${HOST_DIR}/shared/conflict.txt" "${HOST_DIR}/shared/conflict.txt.bak"
  adb shell "rm -f ${EMU_DIR}/shared/conflict.txt ${EMU_DIR}/shared/conflict.txt.bak"

  echo "  Creating conflicting files..."
  adb shell "echo 'emu_content' > ${EMU_DIR}/shared/conflict.txt"
  sleep 1  # ensure different mtime

  echo 'host_content' > "${HOST_DIR}/shared/conflict.txt"

  adb shell "nohup ${EMU_BIN} --data-dir ${EMU_DATA_DIR} serve --port ${SYNC_PORT} ${EMU_DIR} > ${EMU_DATA_DIR}/serve.log 2>&1 &"
  sleep 2
  adb forward tcp:${SYNC_PORT} tcp:${SYNC_PORT}

  echo "  Running sync..."
  ${HOST_BINARY} --data-dir "${HOST_DATA_DIR}" sync "${HOST_DIR}" --device "127.0.0.1" 2>&1 | tail -5
  sleep 1

  check_file_host "host conflict.txt content" "host_content" "${HOST_DIR}/shared/conflict.txt"
  record "host conflict file" $?

  local emu_bak
  emu_bak=$(adb shell "cat ${EMU_DIR}/shared/conflict.txt.bak 2>/dev/null" | tr -d '\r')
  if [ "$emu_bak" = "emu_content" ]; then
    echo "  ${PASS} emu has conflict.txt.bak with original content"
    record "emu .bak file" "ok"
  else
    echo "  ${FAIL} emu conflict.txt.bak — expected 'emu_content', got '${emu_bak}'"
    record "emu .bak file" "fail"
  fi

  adb forward --remove tcp:${SYNC_PORT} 2>/dev/null || true
  kill_emu_serve
}

report() {
  echo ""
  echo "=========================================="
  echo "Results: ${PASSED}/${TESTS} tests passed"
  if [ "$PASSED" -eq "$TESTS" ]; then
    echo "All tests passed!"
  else
    echo "Some tests failed."
    exit 1
  fi
}

main() {
  check_adb_device
  build_binaries
  push_binary
  prepare_dirs
  run_test_a_serve_on_emu_sync_from_host
  run_test_b_serve_on_host_sync_from_emu
  run_test_c_conflict
  report
}

main "$@"
