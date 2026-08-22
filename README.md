# FerriSync

Decentralised, LAN-only folder sync. No cloud, no accounts — pairing is TOFU over TLS 1.3 with mDNS discovery.

## Architecture

```
ferrisync-core   — sync engine, crypto, protocol, storage, transport
ferrisync-cli    — CLI wrapper (dev-only, replaced by TUI)
ferrisync-tui    — terminal UI + CLI dispatch
ferrisync-flutter — Flutter mobile frontend (via flutter_rust_bridge)
```

## Build & Run

Requires Rust 1.80+.

```bash
# Interactive shell (default) — help, status, discover, pair, sync, watch, serve
cargo run -p ferrisync-tui

# Full-screen terminal UI
cargo run -p ferrisync-tui -- tui

# One-shot commands
cargo run -p ferrisync-tui -- status
cargo run -p ferrisync-tui -- pair 192.168.1.42 --port 9847
cargo run -p ferrisync-tui -- sync ~/Documents --device 192.168.1.42:9847
cargo run -p ferrisync-tui -- watch ~/Photos --device 192.168.1.42:9847
```

### Serving folders (REPL)

Inside the interactive shell, `serve` hosts a folder for pairing and sync on the
LAN — other devices can discover it via mDNS and pair against it, exactly like
`ferrisync-cli serve`, but without leaving the shell:

```text
serve ~/Documents              # host on port 9847 (default)
serve ~/Documents --port 7000  # custom port
serves                         # list background servers
unserve 1                      # stop server #1 (see `serves`)
pendings                       # devices waiting for pairing approval
confirm 1                      # approve a held pairing request
deny 1                         # deny a held pairing request
```

Servers run in the background and are stopped automatically when you exit the
shell. Transfers are reported live (`[serve:<folder>] pushed/pulled ...`).

### Pairing consent

Devices already known to a host are accepted instantly. Unknown devices must be
approved by the operator:

- **REPL** (`ferrisync-tui`): requests appear as `[serve:...] pairing request
  from '<name>'`; approve with `confirm <n>` or deny with `deny <n>`
  (`pendings` lists what is waiting).
- **CLI** (`ferrisync-cli serve`): prompts `Allow '<name>' to pair? [y/N]` when
  attached to a terminal; passes `--auto-accept` (or runs without a TTY) to
  accept unknown devices without asking.

Denied devices stay rejected until that server instance is restarted.

## Tests

Unit + black-box REPL tests:

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

mDNS discovery needs no extra rule — UFW's default ruleset already permits
UDP 5353 to the multicast group.

Inspect or undo:

```bash
sudo ufw status numbered
sudo ufw delete <n>
```

## Flutter (mobile)

```bash
cd ferrisync-flutter
flutter pub get
flutter_rust_bridge_codegen generate
flutter run
```

See [ferrisync-flutter/](./ferrisync-flutter/) for details.

### Installing on a physical device

`make test-android-flutter` rebuilds `ferrisync-flutter/build/app/outputs/flutter-apk/app-debug.apk`
for the ABI of the emulator it runs on (x86_64) — that APK will crash on an arm64 phone with
`MissingLibraryException: Could not find 'libflutter.so'`. For real devices build a full APK instead:

```bash
make install-phone   # universal APK (x86_64 + arm64) + adb install
# or just build:
make build-android-apk-universal
```

## Configuration

Data directory: `~/.local/share/ferrisync/` (Linux)

- `metadata.db` — SQLite database (devices, folders, file index)
- TLS certs are generated per-run (TOFU pairing)
- File chunks capped at 64 KB

## License

AGPL-3.0
