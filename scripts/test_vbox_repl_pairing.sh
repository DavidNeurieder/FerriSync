#!/usr/bin/env bash
# VirtualBox REPL<->REPL LAN system test (fully automatic).
#
# Provisions a minimal Ubuntu VM from the official cloud image once
# (scripts/setup_vbox_base.sh, snapshot "clean"), then per run:
#
#   - linked-clones it, boots it headless (static IP 192.168.56.10)
#   - pushes the static-musl ferrisync binary into the guest over ssh
#   - Phase 1: HOST serves  -> GUEST pairs (consent pop-up on host) -> sync
#   - Phase 2: FRESH-identity GUEST REPL serves -> HOST pairs (consent pop-up
#              on the guest; the fresh identity knows nothing of phase 1, so
#              this is true stranger-to-stranger consent in reverse)
#              -> sync
#
# Both REPLs are driven by piping commands into their stdin and asserting on
# their transcripts; all traffic rides the VirtualBox host-only network.
set -euo pipefail

PROJECT_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "${PROJECT_ROOT}"

BASE_VM="${VBOX_BASE_NAME:-ferrisync-base}"
HOSTONLY_IF="${HOSTONLY_IF:-vboxnet0}"
GUEST_IP="${GUEST_IP:-192.168.56.10}"
SSH_USER="fs"
SSH_KEY="$HOME/.ssh/id_ed25519"
BIN_MUSL="target/x86_64-unknown-linux-musl/release/ferrisync"

HOST_LAN_IP="${HOST_LAN_IP:-}"
RUN_VM="fs-run-$(date +%s)"
WORK="$(mktemp -d /tmp/vbox_pair_test.XXXXXX)"

H_IN="${WORK}/host.in"; H_OUT="${WORK}/host.out"; H_ERR="${WORK}/host.err"
G_IN="${WORK}/guest.in"; G_OUT="${WORK}/guest.out"; G_ERR="${WORK}/guest.err"
G2_IN="${WORK}/guest2.in"; G2_OUT="${WORK}/guest2.out"; G2_ERR="${WORK}/guest2.err"

HOST_SERVE_DIR="${WORK}/host-served"
HOST_DATA_DIR="${WORK}/host-data"
GUEST_SERVE_DIR="/home/${SSH_USER}/vbox-test/served"
GUEST_SYNC_DIR="/home/${SSH_USER}/vbox-test/fold"
GUEST_DATA_DIR="/home/${SSH_USER}/vbox-test/data"
GUEST_SERVE2_DIR="/home/${SSH_USER}/vbox-test/served2"
GUEST_DATA2_DIR="/home/${SSH_USER}/vbox-test/data2"

SSH_OPTS=(-o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null -o BatchMode=yes
          -o LogLevel=ERROR -i "${SSH_KEY}")

H_PID="" G_PID="" G2_PID="" H_FD="" G_FD="" G2_FD=""
PORT_NEXT=19890
TESTS=0 PASSED=0
PASS="PASS" FAIL="FAIL"

cleanup() {
  echo ""
  echo "=== Cleaning up ==="
  [ -n "${H_FD}" ] && eval "exec ${H_FD}>&-" 2>/dev/null || true
  [ -n "${G_FD}" ] && eval "exec ${G_FD}>&-" 2>/dev/null || true
  [ -n "${G2_FD}" ] && eval "exec ${G2_FD}>&-" 2>/dev/null || true
  [ -n "${H_PID}" ] && kill "${H_PID}" 2>/dev/null || true
  [ -n "${G_PID}" ] && kill "${G_PID}" 2>/dev/null || true
  [ -n "${G2_PID}" ] && kill "${G2_PID}" 2>/dev/null || true
  if VBoxManage showvminfo "${RUN_VM}" --machinereadable 2>/dev/null | grep -q 'VMState='; then
    VBoxManage controlvm "${RUN_VM}" poweroff > /dev/null 2>&1 || true
    for _ in $(seq 30); do
      VBoxManage showvminfo "${RUN_VM}" --machinereadable 2>/dev/null | grep -q 'VMState="poweroff"' && break
      sleep 2
    done
    VBoxManage unregistervm "${RUN_VM}" --delete > /dev/null 2>&1 || true
    echo "Clone VM '${RUN_VM}' destroyed."
  fi
  rm -rf "${WORK}"
}
trap cleanup EXIT

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

