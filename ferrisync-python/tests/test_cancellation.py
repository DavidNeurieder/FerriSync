from __future__ import annotations

import asyncio
import socket
import threading
from contextlib import contextmanager
from pathlib import Path

import pytest

from helpers.assertions import wait_for_condition, wait_for_text


def _import_ferrisync():
    try:
        import ferrisync  # type: ignore

        return ferrisync
    except Exception as e:
        pytest.skip(f"ferrisync extension not available: {e}")


@contextmanager
def _paired_clients(ferrisync, tmp_path: Path):
    srv_dir = tmp_path / "srv"
    cli_dir = tmp_path / "cli"
    srv_data = tmp_path / "srv-data"
    cli_data = tmp_path / "cli-data"
    for p in (srv_dir, cli_dir, srv_data, cli_data):
        p.mkdir()
    (srv_dir / "a.txt").write_text("a", encoding="utf-8")

    with ferrisync.Client(srv_data) as server, ferrisync.Client(cli_data) as client:
        folder_id = server.add_folder(srv_dir, server.device_id, "bidirectional")
        port = server.serve(folder_id, srv_dir, port=0, auto_accept=True)
        peer = client.pair("127.0.0.1", port)
        client_folder = client.add_folder(cli_dir, peer, "bidirectional")
        yield server, client, client_folder, port, peer, cli_dir, cli_data


def _blackhole_listener():
    """Accept one TCP connection, then hold the TLS handshake open."""
    listener = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    listener.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
    listener.bind(("127.0.0.1", 0))
    listener.listen(1)
    port = int(listener.getsockname()[1])
    accepted = threading.Event()
    release = threading.Event()

    def serve() -> None:
        conn: socket.socket | None = None
        try:
            conn, _ = listener.accept()
            accepted.set()
            release.wait(15)
        except OSError:
            pass
        finally:
            if conn is not None:
                conn.close()
            listener.close()

    thread = threading.Thread(target=serve, name="ferrisync-test-blackhole", daemon=True)
    thread.start()
    return port, accepted, release, thread


@pytest.mark.reliability
@pytest.mark.slow
def test_sync_cancel_event_before_start(tmp_path: Path):
    ferrisync = _import_ferrisync()

    with _paired_clients(ferrisync, tmp_path) as (
        _server,
        client,
        client_folder,
        port,
        peer,
        cli_dir,
        _cli_data,
    ):
        cancel = threading.Event()
        cancel.set()
        with pytest.raises(ferrisync.FerriSyncCancelledError):
            client.sync(client_folder, cli_dir, "127.0.0.1", port, peer, cancel=cancel)

        # Client remains usable after pre-cancelled operation.
        result = client.sync(client_folder, cli_dir, "127.0.0.1", port, peer)
        assert result.success
        assert wait_for_text(cli_dir / "a.txt", "a")


@pytest.mark.reliability
@pytest.mark.slow
def test_sync_cancel_event_interrupts_in_flight_handshake(tmp_path: Path):
    ferrisync = _import_ferrisync()
    port, accepted, release, blackhole = _blackhole_listener()

    with _paired_clients(ferrisync, tmp_path) as (
        _server,
        client,
        client_folder,
        _server_port,
        peer,
        cli_dir,
        _cli_data,
    ):
        cancel = threading.Event()
        errors: list[BaseException] = []

        def run_sync() -> None:
            try:
                client.sync(
                    client_folder, cli_dir, "127.0.0.1", port, peer, cancel=cancel
                )
            except BaseException as e:
                errors.append(e)

        worker = threading.Thread(target=run_sync, name="ferrisync-test-sync", daemon=True)
        worker.start()
        try:
            assert accepted.wait(10), "sync never reached the peer TLS handshake"
            cancel.set()
            worker.join(timeout=10)
            assert not worker.is_alive(), "sync did not stop after cancellation"
            assert errors, "cancelled sync did not raise"
            assert isinstance(errors[0], ferrisync.FerriSyncCancelledError)
            assert not (cli_dir / "a.txt").exists(), "cancelled sync applied a file"
        finally:
            release.set()
            blackhole.join(timeout=5)


