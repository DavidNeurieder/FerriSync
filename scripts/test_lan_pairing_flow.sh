#!/usr/bin/env bash
# LAN pairing-consent flow system test
#
# Exercises the full consent handshake between a real Android emulator CLI
# and the host REPL CLI over the actual LAN IP (no adb forward/reverse):
#
#   host : ferrisync REPL serves a folder (Confirm policy)
#   emu  : ferrisync pair <HOST_LAN_IP> --port <port>   -> held
#   host : PAIRING REQUEST pop-up appears                  -> validated
#   host : pendings lists request, confirm 1 approves it
#   emu  : pairing succeeds
#   emu  : sync --device <ip>:<port>  (single bidirectional session)
#   host : received emulator file / emu: received host file
#
# Env overrides: HOST_LAN_IP, TEST_PORT, AVD_NAME, EMULATOR_BIN
#
# EMU_NET=tap (default) : emulator is booted onto a dedicated TAP link and gets
#                         192.168.179.2 statically; host side is 192.168.179.1.
#                         Connection path = real device-to-device Ethernet,
#                         zero adb forwards/reverses.
# EMU_NET=nat           : classic NAT flow; HOST_LAN_IP autodetected.
set -euo pipefail

AVD_NAME="${AVD_NAME:-test_phone}"
EMULATOR_BIN="${EMULATOR_BIN:-/home/mr/Android/Sdk/emulator/emulator}"
ANDROID_SDK_ROOT="${ANDROID_SDK_ROOT:-/home/mr/Android/Sdk}"
PROJECT_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "${PROJECT_ROOT}"

EMU_NET="${EMU_NET:-tap}"
TAP_IF="${TAP_IF:-tap0}"
TAP_HOST_IP="192.168.179.1"
TAP_EMU_IP="192.168.179.2"

HOST_BIN="target/debug/ferrisync"
PORT="${TEST_PORT:-19880}"
HOST_LAN_IP="${HOST_LAN_IP:-}"
APP_ID="com.example.ferrisync"

WORK="$(mktemp -d /tmp/lan_pair_test.XXXXXX)"
HOST_SERVE_DIR="${WORK}/served"
HOST_DATA_DIR="${WORK}/host-data"
REPL_IN="${WORK}/repl.in"
REPL_OUT="${WORK}/repl.out"
REPL_ERR="${WORK}/repl.err"

EMU_BIN="/data/local/tmp/lan_pair_test_bin"
EMU_DIR="/data/local/tmp/lan_pair_test/fold"
EMU_DATA="/data/local/tmp/lan_pair_test_data"

ANDROID_ABI=""
ANDROID_TARGET=""
REPL_PID=""

TESTS=0
PASSED=0

PASS="PASS"
FAIL="FAIL"

cleanup() {
  echo ""
  echo "=== Cleaning up ==="
  [ -n "${REPL_PID}" ] && kill "${REPL_PID}" 2>/dev/null || true
  exec 3>&- 2>/dev/null || true
  adb shell "pkill -f ${EMU_BIN} 2>/dev/null; rm -rf $(dirname ${EMU_DIR}) ${EMU_DATA} ${EMU_BIN}" 2>/dev/null || true
  # NOTE: the emulator is deliberately left running for faster re-runs.
  rm -rf "${WORK}"
  echo "Cleanup done (emulator left running)."
}
trap cleanup EXIT

abi_to_target() {
  case "$1" in
    arm64-v8a)   echo "aarch64-linux-android" ;;
    x86_64)      echo "x86_64-linux-android" ;;
    armeabi-v7a) echo "armv7-linux-androideabi" ;;
    x86)         echo "i686-linux-android" ;;
    *)           echo ""; return 1 ;;
  esac
}

detect_host_ip() {
  if [ "${EMU_NET}" = "tap" ]; then
    HOST_LAN_IP="${TAP_HOST_IP}"
    echo "TAP mode: host=${TAP_HOST_IP} emulator=${TAP_EMU_IP} (${TAP_IF})"
    return
  fi
  if [ -z "${HOST_LAN_IP}" ]; then
    local dev
    dev=$(ip route get 1.1.1.1 2>/dev/null | awk '{for(i=1;i<=NF;i++) if($i=="dev"){print $(i+1); break}}')
    HOST_LAN_IP=$(ip -4 -o addr show dev "${dev}" scope global 2>/dev/null | awk '{print $4}' | cut -d/ -f1 | head -1)
  fi
  if [ -z "${HOST_LAN_IP}" ]; then
    echo "ERROR: could not detect a global IPv4 address; set HOST_LAN_IP=..."
    exit 1
  fi
  echo "Host LAN IP: ${HOST_LAN_IP}"
}

