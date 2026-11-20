from __future__ import annotations

import asyncio
import threading
from collections.abc import AsyncIterator, Callable, Iterator
from dataclasses import dataclass
from pathlib import Path
from typing import Any

__version__: str

CancelEvent = threading.Event | asyncio.Event

class FerriSyncError(RuntimeError): ...
class ClientClosedError(FerriSyncError): ...
class FerriSyncCancelledError(FerriSyncError): ...
class InvalidArgumentError(FerriSyncError): ...
class ConfigurationError(FerriSyncError): ...
class NotFoundError(FerriSyncError): ...
class ConnectionError(FerriSyncError): ...
class SyncError(FerriSyncError): ...

@dataclass
class DiscoveredPeer:
    id: str
    name: str
    ip: str
    port: int
    @classmethod
    def from_dict(cls, d: dict[str, Any]) -> DiscoveredPeer:
        """Raises: ClientClosedError, FerriSyncError."""

@dataclass
class SyncResult:
    pulled: list[str]
    pushed: list[str]
    conflicts: list[str]
    pulled_bytes: int
    pushed_bytes: int
    @classmethod
    def from_dict(cls, d: dict[str, Any]) -> SyncResult:
        """Raises: ClientClosedError, FerriSyncError."""
    @property
    def success(self) -> bool:
        """Raises: ClientClosedError, FerriSyncError."""

class Watcher:
    id: int
    @property
    def closed(self) -> bool:
        """Raises: ClientClosedError."""
    def stop(self) -> None:
        """Idempotent; does not raise on repeated calls."""
    def close(self) -> None:
        """Idempotent; does not raise on repeated calls."""
    def __enter__(self) -> Watcher:
        """Idempotent; does not raise on repeated calls."""
    def __exit__(self, exc_type: Any, exc: Any, tb: Any) -> bool:
        """Idempotent; does not raise on repeated calls."""

class AsyncWatcher:
    @property
    def id(self) -> int:
        """Cached watch id; does not raise."""
    @property
    def closed(self) -> bool:
        """Cached closed state; does not raise."""
    async def stop(self) -> None:
        """Idempotent; does not raise on repeated calls."""
    async def close(self) -> None:
        """Idempotent; does not raise on repeated calls."""
    async def __aenter__(self) -> AsyncWatcher:
        """Returns this handle without raising."""
    async def __aexit__(self, exc_type: Any, exc: Any, tb: Any) -> bool:
        """Idempotent stop; does not raise on repeated calls."""

