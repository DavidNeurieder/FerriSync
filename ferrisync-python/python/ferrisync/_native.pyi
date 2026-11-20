from __future__ import annotations

import asyncio
import threading
from typing import Any, Protocol

__version__: str

class _SupportsIsSet(Protocol):
    def is_set(self) -> bool:
        """Raises: ClientClosedError, FerriSyncError."""

# Both threading.Event and asyncio.Event are polled by the native bridge.
CancelEvent = threading.Event | asyncio.Event | _SupportsIsSet

class FerriSyncError(RuntimeError): ...
class ClientClosedError(FerriSyncError): ...
class FerriSyncCancelledError(FerriSyncError): ...
class InvalidArgumentError(FerriSyncError): ...
class ConfigurationError(FerriSyncError): ...
class NotFoundError(FerriSyncError): ...
class ConnectionError(FerriSyncError): ...
class SyncError(FerriSyncError): ...

class Client:
    def __init__(self, data_dir: str | None = ...) -> None:
        """Raises: InvalidArgumentError, FerriSyncError."""
    def __enter__(self) -> Client:
        """Idempotent; does not raise on repeated calls."""
    def __exit__(self, _exc_type: Any = ..., _exc: Any = ..., _tb: Any = ...) -> bool:
        """Idempotent; may raise FerriSyncError if shutdown fails."""
    def close(self) -> None:
        """Idempotent; may raise FerriSyncError if shutdown fails."""
    def is_closed(self) -> bool:
        """Idempotent; does not raise on repeated calls."""
    def data_dir(self) -> str:
        """Raises: ClientClosedError."""
    def device_id(self) -> str:
        """Raises: ClientClosedError."""
    def device_name(self) -> str:
        """Raises: ClientClosedError."""
    def set_device_name(self, name: str) -> str:
        """Raises: ClientClosedError, InvalidArgumentError."""
    def discover(self, timeout: int = ...) -> list[dict[str, Any]]:
        """Raises: ClientClosedError, ConnectionError, FerriSyncError."""
    def pair(self, ip: str, port: int) -> str:
        """Raises: ClientClosedError, InvalidArgumentError, ConnectionError, SyncError."""
    def devices(self) -> list[dict[str, Any]]:
        """Raises: ClientClosedError, FerriSyncError."""
    def remove_device(self, device_id: str) -> dict[str, Any]:
        """Raises: ClientClosedError, InvalidArgumentError, NotFoundError."""
    def add_folder(self, local_path: str, device_id: str, direction: str = ...) -> int:
        """Raises: ClientClosedError, InvalidArgumentError, NotFoundError."""
    def folders(self) -> list[dict[str, Any]]:
        """Raises: ClientClosedError, FerriSyncError."""
    def remove_folder(self, folder_id: int) -> None:
        """Raises: ClientClosedError, InvalidArgumentError, NotFoundError."""
    def serve(
        self, folder_id: int, local_path: str, port: int = ..., auto_accept: bool = ...
    ) -> int:
        """Raises: ClientClosedError, InvalidArgumentError, NotFoundError, ConnectionError."""
    def stop_servers(self) -> None:
        """Raises: ClientClosedError, FerriSyncError."""
    def start_watch(
        self,
        folder_id: int,
        local_path: str,
        remote_ip: str,
        remote_port: int,
        device_id: str,
        cancel: CancelEvent | None = ...,
    ) -> int:
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
        local_path: str,
        remote_ip: str,
        remote_port: int,
        device_id: str,
        dry_run: bool = ...,
        cancel: CancelEvent | None = ...,
    ) -> dict[str, Any]:
        """Raises: ClientClosedError, FerriSyncCancelledError, InvalidArgumentError, NotFoundError,
        ConnectionError, SyncError."""
    def poll_events(self) -> list[dict[str, Any]]:
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
    def overall_health(self) -> dict[str, Any]:
        """Raises: ClientClosedError, FerriSyncError."""
    def device_statuses(self) -> list[dict[str, Any]]:
        """Raises: ClientClosedError, FerriSyncError."""
    def folder_statuses(self) -> list[dict[str, Any]]:
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