repl_send_h() { printf '%s\n' "$*" >&"${H_FD}"; }
repl_send_g() { printf '%s\n' "$*" >&"${G_FD}"; }
repl_send_g2() { printf '%s\n' "$*" >&"${G2_FD}"; }

# Poll any transcript for a fixed string.
wait_file() {
  local f="$1" pat="$2" timeout="${3:-30}"
  local start=$((SECONDS))
  until grep -qF -- "${pat}" "${f}" 2>/dev/null; do
    if [ $((SECONDS - start)) -ge "${timeout}" ]; then
      echo "  TIMEOUT waiting for: ${pat} in $(basename "${f}")"
      echo "--- transcript ---"; tail -40 "${f}" 2>/dev/null || true
      return 1
    fi
    sleep 0.3
  done
}

# Poll any transcript for an ERE line.
wait_line() {
  local f="$1" ere="$2" timeout="${3:-30}"
  local start=$((SECONDS))
  until grep -Eq -- "${ere}" "${f}" 2>/dev/null; do
    if [ $((SECONDS - start)) -ge "${timeout}" ]; then
      echo "  TIMEOUT waiting for ERE: ${ere} in $(basename "${f}")"
      echo "--- transcript ---"; tail -40 "${f}" 2>/dev/null || true
      return 1
    fi
    sleep 0.3
  done
}

wait_proc_gone() {
  local pid="$1" timeout="${2:-15}"
  local start=$((SECONDS))
  while kill -0 "${pid}" 2>/dev/null; do
    if [ $((SECONDS - start)) -ge "${timeout}" ]; then return 1; fi
    sleep 0.5
  done
}

# Monotonic allocation: sets <VARNAME> in the caller; never repeats a port.
alloc_port() {
  local __var=$1
  while :; do
    PORT_NEXT=$((PORT_NEXT + 1))
    if ! (exec 3<>"/dev/tcp/127.0.0.1/${PORT_NEXT}") 2>/dev/null; then
      exec 3>&- 3<&- 2>/dev/null || true
      printf -v "${__var}" '%s' "${PORT_NEXT}"
      return 0
    fi
    exec 3>&- 3<&- 2>/dev/null || true
  done
}

ensure_base_vm() {
  if VBoxManage list vms | grep -q "\"${BASE_VM}\"" &&
     VBoxManage snapshot "${BASE_VM}" list 2>/dev/null | grep -q 'clean'; then
    echo "Base VM present."
    return 0
  fi
  echo "Provisioning base VM (one-time, automatic)..."
  ./scripts/setup_vbox_base.sh
}

build_musl_binary() {
  if [ ! -x "${BIN_MUSL}" ]; then
    echo "Building static musl binary..."
    CC_x86_64_unknown_linux_musl=gcc AR_x86_64_unknown_linux_musl=ar \
      CFLAGS_x86_64_unknown_linux_musl="-DSQLITE_DISABLE_LFS" \
      cargo build -q -p ferrisync --target x86_64-unknown-linux-musl --release
  fi
  [ -x "${BIN_MUSL}" ]
}

start_clone_and_ssh() {
  echo "=== Cloning + booting ${RUN_VM} ==="
  if VBoxManage showvminfo "${BASE_VM}" --machinereadable 2>/dev/null | grep -q 'VMState="running"'; then
    VBoxManage controlvm "${BASE_VM}" poweroff > /dev/null 2>&1 || true
    for _ in $(seq 30); do
      VBoxManage showvminfo "${BASE_VM}" --machinereadable 2>/dev/null | grep -q 'VMState="poweroff"' && break
      sleep 2
    done
  fi
  VBoxManage clonevm "${BASE_VM}" --snapshot clean --options link --name "${RUN_VM}" --register > /dev/null
  VBoxManage startvm "${RUN_VM}" --type headless > /dev/null
  local start=$((SECONDS))
  echo -n "Waiting for ssh at ${GUEST_IP}:22"
  until (exec 3<>"/dev/tcp/${GUEST_IP}/22") 2>/dev/null; do
    exec 3>&- 3<&- 2>/dev/null || true
    if [ $((SECONDS - start)) -ge 300 ]; then
      echo ""; echo "ERROR: clone never became reachable"
      return 1
    fi
    printf '.'; sleep 3
  done
  exec 3>&- 3<&- 2>/dev/null || true
  echo ""
  # Wait for userspace to settle after sshd is up.
  ssh "${SSH_OPTS[@]}" "${SSH_USER}@${GUEST_IP}" true
}

