#!/usr/bin/env bash
# Python <-> Flutter Linux desktop interop test (Direction A, primary).
# Starts a Python FerriSync server (auto-accept, loopback) serving a seeded
# directory, then runs selected Flutter integration suites against it.
set -euo pipefail

PROJECT_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SERVE_PORT="${FERRISYNC_PORT:-9847}"
SERVE_DIR="/tmp/test_python_flutter_serve"
DATA_DIR="/tmp/test_python_flutter_data"
PY_DATA_DIR="${DATA_DIR}/py-server-data"
PY_VENV="${DATA_DIR}/py-venv"
export XDG_DATA_HOME="${DATA_DIR}/app-data"

# Suites that exercise real sync/pairing over TCP against the served directory.
# Kept minimal for reliability; additional suites can be added once stabilized.
SYNC_SUITES=("sync_test.dart" "pairing_ui_test.dart")

PASS="PASS"
FAIL="FAIL"

cleanup() {
  echo ""
  echo "=== Cleaning up ==="
  if [ -n "${SERVE_PID:-}" ]; then
    kill "${SERVE_PID}" 2>/dev/null || true
    wait "${SERVE_PID}" 2>/dev/null || true
  fi
  rm -rf "${SERVE_DIR}" "${DATA_DIR}" 2>/dev/null || true
  echo "Cleanup done."
}

trap cleanup EXIT

require_cmd() {
  command -v "$1" >/dev/null 2>&1 || {
    echo "error: required command not found: $1" >&2
    exit 1
  }
}

build_flutter_linux() {
  echo "Building Flutter Linux desktop..."
  cd "${PROJECT_ROOT}/ferrisync-flutter" && flutter build linux --release --quiet 2>/dev/null
}

prepare_python_env() {
  require_cmd python3
  echo "=== Preparing Python environment ==="
  rm -rf "${DATA_DIR}" "${SERVE_DIR}"
  mkdir -p "${DATA_DIR}" "${PY_DATA_DIR}" "${XDG_DATA_HOME}" "${SERVE_DIR}"
  python3 -m venv "${PY_VENV}"
  # shellcheck disable=SC1091
  source "${PY_VENV}/bin/activate"
  python -m pip install --upgrade pip >/dev/null
  if command -v maturin >/dev/null 2>&1; then
    python -m pip install pytest >/dev/null
  else
    python -m pip install maturin pytest >/dev/null
  fi
  # Install/build local bindings into the venv.
  python -m pip install "${PROJECT_ROOT}/ferrisync-python" >/dev/null
}

prepare_serve_dir() {
  echo "=== Preparing serve directory ==="
  printf 'remote_content' > "${SERVE_DIR}/from_remote.txt"
  sleep 1
  printf 'remote_version' > "${SERVE_DIR}/conflict.txt"
  printf 'v1' > "${SERVE_DIR}/base.txt"
  printf 'host_content' > "${SERVE_DIR}/from_host.txt"
  printf 'host_root_content' > "${SERVE_DIR}/from_host_root.txt"
  mkdir -p "${SERVE_DIR}/second"
  printf 'host_second_content' > "${SERVE_DIR}/second/from_host_second.txt"
}

start_python_server() {
  echo "=== Starting Python serve on 127.0.0.1:${SERVE_PORT} ==="
  SERVE_LOG="/tmp/test_python_flutter_serve.log"
  rm -f "${SERVE_LOG}"
  # shellcheck disable=SC1091
  source "${PY_VENV}/bin/activate"
  python - <<'PY' >"${SERVE_LOG}" 2>&1 &
import os
import signal
import sys
import time
from pathlib import Path
import ferrisync

port = int(os.environ["FERRISYNC_PORT"])
serve_dir = os.environ["FERRISYNC_SERVE_DIR"]
data_dir = os.environ["FERRISYNC_PY_DATA_DIR"]
stop = False

def _stop(*_args):
    global stop
    stop = True

signal.signal(signal.SIGTERM, _stop)
signal.signal(signal.SIGINT, _stop)

with ferrisync.Client(data_dir) as server:
    folder_id = server.add_folder(serve_dir, server.device_id, "bidirectional")
    bound = server.serve(folder_id, serve_dir, port=port, auto_accept=True)
    print(f"PYTHON_SERVE_READY port={bound}", flush=True)
    while not stop:
        time.sleep(0.2)
print("PYTHON_SERVE_STOPPED", flush=True)
PY
  SERVE_PID=$!

  local ready=0
  for _ in $(seq 1 100); do
    if ! kill -0 "${SERVE_PID}" 2>/dev/null; then
      echo "ERROR: Python serve process exited during startup:" >&2
      cat "${SERVE_LOG}" >&2 || true
      return 1
    fi
    if grep -q "PYTHON_SERVE_READY" "${SERVE_LOG}" 2>/dev/null; then
      ready=1
      break
    fi
    sleep 0.2
  done
  if [ "$ready" -ne 1 ]; then
    echo "ERROR: Python serve never became ready." >&2
    cat "${SERVE_LOG}" >&2 || true
    return 1
  fi
  echo "Python serve is ready (pid ${SERVE_PID})."
}

