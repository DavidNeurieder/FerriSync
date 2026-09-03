#!/usr/bin/env bash
# Linux Flutter <-> REPL/CLI folder-pair integration test.
#
# The peer is a real `ferrisync` CLI/REPL process acting as the OWNER: it
# publishes a folder with `add` and serves it interactively (under a PTY, so
# PairPolicy::Confirm applies and folder-pairing requests can be approved).
# The Linux Flutter app is the REQUESTER: it pairs with the peer, browses its
# published shared folder, and requests a folder pair. Both stacks are starting
# points (peer publishes+serves; app pairs+requests), meeting over real sockets.
#
# Approval answers are pushed into the interactive serve session through a PTY
# allocated by `script`; the harness answers the device-pairing prompt and the
# folder-pairing prompt as they appear.
set -euo pipefail

PROJECT_ROOT="/home/mr/Projects/FerriSync"
HOST_BINARY="${PROJECT_ROOT}/target/debug/ferrisync"
PEER_PORT=19897
PEER_DIR="/tmp/test_linux_flutter_repl_folder_pair/peer-served"
PEER_DATA="/tmp/test_linux_flutter_repl_folder_pair/peer-data"

WORK="/tmp/test_linux_flutter_repl_folder_pair"
SERVE_LOG="${WORK}/serve.out"
APP_IN="${WORK}/serve.in"

PASS="PASS"
FAIL="FAIL"
PEER_PID=""
APP_FD=""

cleanup() {
  echo ""
  echo "=== Cleaning up ==="
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

prepare_peer_dir() {
  echo "=== Preparing peer (REPL/CLI owner) directory ==="
  rm -rf "${WORK}"
  mkdir -p "${PEER_DIR}" "${PEER_DATA}"
  printf 'peer_seed' > "${PEER_DIR}/seed.txt"

  # The Linux desktop app keeps its engine identity (and its TOFU records for
  # the peer) in ~/.local/share/ferrisync/ferrisync across runs. The peer's
  # identity is regenerated fresh every run, so clear the app's engine data to
  # keep both sides consistent; otherwise the app's stale peer fingerprint
  # trips the sync's strict TOFU check.
  rm -rf "${HOME}/.local/share/ferrisync/ferrisync"
}

# Wait for `pat` in the serve transcript; return 1 on timeout.
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

# Answer a serve prompt by writing `y` into the PTY's stdin.
answer_yes() {
  printf 'y\n' >&"${APP_FD}"
}

start_peer_serve() {
  echo "=== Starting REPL/CLI peer serve (interactive, ${PEER_PORT}) ==="
  # `add` publishes the folder so the app can browse + request a folder pair.
  "${HOST_BINARY}" --data-dir "${PEER_DATA}" add "${PEER_DIR}" --name PeerShared

  # Serve under a PTY so PairPolicy::Confirm is used. `script` forwards our
  # FIFO as the pty's stdin; read_yes_no() prompts are answered via answer_yes.
  mkfifo "${APP_IN}"
  # Open the FIFO read-write so `open` does not block waiting for script's
  # reader before script has launched (write-only would deadlock here).
  exec 7<>"${APP_IN}"; APP_FD=7
  (script -qefc "${HOST_BINARY} --data-dir ${PEER_DATA} serve --port ${PEER_PORT} ${PEER_DIR}" \
      /dev/null < "${APP_IN}" > "${SERVE_LOG}" 2>&1) &
  PEER_PID=$!

  wait_serve "Serving folder" 20
  echo "  peer serve is up (pid ${PEER_PID})."
}

run_integration_test() {
  echo "=== Running Linux Flutter folder-pair suite ==="
  cd "${PROJECT_ROOT}/ferrisync-flutter"

  # The app under test (tester) drives its own actions; this process only
  # needs to answer the peer's prompts as they surface during the run. Run the
  # suite first in the background while we babysit the peer transcript.
  flutter test integration_test/folder_pair_flow_ui_test.dart -d linux \
      --dart-define=FERRISYNC_PORT="${PEER_PORT}" \
      --dart-define=FERRISYNC_PEER_DIR="${PEER_DIR}" \
      > "/tmp/flutter_repl_folder_pair.log" 2>&1 &
  local flutter_pid=$!

  # Answer each prompt type exactly once as it appears (device-pairing then
  # folder-pairing), using lock files so repeat re-polls don't re-answer.
  local dev_lk="${WORK}/.answered_dev"
  local fold_lk="${WORK}/.answered_fold"
  local current_answered=""
  for _ in $(seq 1 400); do
    if [ ! -f "${dev_lk}" ] && grep -q "Confirm connection" "${SERVE_LOG}" 2>/dev/null; then
      answer_yes; touch "${dev_lk}"; current_answered="${current_answered} device"
    fi
    if [ ! -f "${fold_lk}" ] && grep -q "Confirm pairing" "${SERVE_LOG}" 2>/dev/null; then
      answer_yes; touch "${fold_lk}"; current_answered="${current_answered} folder"
    fi
    # Once the folder prompt is answered, hand back to the flutter test.
    if [ -f "${fold_lk}" ]; then
      break
    fi
    # Stop if the flutter test finished (avoids hanging).
    if ! kill -0 "${flutter_pid}" 2>/dev/null; then
      break
    fi
    sleep 0.5
  done
  echo "${PASS} approved prompt(s):${current_answered}"
  if [ ! -f "${fold_lk}" ]; then
    echo "--- serve log tail ---"; tail -40 "${SERVE_LOG}"
    echo "${FAIL} peer never raised the folder-pairing prompt"
    return 1
  fi

  if ! wait "${flutter_pid}"; then
    echo "${FAIL} folder_pair_flow_ui_test.dart failed"
    echo "--- serve log (peer) ---"; tail -30 "${SERVE_LOG}"
    echo "--- flutter log ---"; cat "/tmp/flutter_repl_folder_pair.log"
    return 1
  fi
  echo "${PASS} folder_pair_flow_ui_test.dart"
}

verify_peer_approval() {
  echo "=== Verifying peer-side approval ==="
  local ok=1
  if grep -q "Approved '" "${SERVE_LOG}"; then
    echo "  ${PASS} serve log shows folder-pair approval"
  else
    echo "  ${FAIL} no approval in serve log"
    ok=0
  fi
  [ "${ok}" -eq 1 ]
}

main() {
  build_binaries
  build_flutter_linux
  prepare_peer_dir
  start_peer_serve
  run_integration_test
  verify_peer_approval
  echo ""
  echo "${PASS} Linux Flutter <-> REPL/CLI folder pair OK"
}

main "$@"