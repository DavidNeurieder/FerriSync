from __future__ import annotations

import time
from pathlib import Path

import pytest

from helpers.assertions import assert_file_contents, wait_for_condition, wait_for_text


def _import_ferrisync():
    try:
        import ferrisync  # type: ignore

        return ferrisync
    except Exception as e:
        pytest.skip(f"ferrisync extension not available: {e}")


@pytest.mark.reliability
def test_reopen_same_data_dir_preserves_identity_and_pairings(tmp_path: Path):
    ferrisync = _import_ferrisync()
    data_a = tmp_path / "a-data"
    data_b = tmp_path / "b-data"
    dir_a = tmp_path / "a-dir"
    dir_b = tmp_path / "b-dir"
    for p in (data_a, data_b, dir_a, dir_b):
        p.mkdir()
    (dir_a / "seed.txt").write_text("seed", encoding="utf-8")

    with ferrisync.Client(data_a) as a, ferrisync.Client(data_b) as b:
        a_id = a.device_id
        fid = a.add_folder(dir_a, a.device_id, "bidirectional")
        port = a.serve(fid, dir_a, port=0, auto_accept=True)
        b_peer = b.pair("127.0.0.1", port)
        b_folder = b.add_folder(dir_b, b_peer, "bidirectional")
        res = b.sync(b_folder, dir_b, "127.0.0.1", port, b_peer)
        assert res.success
        assert wait_for_text(dir_b / "seed.txt", "seed")
        b_id = b.device_id
        b_devices = [d["id"] for d in b.devices()]
        b_folders = [f.get("local_path") for f in b.folders()]

    # Reopen same data dirs and verify persistence.
    with ferrisync.Client(data_a) as a2, ferrisync.Client(data_b) as b2:
        assert a2.device_id == a_id
        assert b2.device_id == b_id
        assert b_peer in [d["id"] for d in b2.devices()]
        assert any(str(dir_b) == f.get("local_path") for f in b2.folders())
        # Devices/folders lists remain stable after restart.
        assert set(b_devices).issubset({d["id"] for d in b2.devices()})


@pytest.mark.reliability
def test_server_restart_same_data_dir_and_resync(tmp_path: Path):
    ferrisync = _import_ferrisync()
    server_data = tmp_path / "srv-data"
    client_data = tmp_path / "cli-data"
    server_dir = tmp_path / "srv-dir"
    client_dir = tmp_path / "cli-dir"
    for p in (server_data, client_data, server_dir, client_dir):
        p.mkdir()

    (server_dir / "initial.txt").write_text("initial", encoding="utf-8")

    # Initial pairing and sync.
    with ferrisync.Client(server_data) as server, ferrisync.Client(client_data) as client:
        srv_folder = server.add_folder(server_dir, server.device_id, "bidirectional")
        port = server.serve(srv_folder, server_dir, port=0, auto_accept=True)
        peer_id = client.pair("127.0.0.1", port)
        cli_folder = client.add_folder(client_dir, peer_id, "bidirectional")
        r1 = client.sync(cli_folder, client_dir, "127.0.0.1", port, peer_id)
        assert r1.success
        assert wait_for_text(client_dir / "initial.txt", "initial")

    # Restart server with same data dir/identity and a new port, then resync.
    (server_dir / "after_restart.txt").write_text("after restart", encoding="utf-8")
    (client_dir / "client_after_restart.txt").write_text("client after restart", encoding="utf-8")
    time.sleep(0.05)

    with ferrisync.Client(server_data) as server2, ferrisync.Client(client_data) as client2:
        folders = server2.folders()
        assert folders, "server folders should persist across restart"
        srv_folder2 = int(folders[0]["id"])
        port2 = server2.serve(srv_folder2, server_dir, port=0, auto_accept=True)
        assert port2 > 0
        client_folders = client2.folders()
        assert client_folders, "client folders should persist across restart"
        cli_folder2 = int(client_folders[0]["id"])
        peer_id2 = [d["id"] for d in client2.devices()][0]

        r2 = client2.sync(cli_folder2, client_dir, "127.0.0.1", port2, peer_id2)
        assert r2.success
        assert wait_for_text(client_dir / "after_restart.txt", "after restart")
        assert wait_for_text(server_dir / "client_after_restart.txt", "client after restart")
        assert_file_contents(client_dir / "initial.txt", "initial")

        # No duplicate device entries for the paired peer.
        matching = [d for d in client2.devices() if d.get("id") == peer_id2]
        assert len(matching) == 1
