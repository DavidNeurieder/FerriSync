import asyncio
import tempfile
import threading
import time
from pathlib import Path

import pytest


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


def test_closed_client_raises_typed_error():
    ferrisync = _import_ferrisync()
    with tempfile.TemporaryDirectory(prefix="fs-py-data-") as data_dir:
        client = ferrisync.Client(data_dir)
        assert client.closed is False
        client.close()
        assert client.closed is True
        client.close()  # idempotent
        with pytest.raises(ferrisync.ClientClosedError):
            _ = client.device_id


def test_multiple_clients_are_independent_in_process():
    ferrisync = _import_ferrisync()

    with (
        tempfile.TemporaryDirectory(prefix="fs-srv-dir-") as srv_dir,
        tempfile.TemporaryDirectory(prefix="fs-a-dir-") as a_dir,
        tempfile.TemporaryDirectory(prefix="fs-b-dir-") as b_dir,
        tempfile.TemporaryDirectory(prefix="fs-srv-data-") as srv_data,
        tempfile.TemporaryDirectory(prefix="fs-a-data-") as a_data,
        tempfile.TemporaryDirectory(prefix="fs-b-data-") as b_data,
    ):
        served = Path(srv_dir)
        client_a = Path(a_dir)
        client_b = Path(b_dir)
        (served / "seed.txt").write_text("seed", encoding="utf-8")
        (client_a / "from_a.txt").write_text("A", encoding="utf-8")
        (client_b / "from_b.txt").write_text("B", encoding="utf-8")

        with ferrisync.Client(srv_data) as server, ferrisync.Client(a_data) as a, ferrisync.Client(
            b_data
        ) as b:
            srv_folder_id = server.add_folder(served, server.device_id, "bidirectional")
            port = server.serve(srv_folder_id, served, port=0, auto_accept=True)
            assert port > 0

            # Client A pairs and syncs successfully.
            srv_id_for_a = a.pair("127.0.0.1", port)
            a_folder = a.add_folder(client_a, srv_id_for_a, "bidirectional")
            a.sync(a_folder, client_a, "127.0.0.1", port, srv_id_for_a)
            assert _wait_for_file(client_a / "seed.txt")
            assert _wait_for_file(served / "from_a.txt")
            assert (client_a / "seed.txt").read_text(encoding="utf-8") == "seed"

            # Closing A must not break server or unrelated client B.
            a.close()
            assert a.closed is True
            assert server.closed is False
            assert b.closed is False

            # Server remains usable.
            _ = server.device_id
            _ = server.folders()
            _ = server.status()

            # Client B can still pair and sync with the same server instance.
            srv_id_for_b = b.pair("127.0.0.1", port)
            b_folder = b.add_folder(client_b, srv_id_for_b, "bidirectional")
            b.sync(b_folder, client_b, "127.0.0.1", port, srv_id_for_b)
            assert _wait_for_file(client_b / "seed.txt")
            assert _wait_for_file(client_b / "from_a.txt")
            assert _wait_for_file(served / "from_b.txt")
            assert (client_b / "seed.txt").read_text(encoding="utf-8") == "seed"
            assert (served / "from_b.txt").read_text(encoding="utf-8") == "B"


def test_async_client_basic_lifecycle_and_sync():
    ferrisync = _import_ferrisync()

    async def run():
        with (
            tempfile.TemporaryDirectory(prefix="fs-srv-dir-") as srv_dir,
            tempfile.TemporaryDirectory(prefix="fs-cli-dir-") as cli_dir,
            tempfile.TemporaryDirectory(prefix="fs-srv-data-") as srv_data,
            tempfile.TemporaryDirectory(prefix="fs-cli-data-") as cli_data,
        ):
            served = Path(srv_dir)
            client_dir = Path(cli_dir)
            (served / "hello.txt").write_text("hello", encoding="utf-8")
            (client_dir / "world.txt").write_text("world", encoding="utf-8")

            # Use sync server (simple/reliable) and async client.
            with ferrisync.Client(srv_data) as server:
                srv_folder_id = server.add_folder(served, server.device_id, "bidirectional")
                port = server.serve(srv_folder_id, served, port=0, auto_accept=True)

                async with ferrisync.AsyncClient(cli_data) as client:
                    srv_id = await client.pair("127.0.0.1", port)
                    folder_id = await client.add_folder(client_dir, srv_id, "bidirectional")
                    result = await client.sync(folder_id, client_dir, "127.0.0.1", port, srv_id)
                    assert result.success
                    assert _wait_for_file(client_dir / "hello.txt")
                    assert _wait_for_file(served / "world.txt")
                    assert (client_dir / "hello.txt").read_text(encoding="utf-8") == "hello"
                    assert (served / "world.txt").read_text(encoding="utf-8") == "world"
                    _ = await client.status()
                    _ = await client.poll_events()

                # after async context exit, operations should fail cleanly
                reopened = ferrisync.AsyncClient(cli_data)
                with pytest.raises(ferrisync.ClientClosedError):
                    _ = reopened.client

    asyncio.run(run())


def test_repeated_client_cycles_do_not_leak_ferrisync_workers():
    ferrisync = _import_ferrisync()

    def worker_threads():
        prefixes = ("ferrisync-cancel", "ferrisync-async", "ferrisync-events")
        return [t for t in threading.enumerate() if t.name.startswith(prefixes)]

    baseline = len(worker_threads())
    with (
        tempfile.TemporaryDirectory(prefix="fs-cycle-a-") as a,
        tempfile.TemporaryDirectory(prefix="fs-cycle-b-") as b,
    ):
        for _ in range(8):
            with ferrisync.Client(a) as client:
                # Exercise subscription tracking and normal close.
                stop = client.subscribe(lambda _event: None, poll_interval=0.05)
                time.sleep(0.08)
                stop.set()

            async def run_async_cycle():
                async with ferrisync.AsyncClient(b, max_workers=2) as client:
                    await client.status()
                    stop = await client.subscribe(lambda _event: None, poll_interval=0.05)
                    await asyncio.sleep(0.08)
                    stop.set()

            asyncio.run(run_async_cycle())

    deadline = time.time() + 3.0
    while time.time() < deadline and len(worker_threads()) > baseline:
        time.sleep(0.05)
    assert len(worker_threads()) <= baseline, [t.name for t in worker_threads()]
