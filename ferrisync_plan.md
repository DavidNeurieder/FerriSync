# FerriSync — Plan

Decentralized folder sync between smartphones and PCs on the same LAN. No cloud, no accounts.

- **License**: AGPL-3.0
- **Sync engine**: Rust (`ferrisync-core`)
- **Mobile UI**: Flutter (`ferrisync-flutter`)
- **Desktop UI**: Flutter (`ferrisync-flutter` — Windows, Linux, macOS)
- **Terminal UI**: Rust (`ferrisync-tui`) — TUI + CLI
- **Bridge**: `flutter_rust_bridge`

---

## Competitive Landscape

| | **Syncthing** | **LocalSend** | **Resilio Sync** | **GoodSync** | **FerriSync** |
|---|---|---|---|---|---|
| **Open source** | Yes (MPL-2.0) | Yes (GPL-3.0) | No | No | Yes (AGPL-3.0) |
| **Continuous folder sync** | Yes | No (ad-hoc send) | Yes | Yes | Yes |
| **Native mobile GUI** | Web UI only (Android fork retired) | Yes | Yes | Yes | Yes (Flutter) |
| **Terminal UI / CLI** | No | No | No | No | Yes (ratatui + clap) |
| **iOS** | No official app | Yes | Yes | Yes | Yes (foreground-limited) |
| **Language** | Go | Dart/Flutter | C++ | Unknown | Rust |
| **Reusable as library** | No | No | No | No | Yes (cargo crate) |
| **Internet sync** | Yes (relay) | No | Yes (P2P) | Yes (cloud) | Future (v3) |
| **Desktop app** | Web UI | Flutter | Native | Native | Flutter + TUI |
| **Maturity** | Very high | Medium | High | High | New |

### Where FerriSync wins

- **Dual UI** — Only project with both a modern Flutter GUI and a full TUI/CLI. Syncthing's web UI doesn't work great on mobile; FerriSync provides a native mobile experience. No other tool lets you manage sync from a terminal via `ferrisync status` or `ferrisync watch`.
- **Library-first** — `ferrisync-core` is a reusable crate. Want to add sync to your own app? `cargo add ferrisync-core`. Syncthing is a standalone app that can't be embedded.
- **No web UI dependency** — Syncthing requires a browser to manage. FerriSync runs as a normal app or a normal terminal program.

### Where Syncthing still leads

- **Maturity** — 10+ years, massive community, battle-tested protocol
- **Internet sync** — Built-in relay servers for off-network sync (v3 target for FerriSync)
- **Platform breadth** — BSD, Solaris, etc. (FerriSync focuses on Windows/Linux/macOS/Android/iOS)

### Key gap to close

Internet sync via relay (v3 in the upgrade path) is the missing piece vs Syncthing. For v1 (LAN-only) we compete on UX and developer experience.

---

## Project Structure

```
ferrisync/
├── Cargo.toml                    # Rust workspace
├── ferrisync-core/               # Core sync library
│   ├── src/
│   │   ├── lib.rs
│   │   ├── api/                  # flutter_rust_bridge FFI surface
│   │   ├── crypto/               # Self-signed TLS certs, key gen, TOFU
│   │   ├── discovery/            # mDNS broadcast + listen
│   │   ├── protocol/             # Message framing, sync commands
│   │   ├── storage/              # SQLite metadata DB (SQLCipher-encrypted)
│   │   ├── sync_engine/          # Diff index, reconcile, conflict resolution
│   │   ├── transport/            # Transport abstractions + impls
│   │   │   ├── mod.rs            # Traits: Connector, Listener, Connection
│   │   │   ├── tcp.rs            # TcpTransport (LAN)
│   │   │   ├── quic.rs           # QuicTransport (internet, future)
│   │   │   └── relay.rs          # RelayTransport (future)
│   │   └── watcher/              # notify-based monitor + polling fallback
│   └── Cargo.toml
├── ferrisync-flutter/            # Flutter app (mobile + desktop)
│   ├── lib/
│   │   ├── main.dart
│   │   ├── screens/
│   │   │   ├── devices_screen.dart
│   │   │   ├── folders_screen.dart
│   │   │   ├── activity_screen.dart
│   │   │   └── settings_screen.dart
│   │   ├── widgets/
│   │   ├── providers/
│   │   └── models/
│   ├── rust/                     # ferrisync-core integrated via FRB
│   ├── android/
│   ├── ios/
│   ├── windows/
│   ├── linux/
│   ├── macos/
│   └── pubspec.yaml
├── ferrisync-desktop/            # Desktop-only daemon + system tray (optional)
│   ├── src/
│   │   ├── main.rs               # Background daemon, tray icon
│   │   └── ipc.rs                # IPC server for GUI to query status
│   └── Cargo.toml
├── ferrisync-tui/                # Terminal client (TUI + CLI commands)
│   ├── src/
│   │   ├── main.rs               # clap arg parsing → mode dispatch
│   │   ├── cli/                  # One-shot command handlers
│   │   │   ├── pair.rs
│   │   │   ├── sync.rs
│   │   │   ├── status.rs
│   │   │   └── daemon.rs
│   │   ├── tui/                  # Interactive terminal UI
│   │   │   ├── app.rs            # TUI state machine
│   │   │   ├── ui.rs             # ratatui layout + rendering
│   │   │   └── screens/          # Dashboard, Devices, Folders, Log
│   │   └── ipc.rs                # Local IPC client for daemon mode
│   └── Cargo.toml
└── ferrisync-cli/                # Lightweight CLI for testing (dev only)
    ├── src/main.rs
    └── Cargo.toml
```

