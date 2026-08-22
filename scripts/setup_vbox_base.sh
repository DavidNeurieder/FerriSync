#!/usr/bin/env bash
# One-command automatic bootstrap of the `ferrisync-base` VirtualBox VM.
#
# Creates a minimal Ubuntu Server VM from the official cloud image (no
# interactive installer), provisioned via cloud-init NoCloud seed:
#   - user `fs` authorized with this host's ~/.ssh/id_ed25519
#   - static IP on the VirtualBox host-only network (default 192.168.56.10)
#     so runs are deterministic and no physical LAN is involved
#
# After provisioning it shuts the VM down and takes snapshot "clean", which
# per-run tests linked-clone from. Idempotent: exits early when present.
#
# Env overrides: HOSTONLY_IF, GUEST_IP, VBOX_BASE_NAME
#
# NOTE: requires UFW (or another firewall) to allow inbound on the host-only
# adapter, or guest->host connections will be silently dropped. See
# "Firewall prerequisites" in README.md.
set -euo pipefail

VM_NAME="${VBOX_BASE_NAME:-ferrisync-base}"
SNAPSHOT="clean"
WORK="${HOME}/.cache/ferrisync-vbox"
BOX_URL="https://cloud-images.ubuntu.com/jammy/current/jammy-server-cloudimg-amd64.vmdk"
BOX_IMG="$WORK/jammy-server-cloudimg-amd64.vmdk"
VDI="$WORK/${VM_NAME}.vdi"
SEED="$WORK/seed.img"
KEY="$HOME/.ssh/id_ed25519"
SSH_USER="fs"
HOSTONLY_IF="${HOSTONLY_IF:-vboxnet0}"
GUEST_IP="${GUEST_IP:-192.168.56.10}"
MAC="080027535301"

need() { command -v "$1" > /dev/null || { echo "ERROR: missing tool: $1"; exit 1; }; }
need VBoxManage; need mkfs.vfat; need mcopy; need curl

vm_exists() { VBoxManage list vms | grep -q "\"${VM_NAME}\""; }
snapshot_exists() { VBoxManage showvminfo "${VM_NAME}" 2>/dev/null | grep -q "Snapshot.*${SNAPSHOT}\|Name: ${SNAPSHOT}"; }

if vm_exists && snapshot_exists; then
  echo "Base VM '${VM_NAME}' (snapshot ${SNAPSHOT}) already provisioned. Nothing to do."
  exit 0
fi

if vm_exists; then
  echo "ERROR: VM '${VM_NAME}' exists but has no '${SNAPSHOT}' snapshot."
  echo "Fix manually or remove it:  VBoxManage unregistervm ${VM_NAME} --delete"
  exit 1
fi

mkdir -p "${WORK}"

echo "=== SSH key ==="
if [ ! -f "${KEY}" ]; then
  ssh-keygen -t ed25519 -N "" -f "${KEY}" -C ferrisync-test
else
  echo "Using existing key ${KEY}"
fi
PUBKEY=$(cat "${KEY}.pub")

echo "=== Cloud image ==="
if [ ! -f "${BOX_IMG}" ]; then
  echo "Downloading Ubuntu jammy cloud image (VMDK, ~600MB)..."
  curl -fL --retry 3 -o "${BOX_IMG}" "${BOX_URL}"
else
  echo "Image already downloaded."
fi

echo "=== Disk (${VDI}) ==="
if [ ! -f "${VDI}" ]; then
  # Clear possibly-stale media registry entries from aborted runs.
  VBoxManage closemedium disk "${BOX_IMG}" > /dev/null 2>&1 || true
  VBoxManage closemedium disk "${VDI}" > /dev/null 2>&1 || true
  VBoxManage clonemedium disk "${BOX_IMG}" "${VDI}" --format VDI
fi

echo "=== Host-only network (${HOSTONLY_IF}) ==="
if ! VBoxManage list hostonlyifs | grep -q "^Name:\s*${HOSTONLY_IF}$"; then
  VBoxManage hostonlyif create > /dev/null
fi
VBoxManage hostonlyif ipconfig "${HOSTONLY_IF}" --ip 192.168.56.1 --netmask 255.255.255.0

