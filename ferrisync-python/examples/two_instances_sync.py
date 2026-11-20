"""Sync a folder between two independent Python processes.

Each process creates its own ``ferrisync.Client`` with a separate data dir,
folder, and identity. The first process hosts/serves the folder; the second one
pairs with it and runs a bidirectional sync over loopback.

Run with:

    python ferrisync-python/examples/two_instances_sync.py
"""

from __future__ import annotations

import multiprocessing as mp
import tempfile
from pathlib import Path

import ferrisync


def _serve(
    server_dir: Path,
    data_dir: Path,
    ready: mp.Queue,
    stop: mp.Event,
) -> None:
    """Instance A: host a folder and keep serving until the parent stops us."""
    try:
        with ferrisync.Client(data_dir) as server:
            folder_id = server.add_folder(server_dir, server.device_id, "bidirectional")
            # auto_accept is fine for this loopback-only demo. On a LAN, leave it
            # disabled and approve unknown peers explicitly instead.
            port = server.serve(folder_id, server_dir, port=0, auto_accept=True)
            ready.put(("ok", port))
            stop.wait()
    except BaseException as exc:
        ready.put(("error", repr(exc)))


def _sync(
    client_dir: Path,
    data_dir: Path,
    port: int,
    result: mp.Queue,
) -> None:
    """Instance B: pair with instance A and sync both directions."""
    try:
        with ferrisync.Client(data_dir) as client:
            peer = client.pair("127.0.0.1", port)
            folder_id = client.add_folder(client_dir, peer, "bidirectional")
            sync = client.sync(folder_id, client_dir, "127.0.0.1", port, peer)
            result.put(("ok", peer, list(sync.pulled), list(sync.pushed)))
    except BaseException as exc:
        result.put(("error", repr(exc), [], []))


def main() -> None:
    # "spawn" guarantees genuinely separate Python interpreters rather than a
    # fork that inherits the parent's state.
    ctx = mp.get_context("spawn")

    with tempfile.TemporaryDirectory(prefix="ferrisync-two-instances-") as root:
        root_path = Path(root)
        server_dir = root_path / "server-files"
        client_dir = root_path / "client-files"
        server_data = root_path / "server-data"
        client_data = root_path / "client-data"
        for path in (server_dir, client_dir, server_data, client_data):
            path.mkdir()

        (server_dir / "from_server.txt").write_text("hello from instance A", encoding="utf-8")
        (client_dir / "from_client.txt").write_text("hello from instance B", encoding="utf-8")

        ready: mp.Queue = ctx.Queue()
        result: mp.Queue = ctx.Queue()
        stop = ctx.Event()

        server = ctx.Process(target=_serve, args=(server_dir, server_data, ready, stop))
        server.start()
        try:
            status, payload = ready.get(timeout=30)
            if status != "ok":
                raise RuntimeError(f"instance A failed to start: {payload}")
            port = int(payload)
            print(f"instance A serving on 127.0.0.1:{port}")

            client = ctx.Process(target=_sync, args=(client_dir, client_data, port, result))
            client.start()
            status, peer_or_error, pulled, pushed = result.get(timeout=60)
            client.join(timeout=30)
            if status != "ok" or client.exitcode != 0:
                raise RuntimeError(
                    f"instance B failed: exit={client.exitcode}, {peer_or_error}"
                )

            print("instance B paired with:", peer_or_error)
            print("pulled:", pulled)
            print("pushed:", pushed)
        finally:
            stop.set()
            server.join(timeout=30)
            if server.is_alive():
                server.terminate()
                server.join(timeout=10)

        assert (client_dir / "from_server.txt").read_text(encoding="utf-8") == (
            "hello from instance A"
        )
        assert (server_dir / "from_client.txt").read_text(encoding="utf-8") == (
            "hello from instance B"
        )
        print("both instances converged on the same files")


if __name__ == "__main__":
    main()