---

## Transport Abstraction

The sync engine is transport-agnostic. All transport logic sits behind three traits — one for connecting as a client, one for accepting connections as a server, and one for the connection itself.

```rust
#[async_trait]
pub trait TransportConnector: Send + Sync {
    async fn connect(addr: SocketAddr) -> Result<Box<dyn TransportConnection>>;
}

#[async_trait]
pub trait TransportListener: Send {
    async fn bind(addr: SocketAddr) -> Result<Self>;
    async fn accept(&mut self) -> Result<Box<dyn TransportConnection>>;
}

#[async_trait]
pub trait TransportConnection: Send {
    async fn read(&mut self, buf: &mut [u8]) -> Result<usize>;
    async fn write_all(&mut self, buf: &[u8]) -> Result<()>;
    async fn close(&mut self) -> Result<()>;
}
```

The sync engine uses `TransportConnector` for outbound connections and `TransportListener` for inbound. Concrete implementations live in `transport/`:

| Module | Transport | Use case |
|---|---|---|
| `tcp.rs` | TCP + TLS 1.3 | LAN (v1) |
| `quic.rs` | QUIC (`quinn` crate) | Internet (v3) |
| `relay.rs` | WebSocket or KCP via relay | When NAT punch fails |

---

## v1 — File-Level Sync (LAN)

### Transport

`TcpTransport` — TCP + TLS 1.3. Self-signed certificates generated on first launch. Devices authenticate via QR code pairing (trust-on-first-use — exchange cert fingerprints).

### Discovery

mDNS service type `_ferrisync._tcp`. Devices broadcast `DeviceId`, human-readable name, and IP. No central registry.

### Pairing flow

1. Device A generates a QR code encoding its `DeviceId` + cert fingerprint
2. Device B scans the QR with its camera
3. B sends a pairing request to A over TLS with its own fingerprint
4. A verifies, both store each other's certs as trusted
5. Done — devices now trust each other indefinitely

### Sync protocol

1. **Index exchange** — On connect, each peer sends only files changed since `last_sync_at` timestamp for that peer, plus any unresolved conflicts. On initial sync (no `last_sync_at`) or periodic full refresh, send the full index. Each entry: `(relative_path, local_version, remote_version, mtime, size, blake3_hash)`.

2. **Diff** — Each peer independently computes what the other needs:
   - Files present locally but missing remotely → push
   - Files with newer mtime on remote → pull
   - Files with same path + hash → skip
   - See conflict resolution section for the version counter algorithm

3. **Transfer** — Whole files sent over TCP + TLS. Chunked with progress callbacks for large files. Acknowledge per-file.

4. **Watch** — `notify` detects local changes (create, modify, delete). Triggers immediate re-scan of affected folder and pushes updated index to connected peers.
   - **Android caveat**: `notify` (inotify) is unreliable on many Android devices due to scoped storage, OEM background restrictions, and battery optimizations.
   - **Fallback**: on Android, if inotify fails to initialize or misses events, fall back to periodic polling (scan folder every 30s, compare mtime + size). The fallback is transparent — the sync engine uses a `WatchSource` enum: `Inotify(PollWatcher)` or `Polling(<interval>)`.

