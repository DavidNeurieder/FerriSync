# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/),
and this project adheres to [Semantic Versioning](https://semver.org/).

## [0.1.0] - 2026-08-27

### Added

- **Core sync engine** (`ferrisync-core`) — reusable Rust library for
  peer-to-peer file synchronization
- **TLS 1.3 transport** — all connections are encrypted and mutually
  authenticated using rustls
- **Trust On First Use (TOFU)** — certificate-based device identity with
  first-use pinning and operator consent for unknown peers
- **Bidirectional file sync** — push and pull files between paired devices
- **Conflict handling** — conflicting edits are detected via vector clocks;
  overwritten versions preserved as `.ferrisync-conflict-*` backups
- **Pure reconciler** — deterministic, stateless sync plan computation from
  domain snapshots
- **Atomic file writes** — received files written via temp-file + rename to
  prevent corruption on crash
- **Hash verification** — BLAKE3 content hashes verified before committing
  received files
- **Filesystem watcher** — OS-native file watching with debounced change
  coalescing for real-time sync triggers
- **mDNS/DNS-SD discovery** — automatic LAN peer discovery using mdns-sd
- **CLI** (`ferrisync-cli`) — command-line tool for serving, pairing, and
  syncing folders
- **TUI** (`ferrisync-tui`) — interactive shell (REPL) and full-screen TUI
  with session history, server management, and real-time sync
- **Android client** (`ferrisync-flutter`) — Flutter app using
  flutter-rust-bridge for cross-platform mobile sync
- **Input validation** — frame size limits, index entry caps, path length
  limits, path traversal protection, metadata.db exclusion from transfers
- **Session history** — persistent record of sync sessions with push/pull
  counts and conflict tracking
- **Device rename** — change device name from any frontend; updates propagate
  to advertisements and pairing
- **Public API** — curated re-exports for external Rust consumers; documented
  example (`examples/minimal_sync.rs`)
- **Integration tests** — 201 tests covering unit, integration, cross-process,
  server, and failure scenarios

### Security

- Path traversal protection via `safe_join` with null byte and `..` rejection
- Protocol frame limits (4 MiB control, 1 MiB data) to prevent memory
  exhaustion
- Atomic file writes (temp file + rename) to prevent partial-write corruption
- Hash verification before commit to detect transfer corruption
- Index entry count limit (100,000) to prevent DoS via oversized indexes
- FileRequest path length validation (4 KiB max)
- Certificate pinning with TOFU for device authentication
