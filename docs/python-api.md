# FerriSync Python API reference

This document describes the public Python API provided by the `ferrisync`
package (`ferrisync-python`). The authoritative typed surface is the package's
`.pyi` stubs in `ferrisync-python/python/ferrisync/`.

Status: developer preview (`0.x`). See [Stability policy](#stability-policy).

## Installation

```bash
pip install ferrisync
```

For development builds:

```bash
cd ferrisync-python
python -m pip install --upgrade maturin
maturin develop
```

## Module exports

```python
from ferrisync import (
    __version__,
    CancelEvent,
    Client,
    AsyncClient,
    Watcher,
    AsyncWatcher,
    DiscoveredPeer,
    SyncResult,
    FerriSyncError,
    ClientClosedError,
    FerriSyncCancelledError,
    InvalidArgumentError,
    NotFoundError,
)
```

## Exceptions

- `FerriSyncError`: base exception for FerriSync/Python binding errors.
- `ClientClosedError(FerriSyncError)`: operation attempted on a closed client.
- `FerriSyncCancelledError(FerriSyncError)`: cooperative cancellation was
  observed. The signal is forwarded to the core token, so in-flight
  connect/handshake/chunk/finalization steps stop at their next boundary.
- `InvalidArgumentError(FerriSyncError)`: invalid input, path, parse, or
  argument value.
- `NotFoundError(FerriSyncError)`: requested device/folder/pairing/resource was
  not found.

`AsyncClient` translates cancellation to `asyncio.CancelledError` when an async
operation is cancelled via an `asyncio.Event` or cancellation-aware path.

## Data models

### `DiscoveredPeer`

```python
@dataclass
class DiscoveredPeer:
    id: str
    name: str
    ip: str
    port: int
```

Represents a peer discovered on the LAN.

### `SyncResult`

```python
@dataclass
class SyncResult:
    pulled: list[str]
    pushed: list[str]
    conflicts: list[str]
    pulled_bytes: int
    pushed_bytes: int

    @property
    def success(self) -> bool: ...
```

`success` indicates the sync call completed and returned a result. Callers
should still inspect `conflicts` and handle application-specific outcomes.

## Synchronous client

### `Client`

```python
Client(data_dir: str | Path | None = None)
```

- `data_dir`: FerriSync data directory containing `metadata.db`,
  `identity.crt`, and `identity.key`. If omitted, a platform-default data
  directory is used.
- Each `Client` owns its engine instance, folder servers, and background
  watches.
- Blocking operations release the Python GIL while running Rust/Tokio work.

Context manager:

```python
with ferrisync.Client("/path/to/data-dir") as client:
    ...
```

#### Properties

- `closed: bool`
- `data_dir: str`
- `device_id: str`
- `device_name: str`

#### Lifecycle and identity

- `close() -> None`: stops owned servers/watches and closes the client.
  Idempotent.
- `rename(name: str) -> str`: sets this device's display name.

#### Discovery and pairing

- `discover(timeout: int = 5) -> list[DiscoveredPeer]`
- `pair(ip: str, port: int = 9847) -> str`: returns paired device id.
- `devices() -> list[dict[str, Any]]`
- `remove_device(device_id: str) -> dict[str, Any]`
- `pending_pairings() -> list[tuple[str, str]]`
- `approve_pairing(device_id: str, device_name: str) -> None`
- `deny_pairing(device_id: str) -> None`

#### Folders and hosting

- `add_folder(local_path: str | Path, device_id: str, direction: str = "bidirectional") -> int`
- `folders() -> list[dict[str, Any]]`
- `remove_folder(folder_id: int) -> None`
- `serve(folder_id: int, local_path: str | Path, port: int = 9847, auto_accept: bool = False) -> int`
  - Returns the bound port (useful when `port=0`).
  - `auto_accept=True` admits unknown peers automatically. **Only use on
    trusted private networks.**
- `stop_servers() -> None`

#### Sync

- `sync(folder_id: int, local_path: str | Path, remote_ip: str, remote_port: int, device_id: str, dry_run: bool = False, cancel: CancelEvent | None = None) -> SyncResult`

`cancel` accepts a `threading.Event` or `asyncio.Event`. It is checked at entry
and polled while the native call runs, then forwarded to the core cancellation
token. In-flight connect/handshake/chunk/finalization steps stop at their next
boundary; a file committed before cancellation remains valid. The cancelled
call raises `FerriSyncCancelledError` instead of returning a result, and the
next sync reconciles any partially completed state.

#### Background watches

- `watch(folder_id: int, local_path: str | Path, remote_ip: str, remote_port: int, device_id: str, cancel: CancelEvent | None = None) -> Watcher`
- `start_watch(..., cancel: CancelEvent | None = None) -> int` (low-level; returns watch id)
- `stop_watch(watch_id: int) -> None`
- `list_watches() -> list[dict[str, Any]]`

`CancelEvent` is `threading.Event | asyncio.Event`. Setting a watch's cancel
event stops its background task, cancels any in-flight sync, and removes the
watch from `list_watches()`; `Watcher.stop()` and `Client.close()` do the same.

`Watcher`:

- `id: int`
- `closed: bool`
- `stop() -> None` (idempotent)
- `close() -> None`
- context manager support

#### Events

- `poll_events() -> list[dict[str, Any]]`
- `events(poll_interval: float = 0.25, stop_event: threading.Event | None = None) -> Iterator[dict[str, Any]]`
- `subscribe(callback: Callable[[dict[str, Any]], Any], poll_interval: float = 0.25, daemon: bool = True, on_error: Callable[[BaseException], Any] | None = None) -> threading.Event`

`subscribe()` runs callbacks in a background polling thread and returns a stop
event.

#### Status, history, conflicts

- `status() -> dict[str, Any]`
- `sessions(limit: int = 20) -> list[dict[str, Any]]`
- `conflicts() -> list[dict[str, Any]]`
- `read_conflict(folder_id: int, winner_path: str, loser_path: str) -> dict[str, Any]`
- `resolve_conflict(folder_id: int, backup_path: str, action: str) -> str`
  - Actions: `this`/`keep_backup`, `other`/`keep_original`, `both`/`keep_both`
- `diagnostics() -> list[dict[str, Any]]`
- `shared_folders() -> list[dict[str, Any]]`
- `pending_folder_pairings() -> list[dict[str, Any]]`
- `file_history(folder_id: int | None = None, limit: int = 50) -> list[dict[str, Any]]`

#### Convenience helpers

- `find_device(name_or_id: str) -> dict[str, Any] | None`
- `find_folder(local_path: str | Path) -> dict[str, Any] | None`

## Asynchronous client

### `AsyncClient`

```python
AsyncClient(data_dir: str | Path | None = None, max_workers: int = 4)
```

- Wraps `Client` using a bounded `ThreadPoolExecutor`.
- Methods mirror the synchronous API (awaitable where appropriate).
- `close()` stops owned work and shuts down the executor with
  `cancel_futures=True`.

Async context manager:

```python
async with ferrisync.AsyncClient("/path/to/data-dir") as client:
    ...
```

Key differences/additions:

- `sync(..., cancel: asyncio.Event | None = None) -> SyncResult`
  - Raises `asyncio.CancelledError` when cancelled.
- `start_watch(..., cancel: asyncio.Event | None = None) -> int`
- `watch(..., cancel: asyncio.Event | None = None) -> AsyncWatcher`
- `events(...) -> AsyncIterator[dict[str, Any]]`
- `subscribe(...) -> asyncio.Event` (runs callbacks in an asyncio task)

`AsyncWatcher`:

- `id: int`
- `closed: bool`
- `await stop()`, `await close()`
- async context manager support

## Examples

### Discovery, pairing, and sync

```python
import ferrisync

with ferrisync.Client() as client:
    peers = client.discover(timeout=5)
    if not peers:
        raise SystemExit("no peers found")

    peer = peers[0]
    device_id = client.pair(peer.ip, peer.port)
    folder_id = client.add_folder("/path/to/local", device_id=device_id)
    result = client.sync(
        folder_id=folder_id,
        local_path="/path/to/local",
        remote_ip=peer.ip,
        remote_port=peer.port,
        device_id=device_id,
    )
    print(result.pulled, result.pushed, result.conflicts)
```

### Hosting with explicit consent warning

```python
import ferrisync

with ferrisync.Client("/path/to/data-dir") as server:
    folder_id = server.add_folder("/path/to/share", server.device_id)
    # auto_accept=True grants read/write access to any reachable device.
    # Use only on trusted private networks.
    port = server.serve(folder_id, "/path/to/share", port=9847, auto_accept=False)
    print("serving on", port)
```

### Watcher and event polling

```python
import threading
import ferrisync

with ferrisync.Client() as client:
    watcher = client.watch(folder_id, "/path/to/local", "192.168.1.10", 9847, device_id)
    stop = threading.Event()
    try:
        for event in client.events(stop_event=stop):
            print(event)
    finally:
        watcher.stop()
```

### Async usage with cancellation

```python
import asyncio
import ferrisync

async def main():
    async with ferrisync.AsyncClient(max_workers=4) as client:
        cancel = asyncio.Event()
        try:
            await client.sync(folder_id, "/path/to/local", "127.0.0.1", 9847, device_id, cancel=cancel)
        except asyncio.CancelledError:
            pass

asyncio.run(main())
```

### Conflict handling

```python
conflicts = client.conflicts()
for c in conflicts:
    print(c)
    # client.resolve_conflict(c["folder_id"], c["backup_path"], "both")
```

## Stability policy

- `0.x`: minor releases may introduce breaking API changes.
- Patch releases in `0.x` aim to avoid breaking public API changes.
- Deprecated APIs should emit `DeprecationWarning` and remain available for at
  least one minor release (policy target from `0.2.0` onward).
- Exception hierarchy and file-content transfer semantics are intended to be
  stable from `0.2.0`.
- Cancellation and callback threading semantics are part of the documented
  contract and should not change without documentation and release notes.

## Notes

- Python bindings reuse the same Rust core, identity model, TLS/TOFU pairing,
  and sync engine as other FerriSync frontends.
- Use isolated `data_dir` values for independent client identities.
- Do not use `auto_accept=True` on untrusted networks.
