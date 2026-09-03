# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/),
and this project adheres to [Semantic Versioning](https://semver.org/).

## [Unreleased]

### Added

- **Auto re-pair on startup** — on launch, `ferrisync` re-pairs every known
  device at its last-seen address (gated behind a settings toggle, default
  off), recovering pairings after a host restart without waiting on mDNS
- **Device startup repopulation** — the app now refreshes the device/folder
  list at startup so devices and folders are shown immediately instead of
  staying empty until the first manual pull
- **Two-folder pick** — a peer advertising multiple published folders returns
  and renders them all in the device-detail "AVAILABLE TO SYNC" list

### Changed

- **Remote folder display on the Folders card** — the "PAIRED WITH" pane now
  shows the actual remote folder path on the paired device instead of a generic
  "Path on <device>" label. Both approval paths (auto-approve for trusted
  devices and the app's manual approval) now record the owner's shared-folder
  path as the pairing's `remotePath`
- **Pairing consent model** — device pairing is now the single approval gate.
  Once the requesting device's certificate is stored and trusted, its
  folder-pair requests are auto-approved (no second per-folder prompt); only
  the device pairing itself awaits operator consent

### Fixed

- **`FOREIGN KEY constraint failed` on first folder add** — adding the first
  folder before any device row existed (e.g. during onboarding) crashed because
  the device self-record was missing when the folder↔device link was inserted.
  The device row is now upserted first
- **Flaky `path_safety` test** — tests shared a single temp directory and
  raced; each call now uses its own unique directory


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
- **Single `ferrisync` binary** — one executable for the interactive REPL
  and all one-shot CLI commands
  (`serve`/`pair`/`sync`/`watch`/`status`/`rename`/`remove`); no arguments
  starts the REPL, any subcommand runs headlessly. Replaces the separate
  `ferrisync-cli` and `ferrisync-tui` binaries.
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

### Removed

- **Full-screen terminal UI (TUI)** and its `ratatui`/`crossterm` dependencies
  — `ferrisync` with no arguments now starts the interactive REPL instead

### Security

- Path traversal protection via `safe_join` with null byte and `..` rejection
- Protocol frame limits (4 MiB control, 1 MiB data) to prevent memory
  exhaustion
- Atomic file writes (temp file + rename) to prevent partial-write corruption
- Hash verification before commit to detect transfer corruption
- Index entry count limit (100,000) to prevent DoS via oversized indexes
- FileRequest path length validation (4 KiB max)
- Certificate pinning with TOFU for device authentication