### Conflict resolution

**v1: Last-Writer-Wins + backup copy (auto-resolve).**

Each file tracked with per-device version counters in the metadata DB:

```sql
local_version  INTEGER,  -- monotonic counter, local edits bump this
remote_version INTEGER,  -- last known version on the paired device
local_mtime    INTEGER,  -- for tie-breaking
remote_mtime   INTEGER,
```

Detection on index exchange (each device sends `(path, local_version, remote_version, ...)`):

```
local_version > remote_version  AND  remote_version == their_remote_version
  → only we changed → push ours

remote_version > local_version  AND  local_version == their_local_version
  → only they changed → pull theirs

local_version > remote_version  AND  remote_version != their_remote_version
  → BOTH changed → CONFLICT → auto-resolve
```

"their_remote_version" and "their_local_version" are the values from the remote device's index entry for the same file.

Resolution:

- **Winner**: file with the later `mtime`. If equal, lower `device_id` wins.
- **Loser**: renamed to `filename.conflict.{device_id}.{unix_ts}.ext`
- **Event**: emitted to the UI activity log, non-blocking

Users who need to resolve can check the `.conflict.*` copy. No modal, no prompt.

**v2 improvements**: version vectors (eliminate clock skew dependency), three-way merge for text files, conflict review screen.

### File history

Every sync action is recorded in a local-only history table so users can trace which version came from which device.

```sql
file_history:
  id INTEGER PK,
  folder_id INTEGER FK,
  path TEXT,
  device_id TEXT,       -- which device produced this version
  action TEXT,           -- 'local_edit' | 'synced_from' | 'synced_to'
                        -- | 'conflict_winner' | 'conflict_loser' | 'deleted'
  version INTEGER,       -- the version number after this action
  mtime INTEGER,
  hash BLOB,
  size INTEGER,
  recorded_at INTEGER    -- monotonic timestamp
```

| Event | History entry |
|---|---|
| Local file change detected | `local_edit` with local device ID |
| File pulled from remote | `synced_from` with remote device ID |
| File pushed to remote | `synced_to` (recorded on pusher) |
| Conflict — winner kept | `conflict_winner` |
| Conflict — loser renamed | `conflict_loser` |
| File deleted | `deleted` |

History is **not synced** between devices — each device keeps its own log. Storage cost is negligible (~100 bytes/entry, unlimited retention). The Activity screen reads from this table in reverse chronological order.

### Storage (SQLite)

Database encrypted at rest with **SQLCipher** (via `rusqlite` + `sqlcipher` feature). Key derived from device's TLS private key — so if the device is unlocked and the app can start, the DB is accessible. Protects pairing info and certs if the device is lost.

Files synced by FerriSync are **not encrypted at rest** — they are ordinary files on the filesystem, protected by the OS like any other user file.

```
devices:
  id TEXT PK, name TEXT, cert_der BLOB, last_seen INTEGER

sync_folders:
  id INTEGER PK, local_path TEXT, device_id TEXT FK, active BOOL,
  direction TEXT DEFAULT 'bidirectional',
  last_sync_at INTEGER

file_metadata:
  path TEXT, folder_id INTEGER FK, mtime INTEGER, size INTEGER,
  hash BLOB, device_id TEXT, version INTEGER,
  PRIMARY KEY (folder_id, path)
```

Encrypt the DB schema from the start. Migrating an unencrypted DB to SQLCipher later is painful and risky.

---

## Flutter Screens (Android v1)

| Screen | Content |
|---|---|
| **Dashboard** | Connected devices, sync status (idle/syncing/error), last sync time |
| **Devices** | List of paired devices. "Add device" button → QR scanner. Status indicator (online/offline). Swipe to forget. |
| **Folders** | List of sync pairs `(local_path ↔ remote_device)`. Add: pick folder, pick device, confirm. Toggle on/off per pair. |
| **Activity** | Reverse-chronological log from `file_history`: `[device] [action] [filename] [time]`. Shows which device each version came from. |
| **Settings** | Device display name, notification toggle, about/version. |

