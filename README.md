# FerriSync

**Private, peer-to-peer file synchronization over your local network.**

FerriSync is an open-source, LAN-first synchronization system that lets your devices synchronize folders directly — **without cloud storage, accounts, or a central server.**

> 🚧 **Developer Preview**
>
> FerriSync is in early development and intended for developers and technical
> early adopters. APIs, protocol, configuration, and synchronization behavior
> may change.

---

## Why FerriSync?

Most file synchronization services rely on a cloud service as the middleman:

```text
Device A → Cloud → Device B
```

FerriSync takes a different approach:

```text
Device A ←────────→ Device B
              LAN
```

Your devices discover and authenticate each other, then synchronize directly
over the local network.

**No cloud. No account. No central synchronization server.**

---

## Current status

| Feature                        | Status |
| ------------------------------ | ------ |
| LAN device discovery (mDNS)    | ✅      |
| Device pairing (TOFU + consent)| ✅      |
| Encrypted transfers (TLS 1.3)  | ✅      |
| Bidirectional folder sync      | ✅      |
| Conflict handling (.bak)       | ✅      |
| File watching                  | ✅      |
| CLI                            | ✅      |
| Interactive shell (REPL)       | ✅      |
| Android client (Flutter)       | 🚧 usable |
| Device rename across frontends | ✅      |
| Session history                | ✅      |
| Packaged releases / installers | ❌      |
| Remote (non-LAN) sync          | ❌      |
| iOS client                     | ❌      |
| Production-ready UX            | 🚧      |

This table is a snapshot of reality, not marketing.

---

## Why another sync tool?

