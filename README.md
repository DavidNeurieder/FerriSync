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
# TUI (defaults to interactive mode)
cargo run -p ferrisync-tui

# TUI CLI subcommands
cargo run -p ferrisync-tui -- status
cargo run -p ferrisync-tui -- pair 192.168.1.42 9847

# Tests
cargo test
```

## Flutter (mobile)

```bash
cd ferrisync-flutter
flutter pub get
flutter_rust_bridge_codegen generate
flutter run
```

See [ferrisync-flutter/](./ferrisync-flutter/) for details.

## Configuration

Data directory: `~/.local/share/ferrisync/` (Linux)

- `metadata.db` — SQLite database (devices, folders, file index)
- TLS certs are generated per-run (TOFU pairing)
- File chunks capped at 64 KB

## License

AGPL-3.0