### State management

`riverpod` with `StreamProvider` wrapping the Rust event stream (`StreamSink<SyncEvent>` from FRB).

---

## Background Sync

| Platform | Strategy |
|---|---|
| **Android** | Foreground service hosting the Tokio runtime. Persistent notification shows sync status. WorkManager as periodic fallback wakeup. |
| **iOS** | BGTaskScheduler for periodic short-window sync. Silent push as optional wake trigger. True continuous background sync is not possible on iOS — the app must be foregrounded or given a brief execution window. |

---

## Desktop (Windows, Linux, macOS)

### Two delivery options

| Option | Approach | When to use |
|---|---|---|
| **Flutter desktop** | Add `windows/`, `linux/`, `macos/` targets to the existing Flutter project. Same codebase, same FRB bindings, same UI. | v1 — fast to ship, covers all platforms at once |
| **Headless daemon + tray** | A Rust binary (`ferrisync-desktop`) that runs in the background, hosts the sync engine, and shows a system tray icon. An optional Flutter GUI connects via IPC for settings. | v2+ — leaner, tray-only control, no Flutter dependency for the daemon |

**Recommendation: start with Flutter desktop** (option 1). It's the least work — Flutter supports Windows, Linux, and macOS out of the box, and `flutter_rust_bridge` cross-compiles for all desktop targets the same way it does for Android. You get the full GUI with zero extra UI code.

### Flutter desktop — how it works

```
flutter build windows   # produces .exe
flutter build linux     # produces AppImage / deb / rpm
flutter build macos     # produces .app bundle
```

- FRB compiles `ferrisync-core` to a native `.dll` (Windows), `.so` (Linux), or `.dylib` (macOS) and bundles it with the Flutter executable
- Same Dart code from mobile renders on desktop — windows, menus, mouse events
- The existing mobile screens work as-is; desktop-specific features (tray, auto-start) are added behind `Platform.isWindows / Platform.isLinux / Platform.isMacOS` checks

### System tray + auto-start (desktop-specific)

On desktop, the app should run in the background persistently, not in the foreground like mobile. Add a system tray icon so the user can:
- Right-click → Open, Pause Sync, Quit
- See sync status at a glance (icon changes: green = syncing, grey = idle, red = error)
- Click tray icon → opens the Flutter management window

Flutter packages for this:

| Package | Use |
|---|---|
| `system_tray` | Cross-platform tray icon with menu |
| `auto_start` | Register app to launch on login |
| `window_manager` | Show/hide window, minimize to tray |

Desktop-specific features added to `ferrisync-flutter` behind platform checks:

```dart
if (Platform.isWindows || Platform.isLinux || Platform.isMacOS) {
  TrayManager().setup(
    icon: iconData,
    menu: [MenuItem(label: "Open"), MenuItem(label: "Pause Sync"), MenuItem(label: "Quit")],
  );
}
```

### Key desktop differences from mobile

| Concern | Mobile | Desktop |
|---|---|---|
| **Persistence** | Foreground service (Android), BGTaskScheduler (iOS) | Runs while user is logged in, no restrictions |
| **Sync runtime** | Must request battery exemptions | Full OS support for background processes |
| **Discovery** | mDNS works, but devices sleep frequently | mDNS always on, always reachable while running |
| **Pairing** | QR scan (camera needed) | Manual IP entry or QR code on screen |
| **File watcher** | `notify` works but limited by OS | Full inotify (Linux) / ReadDirectoryChangesW (Windows) / FSEvents (macOS) |
| **Packaging** | APK / AAB (Play Store) | MSI, AppImage, `.deb`, `.rpm`, `.dmg`, `.app` |

### Headless daemon option (v2+)

For users who want sync without a GUI:

```
ferrisync-desktop/                 # Rust binary, no Flutter
  ├── src/
  │   ├── main.rs                  # Starts sync engine, runs in background
  │   ├── tray.rs                  # System tray menu (tray-icon crate)
  │   └── ipc.rs                   # Unix socket / named pipe for GUI queries
  └── Cargo.toml
```

- Uses `ferrisync-core` as a library
- Communicates with the Flutter GUI via a local IPC channel (Unix socket on Linux/macOS, named pipe on Windows) — or simply exposes the Rust event stream over a local WebSocket
- Users who want the tray-only experience never need to install Flutter runtime