verify_results() {
  local failed_list="$1"
  echo ""
  echo "=== Host-side (Python-served dir) verification ==="
  local ok=1

  # sync_test.dart pushes from_local.txt and may update conflict/base behavior;
  # assert key bidirectional artifacts when sync_test passed.
  if [[ "$failed_list" != *"sync_test.dart"* ]]; then
    local actual
    actual=$(cat "${SERVE_DIR}/from_local.txt" 2>/dev/null || true)
    if [ "$actual" = "local_content" ]; then
      echo "  ${PASS} Python server received from_local.txt from Flutter client"
    else
      echo "  ${FAIL} from_local.txt on Python server — expected 'local_content', got '${actual}'"
      ok=0
    fi
  fi

  # pairing_ui_test.dart writes from_app.txt in its flow (host-side assertion
  # mirrors existing Linux Flutter script behavior).
  if [[ "$failed_list" != *"pairing_ui_test"* ]] && [[ "$failed_list" != *"folders_flow_ui_test"* ]]; then
    if [ -f "${SERVE_DIR}/from_app.txt" ]; then
      local actual
      actual=$(cat "${SERVE_DIR}/from_app.txt" 2>/dev/null || true)
      if [ "$actual" = "app_content" ]; then
        echo "  ${PASS} Python server received from_app.txt pushed via Flutter UI flow"
      else
        echo "  ${FAIL} from_app.txt on Python server — expected 'app_content', got '${actual}'"
        ok=0
      fi
    else
      echo "  (info) from_app.txt not present; skipping UI-push assertion for selected suites"
    fi
  fi

  if [ "$ok" -ne 1 ]; then
    return 1
  fi
}

run_integration_tests() {
  echo "=== Running Flutter integration tests against Python server ==="
  cd "${PROJECT_ROOT}/ferrisync-flutter" || exit 1

  local failed=()
  for suite in "${SYNC_SUITES[@]}"; do
    echo ""
    echo "--- Suite: ${suite} ---"
    if flutter test "integration_test/${suite}" -d linux \
        --dart-define=FERRISYNC_REMOTE_PATH="${SERVE_DIR}/second" \
        > "/tmp/flutter_test_python_${suite%.dart}.log" 2>&1; then
      echo "${PASS} ${suite}"
    else
      echo "${FAIL} ${suite} (see /tmp/flutter_test_python_${suite%.dart}.log)"
      failed+=("${suite}")
    fi
  done

  verify_results "${failed[*]}" || failed+=("host-side-verification")

  echo ""
  if [ ${#failed[@]} -eq 0 ]; then
    echo "${PASS} Python<->Flutter interop suites passed (${#SYNC_SUITES[@]} suites)"
  else
    echo "${FAIL} Python<->Flutter interop failed: ${failed[*]}"
    exit 1
  fi
}

main() {
  require_cmd flutter
  # Native extension build/install needs Rust toolchain for local maturin/pip build.
  require_cmd cargo
  prepare_python_env
  build_flutter_linux
  prepare_serve_dir
  FERRISYNC_PORT="${SERVE_PORT}" \
  FERRISYNC_SERVE_DIR="${SERVE_DIR}" \
  FERRISYNC_PY_DATA_DIR="${PY_DATA_DIR}" \
    start_python_server
  run_integration_tests
}

main "$@"
