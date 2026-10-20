# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/),
and this project adheres to [Semantic Versioning](https://semver.org/).

## [0.1.0] - Unreleased

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
- **Integration tests** — 420+ tests covering unit, integration, cross-process,
  server, failure, adversarial, and property-based scenarios
- **Security regression suites** — `protocol_security` (oversized/malformed
  frames, enormous declared lengths, truncation, garbage payloads),
  `crypto_security` (TLS 1.3 enforcement and certificate-bound identity),
  `path_safety_security` (traversal, absolute injection, symlink and
  directory-swap races, with an external sentinel proving root containment),
  and `pairing_security` (safe default holds unknown peers, explicit
  auto-accept opt-in, denial, revocation, and no silent certificate replacement)
- **Adversarial and regression test suite** — deterministic wire-level tests
  for an aborted handshake, a truncated data frame, a declared-size mismatch,
  out-of-order and over-long chunks, duplicate/absolute paths in a remote
  index, simultaneous sessions targeting one destination, a file changed
  between indexing and upload, a permission-denied write, and
  property-based checks for index path safety and chunk sequencing
- **Auto re-pair on startup** — on launch, `ferrisync` re-pairs every known
  device at its last-seen address (gated behind a settings toggle, default
  off), recovering pairings after a host restart without waiting on mDNS
- **Device startup repopulation** — the app now refreshes the device/folder
  list at startup so devices and folders are shown immediately instead of
  staying empty until the first manual pull
- **Two-folder pick** — a peer advertising multiple published folders returns
  and renders them all in the device-detail "AVAILABLE TO SYNC" list
- **Continuous integration** — `.github/workflows/ci.yml` gates every push and
  pull request: Rust formatting (`cargo fmt --all -- --check`), lints with
  warnings denied (`cargo clippy --workspace --all-targets -- -D warnings`),
  the full Rust test suite on Linux, macOS, and Windows, and — after generating
  the Rust/Dart bridge — `flutter analyze` and `flutter test`
- **Authorization regression suite** — end-to-end tests over the real serving
  path (`serve_folder` → `listen_for_sync`) proving an unpaired peer is denied
  file transfers and shared-folder enumeration, a paired peer is authorized,
  and revoking a device immediately revokes both
- **Resource-limit tests** — direct checks that the index entry cap, declared
  file size, chunk size, path length, in-flight file cap, buffered-byte cap, and
  the concurrent-connection cap all reject over-limit input, and that a declared
  size is never pre-allocated

### Changed

- **CLI `serve` no longer trusts unknown devices just because stdin is not a
  terminal** — previously `ferrisync serve <folder>` fell back to
  auto-accepting pairings whenever it was started without a TTY (e.g. a
  systemd unit, a piped shell, or CI), silently granting any reachable device
  read/write access to the served folder. Automatic acceptance is now gated
  solely on the explicit `--auto-accept` flag; without it, unknown devices are
  held for approval and announced as `PAIRING REQUEST` lines even when there is
  no terminal to prompt on. Headless deployments that relied on the implicit
  behavior should add `--auto-accept`
- **TLS 1.3 is now enforced, not merely enabled** — the server and client TLS
  configurations pin the protocol version list to TLS 1.3. rustls' default
  builder accepts TLS 1.2 as well, so a peer could previously negotiate a
  downgrade despite the transport being documented as TLS 1.3; a TLS 1.2-only
  client now fails the handshake
- **CLI/REPL status shows local ↔ remote folder pairs** — the FOLDERS section
  renders `local_path ↔ remote_path` (the remote folder on the paired device)
  instead of just the peer device name, so you can see which local folder maps
  to which remote folder
- **CLI simplifies what it shows — device names, not raw identifiers** — IP
  addresses and internal ids are no longer exposed to the user. Discovery
  (`discover`, REPL) lists devices by name only; interactive `pair` picks a
  device by number without needing its address; `status`, `activity`,
  `pendings`, `share list`, `serve`, `folders`, and `remove` output now render
  display names and folder paths instead of ids/guids. `remove` accepts a
  device name (resolved internally).
- **Remote folder display on the Folders card** — the "PAIRED WITH" pane now
  shows the actual remote folder path on the paired device instead of a generic
  "Path on <device>" label. Both approval paths (auto-approve for trusted
  devices and the app's manual approval) now record the owner's shared-folder
  path as the pairing's `remotePath`
- **Pairing consent model** — device pairing is now the single approval gate.
  Once the requesting device's certificate is stored and trusted, its
  folder-pair requests are auto-approved (no second per-folder prompt); only
  the device pairing itself awaits operator consent

### Removed

- **Full-screen terminal UI (TUI)** and its `ratatui`/`crossterm` dependencies
  — `ferrisync` with no arguments now starts the interactive REPL instead

### Fixed

- **`FOREIGN KEY constraint failed` on first folder add** — adding the first
  folder before any device row existed (e.g. during onboarding) crashed because
  the device self-record was missing when the folder↔device link was inserted.
  The device row is now upserted first
- **Duplicate `devices` rows for a single physical device** — the advertised
  device id was a separate random UUID, distinct from the cert-derived id peers
  record, so one device could appear multiple times (e.g. the same name repeated
  in `status`). Identity is now unified on the certificate: there is a single id
  scheme, and existing duplicate rows are merged via a migration that remaps
  folders, history, and session references before dropping the leftovers
- **Duplicate folders shown in `status`/`folders`** — a folder attached to
  several devices rendered once per device (e.g. `test ↔ this device` and
  `test ↔ test2`), reading as a duplicate. The CLI/REPL now show ONE row per
  physical folder whose peer label lists every attached device (e.g.
  `test ↔ test2, this device`), with health from the peer rows (self rows are
  served-folder bookkeeping and never dominate)
- **Duplicate-announce warning** — the CLI/REPL now probe the LAN on startup
  (`devices`, REPL) and when serving, and print a warning when a second
  instance is already advertising under the same device name (the classic two
  processes on one host that double `status`/`discover` entries). The REPL runs
  the probe in the background so the prompt is never delayed
- **Flaky `path_safety` test** — tests shared a single temp directory and
  raced; each call now uses its own unique directory
- **Interrupted transfers no longer reported as success** — a session now ends
  successfully only after every planned upload is acknowledged and every
  planned download arrives, is reassembled contiguously, and matches the
  index's byte count and BLAKE3 hash. A disconnect, an idle timeout, a missing
  acknowledgement, or a rejected/malformed transfer is reported as a typed
  failure instead of a silent `Ok`, and the local source files are left intact
- **A pushed file is recorded in history only once the peer confirms it** — the
  durable `push` history row (and its per-file state in the UI timeline) was
  written as soon as the outbound chunks were sent, so an upload that was then
  cancelled, timed out, or rejected still appeared as a completed push. The row
  is now written when the peer's acknowledgement arrives, matching the
  "no false success" rule the session already follows
- **Strict remote-index and transfer validation** — an incoming index is
  rejected up front for absolute paths, `..`/self traversal, duplicates,
  NUL bytes, over-long paths, or impossible sizes; incoming chunks must belong
  to the agreed transfer set, arrive in order, and match the expected size and
  hash. A short or absent expected hash is rejected rather than treated as
  permission to skip verification
- **Session history distinguishes complete from incomplete** — each recorded
  session now carries a `status` (`complete`/`incomplete`), surfaced through
  the API, so a partial or failed sync is never indistinguishable from a
  finished one
- **Safe filesystem writes** — every received file is written to a unique,
  exclusively-created temporary file in the destination directory (never a
  shared fixed `*.ferrisync-tmp` name), verified, and atomically renamed over
  the destination. A failed or competing transfer can no longer corrupt or
  delete an existing destination file, and an abandoned temporary file from a
  crash is cleaned up and excluded from the index, so it can never be mistaken
  for a completed transfer
- **Stable upload snapshots** — a file's indexed size, mtime, and BLAKE3 hash
  are now read from one consistent snapshot, and every upload re-verifies the
  bytes against the indexed size and hash immediately before they are sent. If
  a file is edited or deleted between indexing and transmission the upload is
  skipped and the session is reported incomplete, instead of sending bytes the
  peer never agreed to receive or recording a false success. A file still being
  written is skipped for that scan and picked up cleanly on the next sync
- **Conflict backups are local artifacts** — `.ferrisync-conflict-*` files are
  no longer indexed, so a preserved losing version is never copied back to the
  peer and repeated synchronizations converge immediately instead of
  re-transferring backups
- **Deterministic conflict policy** — whether a transfer is needed is decided by
  content hash, never by timestamps; versions only order which differing
  version wins, and a concurrent edit is reported as a conflict whose superseded
  local copy is preserved as a `.ferrisync-conflict-*` backup rather than
  silently discarded. The policy is documented in `sync::reconciler`
- **Shared-folder enumeration required no pairing** — the `ListSharedFolders`
  RPC answered any TLS client, exposing discoverable share names and their real
  local paths before any device trust existed. It now requires the peer's
  certificate to map to a paired device (the same boundary as a sync session),
  and returns an empty list otherwise

### Security

- Protocol deserialization is bounded — every inbound message is decoded with
  an explicit byte budget (`protocol::deserialize_message`), so a small frame
  carrying an oversized `Vec`/string length prefix is rejected without a large
  allocation or a long decode, independent of the frame-size cap
- TLS 1.3 enforced on both sides — the protocol version list is pinned, so a
  client offering only TLS 1.2 is refused instead of negotiating a downgrade
- Headless pairing is safe by default — automatic device acceptance requires an
  explicit `--auto-accept`; a missing terminal no longer implies trust, so an
  unattended server holds unknown peers for approval
- Dependency security — `rustls` upgraded to 0.23.46 and `rustls-webpki` to
  0.103.16, resolving five advisories (a TLS 1.3 handshake-message acceptance
  bug, `Debug` exposure of client session secrets, Brotli certificate
  decompression memory amplification, an unauthenticated-ServerHello downgrade
  vector, and distrusted-intermediate acceptance in certificate verification).
  The `tls12` feature was dropped from the rustls/tokio-rustls dependency set
  since the transport is pinned to TLS 1.3
- Path traversal protection via `safe_join` with null byte and `..` rejection
- Symlink-resistant writes — path resolution rejects any symlink component,
  parent directories are created one level at a time without following links,
  and the destination is re-validated immediately before the atomic rename, so
  a symlink swapped in mid-write cannot redirect a write outside the sync root
- Per-destination write serialization — competing writers to the same path are
  serialized by an in-process lock, so a backup/replace sequence can never be
  interleaved with another session's write
- Device authorization on every file-serving entry point — a sync session and
  shared-folder enumeration both require the peer's TLS certificate to map to a
  paired device; revoking a device (removing it) revokes both immediately, and
  mDNS discovery never grants trust on its own
- Protocol frame limits (1 MiB control frames, 128 KiB file chunks, 10 GiB max
  file) to prevent memory exhaustion
- Atomic file writes (temp file + rename) to prevent partial-write corruption
- Hash verification before commit to detect transfer corruption
- Index entry count limit (100,000) and remote-index validation (entry count,
  per-entry declared size, path safety, duplicates) applied before any transfer
  is planned; a declared file size is never pre-allocated
- Bounded concurrency to prevent resource exhaustion: at most 64 concurrent
  inbound sessions (excess connections are shed immediately), at most 64
  in-flight files and 256 MiB buffered per session
- FileRequest path length validation (1 KiB max) and a cap of 1,000 paths per
  request
- Certificate pinning with TOFU for device authentication

### Known limitations

- **Deletions are not yet propagated between peers.** The two-device session
  reconciles with empty tombstones (`SyncOrchestrator::reconcile_snapshots`),
  so when one peer deletes a file the other still holds, the next sync restores
  it from the peer. Converging deletions reliably (including for devices that
  were offline) needs a durable change history — deletion tombstones carried in
  the protocol and persisted per folder — which is deferred to a follow-up
  protocol extension. Until then, remove files on both peers, or remove the
  folder pairing.
- **Version ordering between peers falls back to modification time.** When two
  peers hold different content for the same path, the winning version is chosen
  by the per-device version counter, which is derived from the file's mtime.
  This is deterministic for a given pair of inputs and the *losing* version is
  always preserved as a `.ferrisync-conflict-*` backup, but a preserved or
  manipulated timestamp can change which side wins. A durable, content-derived
  version counter would remove the mtime dependency.