class Client:
    def __init__(self, data_dir: str | Path | None = ...) -> None:
        """Raises: InvalidArgumentError, FerriSyncError."""
    def __enter__(self) -> Client:
        """Idempotent; does not raise on repeated calls."""
    def __exit__(self, exc_type: Any, exc: Any, tb: Any) -> bool:
        """Idempotent; may raise FerriSyncError if shutdown fails."""
    def close(self) -> None:
        """Idempotent; may raise FerriSyncError if shutdown fails."""
    @property
    def closed(self) -> bool:
        """Raises: ClientClosedError."""
    @property
    def data_dir(self) -> str:
        """Raises: ClientClosedError."""
    @property
    def device_id(self) -> str:
        """Raises: ClientClosedError."""
    @property
    def device_name(self) -> str:
        """Raises: ClientClosedError."""
    def rename(self, name: str) -> str:
        """Raises: ClientClosedError, InvalidArgumentError."""
    def discover(self, timeout: int = ...) -> list[DiscoveredPeer]:
        """Raises: ClientClosedError, ConnectionError, FerriSyncError."""
    def pair(self, ip: str, port: int = ...) -> str:
        """Raises: ClientClosedError, InvalidArgumentError, ConnectionError, SyncError."""
    def devices(self) -> list[dict[str, Any]]:
        """Raises: ClientClosedError, FerriSyncError."""
    def remove_device(self, device_id: str) -> dict[str, Any]:
        """Raises: ClientClosedError, InvalidArgumentError, NotFoundError."""
    def add_folder(
        self, local_path: str | Path, device_id: str, direction: str = ...
    ) -> int:
        """Raises: ClientClosedError, InvalidArgumentError, NotFoundError."""
    def folders(self) -> list[dict[str, Any]]:
        """Raises: ClientClosedError, FerriSyncError."""
    def remove_folder(self, folder_id: int) -> None:
        """Raises: ClientClosedError, InvalidArgumentError, NotFoundError."""
    def serve(
        self, folder_id: int, local_path: str | Path, port: int = ..., auto_accept: bool = ...
    ) -> int:
        """Raises: ClientClosedError, InvalidArgumentError, NotFoundError, ConnectionError."""
    def stop_servers(self) -> None:
        """Raises: ClientClosedError, FerriSyncError."""
    def start_watch(
        self,
        folder_id: int,
        local_path: str | Path,
        remote_ip: str,
        remote_port: int,
        device_id: str,
        cancel: CancelEvent | None = ...,
    ) -> int:
        """Raises: ClientClosedError, FerriSyncCancelledError, InvalidArgumentError, NotFoundError,
        ConnectionError, SyncError."""
    def watch(
        self,
        folder_id: int,
        local_path: str | Path,
        remote_ip: str,
        remote_port: int,
        device_id: str,
        cancel: CancelEvent | None = ...,
    ) -> Watcher:
        """Raises: ClientClosedError, FerriSyncCancelledError, InvalidArgumentError, NotFoundError,
        ConnectionError, SyncError."""
    def stop_watch(self, watch_id: int) -> None:
        """Raises: ClientClosedError, InvalidArgumentError, NotFoundError."""
    def list_watches(self) -> list[dict[str, Any]]:
        """Raises: ClientClosedError, FerriSyncError."""
    def pending_pairings(self) -> list[tuple[str, str]]:
        """Raises: ClientClosedError, FerriSyncError."""
    def approve_pairing(self, device_id: str, device_name: str) -> None:
        """Raises: ClientClosedError, InvalidArgumentError, NotFoundError."""
    def deny_pairing(self, device_id: str) -> None:
        """Raises: ClientClosedError, InvalidArgumentError, NotFoundError."""
    def sync(
        self,
        folder_id: int,
        local_path: str | Path,
        remote_ip: str,
        remote_port: int,
        device_id: str,
        dry_run: bool = ...,
        cancel: CancelEvent | None = ...,
    ) -> SyncResult:
        """Raises: ClientClosedError, FerriSyncCancelledError, InvalidArgumentError, NotFoundError,
        ConnectionError, SyncError."""
    def poll_events(self) -> list[dict[str, Any]]:
        """Raises: ClientClosedError, FerriSyncError."""
    def events(
        self, poll_interval: float = ..., stop_event: threading.Event | None = ...
    ) -> Iterator[dict[str, Any]]:
        """Raises: ClientClosedError, FerriSyncError."""
    def subscribe(
        self,
        callback: Callable[[dict[str, Any]], Any],
        poll_interval: float = ...,
        daemon: bool = ...,
        on_error: Callable[[BaseException], Any] | None = ...,
    ) -> threading.Event:
        """Raises: ClientClosedError, FerriSyncError."""
    def sessions(self, limit: int = ...) -> list[dict[str, Any]]:
        """Raises: ClientClosedError, FerriSyncError."""
    def conflicts(self) -> list[dict[str, Any]]:
        """Raises: ClientClosedError, FerriSyncError."""
    def resolve_conflict(self, folder_id: int, backup_path: str, action: str) -> str:
        """Raises: ClientClosedError, InvalidArgumentError, NotFoundError, SyncError."""
    def read_conflict(
        self, folder_id: int, winner_path: str, loser_path: str
    ) -> dict[str, Any]:
        """Raises: ClientClosedError, InvalidArgumentError, NotFoundError, SyncError."""
    def status(self) -> dict[str, Any]:
        """Raises: ClientClosedError, FerriSyncError."""
    def diagnostics(self) -> list[dict[str, Any]]:
        """Raises: ClientClosedError, FerriSyncError."""
    def shared_folders(self) -> list[dict[str, Any]]:
        """Raises: ClientClosedError, FerriSyncError."""
    def pending_folder_pairings(self) -> list[dict[str, Any]]:
        """Raises: ClientClosedError, FerriSyncError."""
    def file_history(
        self, folder_id: int | None = ..., limit: int = ...
    ) -> list[dict[str, Any]]:
        """Raises: ClientClosedError, FerriSyncError."""
    def find_device(self, name_or_id: str) -> dict[str, Any] | None:
        """Raises: ClientClosedError, FerriSyncError."""
    def find_folder(self, local_path: str | Path) -> dict[str, Any] | None:
        """Raises: ClientClosedError, FerriSyncError."""

