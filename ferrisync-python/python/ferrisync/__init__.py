"""Python bindings for the FerriSync synchronization engine.

This package provides a thin, typed(ish) Python API over the Rust
``ferrisync-core`` engine using PyO3/maturin.

Status: 0.1.0 developer preview MVP. The API may change.
"""

from __future__ import annotations

import asyncio
from dataclasses import dataclass
from pathlib import Path
from typing import Any, Optional

from ._native import Client as _NativeClient
from ._native import __version__

__all__ = [
    "__version__",
    "Client",
    "AsyncClient",
    "DiscoveredPeer",
    "SyncResult",
    "FerriSyncError",
]


class FerriSyncError(RuntimeError):
    """Base exception for FerriSync Python errors."""


@dataclass
class DiscoveredPeer:
    id: str
    name: str
    ip: str
    port: int

    @classmethod
    def from_dict(cls, d: dict[str, Any]) -> "DiscoveredPeer":
        return cls(id=d["id"], name=d["name"], ip=d["ip"], port=int(d["port"]))


@dataclass
class SyncResult:
    pulled: list[str]
    pushed: list[str]
    conflicts: list[str]
    pulled_bytes: int
    pushed_bytes: int

    @classmethod
    def from_dict(cls, d: dict[str, Any]) -> "SyncResult":
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


class Client:
    """Synchronous FerriSync client.

    Wraps the native engine state. Most operations block the calling thread and
    release the GIL while running Rust/Tokio work.

    Note (0.1.0): the underlying core uses a global server registry, so running
    multiple active clients/servers in the same process is not recommended.
    """

    def __init__(self, data_dir: Optional[str | Path] = None):
        dd = str(Path(data_dir).expanduser()) if data_dir is not None else None
        self._native = _NativeClient(dd)

    def __enter__(self) -> "Client":
        return self

    def __exit__(self, exc_type, exc, tb) -> bool:
        self.close()
        return False

    def close(self) -> None:
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
    ) -> SyncResult:
        result = self._native.sync(
            int(folder_id),
            str(Path(local_path).expanduser()),
            str(remote_ip),
            int(remote_port),
            str(device_id),
            bool(dry_run),
        )
        return SyncResult.from_dict(dict(result))

    def poll_events(self) -> list[dict[str, Any]]:
        return [dict(e) for e in self._native.poll_events()]

    def sessions(self, limit: int = 20) -> list[dict[str, Any]]:
        return list(self._native.sessions(int(limit)))

    def conflicts(self) -> list[dict[str, Any]]:
        return list(self._native.conflicts())

    def resolve_conflict(self, folder_id: int, backup_path: str, action: str) -> str:
        """Resolve a conflict backup. action is one of the core actions (e.g. keep_backup, keep_original, keep_both)."""
        return str(self._native.resolve_conflict(int(folder_id), str(backup_path), str(action)))

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

    def file_history(self, folder_id: Optional[int] = None, limit: int = 50) -> list[dict[str, Any]]:
        return list(self._native.file_history(folder_id, int(limit)))


class AsyncClient:
    """Async wrapper around :class:`Client` using ``asyncio.to_thread``.

    This avoids blocking the event loop but does not make the core calls truly
    non-blocking internally. Suitable for 0.1/0.2 usage.
    """

    def __init__(self, data_dir: Optional[str | Path] = None):
        self._data_dir = data_dir
        self._client: Optional[Client] = None

    async def __aenter__(self) -> "AsyncClient":
        self._client = await asyncio.to_thread(Client, self._data_dir)
        return self

    async def __aexit__(self, exc_type, exc, tb) -> bool:
        await self.close()
        return False

    @property
    def client(self) -> Client:
        if self._client is None:
            raise FerriSyncError("AsyncClient is not open")
        return self._client

    async def close(self) -> None:
        if self._client is not None:
            await asyncio.to_thread(self._client.close)
            self._client = None

    async def discover(self, timeout: int = 5) -> list[DiscoveredPeer]:
        return await asyncio.to_thread(self.client.discover, timeout)

    async def pair(self, ip: str, port: int = 9847) -> str:
        return await asyncio.to_thread(self.client.pair, ip, port)

    async def devices(self) -> list[dict[str, Any]]:
        return await asyncio.to_thread(self.client.devices)

    async def add_folder(
        self, local_path: str | Path, device_id: str, direction: str = "bidirectional"
    ) -> int:
        return await asyncio.to_thread(self.client.add_folder, local_path, device_id, direction)

    async def folders(self) -> list[dict[str, Any]]:
        return await asyncio.to_thread(self.client.folders)

    async def sync(
        self,
        folder_id: int,
        local_path: str | Path,
        remote_ip: str,
        remote_port: int,
        device_id: str,
        dry_run: bool = False,
    ) -> SyncResult:
        return await asyncio.to_thread(
            self.client.sync,
            folder_id,
            local_path,
            remote_ip,
            remote_port,
            device_id,
            dry_run,
        )

    async def status(self) -> dict[str, Any]:
        return await asyncio.to_thread(self.client.status)

    async def poll_events(self) -> list[dict[str, Any]]:
        return await asyncio.to_thread(self.client.poll_events)

    async def rename(self, name: str) -> str:
        return await asyncio.to_thread(self.client.rename, name)

    async def sessions(self, limit: int = 20) -> list[dict[str, Any]]:
        return await asyncio.to_thread(self.client.sessions, limit)

    async def conflicts(self) -> list[dict[str, Any]]:
        return await asyncio.to_thread(self.client.conflicts)
