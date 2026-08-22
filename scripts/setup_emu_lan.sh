#!/usr/bin/env bash
# Dedicated point-to-point TAP segment for emulator LAN tests.
#
#   setup     create/refresh tap0 with 192.168.179.1/24 (idempotent)
#   teardown  remove tap0
#
# The emulator side gets 192.168.179.2 statically (configured over adb after
# boot); the pair talk over this direct L2 link with no adb involvement.
set -euo pipefail

TAP_IF="${TAP_IF:-tap0}"
TAP_CIDR="${TAP_CIDR:-192.168.179.1/24}"

as_root() {
  if [ "$(id -u)" = "0" ]; then
    "$@"
  else
    sudo "$@"
  fi
}

have_tap() {
  ip link show "${TAP_IF}" > /dev/null 2>&1
}

setup_tap() {
  if ! have_tap; then
    as_root ip tuntap add dev "${TAP_IF}" mode tap
  fi
  as_root ip address replace "${TAP_CIDR}" dev "${TAP_IF}"
  as_root ip link set "${TAP_IF}" up
  echo "TAP '${TAP_IF}' ready:"
  ip -4 address show "${TAP_IF}" | grep -o 'inet [0-9./]*' || true
}

teardown_tap() {
  if have_tap; then
    as_root ip tuntap del dev "${TAP_IF}" mode tap
    echo "TAP '${TAP_IF}' removed."
  else
    echo "TAP '${TAP_IF}' not present."
  fi
}

case "${1:-setup}" in
  setup)    setup_tap ;;
  teardown) teardown_tap ;;
  *)        echo "usage: $0 [setup|teardown]" >&2; exit 1 ;;
esac