# In TAP mode the emulator must be booted fresh onto tap0, so refuse to run
# against an already-attached device (it would be on the wrong network).
require_no_attached_device() {
  [ "${EMU_NET}" = "tap" ] || return 0
  if adb devices 2>/dev/null | awk 'NR==2{print $2}' | grep -q .; then
    echo "ERROR: EMU_NET=tap boots its own emulator on ${TAP_IF}."
    echo "Detach the current device first (e.g.: adb emu kill) and rerun."
    exit 1
  fi
}

check_adb_device() {
  echo "=== Checking ADB device ==="
  local serial state
  serial=$(adb devices 2>/dev/null | awk 'NR==2{print $1}')
  state=$(adb devices 2>/dev/null | awk 'NR==2{print $2}')
  if [ "$state" = "device" ] && [ -n "$serial" ]; then
    echo "Device connected: ${serial}"
  else
    echo "No device found. Starting emulator '${AVD_NAME}'..."
    local qemu_args=()
    [ "${EMU_NET}" = "tap" ] && qemu_args=(-qemu -net nic -net "tap,ifname=${TAP_IF},script=no,downscript=no")
    ANDROID_SDK_ROOT="${ANDROID_SDK_ROOT}" \
      nohup "${EMULATOR_BIN}" -avd "${AVD_NAME}" -no-window -no-audio -gpu swiftshader_indirect \
      "${qemu_args[@]}" \
      > /tmp/emulator_lan_pair.log 2>&1 &
    echo -n "Waiting for boot"
    adb wait-for-device
    while [ "$(adb shell getprop sys.boot_completed 2>/dev/null | tr -d '\r')" != "1" ]; do
      echo -n "."
      sleep 2
    done
    serial=$(adb devices 2>/dev/null | awk 'NR==2{print $1}')
    echo " booted (${serial})."
  fi
  [ "${EMU_NET}" = "tap" ] && configure_emu_eth0
  ANDROID_ABI=$(adb shell getprop ro.product.cpu.abi | tr -d '\r')
  ANDROID_TARGET=$(abi_to_target "${ANDROID_ABI}") || {
    echo "ERROR: unknown ABI '${ANDROID_ABI}'"
    exit 1
  }
  echo "  Device ABI: ${ANDROID_ABI} -> Rust target: ${ANDROID_TARGET}"
}

build_and_push() {
  echo "=== Building + pushing binaries (forced, so latest code is tested) ==="
  cargo build -q -p ferrisync
  cargo build -q -p ferrisync --target "${ANDROID_TARGET}" --release
  adb push "target/${ANDROID_TARGET}/release/ferrisync" "${EMU_BIN}" > /dev/null
  adb shell "chmod 755 ${EMU_BIN}"
}

build_install_app() {
  echo "=== Building + installing the Android app (universal debug APK) ==="
  cd "${PROJECT_ROOT}/ferrisync-flutter"
  if ! flutter build apk --debug > /tmp/ferrisync_apk_build.log 2>&1; then
    echo "ERROR: flutter build apk --debug failed; see /tmp/ferrisync_apk_build.log"
    exit 1
  fi
  cd "${PROJECT_ROOT}"
  adb install -r "ferrisync-flutter/build/app/outputs/flutter-apk/app-debug.apk"
}

launch_app() {
  adb shell am force-stop "${APP_ID}" 2>/dev/null || true
  adb shell am start -n "${APP_ID}/.MainActivity"
}

repl_send() {
  printf '%s\n' "$*" >&3
}

# Poll the REPL transcript until it contains the fixed string; dump on timeout.
wait_repl() {
  local pattern="$1" timeout="${2:-30}"
  local start=$((SECONDS))
  until grep -qF -- "${pattern}" "${REPL_OUT}" 2>/dev/null; do
    if [ $((SECONDS - start)) -ge "${timeout}" ]; then
      echo "  TIMEOUT waiting for: ${pattern}"
      echo "--- REPL transcript ---"
      grep -v mdns_sd "${REPL_OUT}" || true
      return 1
    fi
    sleep 0.3
  done
  return 0
}

# Poll until a line matching the ERE appears in the REPL transcript.
wait_repl_line() {
  local ere="$1" timeout="${2:-30}"
  local start=$((SECONDS))
  until grep -Eq -- "${ere}" "${REPL_OUT}" 2>/dev/null; do
    if [ $((SECONDS - start)) -ge "${timeout}" ]; then
      echo "  TIMEOUT waiting for line: ${ere}"
      echo "--- REPL transcript ---"
      grep -v mdns_sd "${REPL_OUT}" || true
      return 1
    fi
    sleep 0.3
  done
  return 0
}

