"""Python bindings for the FerriSync synchronization engine.

This package provides a thin, typed(ish) Python API over the Rust
``ferrisync-core`` engine using PyO3/maturin.

Status: 0.1.0 developer preview MVP. The API may change.
"""

from __future__ import annotations

import asyncio
import concurrent.futures
import threading
from collections.abc import AsyncIterator, Callable, Iterator
from dataclasses import dataclass
from pathlib import Path
from typing import Any

from ._native import (
    Client as _NativeClient,
)
from ._native import (
    ClientClosedError,
    FerriSyncCancelledError,
    FerriSyncError,
    InvalidArgumentError,
    NotFoundError,
    __version__,
)

CancelEvent = threading.Event | asyncio.Event

__all__ = [
    "__version__",
    "CancelEvent",
    "Client",
    "AsyncClient",
    "Watcher",
    "AsyncWatcher",
    "DiscoveredPeer",
    "SyncResult",
    "FerriSyncError",
    "ClientClosedError",
    "FerriSyncCancelledError",
    "InvalidArgumentError",
    "NotFoundError",
]


@dataclass
class DiscoveredPeer:
    id: str
    name: str
    ip: str
    port: int

    @classmethod
    def from_dict(cls, d: dict[str, Any]) -> DiscoveredPeer:
        return cls(id=d["id"], name=d["name"], ip=d["ip"], port=int(d["port"]))


@dataclass
class SyncResult:
    pulled: list[str]
    pushed: list[str]
    conflicts: list[str]
    pulled_bytes: int
    pushed_bytes: int

    @classmethod
    def from_dict(cls, d: dict[str, Any]) -> SyncResult:
        return cls(
            pulled=list(d.get("pulled", [])),
            pushed=list(d.get("pushed", [])),
            conflicts=list(d.get("conflicts", [])),
            pulled_bytes=int(d.get("pulled_bytes", 0)),
            pushed_bytes=int(d.get("pushed_bytes", 0)),
        )

    @property
    def success(self) -> bool:
        # "Success" means the operation completed and returned a result; callers
        # should still inspect conflicts/errors according to their needs.
        return True


class Watcher:
    """Handle for a background folder watch owned by a :class:`Client`."""

    def __init__(self, client: Client, watch_id: int, cancel: CancelEvent | None = None):
        self._client = client
        self._id = int(watch_id)
        # Local signal only. The external cancel object (if any) is already
        # registered with the native watch; stop_watch cancels the core token.
        self._cancel_event = threading.Event()
        self._cancel_source = cancel
        self._closed = False
        self._lock = threading.Lock()

    @property
    def id(self) -> int:
        return self._id

    @property
    def closed(self) -> bool:
        return self._closed or self._client.closed

    def stop(self) -> None:
        with self._lock:
            if self._closed:
                return
            self._cancel_event.set()
            try:
                self._client.stop_watch(self._id)
            except FerriSyncError:
                # Watch may already be gone if client closed or server stopped it.
                pass
            self._closed = True

    def close(self) -> None:
        self.stop()

    def __enter__(self) -> Watcher:
        return self

    def __exit__(self, exc_type: Any, exc: Any, tb: Any) -> bool:
        self.stop()
        return False


