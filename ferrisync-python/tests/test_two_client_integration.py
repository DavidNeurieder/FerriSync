import tempfile
import time
from pathlib import Path

import pytest
from helpers.assertions import wait_for_condition
from helpers.scenarios import find_conflict_backups


def _import_ferrisync():
    try:
        import ferrisync  # type: ignore

        return ferrisync
    except Exception as e:
        pytest.skip(f"ferrisync extension not available: {e}")


def _wait_for_file(path: Path, timeout: float = 10.0) -> bool:
    deadline = time.time() + timeout
    while time.time() < deadline:
        if path.exists():
            return True
        time.sleep(0.1)
    return path.exists()


def _wait_for_text(path: Path, expected: str, timeout: float = 10.0) -> bool:
    deadline = time.time() + timeout
    while time.time() < deadline:
        try:
            if path.read_text(encoding="utf-8") == expected:
                return True
        except FileNotFoundError:
            pass
        time.sleep(0.1)
    try:
        return path.read_text(encoding="utf-8") == expected
    except FileNotFoundError:
        return False


def test_pair_and_sync_between_two_local_clients():
    ferrisync = _import_ferrisync()

    with (
        tempfile.TemporaryDirectory(prefix="fs-py-a-") as dir_a,
        tempfile.TemporaryDirectory(prefix="fs-py-b-") as dir_b,
        tempfile.TemporaryDirectory(prefix="fs-py-data-a-") as data_a,
        tempfile.TemporaryDirectory(prefix="fs-py-data-b-") as data_b,
    ):
        served_dir = Path(dir_a)
        client_dir = Path(dir_b)
        (served_dir / "from_server.txt").write_text("hello from server", encoding="utf-8")
        (client_dir / "from_client.txt").write_text("hello from client", encoding="utf-8")

        with ferrisync.Client(data_a) as server, ferrisync.Client(data_b) as client:
            # Serve the server folder. Register it as a local folder so the API
            # has a folder id to bind the listener to.
            server_folder_id = server.add_folder(served_dir, server.device_id, "bidirectional")
            port = server.serve(server_folder_id, served_dir, port=0, auto_accept=True)
            assert isinstance(port, int) and port > 0

            # Pair client -> server directly (no mDNS dependency).
            server_device_id = client.pair("127.0.0.1", port)
            assert isinstance(server_device_id, str) and server_device_id

            # Configure client folder paired with the server and sync.
            client_folder_id = client.add_folder(client_dir, server_device_id, "bidirectional")
            result = client.sync(
                client_folder_id,
                client_dir,
                "127.0.0.1",
                port,
                server_device_id,
            )

            # Bidirectional sync should exchange both files with correct contents.
            assert _wait_for_file(client_dir / "from_server.txt"), (
                "client did not receive server file"
            )
            assert _wait_for_file(served_dir / "from_client.txt"), (
                "server did not receive client file"
            )
            assert (
                (client_dir / "from_server.txt").read_text(encoding="utf-8")
                == "hello from server"
            )
            assert (
                (served_dir / "from_client.txt").read_text(encoding="utf-8")
                == "hello from client"
            )

            transferred = set(result.pulled) | set(result.pushed)
            assert "from_server.txt" in transferred or "from_client.txt" in transferred

            # Second round: simultaneous edits to the same files are conflicts,
            # while new files on each side propagate normally.
            (served_dir / "from_server.txt").write_text("server v2", encoding="utf-8")
            (client_dir / "from_client.txt").write_text("client v2", encoding="utf-8")
            (served_dir / "added_by_server.txt").write_text("new on server", encoding="utf-8")
            (client_dir / "added_by_client.txt").write_text("new on client", encoding="utf-8")
            time.sleep(0.05)  # ensure mtimes can advance on coarse filesystems

            result2 = client.sync(
                client_folder_id,
                client_dir,
                "127.0.0.1",
                port,
                server_device_id,
            )
            assert result2.success
            assert set(result2.conflicts) >= {"from_server.txt", "from_client.txt"}

            # The peer version wins on the client; superseded local copies are
            # preserved as .ferrisync-conflict-* backups rather than discarded.
            assert _wait_for_text(client_dir / "from_server.txt", "server v2")
            assert _wait_for_text(client_dir / "from_client.txt", "hello from client")
            assert wait_for_condition(
                lambda: bool(find_conflict_backups(client_dir, "from_server.txt"))
            )
            assert wait_for_condition(
                lambda: bool(find_conflict_backups(client_dir, "from_client.txt"))
            )
            server_backups = find_conflict_backups(client_dir, "from_server.txt")
            client_backups = find_conflict_backups(client_dir, "from_client.txt")
            assert server_backups[0].read_text(encoding="utf-8") == "hello from server"
            assert client_backups[0].read_text(encoding="utf-8") == "client v2"

            # New files converge in both directions without conflicts.
            assert _wait_for_text(client_dir / "added_by_server.txt", "new on server")
            assert _wait_for_text(served_dir / "added_by_client.txt", "new on client")

            # Polling/events API should remain usable after sync.
            _ = client.poll_events()
            _ = server.poll_events()