# Poll the emulator filesystem until the path exists and is non-empty.
wait_emu_file() {
  local path="$1" timeout="${2:-30}"
  local start=$((SECONDS))
  until [ -n "$(adb shell "[ -f '${path}' ] && cat '${path}'" 2>/dev/null | tr -d '\r')" ]; do
    if [ $((SECONDS - start)) -ge "${timeout}" ]; then
      return 1
    fi
    sleep 0.5
  done
  return 0
}

# Poll until the emulator can reach the host over the LAN (NAT + routes ready).
wait_emu_reachable() {
  local timeout="${1:-90}"
  local start=$((SECONDS))
  until adb shell "ping -c1 -W1 ${HOST_LAN_IP} >/dev/null 2>&1" 2>/dev/null; do
    if [ $((SECONDS - start)) -ge "${timeout}" ]; then
      echo "  TIMEOUT: emulator cannot reach ${HOST_LAN_IP}"
      return 1
    fi
    sleep 2
  done
  return 0
}

# Poll until the host can reach the emulator's TAP address.
wait_host_reaches_emu() {
  local timeout="${1:-60}"
  local start=$((SECONDS))
  until ping -c1 -W1 "${TAP_EMU_IP}" > /dev/null 2>&1; do
    if [ $((SECONDS - start)) -ge "${timeout}" ]; then
      echo "  TIMEOUT: host cannot reach ${TAP_EMU_IP}"
      return 1
    fi
    sleep 2
  done
  return 0
}

# Give the emulator's eth0 its static TAP address (no DHCP server on the
# point-to-point link, so we configure it over adb; root images only).
configure_emu_eth0() {
  echo "Configuring ${TAP_IF} peer address ${TAP_EMU_IP} on emulator..."
  adb root > /dev/null 2>&1 || true
  sleep 1
  adb wait-for-device
  local start=$((SECONDS))
  while true; do
    adb shell "ip link set eth0 up; ip addr add ${TAP_EMU_IP}/24 dev eth0" > /dev/null 2>&1 || true
    if adb shell "ip -4 addr show eth0" 2>/dev/null | grep -q "${TAP_EMU_IP}"; then
      echo "  eth0 has ${TAP_EMU_IP}."
      return 0
    fi
    if [ $((SECONDS - start)) -ge 60 ]; then
      echo "  TIMEOUT configuring eth0; addresses:"
      adb shell "ip addr show" 2>/dev/null | sed 's/^/    /'
      return 1
    fi
    sleep 2
  done
}

# set-e-safe assertion: run the command in condition context, record result.
checked() {
  local desc="$1"; shift
  TESTS=$((TESTS + 1))
  if "$@"; then
    echo "  ${PASS} ${desc}"
    PASSED=$((PASSED + 1))
  else
    echo "  ${FAIL} ${desc}"
  fi
}

# Poll until the process is gone.
wait_proc_gone() {
  local pid="$1" timeout="${2:-15}"
  local start=$((SECONDS))
  while kill -0 "${pid}" 2>/dev/null; do
    if [ $((SECONDS - start)) -ge "${timeout}" ]; then
      return 1
    fi
    sleep 0.5
  done
  return 0
}