class Client:
    """Synchronous FerriSync client.

    Wraps native FerriSync engine state. Blocking operations release the GIL
    while running Rust/Tokio work.

    Each client owns its own engine instance, folder servers, and background
    watches. Close the client (or use it as a context manager) to stop
    background activity owned by that instance.
    """

    def __init__(self, data_dir: str | Path | None = None):
        dd = str(Path(data_dir).expanduser()) if data_dir is not None else None
        self._native = _NativeClient(dd)
        self._watchers_lock = threading.Lock()
        self._watchers: dict[int, Watcher] = {}

    def __enter__(self) -> Client:
        return self

    def __exit__(self, exc_type: Any, exc: Any, tb: Any) -> bool:
        self.close()
        return False

    def close(self) -> None:
        with self._watchers_lock:
            watchers = list(self._watchers.values())
            self._watchers.clear()
        for w in watchers:
            # Mark closed locally; native close stops owned watches/servers.
            w._cancel_event.set()
            w._closed = True
        self._native.close()

    @property
    def closed(self) -> bool:
        return bool(self._native.is_closed())

    @property
    def data_dir(self) -> str:
        return str(self._native.data_dir())

    @property
    def device_id(self) -> str:
        return str(self._native.device_id())

    @property
    def device_name(self) -> str:
        return str(self._native.device_name())

    def rename(self, name: str) -> str:
        return str(self._native.set_device_name(name))

    def discover(self, timeout: int = 5) -> list[DiscoveredPeer]:
        peers = self._native.discover(int(timeout))
        return [DiscoveredPeer.from_dict(p) for p in peers]

    def pair(self, ip: str, port: int = 9847) -> str:
        """Pair with a peer at ip:port. Returns the paired device id."""
        return str(self._native.pair(str(ip), int(port)))

    def devices(self) -> list[dict[str, Any]]:
        return list(self._native.devices())

    def remove_device(self, device_id: str) -> dict[str, Any]:
        return dict(self._native.remove_device(str(device_id)))

    def add_folder(
        self,
        local_path: str | Path,
        device_id: str,
        direction: str = "bidirectional",
    ) -> int:
        return int(
            self._native.add_folder(
                str(Path(local_path).expanduser()),
                str(device_id),
                str(direction),
            )
        )

    def folders(self) -> list[dict[str, Any]]:
        return list(self._native.folders())

    def remove_folder(self, folder_id: int) -> None:
        self._native.remove_folder(int(folder_id))

    def serve(
        self,
        folder_id: int,
        local_path: str | Path,
        port: int = 9847,
        auto_accept: bool = False,
    ) -> int:
        """Start serving a folder for incoming sync/pairing. Returns the bound port.

        If auto_accept is True, unknown peers are admitted automatically.
        WARNING: this grants read/write access to anyone who can reach this host
        on a trusted private network only.
        """
        return int(
            self._native.serve(
                int(folder_id),
                str(Path(local_path).expanduser()),
                int(port),
                bool(auto_accept),
            )
        )

    def stop_servers(self) -> None:
        self._native.stop_servers()

    def start_watch(
        self,
        folder_id: int,
        local_path: str | Path,
        remote_ip: str,
        remote_port: int,
        device_id: str,
        cancel: CancelEvent | None = None,
    ) -> int:
        """Start a background watch that syncs ``local_path`` to the peer on change.

        Returns a watch id. Prefer :meth:`watch` for a managed :class:`Watcher`.
        """
        return int(
            self._native.start_watch(
                int(folder_id),
                str(Path(local_path).expanduser()),
                str(remote_ip),
                int(remote_port),
                str(device_id),
                cancel,
            )
        )

    def watch(
        self,
        folder_id: int,
        local_path: str | Path,
        remote_ip: str,
        remote_port: int,
        device_id: str,
        cancel: CancelEvent | None = None,
    ) -> Watcher:
        """Start a background watch and return a :class:`Watcher` handle."""
        watch_id = self.start_watch(
            folder_id, local_path, remote_ip, remote_port, device_id, cancel
        )
        watcher = Watcher(self, watch_id, cancel)
        with self._watchers_lock:
            self._watchers[watch_id] = watcher
        return watcher

    def stop_watch(self, watch_id: int) -> None:
        self._native.stop_watch(int(watch_id))
        with self._watchers_lock:
            w = self._watchers.pop(int(watch_id), None)
        if w is not None:
            w._cancel_event.set()
            w._closed = True

    def list_watches(self) -> list[dict[str, Any]]:
        entries = [dict(e) for e in self._native.list_watches()]
        live_ids = {int(e["id"]) for e in entries}
        with self._watchers_lock:
            for watch_id, watcher in list(self._watchers.items()):
                if watch_id not in live_ids:
                    watcher._cancel_event.set()
                    watcher._closed = True
                    self._watchers.pop(watch_id, None)
        return entries

    def pending_pairings(self) -> list[tuple[str, str]]:
        return [(str(a), str(b)) for a, b in self._native.pending_pairings()]

    def approve_pairing(self, device_id: str, device_name: str) -> None:
        self._native.approve_pairing(str(device_id), str(device_name))

    def deny_pairing(self, device_id: str) -> None:
        self._native.deny_pairing(str(device_id))

    def sync(
        self,
        folder_id: int,
        local_path: str | Path,
        remote_ip: str,
        remote_port: int,
        device_id: str,
        dry_run: bool = False,
        cancel: CancelEvent | None = None,
    ) -> SyncResult:
        """Run a sync operation.

        ``cancel`` is a cooperative cancellation signal. It is polled while the
        native call runs and forwarded to the core token, so in-flight
        connect/handshake/transfer steps stop at their next boundary. A file
        already committed before cancellation remains valid; the cancelled
        session returns ``FerriSyncCancelledError`` instead of a result.
        """
        result = self._native.sync(
            int(folder_id),
            str(Path(local_path).expanduser()),
            str(remote_ip),
            int(remote_port),
            str(device_id),
            bool(dry_run),
            cancel,
        )
        return SyncResult.from_dict(dict(result))

    def poll_events(self) -> list[dict[str, Any]]:
        return [dict(e) for e in self._native.poll_events()]

    def events(
        self,
        poll_interval: float = 0.25,
        stop_event: threading.Event | None = None,
    ) -> Iterator[dict[str, Any]]:
        """Yield sync events by polling the engine.

        This is a convenience iterator for consumers that want a push-like loop
        without managing polling manually. It stops when ``stop_event`` is set
        or the client is closed.
        """
        interval = max(float(poll_interval), 0.01)
        local_stop = stop_event or threading.Event()
        while not local_stop.is_set() and not self.closed:
            events = self.poll_events()
            if events:
                yield from events
                continue
            local_stop.wait(interval)

    def subscribe(
        self,
        callback: Callable[[dict[str, Any]], Any],
        poll_interval: float = 0.25,
        daemon: bool = True,
        on_error: Callable[[BaseException], Any] | None = None,
    ) -> threading.Event:
        """Run ``callback(event)`` in a background polling thread.

        Returns a :class:`threading.Event` that stops the subscription when set.
        """
        stop_event = threading.Event()

        def _run() -> None:
            try:
                for event in self.events(poll_interval=poll_interval, stop_event=stop_event):
                    try:
                        callback(event)
                    except BaseException as exc:  # callback errors must not kill subscription
                        if on_error is not None:
                            on_error(exc)
            except BaseException as exc:
                if on_error is not None:
                    on_error(exc)

        threading.Thread(target=_run, name="ferrisync-events", daemon=daemon).start()
        return stop_event

    def sessions(self, limit: int = 20) -> list[dict[str, Any]]:
        return list(self._native.sessions(int(limit)))

    def conflicts(self) -> list[dict[str, Any]]:
        return list(self._native.conflicts())

    def read_conflict(self, folder_id: int, winner_path: str, loser_path: str) -> dict[str, Any]:
        return dict(self._native.read_conflict(int(folder_id), str(winner_path), str(loser_path)))

    def status(self) -> dict[str, Any]:
        return {
            "health": self._native.overall_health(),
            "devices": self._native.device_statuses(),
            "folders": self._native.folder_statuses(),
        }

    def diagnostics(self) -> list[dict[str, Any]]:
        return list(self._native.diagnostics())

    def shared_folders(self) -> list[dict[str, Any]]:
        return list(self._native.shared_folders())

    def pending_folder_pairings(self) -> list[dict[str, Any]]:
        return list(self._native.pending_folder_pairings())

    def file_history(self, folder_id: int | None = None, limit: int = 50) -> list[dict[str, Any]]:
        return list(self._native.file_history(folder_id, int(limit)))

    # --- Convenience helpers ---

    def find_device(self, name_or_id: str) -> dict[str, Any] | None:
        """Find a paired device by exact id, exact name, or unique name/id prefix."""
        needle = str(name_or_id)
        devices = self.devices()
        for d in devices:
            if d.get("id") == needle or d.get("name") == needle:
                return d
        matches = [
            d for d in devices
            if str(d.get("id", "")).startswith(needle) or str(d.get("name", "")).startswith(needle)
        ]
        if len(matches) == 1:
            return matches[0]
        if len(matches) > 1:
            matched_names = [m.get("name") or m.get("id") for m in matches]
            raise FerriSyncError(f"ambiguous device {needle!r}: matches {matched_names}")
        return None

    def find_folder(self, local_path: str | Path) -> dict[str, Any] | None:
        """Find a configured sync folder by local path."""
        target = str(Path(local_path).expanduser())
        for f in self.folders():
            if f.get("local_path") == target:
                return f
        return None

    def resolve_conflict(self, folder_id: int, backup_path: str, action: str) -> str:
        """Resolve a conflict backup.

        Convenience action names: this/keep_backup, other/keep_original, both/keep_both.
        Core action strings are also accepted.
        """
        mapping = {
            "this": "keep_backup",
            "keep_backup": "keep_backup",
            "other": "keep_original",
            "keep_original": "keep_original",
            "both": "keep_both",
            "keep_both": "keep_both",
        }
        core_action = mapping.get(action, action)
        return str(
            self._native.resolve_conflict(int(folder_id), str(backup_path), str(core_action))
        )


