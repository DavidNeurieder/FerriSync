#!/usr/bin/env bash
# Linux Flutter <-> REPL auto-repair integration test.
#
# Starts a real `ferrisync serve` process (auto-accept, since its stdin is not
# a TTY) as the already-running peer, then runs the Flutter Linux integration
# suite auto_repair_ui_test.dart. That suite pairs with the peer, enables
# auto-repair, re-initializes the engine (simulating an app restart while the
# peer is still up), and asserts the startup pass re-pairs the known device.
#
# The serve process is left running the whole time so the restart finds the
# peer reachable at its last-known address — exactly the "flutter linux did not
# pair again with the running repl cli" scenario.
set -euo pipefail

PROJECT_ROOT="/home/mr/Projects/FerriSync"
HOST_BINARY="${PROJECT_ROOT}/target/debug/ferrisync"
SERVE_PORT=19898
SERVE_DIR="/tmp/test_linux_flutter_auto_repair/served"
DATA_DIR="/tmp/test_linux_flutter_auto_repair/peer-data"

PASS="PASS"
FAIL="FAIL"

cleanup() {
  echo ""
  echo "=== Cleaning up ==="
  kill ${SERVE_PID:-} 2>/dev/null || true
  wait ${SERVE_PID:-} 2>/dev/null || true
  rm -rf /tmp/test_linux_flutter_auto_repair 2>/dev/null || true
  echo "Cleanup done."
}
trap cleanup EXIT

build_binaries() {
  echo "Building host binary..."
  cd "${PROJECT_ROOT}" && cargo build -p ferrisync --quiet
}

build_flutter_linux() {
  echo "Building Flutter Linux desktop..."
  cd "${PROJECT_ROOT}/ferrisync-flutter" && flutter build linux --release --quiet 2>/dev/null
}

start_serve() {
  echo "=== Starting serve on host (port ${SERVE_PORT}) ==="
  rm -rf "${SERVE_DIR}" "${DATA_DIR}"
  mkdir -p "${SERVE_DIR}"
  printf 'peer_seed' > "${SERVE_DIR}/seed.txt"

  SERVE_LOG="/tmp/test_linux_flutter_auto_repair_serve.log"
  rm -f "${SERVE_LOG}"
  ${HOST_BINARY} --data-dir "${DATA_DIR}" serve --port ${SERVE_PORT} "${SERVE_DIR}" \
    < /dev/null > "${SERVE_LOG}" 2>&1 &
  SERVE_PID=$!

  local ready=0
  for _ in $(seq 1 50); do
    if ! kill -0 "${SERVE_PID}" 2>/dev/null; then
      echo "ERROR: serve process exited during startup:" >&2
      cat "${SERVE_LOG}" >&2
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

run_integration_test() {
  echo "=== Running auto-repair integration test ==="
  cd "${PROJECT_ROOT}/ferrisync-flutter"
  if flutter test integration_test/auto_repair_ui_test.dart -d linux \
      --dart-define=FERRISYNC_PORT="${SERVE_PORT}" \
      > /tmp/flutter_auto_repair.log 2>&1; then
    echo "${PASS} auto_repair_ui_test.dart"
  else
    echo "${FAIL} auto_repair_ui_test.dart"
    echo "--- flutter log ---"; cat /tmp/flutter_auto_repair.log
    return 1
  fi

  # The serve process should have logged a re-pair handshake from the app
  # (auto-accept serve prints "[serve] paired with <name>" on DevicePaired).
  echo "=== Verifying peer observed the re-pair over TLS ==="
  if grep -q "paired with" "${SERVE_LOG}"; then
    echo "${PASS} serve log shows a re-pair from the app"
  else
    echo "${FAIL} no 'paired with' in serve log"
    echo "--- serve log ---"; cat "${SERVE_LOG}"
    exit 1
  fi
}

main() {
  build_binaries
  build_flutter_linux
  start_serve
  run_integration_test
  echo ""
  echo "${PASS} Linux Flutter auto-repair OK"
}

main "$@"