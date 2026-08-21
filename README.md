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
```

Servers run in the background and are stopped automatically when you exit the
shell. Transfers are reported live (`[serve:<folder>] pushed/pulled ...`).

## Tests

```bash
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
