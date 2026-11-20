from __future__ import annotations

import threading
import time
from contextlib import ExitStack
from pathlib import Path

import pytest

from helpers.assertions import wait_for_condition, wait_for_text


def _import_ferrisync():
    try:
        import ferrisync  # type: ignore

        return ferrisync
    except Exception as e:
        pytest.skip(f"ferrisync extension not available: {e}")


@pytest.mark.reliability
def test_watcher_handle_lifecycle_and_cleanup(tmp_path: Path):
    ferrisync = _import_ferrisync()
    srv_dir = tmp_path / "srv"
    cli_dir = tmp_path / "cli"
    srv_data = tmp_path / "srv-data"
    cli_data = tmp_path / "cli-data"
    for p in (srv_dir, cli_dir, srv_data, cli_data):
        p.mkdir()

    (srv_dir / "seed.txt").write_text("seed", encoding="utf-8")

    with ferrisync.Client(srv_data) as server, ferrisync.Client(cli_data) as client:
        srv_folder = server.add_folder(srv_dir, server.device_id, "bidirectional")
        port = server.serve(srv_folder, srv_dir, port=0, auto_accept=True)
        peer_id = client.pair("127.0.0.1", port)
        cli_folder = client.add_folder(cli_dir, peer_id, "bidirectional")

        # Initial sync so both sides are ready.
        res = client.sync(cli_folder, cli_dir, "127.0.0.1", port, peer_id)
        assert res.success
        assert wait_for_text(cli_dir / "seed.txt", "seed")

        watcher = client.watch(cli_folder, cli_dir, "127.0.0.1", port, peer_id)
        assert watcher.id > 0
        assert watcher.closed is False
        watches = client.list_watches()
        assert any(w.get("id") == watcher.id for w in watches)

        # Stopping via handle is idempotent and removes the watch.
        watcher.stop()
        assert watcher.closed is True
        watcher.stop()  # idempotent
        watches_after = client.list_watches()
        assert all(w.get("id") != watcher.id for w in watches_after)

        # Context manager support.
        with client.watch(cli_folder, cli_dir, "127.0.0.1", port, peer_id) as w2:
            assert w2.closed is False
            assert any(w.get("id") == w2.id for w in client.list_watches())
        assert w2.closed is True
        assert all(w.get("id") != w2.id for w in client.list_watches())


@pytest.mark.reliability
@pytest.mark.slow
def test_watch_propagates_changes_and_stops_delivering_after_stop(tmp_path: Path):
    ferrisync = _import_ferrisync()
    srv_dir = tmp_path / "srv"
    cli_dir = tmp_path / "cli"
    srv_data = tmp_path / "srv-data"
    cli_data = tmp_path / "cli-data"
    for p in (srv_dir, cli_dir, srv_data, cli_data):
        p.mkdir()

    (srv_dir / "base.txt").write_text("base", encoding="utf-8")

    with ferrisync.Client(srv_data) as server, ferrisync.Client(cli_data) as client:
        srv_folder = server.add_folder(srv_dir, server.device_id, "bidirectional")
        port = server.serve(srv_folder, srv_dir, port=0, auto_accept=True)
        peer_id = client.pair("127.0.0.1", port)
        cli_folder = client.add_folder(cli_dir, peer_id, "bidirectional")

        # Initial sync
        client.sync(cli_folder, cli_dir, "127.0.0.1", port, peer_id)
        assert wait_for_text(cli_dir / "base.txt", "base")

        watcher = client.watch(cli_folder, cli_dir, "127.0.0.1", port, peer_id)
        try:
            # Trigger watch-driven sync from client to server.
            (cli_dir / "watched.txt").write_text("from watch", encoding="utf-8")
            assert wait_for_text(srv_dir / "watched.txt", "from watch", timeout=15.0)

            # Drain events to a known point.
            _ = client.poll_events()
            _ = server.poll_events()
        finally:
            watcher.stop()

        assert watcher.closed is True
        assert all(w.get("id") != watcher.id for w in client.list_watches())

        # After stop, new local changes should not be auto-synced by this watcher.
        (cli_dir / "after_stop.txt").write_text("should not auto sync", encoding="utf-8")
        time.sleep(1.5)
        assert not (srv_dir / "after_stop.txt").exists(), "stopped watcher must not propagate new changes"

        # But manual sync still works.
        res = client.sync(cli_folder, cli_dir, "127.0.0.1", port, peer_id)
        assert res.success
        assert wait_for_text(srv_dir / "after_stop.txt", "should not auto sync")