echo "=== cloud-init seed (static IP ${GUEST_IP} on ${HOSTONLY_IF}) ==="
cat > "${WORK}/user-data" <<EOF
#cloud-config
hostname: ferrisync-test
manage_etc_hosts: true
users:
  - name: ${SSH_USER}
    sudo: ALL=(ALL) NOPASSWD:ALL
    groups: sudo
    shell: /bin/bash
    lock_passwd: true
    ssh_authorized_keys:
      - ${PUBKEY}
ssh_pwauth: false
chpasswd:
  expire: false
EOF
cat > "${WORK}/meta-data" <<EOF
instance-id: ferrisync-base-001
local-hostname: ferrisync-test
EOF
cat > "${WORK}/network-config" <<EOF
version: 2
ethernets:
  nic:
    match:
      name: "en*"
    dhcp4: false
    addresses: [${GUEST_IP}/24]
EOF
rm -f "${SEED}"
dd if=/dev/zero of="${SEED}" bs=1M count=64 status=none
mkfs.vfat -n CIDATA "${SEED}" > /dev/null
mcopy -i "${SEED}" -o "${WORK}/user-data" ::user-data
mcopy -i "${SEED}" -o "${WORK}/meta-data" ::meta-data
mcopy -i "${SEED}" -o "${WORK}/network-config" ::network-config
# Wrap the raw FAT image in a real VDI container (VBox rejects raw files).
rm -f "${SEED}.vdi"
VBoxManage closemedium disk "${SEED}.vdi" > /dev/null 2>&1 || true
VBoxManage convertfromraw "${SEED}" "${SEED}.vdi" --format VDI > /dev/null
SEED_MEDIUM="${SEED}.vdi"

echo "=== Creating VM ${VM_NAME} (host-only on ${HOSTONLY_IF}) ==="
VBoxManage createvm --name "${VM_NAME}" --ostype Ubuntu_64 --register > /dev/null
VBoxManage modifyvm "${VM_NAME}" \
  --memory 1024 --cpus 1 \
  --nic1 hostonly --host-only-adapter1 "${HOSTONLY_IF}" --macaddress1 "${MAC}" \
  --audio-driver none --usb off --boot1 disk
# Serial console log for debugging (best effort; syntax varies by VBox version).
VBoxManage modifyvm "${VM_NAME}" --uart-mode1 file "${WORK}/serial.log" > /dev/null 2>&1 || true
VBoxManage storagectl "${VM_NAME}" --name SATA --add sata --controller IntelAhci
VBoxManage storageattach "${VM_NAME}" --storagectl "SATA" --port 0 --device 0 \
  --type hdd --medium "${VDI}"
VBoxManage storageattach "${VM_NAME}" --storagectl "SATA" --port 1 --device 0 \
  --type hdd --medium "${SEED_MEDIUM}"

echo "=== First boot + provisioning (may take a few minutes) ==="
VBoxManage startvm "${VM_NAME}" --type headless > /dev/null

echo -n "Waiting for sshd at ${GUEST_IP}:22"
ok=""
for i in $(seq 120); do
  if (exec 3<>"/dev/tcp/${GUEST_IP}/22") 2>/dev/null; then ok=1; exec 3>&- 3<&- || true; break; fi
  echo -n "."; sleep 5
done
echo ""
[ -n "${ok}" ] || {
  echo "ERROR: VM never became reachable at ${GUEST_IP}. Debug:"
  echo "  VBoxManage showvminfo ${VM_NAME} | head -30"
  exit 1
}

echo "Waiting for cloud-init to finish..."
ssh -o StrictHostKeyChecking=accept-new -o UserKnownHostsFile="$WORK/known_hosts" \
    -i "${KEY}" "${SSH_USER}@${GUEST_IP}" \
    "sudo cloud-init status --wait" || {
  echo "ERROR: cloud-init did not complete cleanly."
  exit 1
}

echo "Shutting down + snapshot '${SNAPSHOT}'..."
VBoxManage controlvm "${VM_NAME}" acpipowerbutton > /dev/null
for i in $(seq 60); do
  VBoxManage showvminfo "${VM_NAME}" --machinereadable 2>/dev/null | grep -q 'VMState="poweroff"' && break
  sleep 2
done
VBoxManage snapshot "${VM_NAME}" take "${SNAPSHOT}" > /dev/null

echo "=== Base VM ready: '${VM_NAME}' @ ${GUEST_IP} (snapshot ${SNAPSHOT}) ==="