@pytest.mark.reliability
@pytest.mark.slow
def test_async_cancel_event_interrupts_in_flight_handshake(tmp_path: Path):
    ferrisync = _import_ferrisync()

    async def run() -> None:
        srv_dir = tmp_path / "srv"
        cli_dir = tmp_path / "cli"
        srv_data = tmp_path / "srv-data"
        cli_data = tmp_path / "cli-data"
        for p in (srv_dir, cli_dir, srv_data, cli_data):
            p.mkdir()
        (srv_dir / "a.txt").write_text("a", encoding="utf-8")

        with ferrisync.Client(srv_data) as server:
            folder_id = server.add_folder(srv_dir, server.device_id, "bidirectional")
            server_port = server.serve(folder_id, srv_dir, port=0, auto_accept=True)

            async with ferrisync.AsyncClient(cli_data, max_workers=2) as client:
                peer = await client.pair("127.0.0.1", server_port)
                client_folder = await client.add_folder(cli_dir, peer, "bidirectional")

                pre_cancelled = asyncio.Event()
                pre_cancelled.set()
                with pytest.raises(asyncio.CancelledError):
                    await client.sync(
                        client_folder,
                        cli_dir,
                        "127.0.0.1",
                        server_port,
                        peer,
                        cancel=pre_cancelled,
                    )

                port, accepted, release, blackhole = _blackhole_listener()
                cancel = asyncio.Event()
                task = asyncio.create_task(
                    client.sync(client_folder, cli_dir, "127.0.0.1", port, peer, cancel=cancel)
                )
                try:
                    await asyncio.wait_for(
                        asyncio.get_running_loop().run_in_executor(None, accepted.wait, 10),
                        timeout=11,
                    )
                    cancel.set()
                    with pytest.raises(asyncio.CancelledError):
                        await asyncio.wait_for(task, timeout=10)
                    assert not (cli_dir / "a.txt").exists(), "cancelled sync applied a file"
                finally:
                    release.set()
                    blackhole.join(timeout=5)

                # Client remains usable; a follow-up sync converges correctly.
                result = await client.sync(
                    client_folder, cli_dir, "127.0.0.1", server_port, peer
                )
                assert result.success
                assert wait_for_text(cli_dir / "a.txt", "a")

    asyncio.run(run())


@pytest.mark.reliability
def test_watch_cancel_event_stops_and_unregisters_watch(tmp_path: Path):
    ferrisync = _import_ferrisync()

    with _paired_clients(ferrisync, tmp_path) as (
        _server,
        client,
        client_folder,
        port,
        peer,
        cli_dir,
        _cli_data,
    ):
        cancel = threading.Event()
        watcher = client.watch(client_folder, cli_dir, "127.0.0.1", port, peer, cancel=cancel)
        assert len(client.list_watches()) == 1

        cancel.set()
        assert wait_for_condition(lambda: not client.list_watches(), timeout=10)
        assert watcher.closed
        watcher.stop()  # idempotent after external cancellation
        assert watcher.closed
        assert not client.list_watches()


@pytest.mark.reliability
def test_async_close_with_pending_work_does_not_hang(tmp_path: Path):
    ferrisync = _import_ferrisync()

    async def run() -> None:
        data = tmp_path / "data"
        data.mkdir()
        client = ferrisync.AsyncClient(data, max_workers=2)
        await client.__aenter__()
        # Queue lightweight work and close promptly.
        tasks = [asyncio.create_task(client.status()) for _ in range(5)]
        await asyncio.sleep(0)
        await asyncio.wait_for(client.close(), timeout=10.0)
        for t in tasks:
            try:
                await asyncio.wait_for(t, timeout=10.0)
            except Exception:
                # Some tasks may fail due to close; that is acceptable as long as close completes.
                pass
        with pytest.raises(ferrisync.ClientClosedError):
            _ = client.client

    asyncio.run(run())