class AsyncWatcher:
    """Async handle for a background folder watch."""

    def __init__(self, watcher: Watcher):
        self._watcher = watcher

    @property
    def id(self) -> int:
        return self._watcher.id

    @property
    def closed(self) -> bool:
        return self._watcher.closed

    async def stop(self) -> None:
        await asyncio.to_thread(self._watcher.stop)

    async def close(self) -> None:
        await self.stop()

    async def __aenter__(self) -> AsyncWatcher:
        return self

    async def __aexit__(self, exc_type: Any, exc: Any, tb: Any) -> bool:
        await self.stop()
        return False


class AsyncClient:
    """Async wrapper around :class:`Client`.

    Uses a bounded executor so blocking native operations do not starve the
    event loop. ``asyncio.Event`` cancellation signals are polled by the native
    bridge and forwarded to the core token, so in-flight sync steps stop at
    their next boundary. Cancelled awaitables raise ``asyncio.CancelledError``;
    a file committed before cancellation remains valid for the next sync.
    """

    def __init__(self, data_dir: str | Path | None = None, max_workers: int = 4):
        self._data_dir = data_dir
        self._client: Client | None = None
        self._max_workers = max(1, int(max_workers))
        self._executor: concurrent.futures.ThreadPoolExecutor | None = None

    def _ensure_executor(self) -> concurrent.futures.ThreadPoolExecutor:
        if self._executor is None:
            self._executor = concurrent.futures.ThreadPoolExecutor(
                max_workers=self._max_workers, thread_name_prefix="ferrisync-async"
            )
        return self._executor

    async def _run(self, fn: Callable[..., Any], *args: Any, **kwargs: Any) -> Any:
        loop = asyncio.get_running_loop()
        return await loop.run_in_executor(self._ensure_executor(), lambda: fn(*args, **kwargs))

    async def __aenter__(self) -> AsyncClient:
        self._client = await self._run(Client, self._data_dir)
        return self

    async def __aexit__(self, exc_type: Any, exc: Any, tb: Any) -> bool:
        await self.close()
        return False

    @property
    def client(self) -> Client:
        if self._client is None:
            raise ClientClosedError("AsyncClient is not open")
        return self._client

    async def close(self) -> None:
        if self._client is not None:
            client = self._client
            self._client = None
            try:
                await self._run(client.close)
            finally:
                if self._executor is not None:
                    self._executor.shutdown(wait=False, cancel_futures=True)
                    self._executor = None
        elif self._executor is not None:
            self._executor.shutdown(wait=False, cancel_futures=True)
            self._executor = None

    async def discover(self, timeout: int = 5) -> list[DiscoveredPeer]:
        return await self._run(self.client.discover, timeout)

    async def pair(self, ip: str, port: int = 9847) -> str:
        return await self._run(self.client.pair, ip, port)

    async def devices(self) -> list[dict[str, Any]]:
        return await self._run(self.client.devices)

    async def add_folder(
        self, local_path: str | Path, device_id: str, direction: str = "bidirectional"
    ) -> int:
        return await self._run(self.client.add_folder, local_path, device_id, direction)

    async def folders(self) -> list[dict[str, Any]]:
        return await self._run(self.client.folders)

    async def sync(
        self,
        folder_id: int,
        local_path: str | Path,
        remote_ip: str,
        remote_port: int,
        device_id: str,
        dry_run: bool = False,
        cancel: asyncio.Event | None = None,
    ) -> SyncResult:
        if cancel is not None and cancel.is_set():
            raise asyncio.CancelledError()
        try:
            # The native bridge polls the asyncio.Event directly while the
            # worker thread is blocked, so cancellation reaches the core token.
            return await self._run(
                self.client.sync,
                folder_id,
                local_path,
                remote_ip,
                remote_port,
                device_id,
                dry_run,
                cancel,
            )
        except FerriSyncCancelledError as e:
            raise asyncio.CancelledError(str(e)) from e

    async def status(self) -> dict[str, Any]:
        return await self._run(self.client.status)

    async def poll_events(self) -> list[dict[str, Any]]:
        return await self._run(self.client.poll_events)

    async def events(
        self,
        poll_interval: float = 0.25,
        stop_event: asyncio.Event | None = None,
    ) -> AsyncIterator[dict[str, Any]]:
        """Async iterator yielding sync events by polling in a worker thread."""
        interval = max(float(poll_interval), 0.01)
        local_stop = stop_event or asyncio.Event()
        while not local_stop.is_set():
            try:
                closed = self.client.closed
            except FerriSyncError:
                break
            if closed:
                break
            events = await self.poll_events()
            if events:
                for event in events:
                    yield event
                continue
            try:
                await asyncio.wait_for(local_stop.wait(), timeout=interval)
            except asyncio.TimeoutError:
                pass

    async def subscribe(
        self,
        callback: Callable[[dict[str, Any]], Any],
        poll_interval: float = 0.25,
        on_error: Callable[[BaseException], Any] | None = None,
    ) -> asyncio.Event:
        """Run ``callback(event)`` in an asyncio task (callback may be sync or async)."""
        stop_event = asyncio.Event()

        async def _run() -> None:
            try:
                async for event in self.events(poll_interval=poll_interval, stop_event=stop_event):
                    try:
                        result = callback(event)
                        if asyncio.iscoroutine(result):
                            await result
                    except BaseException as exc:
                        if on_error is not None:
                            on_error(exc)
            except BaseException as exc:
                if on_error is not None:
                    on_error(exc)

        asyncio.create_task(_run())
        return stop_event

    async def rename(self, name: str) -> str:
        return await self._run(self.client.rename, name)

    async def sessions(self, limit: int = 20) -> list[dict[str, Any]]:
        return await self._run(self.client.sessions, limit)

    async def conflicts(self) -> list[dict[str, Any]]:
        return await self._run(self.client.conflicts)

    async def read_conflict(
        self, folder_id: int, winner_path: str, loser_path: str
    ) -> dict[str, Any]:
        return await self._run(self.client.read_conflict, folder_id, winner_path, loser_path)

    async def find_device(self, name_or_id: str) -> dict[str, Any] | None:
        return await self._run(self.client.find_device, name_or_id)

    async def find_folder(self, local_path: str | Path) -> dict[str, Any] | None:
        return await self._run(self.client.find_folder, local_path)

    async def resolve_conflict(self, folder_id: int, backup_path: str, action: str) -> str:
        return await self._run(self.client.resolve_conflict, folder_id, backup_path, action)

    async def serve(
        self, folder_id: int, local_path: str | Path, port: int = 9847, auto_accept: bool = False
    ) -> int:
        return await self._run(self.client.serve, folder_id, local_path, port, auto_accept)

    async def stop_servers(self) -> None:
        await self._run(self.client.stop_servers)

    async def start_watch(
        self,
        folder_id: int,
        local_path: str | Path,
        remote_ip: str,
        remote_port: int,
        device_id: str,
        cancel: asyncio.Event | None = None,
    ) -> int:
        if cancel is not None and cancel.is_set():
            raise asyncio.CancelledError()
        # The native bridge polls the asyncio.Event while the worker thread is
        # blocked in start_watch/engine setup; no asyncio→threading task needed.
        return await self._run(
            self.client.start_watch,
            folder_id,
            local_path,
            remote_ip,
            remote_port,
            device_id,
            cancel,
        )

    async def watch(
        self,
        folder_id: int,
        local_path: str | Path,
        remote_ip: str,
        remote_port: int,
        device_id: str,
        cancel: asyncio.Event | None = None,
    ) -> AsyncWatcher:
        if cancel is not None and cancel.is_set():
            raise asyncio.CancelledError()
        watcher = await self._run(
            self.client.watch,
            folder_id,
            local_path,
            remote_ip,
            remote_port,
            device_id,
            cancel,
        )
        if cancel is not None and cancel.is_set():
            await self._run(watcher.stop)
            raise asyncio.CancelledError()
        return AsyncWatcher(watcher)

    async def stop_watch(self, watch_id: int) -> None:
        await self._run(self.client.stop_watch, watch_id)

    async def list_watches(self) -> list[dict[str, Any]]:
        return await self._run(self.client.list_watches)

    async def remove_device(self, device_id: str) -> dict[str, Any]:
        return await self._run(self.client.remove_device, device_id)

    async def remove_folder(self, folder_id: int) -> None:
        await self._run(self.client.remove_folder, folder_id)

    async def diagnostics(self) -> list[dict[str, Any]]:
        return await self._run(self.client.diagnostics)

    async def file_history(
        self, folder_id: int | None = None, limit: int = 50
    ) -> list[dict[str, Any]]:
        return await self._run(self.client.file_history, folder_id, limit)
