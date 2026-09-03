#!/usr/bin/env bash
# Linux Flutter <-> REPL two-folder pick integration test.
#
# The owner is a real `ferrisync` REPL/CLI that publishes TWO folders (FolderA
# and FolderB) with `add` and serves interactively under a PTY (so PairPolicy::
# Confirm applies and device-pairing requests can be approved). The Linux
# Flutter app pairs with the owner, browses its shared folders, and opens the
# "Sync with another device" chooser. The test asserts BOTH folders are
# discoverable and render in the chooser with distinct keys so the correct one
# can be chosen.
#
# The harness answers the device-pairing ("Confirm connection") prompt pushed
# into the serve session through the PTY.
set -euo pipefail

PROJECT_ROOT="/home/mr/Projects/FerriSync"
HOST_BINARY="${PROJECT_ROOT}/target/debug/ferrisync"
PEER_PORT=19899
WORK="/tmp/test_linux_flutter_two_folder_pick"
PEER_DATA="${WORK}/peer-data"
PEER_DIR_A="${WORK}/folderA"
PEER_DIR_B="${WORK}/folderB"
SERVE_LOG="${WORK}/serve.out"
APP_IN="${WORK}/serve.in"

PASS="PASS"
FAIL="FAIL"
PEER_PID=""
APP_FD=""

cleanup() {
  eval "exec ${APP_FD}>&-" 2>/dev/null || true
  kill ${PEER_PID} 2>/dev/null || true
  wait ${PEER_PID} 2>/dev/null || true
  rm -rf "${WORK}"
  echo "Cleanup done."
}
trap cleanup EXIT

build_binaries() {
  echo "Building peer binary..."
  cd "${PROJECT_ROOT}" && cargo build -p ferrisync --quiet
}

build_flutter_linux() {
  echo "Building Flutter Linux desktop..."
  cd "${PROJECT_ROOT}/ferrisync-flutter" && flutter build linux --release --quiet 2>/dev/null
}

wait_serve() {
  local pat="$1" timeout="${2:-30}"
  local start=$((SECONDS))
  until grep -qF -- "${pat}" "${SERVE_LOG}" 2>/dev/null; do
    if [ $((SECONDS - start)) -ge "${timeout}" ]; then
      echo "  TIMEOUT waiting for: ${pat} in serve log"
      echo "--- serve log ---"; cat "${SERVE_LOG}" 2>/dev/null || true
      return 1
    fi
    sleep 0.3
  done
}

answer_yes() { printf 'y\n' >&"${APP_FD}"; }

prepare_owner() {
  echo "=== Preparing owner (two published folders) ==="
  rm -rf "${WORK}"
  mkdir -p "${PEER_DATA}" "${PEER_DIR_A}" "${PEER_DIR_B}"
  printf 'seedA' > "${PEER_DIR_A}/a.txt"
  printf 'seedB' > "${PEER_DIR_B}/b.txt"
  "${HOST_BINARY}" --data-dir "${PEER_DATA}" add "${PEER_DIR_A}" --name FolderA >/dev/null
  "${HOST_BINARY}" --data-dir "${PEER_DATA}" add "${PEER_DIR_B}" --name FolderB >/dev/null
  # Fresh engine for the app so TOFU stays consistent with the fresh owner.
  rm -rf "${HOME}/.local/share/ferrisync/ferrisync"
}

start_owner_serve() {
  echo "=== Starting owner serve (interactive, ${PEER_PORT}) ==="
  mkfifo "${APP_IN}"
  exec 7<>"${APP_IN}"; APP_FD=7
  (script -qefc "${HOST_BINARY} --data-dir ${PEER_DATA} serve --port ${PEER_PORT} ${PEER_DIR_A}" \
      /dev/null < "${APP_IN}" > "${SERVE_LOG}" 2>&1) &
  PEER_PID=$!
  wait_serve "Serving folder" 20
  echo "  owner serve is up (pid ${PEER_PID})."
}

run_integration_test() {
  echo "=== Running two-folder pick suite ==="
  cd "${PROJECT_ROOT}/ferrisync-flutter"
  flutter test integration_test/two_folder_pick_ui_test.dart -d linux \
      --dart-define=FERRISYNC_PORT="${PEER_PORT}" \
      > "/tmp/flutter_two_folder_pick.log" 2>&1 &
  local flutter_pid=$!
  local dev_lk="${WORK}/.answered_dev"
  for _ in $(seq 1 300); do
    if [ ! -f "${dev_lk}" ] && grep -q "Confirm connection" "${SERVE_LOG}" 2>/dev/null; then
      answer_yes; touch "${dev_lk}"; echo "  ${PASS} answered device-pairing prompt"
    fi
    if ! kill -0 "${flutter_pid}" 2>/dev/null; then
      break
    fi
    sleep 0.5
  done
  if ! wait "${flutter_pid}"; then
    echo "${FAIL} two_folder_pick_ui_test.dart failed"
    echo "--- flutter log ---"; cat "/tmp/flutter_two_folder_pick.log"
    echo "--- serve log ---"; tail -30 "${SERVE_LOG}"
    return 1
  fi
  echo "${PASS} two_folder_pick_ui_test.dart"
}

main() {
  build_binaries
  build_flutter_linux
  prepare_owner
  start_owner_serve
  run_integration_test
  echo ""
  echo "${PASS} Linux Flutter two-folder pick OK"
}

main "$@"