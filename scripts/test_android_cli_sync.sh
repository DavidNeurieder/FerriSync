#!/usr/bin/env bash
# Android CLI sync integration test
# Tests bidirectional sync between an Android emulator and Linux host
set -euo pipefail

AVD_NAME="${AVD_NAME:-test_phone}"
EMULATOR_BIN="/home/mr/Android/Sdk/emulator/emulator"
ANDROID_SDK_ROOT="/home/mr/Android/Sdk"
PROJECT_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
HOST_BINARY="target/debug/ferrisync-cli"
EMU_DIR="/data/local/tmp/test_android_cli"
HOST_DIR="/tmp/test_android_cli"
EMU_BIN="/data/local/tmp/ferrisync-cli"
EMU_DATA_DIR="/data/local/tmp/ferrisync-test-data"
# Keep the host's identity/state OUTSIDE the synced folder so the CLI's own
# cert.der/key.der/metadata.db are never treated as folder contents.
HOST_DATA_DIR="/tmp/test_android_cli_data"
SYNC_PORT=9847  # hardcoded in CLI sync command

# Auto-detect
ANDROID_ABI=""
ANDROID_TARGET=""

# Map Android ABI → Rust target
abi_to_target() {
  case "$1" in
    arm64-v8a)  echo "aarch64-linux-android" ;;
    x86_64)     echo "x86_64-linux-android" ;;
    armeabi-v7a) echo "armv7-linux-androideabi" ;;
    x86)        echo "i686-linux-android" ;;
    *)          echo ""; return 1 ;;
  esac
}

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
  rm -rf "${HOST_DIR}" "${HOST_DATA_DIR}" 2>/dev/null || true
  echo "Cleanup done."
}

trap cleanup EXIT

check_adb_device() {
  echo "=== Checking ADB device ==="
  local serial state
  serial=$(adb devices 2>/dev/null | awk 'NR==2{print $1}')
  state=$(adb devices 2>/dev/null | awk 'NR==2{print $2}')
  if [ "$state" = "device" ] && [ -n "$serial" ]; then
    echo "Device connected: $serial"
  else
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
    serial=$(adb devices 2>/dev/null | awk 'NR==2{print $1}')
    echo " booted ($serial)."
  fi

  ANDROID_ABI=$(adb shell getprop ro.product.cpu.abi | tr -d '\r')
  ANDROID_TARGET=$(abi_to_target "$ANDROID_ABI")
  if [ -z "$ANDROID_TARGET" ]; then
    echo "ERROR: Unknown device ABI '$ANDROID_ABI', cannot determine Rust target."
    exit 1
  fi
  echo "  Device ABI: $ANDROID_ABI → Rust target: $ANDROID_TARGET"
}

build_binaries() {
  echo "=== Building binaries ==="
  local android_binary="target/${ANDROID_TARGET}/release/ferrisync-cli"
  if [ ! -f "$android_binary" ]; then
    echo "Building Android binary for ${ANDROID_TARGET}..."
    cd "${PROJECT_ROOT}" && cargo build -p ferrisync-cli --target "${ANDROID_TARGET}" --release
  else
    echo "Android binary already exists (${ANDROID_TARGET})."
  fi
  if [ ! -f "${HOST_BINARY}" ]; then
    echo "Building host binary..."
    cd "${PROJECT_ROOT}" && cargo build -p ferrisync-cli
  else
    echo "Host binary already exists."
  fi
}