@pytest.mark.reliability
@pytest.mark.slow
def test_two_watches_emit_events_and_stop_is_isolated(tmp_path: Path):
    ferrisync = _import_ferrisync()
    server_specs = []
    for name in ("one", "two"):
        srv_dir = tmp_path / f"{name}-srv"
        cli_dir = tmp_path / f"{name}-cli"
        srv_data = tmp_path / f"{name}-srv-data"
        for p in (srv_dir, cli_dir, srv_data):
            p.mkdir()
        server_specs.append((srv_dir, srv_data, cli_dir))

    client_data = tmp_path / "cli-data"
    client_data.mkdir()

    with ExitStack() as stack:
        client = stack.enter_context(ferrisync.Client(client_data))
        folders = []
        for srv_dir, srv_data, cli_dir in server_specs:
            server = stack.enter_context(ferrisync.Client(srv_data))
            srv_folder = server.add_folder(srv_dir, server.device_id, "bidirectional")
            port = server.serve(srv_folder, srv_dir, port=0, auto_accept=True)
            peer_id = client.pair("127.0.0.1", port)
            cli_folder = client.add_folder(cli_dir, peer_id, "bidirectional")
            assert client.sync(cli_folder, cli_dir, "127.0.0.1", port, peer_id).success
            folders.append((server, srv_dir, cli_dir, cli_folder, port, peer_id))

        client_events: list[dict] = []
        server_events: dict[int, list[dict]] = {
            id(server): [] for server, *_ in folders
        }
        watchers = [
            client.watch(cli_folder, cli_dir, "127.0.0.1", port, peer_id)
            for _server, _srv_dir, cli_dir, cli_folder, port, peer_id in folders
        ]
        try:
            assert len(client.list_watches()) == 2

            for _server, srv_dir, cli_dir, _cli_folder, _port, _peer_id in folders:
                (cli_dir / "watched.txt").write_text(srv_dir.name, encoding="utf-8")

            def _events_ready() -> bool:
                client_events.extend(client.poll_events())
                for server, *_ in folders:
                    server_events[id(server)].extend(server.poll_events())
                pushed = sum(
                    1 for e in client_events if e.get("type") == "file_pushed"
                )
                pulled = all(
                    any(e.get("type") == "file_pulled" for e in events)
                    for events in server_events.values()
                )
                return pushed >= 2 and pulled

            assert wait_for_condition(_events_ready, timeout=20.0)
            pushed_paths = {
                e.get("path") for e in client_events if e.get("type") == "file_pushed"
            }
            assert pushed_paths == {"watched.txt"}
            for events in server_events.values():
                pulled_paths = {
                    e.get("path") for e in events if e.get("type") == "file_pulled"
                }
                assert pulled_paths == {"watched.txt"}

            # Stop only the first watch; the second must keep working.
            watchers[0].stop()
            assert watchers[0].closed
            assert not watchers[1].closed
            assert len(client.list_watches()) == 1

            _first_server, first_srv, first_cli, _, _, _ = folders[0]
            _second_server, second_srv, second_cli, _, _, _ = folders[1]
            (first_cli / "after_stop.txt").write_text("isolated", encoding="utf-8")
            (second_cli / "still_running.txt").write_text("live", encoding="utf-8")

            assert wait_for_text(second_srv / "still_running.txt", "live", timeout=15.0)
            time.sleep(1.5)
            assert not (first_srv / "after_stop.txt").exists(), (
                "stopped watch must not propagate new changes"
            )
        finally:
            for watcher in watchers:
                watcher.stop()


@pytest.mark.reliability
def test_repeated_watch_start_stop_leaves_no_duplicates(tmp_path: Path):
    ferrisync = _import_ferrisync()
    srv_dir = tmp_path / "srv"
    cli_dir = tmp_path / "cli"
    srv_data = tmp_path / "srv-data"
    cli_data = tmp_path / "cli-data"
    for p in (srv_dir, cli_dir, srv_data, cli_data):
        p.mkdir()

    with ferrisync.Client(srv_data) as server, ferrisync.Client(cli_data) as client:
        srv_folder = server.add_folder(srv_dir, server.device_id, "bidirectional")
        port = server.serve(srv_folder, srv_dir, port=0, auto_accept=True)
        peer_id = client.pair("127.0.0.1", port)
        cli_folder = client.add_folder(cli_dir, peer_id, "bidirectional")

        ids: list[int] = []
        for _ in range(4):
            watcher = client.watch(cli_folder, cli_dir, "127.0.0.1", port, peer_id)
            ids.append(watcher.id)
            assert len(client.list_watches()) == 1
            watcher.stop()
            assert not client.list_watches()

        assert len(set(ids)) == len(ids), "watch ids must not be reused"


@pytest.mark.reliability
@pytest.mark.slow
def test_subscription_callback_can_reenter_client(tmp_path: Path):
    ferrisync = _import_ferrisync()
    srv_dir = tmp_path / "srv"
    cli_dir = tmp_path / "cli"
    srv_data = tmp_path / "srv-data"
    cli_data = tmp_path / "cli-data"
    for p in (srv_dir, cli_dir, srv_data, cli_data):
        p.mkdir()

    with ferrisync.Client(srv_data) as server, ferrisync.Client(cli_data) as client:
        srv_folder = server.add_folder(srv_dir, server.device_id, "bidirectional")
        port = server.serve(srv_folder, srv_dir, port=0, auto_accept=True)
        peer_id = client.pair("127.0.0.1", port)
        cli_folder = client.add_folder(cli_dir, peer_id, "bidirectional")

        reentered = threading.Event()
        errors: list[BaseException] = []

        def on_event(event: dict) -> None:
            if event.get("type") == "file_pushed":
                try:
                    _ = client.status()
                    _ = client.devices()
                    reentered.set()
                except BaseException as e:
                    errors.append(e)

        stop = client.subscribe(on_event, poll_interval=0.05, on_error=errors.append)
        watcher = client.watch(cli_folder, cli_dir, "127.0.0.1", port, peer_id)
        try:
            (cli_dir / "callback.txt").write_text("callback", encoding="utf-8")
            assert wait_for_text(srv_dir / "callback.txt", "callback", timeout=15.0)
            assert reentered.wait(10.0), "callback did not receive a transfer event"
            assert not errors, f"reentrant callback failed: {errors}"
        finally:
            stop.set()
            watcher.stop()