main() {
  detect_host_ip
  require_no_attached_device

  if [ "${EMU_NET}" = "tap" ]; then
    echo "=== Ensuring ${TAP_IF} exists ==="
    "${PROJECT_ROOT}/scripts/setup_emu_lan.sh" setup
  fi

  if (exec 3<>"/dev/tcp/127.0.0.1/${PORT}") 2>/dev/null; then
    echo "ERROR: port ${PORT} already in use; set TEST_PORT=<free port>"
    exit 1
  fi

  check_adb_device
  build_and_push
  build_install_app
  launch_app

  echo "=== Starting host REPL serve on ${HOST_LAN_IP}:${PORT} ==="
  mkdir -p "${HOST_SERVE_DIR}" "${HOST_DATA_DIR}"
  mkfifo "${REPL_IN}"
  # The no-arg entrypoint is the full-screen TUI, so the REPL is forced
  # explicitly (it also runs over a FIFO, far from any terminal).
  "${HOST_BIN}" --data-dir "${HOST_DATA_DIR}" repl < "${REPL_IN}" > "${REPL_OUT}" 2> "${REPL_ERR}" &
  REPL_PID=$!
  sleep 0.2
  exec 3> "${REPL_IN}"

  repl_send "serve ${HOST_SERVE_DIR} --port ${PORT}"
  checked "transcript shows serve confirmation" wait_repl "serve #1 started"

  echo "=== Pairing from emulator (expect hold + pop-up) ==="
  checked "app installed on emulator" bash -c "adb shell pm list packages ${APP_ID} | grep -q 'package:${APP_ID}'"
  checked "app launched" launch_app
  sleep 3
  checked "app process running on emulator" bash -c "adb shell pidof ${APP_ID} | grep -q ."
  echo "  (the CLI flow below drives ferrisync headlessly via adb — no app UI involved)"
  checked "emulator reaches host over LAN" wait_emu_reachable 90
  [ "${EMU_NET}" = "tap" ] && checked "host reaches emulator over TAP" wait_host_reaches_emu 60
  checked "no adb forwards/reverses in connection path" bash -c '[ -z "$(adb forward --list)" ] && [ -z "$(adb reverse --list)" ]'

  adb shell "rm -rf $(dirname ${EMU_DIR}) ${EMU_DATA}; mkdir -p ${EMU_DIR} ${EMU_DATA}"
  # setsid detaches the pair process from the adb session so it survives.
  adb shell "setsid sh -c '${EMU_BIN} --data-dir ${EMU_DATA} pair ${HOST_LAN_IP} --port ${PORT} > ${EMU_DATA}/pair.log 2>&1; echo \$? > ${EMU_DATA}/pair.exit' >/dev/null 2>&1 &"

  checked "pair request received on CLI" wait_repl "PAIRING REQUEST — confirm connection with '" 30

  repl_send "pendings"
  checked "pendings lists held request" wait_repl_line '^[[:space:]]{2}1[[:space:]]{2}[^ ]+ \([0-9a-f-]{36}\)$' 15

  echo "=== Confirming pair request on CLI ==="
  repl_send "confirm 1"
  checked "approval reported" wait_repl "approved '" 15

  checked "emulator pairing exited 0" wait_emu_file "${EMU_DATA}/pair.exit" 30
  local pair_exit
  pair_exit=$(adb shell "cat ${EMU_DATA}/pair.exit 2>/dev/null" | tr -d '\r')
  if [ "${pair_exit}" != "0" ]; then
    echo "    pair.exit='${pair_exit}'; pair.log:"
    adb shell "cat ${EMU_DATA}/pair.log 2>/dev/null" | tr -d '\r' | sed 's/^/      /' || true
  fi
  checked "emulator pairing exited 0 (code)" [ "${pair_exit}" = "0" ]
  local pair_log
  pair_log=$(adb shell "cat ${EMU_DATA}/pair.log 2>/dev/null" | tr -d '\r')
  checked "emulator saw 'Paired with'" grep -q "Paired with" <<<"${pair_log}"

  echo "=== Seeding files both sides ==="
  local host_content="host-payload-lan-$RANDOM"
  local emu_content="emu-payload-lan-$RANDOM"
  echo "${host_content}" > "${HOST_SERVE_DIR}/host_file.txt"
  adb shell "echo '${emu_content}' > ${EMU_DIR}/emu_file.txt"

  echo "=== Bidirectional sync initiated from emulator ==="
  adb shell "${EMU_BIN} --data-dir ${EMU_DATA} sync ${EMU_DIR} --device ${HOST_LAN_IP}:${PORT} > ${EMU_DATA}/sync.log 2>&1; echo EXIT=\$? >> ${EMU_DATA}/sync.log"

  local sync_log
  sync_log=$(adb shell "cat ${EMU_DATA}/sync.log 2>/dev/null" | tr -d '\r')
  checked "sync exited 0 on emulator" grep -q "^EXIT=0$" <<<"${sync_log}"
  checked "'Sync complete.' reported" grep -q "Sync complete." <<<"${sync_log}"

  echo "=== Verifying transfers ==="
  local got_host_file got_emu_file
  got_host_file=$(cat "${HOST_SERVE_DIR}/emu_file.txt" 2>/dev/null || echo "<missing>")
  got_emu_file=$(adb shell "cat ${EMU_DIR}/host_file.txt 2>/dev/null" | tr -d '\r')

  checked "emulator -> host transfer" [ "${got_host_file}" = "${emu_content}" ]
  checked "host -> emulator transfer" [ "${got_emu_file}" = "${host_content}" ]

  # Host-side event log: "pulled X <- remote" = X arrived from emulator,
  # "pushed Y -> remote" = Y was sent to the emulator.
  checked "CLI logged receipt of emulator file" wait_repl "pulled emu_file.txt" 10
  checked "CLI logged send of host file" wait_repl "pushed host_file.txt" 10

  repl_send "exit"
  checked "REPL confirmed server shutdown" wait_repl "server #1 stopped" 15
  checked "REPL exits cleanly after 'exit'" wait_proc_gone "${REPL_PID}" 15

  echo ""
  if [ "${TESTS}" -gt 0 ] && [ "${PASSED}" -eq "${TESTS}" ]; then
    echo "=== RESULT: ALL ${TESTS} CHECKS PASSED ==="
  else
    echo "=== RESULT: ${PASSED}/${TESTS} checks passed ==="
    exit 1
  fi
}

main "$@"