There are already excellent synchronization tools, including
[Syncthing](https://syncthing.net/). FerriSync explores a different approach to
the synchronization experience, with a strong focus on LAN-first operation and
simple device discovery and pairing:

- **LAN-first operation** — designed around the local network, not the internet
- **Simple discovery and pairing** — discover, tap, confirm
- **A reusable core** — `ferrisync-core` behind CLI, REPL, and mobile clients
- **Developer-friendly architecture** — small crates, black-box test scripts

Honest shortcomings today: no packaged releases, no NAT traversal / remote
sync, single-platform mobile client, evolving UX.

---

## Roadmap

### Developer Preview

- [x] LAN discovery
- [x] Device pairing (TOFU + consent flow)
- [x] Bidirectional sync with conflict handling
- [x] Android prototype

### Next

- [ ] Packaged releases & installers
- [ ] Improved conflict-resolution UX
- [ ] Improved mobile UX
- [ ] Headless daemon with system tray

### Exploring

- [ ] Remote synchronization (QUIC transport)
- [ ] iOS client
- [ ] Desktop GUI
- [ ] Version vectors · block-level incremental sync

---

## Quick start

**The fastest way to evaluate FerriSync is to run it on two machines connected
to the same LAN.**

FerriSync currently has no packaged releases — you need to build from source.
Requirements: Rust 1.80+ (`rustup`), optionally Flutter for the Android client.

```bash
git clone https://github.com/DavidNeurieder/FerriSync.git
cd FerriSync
cargo build --release
```

### Minimal path — two machines on the same LAN

```text
# Interactive shell (REPL) — no argument needed
./target/release/ferrisync

Machine A:
serve ~/Photos                             # host a folder (default port 9847)

Machine B:
discover                                   # list FerriSync devices on the LAN
pair <A's address>                         # A confirms the request
rename "Desktop"                           # give A a friendly name (once)
sync ~/PhotosCopy --device "Desktop"       # one-shot bidirectional sync by name
watch ~/PhotosCopy --device "Desktop"      # ...or keep syncing on every change
```

Once paired, address devices by **name**, **UUID**, or `ip[:port]` — so after
`rename`, `sync` and `watch` need no IP addresses at all.

> **Advanced networking:** if you prefer (or pairing discovery is blocked by
> your network), you can address a device directly by IP:
> `sync ~/PhotosCopy --device 192.168.1.42:9847`.

### Phone ↔ PC

Build and install the Android client (see [Flutter](#flutter-android)), then:

1. Start `serve` on the PC (or open the folder in the phone app)
2. In the app: discover → tap pair → approve the request on the PC
3. Pick folders on either side and sync

---

## Using it

### Interactive shell (REPL)

`ferrisync` with no arguments is a persistent shell — servers, watches, and
session history live across commands:

```text
serve ~/Documents [--port 7000]   host a folder for pairing + sync
serves / unserve <id>             list / stop background servers
pendings                          devices waiting for pairing approval
confirm <n> / deny <n>            resolve held pairing requests
discover [seconds]                scan the LAN for peers
pair <ip> [--port <p>]            initiate pairing
sync [folder --device <dev>] [--wait secs]
watch <folder> --device <dev>     sync on every change (background)
watches / unwatch <id>
devices / folders                 list devices (with presence) / folders (with health)
activity                          recent sessions + file changes
conflicts                         unresolved conflict backups
doctor                            on-device diagnostics
sessions                          recent sync sessions (both directions)
rename <name>                     change this device's network name
status                            paired devices + folders (presence + health)
```

The shell prints a folder-centric startup dashboard — this device, the
"how is everything?" headline, then devices and folders with presence/health:

```text
FerriSync · Mr Desktop
✓ Everything is synced
Devices
  ● Pixel 9    · connected
  ● Laptop     · connected
Folders
  ✓ ~/Photos        ↔ Pixel 9 — healthy, last sync: 2 minutes ago
  ✓ ~/Documents     ↔ Laptop   — healthy, last sync: 14 minutes ago
```

### One-shot commands

```bash
cargo run -p ferrisync -- status
cargo run -p ferrisync -- status --verbose        # ids + absolute timestamps
cargo run -p ferrisync -- status --json           # machine-readable health
cargo run -p ferrisync -- devices                 # paired devices + presence
cargo run -p ferrisync -- devices discover        # scan the LAN
cargo run -p ferrisync -- devices pair            # interactive discovery+pair
cargo run -p ferrisync -- devices pair 192.168.1.42 --port 9847
cargo run -p ferrisync -- devices rename "Pixel 9" "PixelX"
cargo run -p ferrisync -- folders add ~/Docs --device "Pixel 9"
cargo run -p ferrisync -- folders remove ~/Docs
cargo run -p ferrisync -- activity                 # recent sessions + changes
cargo run -p ferrisync -- conflicts                # unresolved conflicts
cargo run -p ferrisync -- conflict-resolve notes.txt --keep this
cargo run -p ferrisync -- doctor                   # on-device diagnostics
cargo run -p ferrisync -- doctor --explain firewall
cargo run -p ferrisync -- sync ~/Documents --device "Pixel 9" --wait 30
cargo run -p ferrisync -- sync ~/Documents --device 192.168.1.42:9847  # by IP
cargo run -p ferrisync -- serve ~/Documents --auto-accept
cargo run -p ferrisync -- rename "Mr Desktop"
```

Every command also accepts `ferrisync --json <command>` (or places `--json`
after the command) to emit parseable output instead of human text.

Running `ferrisync` (no subcommand) with stdin piped feeds commands to the
same shell and exits on EOF — handy for scripting.

### Health vocabulary

All frontends share the same derived status language. A **device presence** is:

- **connected** — seen within the last 5 minutes
- **recently seen** — seen within the last 24 hours
- **offline** — silent for over a day (or never seen)

A configured **folder** gets a combined **health**:

- **healthy** — peer connected and the folder has synced before
- **syncing** — transferring right now
- **waiting** — peer is around but not connected, or never synced yet
- **offline** — peer fell off before the first sync
- **conflict** — has unresolved conflict backups
- **error** — the last sync attempt failed
- **not configured** — hosted locally, not yet attached to a remote

Folders that are `conflict`, `error`, or `offline` appear under `ATTENTION` in
`status` output.

### Pairing consent

Known devices connect instantly (TOFU). Unknown devices are held for approval:

- **REPL**: `[serve:...] PAIRING REQUEST` appears inline; answer `y`/`n`, or
  use `pendings` + `confirm <n>` / `deny <n>`
- **CLI serve**: prompts `Allow '<name>' to pair? [y/N]`; `--auto-accept`
  skips asking

> **Warning**: `--auto-accept` (also triggered implicitly by running `serve`
> without a TTY) trusts **any** device that can reach this host and grants it
> read/write access to the folder. Only use it on trusted, private networks —
> never on the public internet or untrusted Wi-Fi.

Denied devices stay rejected until that server restarts.

### Device names

Every frontend shares one persisted name (fallback: OS hostname). Renaming on
any side updates advertisements and pairing immediately — running folder
servers restart under the new name automatically.

---

## How it works

```text
                 ┌─────────────────┐
                 │  ferrisync-core │
                 │  engine · crypto│
                 │  protocol · io  │
                 └────────┬────────┘
          ┌───────────────┼───────────────┐
          │               │               │
         REPL            CLI           Flutter
   (interactive)      (commands)   (flutter_rust_bridge)
```

One reusable Rust core and one `ferrisync` binary; every interface is a thin
presentation layer over the same synchronization engine.

- **Discovery** — mDNS/DNS-SD advertisement and browsing on the LAN
- **Authentication** — trust on first use: certificates are generated locally,
  fingerprints pinned at pairing; unknown peers require operator consent
- **Transport** — TLS 1.3 over TCP (default port 9847)
- **Integrity & comparison** — BLAKE3 hashes detect content differences;
  conflicts currently use newer-mtime-wins with hash-based tie-breaking,
  preserving the overwritten side as `.bak`
- **Synchronization** — disk-index based comparison; missing files always pull,
  divergent files transfer exactly once

---

## Security model

FerriSync is designed to protect transfers from network eavesdropping and
unauthorized peers: TLS 1.3 on every connection, TOFU certificate pinning at
pairing, and explicit operator consent for unknown devices.

It currently **assumes the local device and operating system are trusted**.
In particular, FerriSync does not yet protect against:

- a paired device that is itself compromised
- malicious file *content* received from a trusted peer (no malware scanning)
- other software running on the same machine reading synced data

---

## Who should try this?

**FerriSync is currently for:**

- Rust developers and open-source contributors
- Homelab / self-hosting enthusiasts
- Privacy-focused developers
- People interested in peer-to-peer networking and sync systems

**FerriSync is not yet for:**

- Anyone expecting Dropbox-level polish
- Production-critical data
- People who don't want to build early-stage software from source

### What we're looking for

We're especially interested in developers who can test FerriSync across
multiple physical devices and provide feedback on the synchronization model,
the pairing flow, and failure cases.

---

## Testing

```bash
cargo test
```

System tests over real network sockets live in [`scripts/`](./scripts/):

| Script | What it exercises |
|---|---|
| `test_vbox_repl_pairing.sh` | REPL↔REPL over VirtualBox host-only NIC: consent pop-ups both directions, bidirectional sync |
| `test_lan_pairing_flow.sh` | Host REPL ↔ Android emulator over LAN/TAP (no adb forwards) |

### Firewall prerequisites (UFW)

With a default-deny incoming policy, every **device→host** connection (pairing,
sync, serving) is silently dropped. Host-initiated connections work regardless,
so failures show up as the peer hanging in its retry loop while nothing arrives.

The tests use two virtual networks; allow them explicitly:

```bash
# VirtualBox REPL<->REPL tests (host-only adapter vboxnet0, 192.168.56.0/24)
sudo ufw allow in on vboxnet0 comment 'ferrisync vbox tests'

# Android emulator TAP tests (tap0, 192.168.179.0/24)
sudo ufw allow in on tap0 comment 'ferrisync emu tests'
```

Equivalent subnet form: `sudo ufw allow from 192.168.56.0/24`. To tighten
further, restrict to the sync port range instead of whole interfaces:

```bash
sudo ufw allow in on vboxnet0 proto tcp to any port 19880:20000
```

mDNS needs no extra rule — UFW's default ruleset already permits UDP 5353.

Inspect or undo:

```bash
sudo ufw status numbered
sudo ufw delete <n>
```

## Flutter (Android)

```bash
cd ferrisync-flutter
flutter pub get
flutter_rust_bridge_codegen generate
flutter run
```

See [ferrisync-flutter/](./ferrisync-flutter/) for details.

### Installing on a physical device

`make test-android-flutter` builds an x86_64 emulator APK that will crash on an
arm64 phone (`MissingLibraryException: libflutter.so`). For real devices:

```bash
make install-phone   # universal APK (x86_64 + arm64) + adb install
# or just build:
make build-android-apk-universal
```

## Configuration

Data directory: `~/.local/share/ferrisync/` (Linux)

- `metadata.db` — SQLite database (devices, folders, file index, session history)
- `identity.crt` / `identity.key` — persisted TLS identity (TOFU pairing)
- `device.name` — user-chosen device name (optional; hostname fallback)
- File chunks capped at 64 KB

## Contributing

FerriSync is early and feedback is welcome.

Good places to start:

- Try the developer preview between two real devices
- Report bugs and rough edges
- Test synchronization across platforms
- Improve documentation
- Work on the CLI/REPL or the Flutter client
- Improve the synchronization core

**Issues and discussions are welcome.**

## License

AGPL-3.0
