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
| Full-screen TUI                | ✅      |
| Android client (Flutter)       | 🚧 usable |
| Device rename across frontends | ✅      |
| Session history                | ✅      |
| Packaged releases / installers | ❌      |
| Remote (non-LAN) sync          | ❌      |
| iOS client                     | ❌      |
| Production-ready UX            | 🚧      |

This table is a snapshot of reality, not marketing.

---

## Quick start

Requirements: Rust 1.80+ (`rustup`), optionally Flutter for the Android client.

```bash
git clone https://github.com/DavidNeurieder/FerriSync.git
cd FerriSync
cargo build --release
```

### Minimal path — two machines on the same LAN

```text
Machine A:
./target/release/ferrisync-tui            # opens the interactive shell
serve ~/Photos                             # host a folder (default port 9847)

Machine B:
./target/release/ferrisync-tui
pair <A's IP>                              # A confirms the request
sync ~/PhotosCopy --device <A's IP>        # one-shot bidirectional sync
watch ~/PhotosCopy --device <A's IP>       # ...or keep syncing on every change
```

Devices can be addressed by `ip[:port]`, paired name, or UUID.

### Phone ↔ PC

Build and install the Android client (see [Flutter](#flutter-android)), then:

1. Start `serve` on the PC (or open the folder in the phone app)
2. In the app: discover → tap pair → approve the request on the PC
3. Pick folders on either side and sync

---

## Using it

### Interactive shell (REPL)

`ferrisync-tui` with no arguments is a persistent shell — servers, watches, and
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
sessions                          recent sync sessions (both directions)
rename <name>                     change this device's network name
status                            paired devices + configured folders
```

### One-shot commands

```bash
cargo run -p ferrisync-tui -- status
cargo run -p ferrisync-tui -- pair 192.168.1.42 --port 9847
cargo run -p ferrisync-tui -- sync ~/Documents --device 192.168.1.42:9847 --wait 30
```

The standalone `ferrisync-cli` binary mirrors these plus:

```bash
cargo run -p ferrisync-cli -- serve ~/Documents --auto-accept
cargo run -p ferrisync-cli -- rename "Mr Desktop"
```

### Pairing consent

Known devices connect instantly (TOFU). Unknown devices are held for approval:

- **REPL**: `[serve:...] PAIRING REQUEST` appears inline; answer `y`/`n`, or
  use `pendings` + `confirm <n>` / `deny <n>`
- **CLI serve**: prompts `Allow '<name>' to pair? [y/N]`; `--auto-accept`
  skips asking

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
       CLI/REPL          TUI           Flutter
                                     (flutter_rust_bridge)
```

One reusable Rust core; every frontend is a thin wrapper over the same
synchronization engine.

- **Discovery** — mDNS/DNS-SD advertisement and browsing on the LAN
- **Authentication** — trust on first use: certificates are generated locally,
  fingerprints pinned at pairing; unknown peers require operator consent
- **Transport** — TLS 1.3 over TCP (default port 9847)
- **Integrity** — BLAKE3 hashes decide what transfers and resolve conflicts
- **Synchronization** — disk-index based comparison; missing files always pull;
  divergent files transfer exactly once (newer mtime wins, hash breaks ties),
  the overwritten side is preserved as `.bak`

---

## Why another sync tool?

There are already excellent tools, including
[Syncthing](https://syncthing.net/). FerriSync is not re-implementing them —
it explores a different approach to the synchronization experience:

- **LAN-first operation** — designed around the local network, not the internet
- **Simple discovery and pairing** — discover, tap, confirm
- **A reusable core** — `ferrisync-core` behind CLI, TUI, and mobile clients
- **Developer-friendly architecture** — small crates, black-box test scripts

Honest shortcomings today: no packaged releases, no NAT traversal / remote
sync, single-platform mobile client, evolving UX.

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
- Work on the CLI/TUI or the Flutter client
- Improve the synchronization core

**Issues and discussions are welcome.**

## License

AGPL-3.0
