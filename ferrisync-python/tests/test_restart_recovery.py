from __future__ import annotations

import re
import socket
import threading
import time
from pathlib import Path

import pytest
from helpers.assertions import assert_file_contents, wait_for_condition, wait_for_text
from helpers.processes import (
    find_free_port,
    start_cli_process,
    stop_process,
    wait_for_port,
)
from helpers.scenarios import deterministic_bytes, sha256_file


def _paired_device_id(pair_id: str) -> str:
    """Extract the canonical device id from ``pair()``'s display string."""
    match = re.search(r"\(([^()]+)\)$", pair_id)
    assert match, f"unexpected paired device id format: {pair_id}"
    return match.group(1)


def _canonical_device_ids(devices: list[dict[str, object]]) -> set[str]:
    """Collapse display-form device rows onto their canonical ids."""
    canonical: set[str] = set()
    for device in devices:
        device_id = str(device.get("id") or "")
        match = re.search(r"\(([^()]+)\)$", device_id)
        canonical.add(match.group(1) if match else device_id)
    return canonical


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
        b_devices = _canonical_device_ids(b.devices())

    # Reopen same data dirs and verify persistence.
    with ferrisync.Client(data_a) as a2, ferrisync.Client(data_b) as b2:
        assert a2.device_id == a_id
        assert b2.device_id == b_id
        assert _paired_device_id(b_peer) in [d["id"] for d in b2.devices()]
        assert any(str(dir_b) == f.get("local_path") for f in b2.folders())
        # Devices/folders lists remain stable after restart.
        assert b_devices.issubset(_canonical_device_ids(b2.devices()))


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


def _hold_port(port: int):
    """Accept one connection on ``port`` and hold it until released."""
    listener = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    listener.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
    for _ in range(50):
        try:
            listener.bind(("127.0.0.1", port))
            break
        except OSError:
            time.sleep(0.1)
    else:
        listener.close()
        pytest.fail(f"could not bind port {port} after stopping the server")
    listener.listen(1)
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

    thread = threading.Thread(target=serve, name="ferrisync-test-port-hold", daemon=True)
    thread.start()
    return accepted, release, thread


@pytest.mark.reliability
@pytest.mark.slow
def test_interrupted_sync_restarts_on_same_port_and_data_dir(tmp_path: Path):
    ferrisync = _import_ferrisync()
    server_data = tmp_path / "srv-data"
    client_data = tmp_path / "cli-data"
    server_dir = tmp_path / "srv-dir"
    client_dir = tmp_path / "cli-dir"
    for p in (server_data, client_data, server_dir, client_dir):
        p.mkdir()

    payload = deterministic_bytes(16 * 1024 * 1024, seed=b"restart")
    (server_dir / "large.bin").write_bytes(payload)
    expected_hash = sha256_file(server_dir / "large.bin")
    port = find_free_port()
    serve_args = ["serve", "--port", str(port), "--auto-accept", str(server_dir)]
    log_path = tmp_path / "cli-serve.log"

    first_server = start_cli_process(serve_args, data_dir=server_data, log_path=log_path)
    accepted = release = holder = None
    second_server = None
    try:
        assert wait_for_port("127.0.0.1", port, timeout=15.0)

        with ferrisync.Client(client_data) as client:
            peer_id = client.pair("127.0.0.1", port)
            folder_id = client.add_folder(client_dir, peer_id, "bidirectional")

            # Simulate a crash: kill the server, hold its port with a silent
            # peer, and start a sync that is still in flight when we recover.
            first_server.kill()
            first_server.wait(timeout=10)
            accepted, release, holder = _hold_port(port)

            errors: list[BaseException] = []

            def run_sync() -> None:
                try:
                    client.sync(folder_id, client_dir, "127.0.0.1", port, peer_id)
                except BaseException as e:
                    errors.append(e)

            worker = threading.Thread(target=run_sync, name="ferrisync-test-restart-sync")
            worker.start()
            try:
                assert accepted.wait(10), "sync never reached the held port"
                # Let the in-flight session see EOF, then bring the real server
                # back on the exact same port and data dir.
                release.set()
                worker.join(timeout=15)
                assert not worker.is_alive(), "interrupted sync did not terminate"
                assert errors, "interrupted sync did not surface an error"

                second_server = start_cli_process(
                    serve_args, data_dir=server_data, log_path=tmp_path / "cli-restart.log"
                )
                assert wait_for_port("127.0.0.1", port, timeout=15.0)

                result = client.sync(folder_id, client_dir, "127.0.0.1", port, peer_id)
                assert result.success
                assert wait_for_condition(
                    lambda: (client_dir / "large.bin").exists()
                    and sha256_file(client_dir / "large.bin") == expected_hash,
                    timeout=30.0,
                )
                canonical_peer_id = _paired_device_id(peer_id)
                matching = [
                    d for d in client.devices() if d.get("id") == canonical_peer_id
                ]
                assert len(matching) == 1, "restart created a duplicate paired device"
            finally:
                release.set()
                worker.join(timeout=15)
                if holder is not None:
                    holder.join(timeout=5)
    finally:
        if holder is not None:
            if release is not None:
                release.set()
            holder.join(timeout=5)
        for proc in (first_server, second_server):
            if proc is not None:
                stop_process(proc)
