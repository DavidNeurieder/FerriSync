from __future__ import annotations

import time
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
