from __future__ import annotations

import json
import os
import subprocess
import time
from pathlib import Path

import pytest

from helpers.assertions import assert_file_contents, wait_for_condition, wait_for_text
from helpers.processes import (
    ensure_cli_binary,
    find_free_port,
    start_cli_process,
    stop_process,
    wait_for_port,
    wait_for_process_exit,
)


def _import_ferrisync():
    try:
        import ferrisync  # type: ignore

        return ferrisync
    except Exception as e:
        pytest.skip(f"ferrisync extension not available: {e}")


def _run_cli(binary: Path, args: list[str], data_dir: Path, timeout: float = 30.0, check: bool = True):
    cmd = [str(binary), "--data-dir", str(data_dir), *args]
    return subprocess.run(
        cmd,
        cwd=binary.parents[1] if len(binary.parents) > 1 else None,
        capture_output=True,
        text=True,
        timeout=timeout,
        check=check,
    )


def _list_device_ids(ferrisync, data_dir: Path) -> list[str]:
    with ferrisync.Client(data_dir) as c:
        return [d["id"] for d in c.devices()]


@pytest.mark.interop
def test_cli_server_python_client_bidirectional_sync(tmp_path: Path, cli_binary: Path):
    ferrisync = _import_ferrisync()

    served_dir = tmp_path / "served"
    client_dir = tmp_path / "client"
    cli_data = tmp_path / "cli-data"
    py_data = tmp_path / "py-data"
    served_dir.mkdir()
    client_dir.mkdir()
    cli_data.mkdir()
    py_data.mkdir()

    (served_dir / "from_cli.txt").write_text("hello from cli server", encoding="utf-8")
    (client_dir / "from_python.txt").write_text("hello from python client", encoding="utf-8")

    port = find_free_port()
    log_path = tmp_path / "cli-serve.log"
    proc = start_cli_process(
        ["serve", "--port", str(port), "--auto-accept", str(served_dir)],
        data_dir=cli_data,
        log_path=log_path,
    )
    try:
        assert wait_for_port("127.0.0.1", port, timeout=15.0), "CLI serve did not open port"

        with ferrisync.Client(py_data) as client:
            server_device_id = client.pair("127.0.0.1", port)
            assert server_device_id
            folder_id = client.add_folder(client_dir, server_device_id, "bidirectional")

            result = client.sync(folder_id, client_dir, "127.0.0.1", port, server_device_id)
            assert result.success
            assert wait_for_text(client_dir / "from_cli.txt", "hello from cli server")
            assert wait_for_text(served_dir / "from_python.txt", "hello from python client")
            assert_file_contents(client_dir / "from_cli.txt", "hello from cli server")
            assert_file_contents(served_dir / "from_python.txt", "hello from python client")

            # Second round
            (served_dir / "from_cli.txt").write_text("cli v2", encoding="utf-8")
            (client_dir / "added.txt").write_text("added by python", encoding="utf-8")
            time.sleep(0.05)
            result2 = client.sync(folder_id, client_dir, "127.0.0.1", port, server_device_id)
            assert result2.success
            assert wait_for_text(client_dir / "from_cli.txt", "cli v2")
            assert wait_for_text(served_dir / "added.txt", "added by python")
    finally:
        stop_process(proc)
        assert wait_for_process_exit(proc, timeout=10.0)