push_binary() {
  echo "=== Pushing binary to device ==="
  local android_binary="target/${ANDROID_TARGET}/release/ferrisync-cli"
  adb push "${PROJECT_ROOT}/${android_binary}" "${EMU_BIN}"
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

# Pair the host CLI with an emulator `serve` (requires an adb forward to be
# active). The emulator runs non-interactively so its PairPolicy is
# AutoAccept; the host presents its persisted TLS cert, which the emulator
# stores as this device's principal.
pair_host_with_emu() {
  echo "  Pairing host with emulator serve..."
  ${HOST_BINARY} --data-dir "${HOST_DATA_DIR}" pair 127.0.0.1 --port ${SYNC_PORT} 2>&1 | tail -2
}

# Pair the emulator CLI with a host `serve` (requires an adb reverse to be
# active).
pair_emu_with_host() {
  echo "  Pairing emulator with host serve..."
  adb shell "${EMU_BIN} --data-dir ${EMU_DATA_DIR} pair 127.0.0.1 --port ${SYNC_PORT}" 2>&1 | tail -2
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

# Helper: verify a .ferrisync-conflict-* backup exists on host with content
check_conflict_backup_host() {
  local desc="$1" expected="$2" dir="$3" base="$4"
  local bak actual
  bak=$(ls "${dir}"/"${base}".ferrisync-conflict-* 2>/dev/null | head -1)
  actual=$(cat "$bak" 2>/dev/null)
  if [ -n "$bak" ] && [ "$actual" = "$expected" ]; then
    echo "  ${PASS} ${desc}"
    return 0
  else
    echo "  ${FAIL} ${desc} — expected backup '${base}.ferrisync-conflict-*' to contain '${expected}', got '${actual}' (${bak:-<none>})"
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

  pair_host_with_emu

  echo "  Running sync from host to emulator..."
  ${HOST_BINARY} --data-dir "${HOST_DATA_DIR}" sync "${HOST_DIR}" --device "127.0.0.1" 2>&1 | tail -5

  echo "  Verifying..."
  check_file_host "host has emu_file.txt" "from_emulator" "${HOST_DIR}/emu_only/emu_file.txt"
  record "emu_file on host" $?

  check_file_emu "emu has host_file.txt" "from_host" "${EMU_DIR}/host_only/host_file.txt"
  record "host_file on emu" $?

  # both_sides.txt diverged on both sides before any sync → conflict. The
  # serving side (emulator) wins: the host pulls the emulator's version, and
  # its own is preserved as a .ferrisync-conflict-* backup.
  check_file_host "host shared resolved (server/emu version wins)" "emu_version" "${HOST_DIR}/shared/both_sides.txt"
  record "shared on host" $?

  check_file_emu "emu shared keeps its version" "emu_version" "${EMU_DIR}/shared/both_sides.txt"
  record "shared on emu" $?

  check_conflict_backup_host "host preserved its own version as conflict backup" "host_version" "${HOST_DIR}/shared" "both_sides.txt"
  record "host conflict backup" $?

  adb forward --remove tcp:${SYNC_PORT} 2>/dev/null || true
  kill_emu_serve
}

run_test_b_serve_on_host_sync_from_emu() {
  echo ""
  echo "=== Test B: Serve on host → sync from emulator ==="

  kill_emu_serve

  echo "  Starting serve on host (port ${SYNC_PORT})..."
  ${HOST_BINARY} --data-dir "${HOST_DATA_DIR}" serve --auto-accept --port ${SYNC_PORT} "${HOST_DIR}" &
  local serve_pid=$!
  sleep 2

  adb reverse tcp:${SYNC_PORT} tcp:${SYNC_PORT}

  pair_emu_with_host

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

  pair_host_with_emu

  echo "  Running sync..."
  ${HOST_BINARY} --data-dir "${HOST_DATA_DIR}" sync "${HOST_DIR}" --device "127.0.0.1" 2>&1 | tail -5
  sleep 1

  # Conflict: the serving side (emulator) wins — the host pulls the
  # emulator's content and its own is preserved as a conflict backup.
  check_file_host "host conflict.txt (emulator version wins)" "emu_content" "${HOST_DIR}/shared/conflict.txt"
  record "host conflict file" $?

  check_file_emu "emu keeps its conflict.txt" "emu_content" "${EMU_DIR}/shared/conflict.txt"
  record "emu conflict file" $?

  check_conflict_backup_host "host preserved its own version as conflict backup" "host_content" "${HOST_DIR}/shared" "conflict.txt"
  record "host conflict backup" $?

  adb forward --remove tcp:${SYNC_PORT} 2>/dev/null || true
  kill_emu_serve
}

run_test_d_incremental() {
  echo ""
  echo "=== Test D: Incremental changes propagate on second sync ==="

  kill_emu_serve

  adb shell "rm -rf ${EMU_DATA_DIR} && mkdir -p ${EMU_DATA_DIR}"
  rm -rf "${HOST_DATA_DIR}"
  rm -f "${HOST_DIR}/shared/incr.txt" "${HOST_DIR}/shared/incr.txt.bak"
  adb shell "rm -f ${EMU_DIR}/shared/incr.txt ${EMU_DIR}/shared/incr.txt.bak"
  adb shell "rm -f ${EMU_DIR}/emu_new.txt"

  printf 'v1' > "${HOST_DIR}/shared/incr.txt"

  adb shell "nohup ${EMU_BIN} --data-dir ${EMU_DATA_DIR} serve --port ${SYNC_PORT} ${EMU_DIR} > ${EMU_DATA_DIR}/serve.log 2>&1 &"
  sleep 2
  adb forward tcp:${SYNC_PORT} tcp:${SYNC_PORT}

  pair_host_with_emu

  echo "  Round 1: initial sync..."
  ${HOST_BINARY} --data-dir "${HOST_DATA_DIR}" sync "${HOST_DIR}" --device "127.0.0.1" > /dev/null 2>&1
  check_file_emu "round1: emu received incr.txt v1" "v1" "${EMU_DIR}/shared/incr.txt"
  record "D round1 pull" $?

  echo "  Mutating both sides..."
  sleep 1
  printf 'v2-host-edit' > "${HOST_DIR}/shared/incr.txt"
  adb shell "echo 'emu-added' > ${EMU_DIR}/emu_new.txt"

  echo "  Round 2: incremental sync..."
  ${HOST_BINARY} --data-dir "${HOST_DATA_DIR}" sync "${HOST_DIR}" --device "127.0.0.1" > /dev/null 2>&1

  # incr.txt was edited on the host after round 1 → conflict; the serving
  # side (emulator) wins, the host's edit is preserved as a conflict backup.
  check_file_emu "round2: emu keeps its version of incr.txt" "v1" "${EMU_DIR}/shared/incr.txt"
  record "D round2 emu state" $?

  check_file_host "round2: host got new emu file"     "emu-added"    "${HOST_DIR}/emu_new.txt"
  record "D round2 pull-new" $?

  check_file_host "round2: host resolved to emulator version (server wins)" "v1" "${HOST_DIR}/shared/incr.txt"
  record "D round2 conflict resolution" $?

  check_conflict_backup_host "round2: host preserved its edit as conflict backup" "v2-host-edit" "${HOST_DIR}/shared" "incr.txt"
  record "D round2 conflict backup" $?

  adb forward --remove tcp:${SYNC_PORT} 2>/dev/null || true
  kill_emu_serve
}

run_test_e_nested_dirs() {
  echo ""
  echo "=== Test E: Nested directories sync ==="

  kill_emu_serve

  adb shell "rm -rf ${EMU_DATA_DIR} && mkdir -p ${EMU_DATA_DIR}"
  rm -rf "${HOST_DATA_DIR}"

  mkdir -p "${HOST_DIR}/deep/x/y"
  echo 'from-deep-host' > "${HOST_DIR}/deep/x/y/z.txt"
  adb shell "mkdir -p ${EMU_DIR}/other/p/q"
  adb shell "echo 'from-deep-emu' > ${EMU_DIR}/other/p/q/r.txt"

  adb shell "nohup ${EMU_BIN} --data-dir ${EMU_DATA_DIR} serve --port ${SYNC_PORT} ${EMU_DIR} > ${EMU_DATA_DIR}/serve.log 2>&1 &"
  sleep 2
  adb forward tcp:${SYNC_PORT} tcp:${SYNC_PORT}

  pair_host_with_emu

  ${HOST_BINARY} --data-dir "${HOST_DATA_DIR}" sync "${HOST_DIR}" --device "127.0.0.1" > /dev/null 2>&1
  sleep 1

  check_file_host "host received nested emu file"  "from-deep-emu"  "${HOST_DIR}/other/p/q/r.txt"
  record "E host nested" $?

  check_file_emu "emu received nested host file"   "from-deep-host" "${EMU_DIR}/deep/x/y/z.txt"
  record "E emu nested" $?

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
  run_test_d_incremental
  run_test_e_nested_dirs
  report
}

main "$@"