### Desktop Flutter dependencies (additions)

| Package | Use |
|---|---|
| `system_tray` | System tray icon + context menu |
| `window_manager` | Minimize to tray, show/hide window |
| `auto_start` | Register for auto-start on login |
| `url_launcher` | Open log files, docs |

---

## Terminal Client (Windows, Linux, macOS)

`ferrisync-tui` — a Rust binary providing both an interactive TUI and CLI commands. It uses `ferrisync-core` as a library, sharing the same sync engine as the Flutter app.

### Modes

| Mode | Invocation | Use case |
|---|---|---|
| **TUI** | `ferrisync` or `ferrisync tui` | Interactive terminal UI for daily use |
| **CLI** | `ferrisync <command> [args]` | Scripting, automation, one-shot syncs |
| **Daemon** | `ferrisync daemon` | Headless background sync (no UI at all) |

### CLI commands

| Command | Action |
|---|---|
| `ferrisync pair <ip>` | Pair with a device at the given IP |
| `ferrisync pair --scan` | Scan network for discoverable devices |
| `ferrisync sync <folder> --device <id>` | One-shot folder sync with a paired device |
| `ferrisync status` | Show paired devices, sync status, last sync time |
| `ferrisync watch <folder>` | Continuous foreground sync with live log |
| `ferrisync daemon` | Start background sync daemon (no TUI) |
| `ferrisync config set <key> <value>` | View or change configuration |

### TUI mode

Built with `ratatui` + `crossterm` (cross-platform terminal), key-driven navigation.

| Tab | Content |
|---|---|
| **Dashboard** | Connected devices, sync status (idle/syncing/error), live throughput gauge |
| **Devices** | List of paired devices, online/offline status, pair new |
| **Folders** | Manage sync folder pairs, pause/resume per folder |
| **Activity Log** | Real-time scrolling log of file events and errors |

The TUI communicates with the sync engine via the same event channel (`StreamSink<SyncEvent>`) that the Flutter app uses. `ferrisync-core`'s Tokio runtime runs in a background thread, and the TUI polls events on the main thread via `crossterm`'s event loop.

### Commonalities with Flutter app

- Same `ferrisync-core` library — identical sync, conflict resolution, crypto, discovery
- Same pairing mechanism — devices paired via one client appear on all clients
- Same SQLite metadata DB format — can share a database with the desktop daemon
- Same event stream format — the TUI's Activity Log shows the same events as the Flutter Activity screen

### Key Dependencies

| Crate | Use |
|---|---|
| `ratatui` | Terminal UI framework (tabs, panels, status bars) |
| `crossterm` | Cross-platform terminal control (events, raw mode) |
| `clap` | CLI argument parsing |
| `tokio` | Shared with `ferrisync-core` runtime |
| `ferrisync-core` | Core sync library (workspace dependency) |

---

## Rust Key Dependencies

| Crate | Use |
|---|---|
| `tokio` | Async runtime |
| `mdns-sd` | mDNS service discovery |
| `notify` | Filesystem change events |
| `rusqlite` + `sqlcipher` | Metadata database, encrypted at rest |
| `rustls` + `rcgen` | TLS + self-signed cert generation |
| `blake3` | Fast content hashing |
| `serde` + `bincode` | Message serialization |
| `uuid` | Device IDs |
| `chrono` | Timestamps |
| `quinn` | QUIC transport (v3, optional via feature flag) |

## Flutter Key Dependencies

| Package | Use |
|---|---|
| `flutter_rust_bridge` | Rust FFI bridge |
| `riverpod` | State management |
| `go_router` | Navigation |
| `mobile_scanner` | QR code scanning |
| `file_picker` | Folder selection dialog |
| `workmanager` | Background task scheduling |

---

## Upgrade Path: LAN → VPN → Internet

The `Transport` traits make adding network layers a matter of implementing new connector/listener/connection types, not rewriting the engine.

| Step | Transport | Discovery | What changes |
|---|---|---|---|
| **v1 — LAN** | `TcpTransport` (TCP+TLS) | mDNS | Initial implementation |
| **v2 — VPN** | `TcpTransport` | mDNS over Tailscale/WireGuard | **Zero Rust changes** — just install Tailscale on both devices. The virtual network makes LAN code work as-is. |
| **v3 — Internet** | `QuicTransport` (QUIC) | STUN/TURN relay server + DHT | New `transport::quic`, optional relay binary, NAT hole-punching. Sync engine untouched. |