class AsyncClient:
    def __init__(self, data_dir: str | Path | None = ..., max_workers: int = ...) -> None:
        """Raises: InvalidArgumentError, FerriSyncError."""
    async def __aenter__(self) -> AsyncClient:
        """Raises: ClientClosedError, FerriSyncError."""
    async def __aexit__(self, exc_type: Any, exc: Any, tb: Any) -> bool:
        """Raises: ClientClosedError, FerriSyncError."""
    @property
    def client(self) -> Client:
        """Raises: ClientClosedError."""
    async def close(self) -> None:
        """Idempotent; may raise FerriSyncError if shutdown fails."""
    async def discover(self, timeout: int = ...) -> list[DiscoveredPeer]:
        """Raises: ClientClosedError, ConnectionError, FerriSyncError."""
    async def pair(self, ip: str, port: int = ...) -> str:
        """Raises: ClientClosedError, InvalidArgumentError, ConnectionError, SyncError."""
    async def devices(self) -> list[dict[str, Any]]:
        """Raises: ClientClosedError, FerriSyncError."""
    async def add_folder(
        self, local_path: str | Path, device_id: str, direction: str = ...
    ) -> int:
        """Raises: ClientClosedError, InvalidArgumentError, NotFoundError."""
    async def folders(self) -> list[dict[str, Any]]:
        """Raises: ClientClosedError, FerriSyncError."""
    async def sync(
        self,
        folder_id: int,
        local_path: str | Path,
        remote_ip: str,
        remote_port: int,
        device_id: str,
        dry_run: bool = ...,
        cancel: asyncio.Event | None = ...,
    ) -> SyncResult:
        """Raises: ClientClosedError, asyncio.CancelledError, InvalidArgumentError, NotFoundError,
        ConnectionError, SyncError."""
    async def status(self) -> dict[str, Any]:
        """Raises: ClientClosedError, FerriSyncError."""
    async def poll_events(self) -> list[dict[str, Any]]:
        """Raises: ClientClosedError, FerriSyncError."""
    def events(
        self, poll_interval: float = ..., stop_event: asyncio.Event | None = ...
    ) -> AsyncIterator[dict[str, Any]]:
        """Raises: ClientClosedError, FerriSyncError."""
    async def subscribe(
        self,
        callback: Callable[[dict[str, Any]], Any],
        poll_interval: float = ...,
        on_error: Callable[[BaseException], Any] | None = ...,
    ) -> asyncio.Event:
        """Raises: ClientClosedError, FerriSyncError."""
    async def rename(self, name: str) -> str:
        """Raises: ClientClosedError, InvalidArgumentError."""
    async def sessions(self, limit: int = ...) -> list[dict[str, Any]]:
        """Raises: ClientClosedError, FerriSyncError."""
    async def conflicts(self) -> list[dict[str, Any]]:
        """Raises: ClientClosedError, FerriSyncError."""
    async def read_conflict(
        self, folder_id: int, winner_path: str, loser_path: str
    ) -> dict[str, Any]:
        """Raises: ClientClosedError, InvalidArgumentError, NotFoundError, SyncError."""
    async def find_device(self, name_or_id: str) -> dict[str, Any] | None:
        """Raises: ClientClosedError, FerriSyncError."""
    async def find_folder(self, local_path: str | Path) -> dict[str, Any] | None:
        """Raises: ClientClosedError, FerriSyncError."""
    async def resolve_conflict(self, folder_id: int, backup_path: str, action: str) -> str:
        """Raises: ClientClosedError, InvalidArgumentError, NotFoundError, SyncError."""
    async def serve(
        self, folder_id: int, local_path: str | Path, port: int = ..., auto_accept: bool = ...
    ) -> int:
        """Raises: ClientClosedError, InvalidArgumentError, NotFoundError, ConnectionError."""
    async def stop_servers(self) -> None:
        """Raises: ClientClosedError, FerriSyncError."""
    async def start_watch(
        self,
        folder_id: int,
        local_path: str | Path,
        remote_ip: str,
        remote_port: int,
        device_id: str,
        cancel: asyncio.Event | None = ...,
    ) -> int:
        """Raises: ClientClosedError, asyncio.CancelledError, InvalidArgumentError, NotFoundError,
        ConnectionError, SyncError."""
    async def watch(
        self,
        folder_id: int,
        local_path: str | Path,
        remote_ip: str,
        remote_port: int,
        device_id: str,
        cancel: asyncio.Event | None = ...,
    ) -> AsyncWatcher:
        """Raises: ClientClosedError, asyncio.CancelledError, InvalidArgumentError, NotFoundError,
        ConnectionError, SyncError."""
    async def stop_watch(self, watch_id: int) -> None:
        """Raises: ClientClosedError, InvalidArgumentError, NotFoundError."""
    async def list_watches(self) -> list[dict[str, Any]]:
        """Raises: ClientClosedError, FerriSyncError."""
    async def remove_device(self, device_id: str) -> dict[str, Any]:
        """Raises: ClientClosedError, InvalidArgumentError, NotFoundError."""
    async def remove_folder(self, folder_id: int) -> None:
        """Raises: ClientClosedError, InvalidArgumentError, NotFoundError."""
    async def diagnostics(self) -> list[dict[str, Any]]:
        """Raises: ClientClosedError, FerriSyncError."""
    async def file_history(
        self, folder_id: int | None = ..., limit: int = ...
    ) -> list[dict[str, Any]]:
        """Raises: ClientClosedError, FerriSyncError."""