main() {
  if [ -z "${HOST_LAN_IP}" ]; then
    HOST_LAN_IP=$(VBoxManage list hostonlyifs | awk -v if_="${HOSTONLY_IF}" '
      $0 ~ "^Name:[[:space:]]*" if_ "$" {in_if=1; next}
      /^Name:/ {in_if=0}
      in_if && /^IPAddress:/ {print $2; exit}')
  fi
  if [ -z "${HOST_LAN_IP}" ] || [ ! -d "/sys/class/net/${HOSTONLY_IF}" ]; then
    local dev
    dev=$(ip route get 1.1.1.1 2>/dev/null | awk '{for(i=1;i<=NF;i++) if($i=="dev"){print $(i+1); break}}')
    HOST_LAN_IP=$(ip -4 -o addr show dev "${dev}" scope global 2>/dev/null | awk '{print $4}' | cut -d/ -f1 | head -1)
  fi
  [ -n "${HOST_LAN_IP}" ] || { echo "ERROR: cannot detect host LAN IP"; exit 1; }
  echo "Host LAN IP: ${HOST_LAN_IP}   Guest IP: ${GUEST_IP}"

  local PORT_P PORT_Q
  alloc_port PORT_P; alloc_port PORT_Q
  echo "Ports: phase1=${PORT_P} phase2=${PORT_Q}"

  checked "base VM provisioned (snapshot clean)" ensure_base_vm
  checked "static musl binary available" build_musl_binary
  checked "clone booted + ssh reachable" start_clone_and_ssh

  echo "=== Pushing binary to guest ==="
  ssh "${SSH_OPTS[@]}" "${SSH_USER}@${GUEST_IP}" "rm -rf ~/vbox-test; mkdir -p ~/vbox-test/data"
  scp "${SSH_OPTS[@]}" "${BIN_MUSL}" "${SSH_USER}@${GUEST_IP}:~/ferrisync" > /dev/null
  ssh "${SSH_OPTS[@]}" "${SSH_USER}@${GUEST_IP}" "chmod 755 ~/ferrisync"

  mkdir -p "${HOST_SERVE_DIR}" "${HOST_DATA_DIR}"
  mkfifo "${H_IN}" "${G_IN}"

  ############################## PHASE 1 ####################################
  echo ""
  echo "=== PHASE 1: host serves, guest pairs ==="
  # stdin here is a FIFO, far from any terminal; a bare invocation opens
  # the shell regardless (no subcommand = REPL).
  "${BIN_MUSL}" --data-dir "${HOST_DATA_DIR}" < "${H_IN}" > "${H_OUT}" 2> "${H_ERR}" &
  H_PID=$!
  exec 3> "${H_IN}"; H_FD=3
  sleep 0.3

  repl_send_h "serve ${HOST_SERVE_DIR} --port ${PORT_P}"
  checked "P1: host serve started" wait_file "${H_OUT}" "serve #1 started" 20

  ssh "${SSH_OPTS[@]}" "${SSH_USER}@${GUEST_IP}" \
      "\"\$HOME/ferrisync\" --data-dir \"\$HOME/vbox-test/data\"" < "${G_IN}" > "${G_OUT}" 2> "${G_ERR}" &
  G_PID=$!
  exec 4> "${G_IN}"; G_FD=4
  sleep 1

  repl_send_g "pair ${HOST_LAN_IP} --port ${PORT_P}"
  checked "P1: PAIRING REQUEST pop-up on host" wait_file "${H_OUT}" "PAIRING REQUEST — confirm connection with '" 30

  repl_send_h "pendings"
  checked "P1: pendings lists held request" wait_line "${H_OUT}" '^[[:space:]]{2}1[[:space:]]{2}[^ ]+ \([0-9a-f-]{36}\)$' 15

  repl_send_h "confirm 1"
  checked "P1: host approval reported" wait_file "${H_OUT}" "approved '" 15
  checked "P1: guest saw 'Paired with'" wait_file "${G_OUT}" "Paired with" 30

  local host_content="host-p1-$RANDOM" guest_content="guest-p1-$RANDOM"
  echo "${host_content}" > "${HOST_SERVE_DIR}/host_file.txt"
  ssh "${SSH_OPTS[@]}" "${SSH_USER}@${GUEST_IP}" "mkdir -p ~/vbox-test/fold; echo '${guest_content}' > ~/vbox-test/fold/guest_file.txt"

  repl_send_g "sync ${GUEST_SYNC_DIR} --device ${HOST_LAN_IP}:${PORT_P}"
  checked "P1: guest reported Sync complete" wait_file "${G_OUT}" "Sync complete." 30
  checked "P1: host logged receipt of guest file" wait_file "${H_OUT}" "pulled guest_file.txt" 30
  checked "P1: host logged send of host file" wait_file "${H_OUT}" "pushed host_file.txt" 30

  local got_guest got_host
  got_guest=$(cat "${HOST_SERVE_DIR}/guest_file.txt" 2>/dev/null || echo "<missing>")
  got_host=$(ssh "${SSH_OPTS[@]}" "${SSH_USER}@${GUEST_IP}" "cat ~/vbox-test/fold/host_file.txt 2>/dev/null" || true)
  checked "P1: guest -> host content matches" [ "${got_guest}" = "${guest_content}" ]
  checked "P1: host -> guest content matches" [ "${got_host}" = "${host_content}" ]

  ############################## PHASE 2 ####################################
  echo ""
  echo "=== PHASE 2: fresh-identity guest serves, host pairs (reverse consent) ==="
  ssh "${SSH_OPTS[@]}" "${SSH_USER}@${GUEST_IP}" \
      "mkdir -p ${GUEST_SERVE2_DIR} ${GUEST_DATA2_DIR}; echo 'seeded-by-guest' > ${GUEST_SERVE2_DIR}/guest_only.txt"

  mkfifo "${G2_IN}"
  ssh "${SSH_OPTS[@]}" "${SSH_USER}@${GUEST_IP}" \
      "\"\$HOME/ferrisync\" --data-dir \"${GUEST_DATA2_DIR}\"" < "${G2_IN}" > "${G2_OUT}" 2> "${G2_ERR}" &
  G2_PID=$!
  exec 5> "${G2_IN}"; G2_FD=5
  sleep 1

  repl_send_g2 "serve ${GUEST_SERVE2_DIR} --port ${PORT_Q}"
  checked "P2: guest serve started" wait_file "${G2_OUT}" "serve #1 started" 20

  repl_send_h "pair ${GUEST_IP} --port ${PORT_Q}"
  checked "P2: PAIRING REQUEST pop-up on guest" wait_file "${G2_OUT}" "PAIRING REQUEST — confirm connection with '" 30

  repl_send_g2 "confirm 1"
  checked "P2: guest approval reported" wait_file "${G2_OUT}" "approved '" 15
  checked "P2: host saw 'Paired with'" wait_file "${H_OUT}" "Paired with" 30

  repl_send_h "sync ${HOST_SERVE_DIR} --device ${GUEST_IP}:${PORT_Q}"
  # Server-side per-file events land in the GUEST transcript.
  checked "P2: guest logged receipt of host file" wait_line "${G2_OUT}" '\[serve:.*\] pulled host_file\.txt <- remote$' 30
  checked "P2: guest logged send of its file" wait_line "${G2_OUT}" '\[serve:.*\] pushed guest_only\.txt -> remote$' 30

  local got_p2
  got_p2=$(cat "${HOST_SERVE_DIR}/guest_only.txt" 2>/dev/null || echo "<missing>")
  checked "P2: guest -> host content matches" [ "${got_p2}" = "seeded-by-guest" ]

  echo ""
  echo "=== Shutting down all REPLs ==="
  repl_send_g2 "exit"
  checked "P-exit: guest2 stopped server" wait_line "${G2_OUT}" 'server #1 stopped' 15
  checked "P-exit: guest2 REPL exited" wait_proc_gone "${G2_PID}" 15
  repl_send_g "exit"
  checked "P-exit: guest REPL exited" wait_proc_gone "${G_PID}" 15
  repl_send_h "exit"
  checked "P-exit: host stopped server" wait_line "${H_OUT}" 'server #1 stopped' 15
  checked "P-exit: host REPL exited" wait_proc_gone "${H_PID}" 15

  echo ""
  if [ "${TESTS}" -gt 0 ] && [ "${PASSED}" -eq "${TESTS}" ]; then
    echo "=== RESULT: ALL ${TESTS} CHECKS PASSED ==="
  else
    echo "=== RESULT: ${PASSED}/${TESTS} checks passed ==="
    exit 1
  fi
}

main "$@"
