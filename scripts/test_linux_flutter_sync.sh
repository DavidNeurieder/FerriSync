#!/usr/bin/env bash
# Linux Flutter integration tests
# Starts a ferrisync-cli serve process on the host, then runs all Flutter
# integration suites as a native Linux desktop app.
set -euo pipefail

PROJECT_ROOT="/home/mr/Projects/FerriSync"
HOST_BINARY="${PROJECT_ROOT}/target/debug/ferrisync-cli"
SERVE_PORT=9847
SERVE_DIR="/tmp/test_linux_flutter_serve"
DATA_DIR="/tmp/test_linux_flutter_data"

# Suites that need a running serve process (real sync over TCP).
SYNC_SUITES=("sync_test.dart" "sync_incremental_test.dart" "pairing_ui_test.dart" "folders_flow_ui_test.dart")
# Suites that run standalone (no network needed).
STANDALONE_SUITES=("app_test.dart" "frb_smoke_test.dart")
# All suites in recommended order: standalone first (clean app state), then sync.
INTEGRATION_SUITES=("${STANDALONE_SUITES[@]}" "${SYNC_SUITES[@]}" "settings_notifications_test.dart")

PASS="PASS"
FAIL="FAIL"

cleanup() {
  echo ""
  echo "=== Cleaning up ==="
  kill ${SERVE_PID:-} 2>/dev/null || true
  wait ${SERVE_PID:-} 2>/dev/null || true
  rm -rf "${SERVE_DIR}" "${DATA_DIR}" 2>/dev/null || true
  echo "Cleanup done."
}

trap cleanup EXIT

build_binaries() {
  echo "Building host CLI..."
  cd "${PROJECT_ROOT}" && cargo build -p ferrisync-cli --quiet
}

build_flutter_linux() {
  echo "Building Flutter Linux desktop..."
  cd "${PROJECT_ROOT}/ferrisync-flutter" && flutter build linux --release --quiet 2>/dev/null
}

prepare_serve_dir() {
  echo "=== Preparing serve directory ==="
  rm -rf "${SERVE_DIR}" "${DATA_DIR}"
  mkdir -p "${SERVE_DIR}"

  printf 'remote_content' > "${SERVE_DIR}/from_remote.txt"
  sleep 1
  printf 'remote_version' > "${SERVE_DIR}/conflict.txt"
  printf 'v1' > "${SERVE_DIR}/base.txt"
  printf 'host_content' > "${SERVE_DIR}/from_host.txt"
}

start_serve() {
  echo "=== Starting serve on host (port ${SERVE_PORT}) ==="
  SERVE_LOG="/tmp/test_linux_flutter_serve.log"
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

verify_host_incremental_results() {
  local failed_list="$1"
  if [[ "$failed_list" == *"sync_incremental_test"* ]]; then
    echo "Skipping host-side incremental verification (suite did not pass)."
    return 0
  fi
  echo ""
  echo "=== Host-side verification of app-pushed changes ==="
  local ok=1
  check_host_file() {
    local desc="$1" expected="$2" path="$3"
    local actual
    actual=$(cat "$path" 2>/dev/null)
    if [ "$actual" = "$expected" ]; then
      echo "  ${PASS} ${desc}"
    else
      echo "  ${FAIL} ${desc} — expected '${expected}', got '${actual}'"
      ok=0
    fi
  }
  check_host_file "host base.txt updated by app"   "v2-from-app"  "${SERVE_DIR}/base.txt"
  # Fast-forward edit (the app inherits the serve version before editing), so
  # no conflict, no backup.
  check_host_file "host received app_new.txt"      "made-by-app"  "${SERVE_DIR}/app_new.txt"
  if [ "$ok" -ne 1 ]; then
    return 1
  fi
}

verify_host_ui_results() {
  local failed_list="$1"
  if [[ "$failed_list" == *"pairing_ui_test"* ]] || [[ "$failed_list" == *"folders_flow_ui_test"* ]]; then
    echo "Skipping host-side UI-suite verification (suites did not pass)."
    return 0
  fi
  echo ""
  echo "=== Host-side verification of UI-driven suites ==="
  local ok=1
  if grep -q "Pair request from" "${SERVE_LOG}" 2>/dev/null; then
    echo "  ${PASS} serve log shows app's pair request over TLS"
  else
    echo "  ${FAIL} no 'Pair request from' in ${SERVE_LOG}"
    ok=0
  fi
  local actual
  actual=$(cat "${SERVE_DIR}/from_app.txt" 2>/dev/null)
  if [ "$actual" = "app_content" ]; then
    echo "  ${PASS} host received from_app.txt pushed via Sync-now button"
  else
    echo "  ${FAIL} from_app.txt on host — expected 'app_content', got '${actual}'"
    ok=0
  fi
  if [ "$ok" -ne 1 ]; then
    return 1
  fi
}

run_integration_tests() {
  echo "=== Running Flutter integration tests on Linux desktop ==="
  cd "${PROJECT_ROOT}/ferrisync-flutter" || exit 1

  local failed=()
  for suite in "${INTEGRATION_SUITES[@]}"; do
    echo ""
    echo "--- Suite: ${suite} ---"
    if flutter test "integration_test/${suite}" -d linux \
        > "/tmp/flutter_test_${suite%.dart}.log" 2>&1; then
      echo "${PASS} ${suite}"
    else
      echo "${FAIL} ${suite} (see /tmp/flutter_test_${suite%.dart}.log)"
      failed+=("${suite}")
    fi
  done

  verify_host_incremental_results "${failed[*]}"
  verify_host_ui_results "${failed[*]}"

  echo ""
  if [ ${#failed[@]} -eq 0 ]; then
    echo "${PASS} all ${#INTEGRATION_SUITES[@]} integration suites passed"
  else
    echo "${FAIL} ${#failed[@]} of ${#INTEGRATION_SUITES[@]} integration suites failed: ${failed[*]}"
    exit 1
  fi
}

main() {
  build_binaries
  build_flutter_linux
  prepare_serve_dir
  start_serve
  run_integration_tests
}

main "$@"