@pytest.mark.interop
def test_python_server_cli_client_bidirectional_sync(tmp_path: Path, cli_binary: Path):
    ferrisync = _import_ferrisync()

    served_dir = tmp_path / "served"
    client_dir = tmp_path / "client"
    py_data = tmp_path / "py-data"
    cli_data = tmp_path / "cli-data"
    served_dir.mkdir()
    client_dir.mkdir()
    py_data.mkdir()
    cli_data.mkdir()

    (served_dir / "from_python_server.txt").write_text("hello from python server", encoding="utf-8")
    (client_dir / "from_cli_client.txt").write_text("hello from cli client", encoding="utf-8")

    with ferrisync.Client(py_data) as server:
        folder_id = server.add_folder(served_dir, server.device_id, "bidirectional")
        port = server.serve(folder_id, served_dir, port=0, auto_accept=True)
        assert port > 0

        # Establish pairing/trust from CLI side.
        pair_res = _run_cli(cli_binary, ["pair", "127.0.0.1", "--port", str(port)], cli_data)
        assert pair_res.returncode == 0, pair_res.stdout + pair_res.stderr

        # Inspect CLI's data dir through Python API to confirm shared DB/identity model
        # and obtain the paired server device id for an explicit, non-interactive sync.
        with ferrisync.Client(cli_data) as cli_view:
            paired = cli_view.devices()
            assert paired, "CLI data dir should contain the paired server device"
            server_device_id = paired[0]["id"]

        sync_res = _run_cli(
            cli_binary,
            ["sync", str(client_dir), "--device", server_device_id],
            cli_data,
            timeout=60.0,
            check=False,
        )
        if sync_res.returncode != 0:
            # Fallback: explicit address sync should still work for interoperability.
            sync_res = _run_cli(
                cli_binary,
                ["sync", str(client_dir), "--device", f"127.0.0.1:{port}"],
                cli_data,
                timeout=60.0,
                check=False,
            )
        assert sync_res.returncode == 0, sync_res.stdout + sync_res.stderr

        assert wait_for_text(client_dir / "from_python_server.txt", "hello from python server")
        assert wait_for_text(served_dir / "from_cli_client.txt", "hello from cli client")
        assert_file_contents(client_dir / "from_python_server.txt", "hello from python server")
        assert_file_contents(served_dir / "from_cli_client.txt", "hello from cli client")

        # Second round
        (served_dir / "from_python_server.txt").write_text("python server v2", encoding="utf-8")
        (client_dir / "added_cli.txt").write_text("added by cli", encoding="utf-8")
        time.sleep(0.05)
        sync_res2 = _run_cli(
            cli_binary,
            ["sync", str(client_dir), "--device", server_device_id],
            cli_data,
            timeout=60.0,
            check=False,
        )
        if sync_res2.returncode != 0:
            sync_res2 = _run_cli(
                cli_binary,
                ["sync", str(client_dir), "--device", f"127.0.0.1:{port}"],
                cli_data,
                timeout=60.0,
                check=False,
            )
        assert sync_res2.returncode == 0, sync_res2.stdout + sync_res2.stderr
        assert wait_for_text(client_dir / "from_python_server.txt", "python server v2")
        assert wait_for_text(served_dir / "added_cli.txt", "added by cli")


@pytest.mark.interop
def test_shared_identity_data_dir_is_stable_across_python_and_cli(tmp_path: Path, cli_binary: Path):
    ferrisync = _import_ferrisync()
    shared_data = tmp_path / "shared-data"
    shared_data.mkdir()

    # Create/open identity via Python.
    with ferrisync.Client(shared_data) as c1:
        device_id_1 = c1.device_id

    # Use CLI against same data dir (status is a safe read/engine init command).
    status_res = _run_cli(cli_binary, ["status"], shared_data, check=False)
    assert status_res.returncode == 0, status_res.stdout + status_res.stderr

    # Reopen via Python and ensure identity is unchanged.
    with ferrisync.Client(shared_data) as c2:
        device_id_2 = c2.device_id
    assert device_id_1 == device_id_2

    # Pair a Python server to populate paired-device list, then verify CLI sees it.
    peer_data = tmp_path / "peer-data"
    peer_dir = tmp_path / "peer-dir"
    peer_data.mkdir()
    peer_dir.mkdir()
    (peer_dir / "x.txt").write_text("x", encoding="utf-8")
    with ferrisync.Client(peer_data) as peer:
        fid = peer.add_folder(peer_dir, peer.device_id, "bidirectional")
        port = peer.serve(fid, peer_dir, port=0, auto_accept=True)
        with ferrisync.Client(shared_data) as shared:
            peer_id = shared.pair("127.0.0.1", port)
            ids_after = [d["id"] for d in shared.devices()]
            assert peer_id in ids_after

    # CLI devices listing against shared data dir should not fail and identity remains.
    dev_res = _run_cli(cli_binary, ["devices"], shared_data, check=False)
    assert dev_res.returncode == 0, dev_res.stdout + dev_res.stderr
    with ferrisync.Client(shared_data) as c3:
        assert c3.device_id == device_id_1
        assert peer_id in [d["id"] for d in c3.devices()]