### v3 — Internet details

- **QUIC** via `quinn` crate — multiplexed streams, built-in encryption, connection migration (survives IP changes), better NAT traversal than TCP
- **Discovery**: lightweight relay server (or DHT bootstrap) for peers to find each other when not on the same subnet
- **NAT traversal**: STUN for address discovery, optional TURN relay as fallback
- **End-to-end encryption**: same TLS certs from v1, QUIC's TLS 1.3 built-in
- **Sync engine**: identical code — works over any `Transport` impl

```
┌──────────────────────┬───────────────────────────────┐
│  Flutter UI          │  Terminal UI (ratatui)         │
│  (mobile + desktop)  │  + CLI (clap)                 │
├──────────────────────┴───────────────────────────────┤
│  Sync engine (same code — diff, reconcile)            │
├─────────────┬───────────────┬────────────────────────┤
│  TcpTransport │ QuicTransport │ RelayTransport         │
│  (LAN, v1)   │ (Internet, v3)│ (fallback, v3)         │
└─────────────┴───────────────┴────────────────────────┘
```

## v2 Feature Ideas (post-MVP, any network type)

- Block-level incremental sync (only transfer changed blocks of large files)
- 3-way merge / version vector conflict resolution
- Folder templates (Camera auto-sync, Documents, etc.)
- Bandwidth throttling per sync pair

## Backup / RAID Integration

FerriSync is a sync tool, not a backup tool. For versioned, deduplicated, restorable backups, use existing RAID and snapshot tools alongside FerriSync:

```
Source device ──FerriSync (send-only)──▶ Backup node
                                                │
                                          ZFS pool with snapshots
                                          (hourly, daily, weekly)
                                                │
                                          restic / borg (off-site)
```

| Concern | Handled by | Instead of building into FerriSync |
|---|---|---|
| Point-in-time file recovery | ZFS snapshots / btrbk | Deleted file trash (snapshots catch deletions) |
| Version history & dedup | restic / borg / ZFS send/recv | Content versioning (blob store, DAG) |
| Off-site replication | restic / rsync / ZFS send | Custom replication logic |

**What FerriSync provides** — the only piece existing tools can't do:

- **One-way sync (send-only / receive-only)** — per-folder direction flag. A backup node is configured as receive-only so it never pushes changes back. This protects against accidental mass-delete propagation and ransomware spread. The flag lives in the sync engine config and is enforced at the protocol level: receive-only devices reject outgoing index diffs for that folder.

---

## Implementation Order

The CLI is built early and doubles as the integration test harness — no Flutter or emulator needed to validate the sync engine.

1. **Scaffold** — Rust workspace, `ferrisync-core` crate, `ferrisync-cli` crate
2. **crypto** — TLS cert generation, key storage
3. **discovery** — mDNS advertise + browse
4. **protocol** — Message types, framing, handshake
5. **storage** — SQLite schema, CRUD for metadata
6. **watcher** — File system monitoring, debounced re-index
7. **transfer** — TCP + TLS send/receive, chunking, progress
8. **sync_engine** — Diff, reconcile, conflict resolution, orchestration
9. **CLI** — `ferrisync-cli` with `pair`, `sync`, `watch`, `status` commands. The CLI tests the full sync loop: run two instances, add files on one side, verify they appear on the other. All bugs surface here before any UI code is written.
10. **Integration tests** — Automated two-process test harness using the CLI. Tests: basic sync, conflict, disconnect/reconnect, large files.
11. **TUI client** — `ferrisync-tui` with `ratatui`, interactive mode, daemon mode
12. **api** — FRB-annotated bridge surface for Flutter
13. **Flutter UI** — Mobile screens, state, background service
14. **Desktop Flutter targets** — Add `windows/`, `linux/`, `macos/` config, system tray, auto-start
15. **Desktop packaging** — MSI (Windows), AppImage/deb/rpm (Linux), `.app`/`.dmg` (macOS), CI builds
16. **Headless daemon** (v2) — `ferrisync-desktop` binary, tray-only, IPC with Flutter GUI