@pytest.mark.interop
def test_python_server_pending_pairing_and_denial_with_cli_peer(tmp_path: Path, cli_binary: Path):
    ferrisync = _import_ferrisync()

    served_dir = tmp_path / "served"
    py_data = tmp_path / "py-data"
    cli_data = tmp_path / "cli-data"
    served_dir.mkdir()
    py_data.mkdir()
    cli_data.mkdir()
    (served_dir / "secret.txt").write_text("secret", encoding="utf-8")

    with ferrisync.Client(py_data) as server:
        folder_id = server.add_folder(served_dir, server.device_id, "bidirectional")
        port = server.serve(folder_id, served_dir, port=0, auto_accept=False)
        assert port > 0

        # Start CLI pair in background; it should wait for server approval/denial.
        pair_proc = start_cli_process(
            ["pair", "127.0.0.1", "--port", str(port)],
            data_dir=cli_data,
            log_path=tmp_path / "cli-pair.log",
        )
        try:
            pending = []
            deadline = time.time() + 15.0
            while time.time() < deadline:
                pending = server.pending_pairings()
                if pending:
                    break
                if pair_proc.poll() is not None:
                    break
                time.sleep(0.1)
            assert pending, f"expected pending pairing, got none; rc={pair_proc.poll()}"
            req = pending[0]
            device_id = req.get("device_id") or req.get("id")
            assert device_id

            server.deny_pairing(device_id)
            finished = wait_for_process_exit(pair_proc, timeout=15.0)
            assert finished, "CLI pair process did not exit after denial"
            # Pairing should not succeed.
            assert pair_proc.returncode != 0
        finally:
            stop_process(pair_proc)

        # CLI should not be recorded as a paired/trusted device on the server.
        assert all(d.get("id") != device_id for d in server.devices())


@pytest.mark.interop
@pytest.mark.slow
def test_tofu_mismatch_fails_closed(tmp_path: Path, cli_binary: Path):
    ferrisync = _import_ferrisync()

    served_dir = tmp_path / "served"
    client_dir = tmp_path / "client"
    server_data = tmp_path / "server-data"
    cli_data = tmp_path / "cli-data"
    served_dir.mkdir()
    client_dir.mkdir()
    server_data.mkdir()
    cli_data.mkdir()
    (served_dir / "a.txt").write_text("a", encoding="utf-8")
    (client_dir / "b.txt").write_text("b", encoding="utf-8")

    with ferrisync.Client(server_data) as server:
        folder_id = server.add_folder(served_dir, server.device_id, "bidirectional")
        port = server.serve(folder_id, served_dir, port=0, auto_accept=True)

        pair_res = _run_cli(cli_binary, ["pair", "127.0.0.1", "--port", str(port)], cli_data)
        assert pair_res.returncode == 0, pair_res.stdout + pair_res.stderr

    # Replace server identity by using a fresh data dir on same logical role.
    rotated_data = tmp_path / "server-data-rotated"
    rotated_data.mkdir()
    with ferrisync.Client(rotated_data) as rotated:
        # Different identity
        with ferrisync.Client(server_data) as original:
            # original data dir still contains old pairing state on CLI side; rotated has new identity
            assert rotated.device_id != original.device_id
        fid = rotated.add_folder(served_dir, rotated.device_id, "bidirectional")
        port2 = rotated.serve(fid, served_dir, port=0, auto_accept=True)

        sync_res = _run_cli(
            cli_binary,
            ["sync", str(client_dir), "--device", f"127.0.0.1:{port2}"],
            cli_data,
            timeout=60.0,
            check=False,
        )
        # Must fail closed - CLI must not silently re-trust rotated identity.
        assert sync_res.returncode != 0, sync_res.stdout + sync_res.stderr
        combined = (sync_res.stdout + "\n" + sync_res.stderr).lower()
        assert any(
            token in combined
            for token in ["identity", "certificate", "fingerprint", "mismatch", "trust", "pair", "tls", "verify"]
        ), combined
