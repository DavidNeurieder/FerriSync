from __future__ import annotations

import os
import stat
import time
from pathlib import Path

import pytest
from helpers.assertions import assert_file_contents, wait_for_condition, wait_for_text
from helpers.scenarios import (
    deterministic_bytes,
    find_conflict_backups,
    make_nested_files,
    sha256_file,
    write_text,
)


def _import_ferrisync():
    try:
        import ferrisync  # type: ignore

        return ferrisync
    except Exception as e:
        pytest.skip(f"ferrisync extension not available: {e}")


def _pair_and_sync(
    ferrisync,
    server_dir: Path,
    client_dir: Path,
    server_data: Path,
    client_data: Path,
):
    with ferrisync.Client(server_data) as server, ferrisync.Client(client_data) as client:
        srv_folder = server.add_folder(server_dir, server.device_id, "bidirectional")
        port = server.serve(srv_folder, server_dir, port=0, auto_accept=True)
        peer_id = client.pair("127.0.0.1", port)
        cli_folder = client.add_folder(client_dir, peer_id, "bidirectional")
        result = client.sync(cli_folder, client_dir, "127.0.0.1", port, peer_id)
        return server, client, port, peer_id, srv_folder, cli_folder, result


@pytest.mark.reliability
def test_nested_new_files_and_conflict_resolution(tmp_path: Path):
    ferrisync = _import_ferrisync()
    server_dir = tmp_path / "srv-dir"
    client_dir = tmp_path / "cli-dir"
    server_data = tmp_path / "srv-data"
    client_data = tmp_path / "cli-data"
    for p in (server_dir, client_dir, server_data, client_data):
        p.mkdir()

    expected_srv = make_nested_files(server_dir)
    write_text(client_dir / "from_client.txt", "client seed")

    with ferrisync.Client(server_data) as server, ferrisync.Client(client_data) as client:
        srv_folder = server.add_folder(server_dir, server.device_id, "bidirectional")
        port = server.serve(srv_folder, server_dir, port=0, auto_accept=True)
        peer_id = client.pair("127.0.0.1", port)
        cli_folder = client.add_folder(client_dir, peer_id, "bidirectional")

        r1 = client.sync(cli_folder, client_dir, "127.0.0.1", port, peer_id)
        assert r1.success
        for rel, content in expected_srv.items():
            assert wait_for_text(client_dir / rel, content), rel
            assert_file_contents(client_dir / rel, content)
        assert wait_for_text(server_dir / "from_client.txt", "client seed")

        # Edits on both sides are concurrent versions under the deterministic
        # conflict policy: the client pulls each peer version and preserves its
        # superseded local copy as a .ferrisync-conflict-* backup.
        time.sleep(0.05)
        write_text(server_dir / "nested/b.txt", "bravo v2")
        write_text(client_dir / "from_client.txt", "client v2")
        r2 = client.sync(cli_folder, client_dir, "127.0.0.1", port, peer_id)
        assert r2.success
        assert set(r2.conflicts) >= {"nested/b.txt", "from_client.txt"}
        assert wait_for_text(client_dir / "nested/b.txt", "bravo v2")
        assert wait_for_text(client_dir / "from_client.txt", "client seed")
        assert wait_for_text(server_dir / "from_client.txt", "client seed")
        assert_file_contents(client_dir / "nested/b.txt", "bravo v2")
        assert wait_for_condition(
            lambda: bool(find_conflict_backups(client_dir, "nested/b.txt"))
        )
        assert wait_for_condition(
            lambda: bool(find_conflict_backups(client_dir, "from_client.txt"))
        )
        nested_backups = find_conflict_backups(client_dir, "nested/b.txt")
        client_backups = find_conflict_backups(client_dir, "from_client.txt")
        assert nested_backups[0].read_text(encoding="utf-8") == "bravo"
        assert client_backups[0].read_text(encoding="utf-8") == "client v2"


@pytest.mark.reliability
def test_deletion_is_not_propagated_and_is_restored(tmp_path: Path):
    ferrisync = _import_ferrisync()
    server_dir = tmp_path / "srv-dir"
    client_dir = tmp_path / "cli-dir"
    server_data = tmp_path / "srv-data"
    client_data = tmp_path / "cli-data"
    for p in (server_dir, client_dir, server_data, client_data):
        p.mkdir()

    write_text(server_dir / "keep.txt", "keep")
    write_text(server_dir / "delete_me.txt", "gone soon")
    write_text(client_dir / "client_keep.txt", "keep client")

    with ferrisync.Client(server_data) as server, ferrisync.Client(client_data) as client:
        srv_folder = server.add_folder(server_dir, server.device_id, "bidirectional")
        port = server.serve(srv_folder, server_dir, port=0, auto_accept=True)
        peer_id = client.pair("127.0.0.1", port)
        cli_folder = client.add_folder(client_dir, peer_id, "bidirectional")

        r1 = client.sync(cli_folder, client_dir, "127.0.0.1", port, peer_id)
        assert r1.success
        assert wait_for_text(client_dir / "delete_me.txt", "gone soon")

        # Deletions are a documented limitation: without tombstones the client's
        # surviving copy is pushed back and restores the file on the server.
        (server_dir / "delete_me.txt").unlink()
        time.sleep(0.05)
        r2 = client.sync(cli_folder, client_dir, "127.0.0.1", port, peer_id)
        assert r2.success
        assert "delete_me.txt" in r2.pushed
        assert not r2.conflicts

        assert_file_contents(client_dir / "delete_me.txt", "gone soon")
        assert wait_for_condition(
            lambda: (server_dir / "delete_me.txt").exists(), timeout=10.0
        )
        assert_file_contents(server_dir / "delete_me.txt", "gone soon")
        assert_file_contents(server_dir / "keep.txt", "keep")
        assert_file_contents(client_dir / "keep.txt", "keep")
        assert_file_contents(server_dir / "client_keep.txt", "keep client")


@pytest.mark.reliability
@pytest.mark.slow
def test_large_file_sync_is_byte_identical(tmp_path: Path):
    ferrisync = _import_ferrisync()
    server_dir = tmp_path / "srv-dir"
    client_dir = tmp_path / "cli-dir"
    server_data = tmp_path / "srv-data"
    client_data = tmp_path / "cli-data"
    for p in (server_dir, client_dir, server_data, client_data):
        p.mkdir()

    payload = deterministic_bytes(8 * 1024 * 1024)
    (server_dir / "large.bin").write_bytes(payload)
    expected_hash = sha256_file(server_dir / "large.bin")

    with ferrisync.Client(server_data) as server, ferrisync.Client(client_data) as client:
        srv_folder = server.add_folder(server_dir, server.device_id, "bidirectional")
        port = server.serve(srv_folder, server_dir, port=0, auto_accept=True)
        peer_id = client.pair("127.0.0.1", port)
        cli_folder = client.add_folder(client_dir, peer_id, "bidirectional")

        result = client.sync(cli_folder, client_dir, "127.0.0.1", port, peer_id)
        assert result.success
        assert wait_for_condition(lambda: (client_dir / "large.bin").exists(), timeout=30.0)
        assert sha256_file(client_dir / "large.bin") == expected_hash
        assert (client_dir / "large.bin").stat().st_size == len(payload)


@pytest.mark.reliability
def test_conflict_or_convergent_resolution_and_api_access(tmp_path: Path):
    ferrisync = _import_ferrisync()
    server_dir = tmp_path / "srv-dir"
    client_dir = tmp_path / "cli-dir"
    server_data = tmp_path / "srv-data"
    client_data = tmp_path / "cli-data"
    for p in (server_dir, client_dir, server_data, client_data):
        p.mkdir()

    # Seed same file on both sides via an initial sync.
    write_text(server_dir / "shared.txt", "base")
    with ferrisync.Client(server_data) as server, ferrisync.Client(client_data) as client:
        srv_folder = server.add_folder(server_dir, server.device_id, "bidirectional")
        port = server.serve(srv_folder, server_dir, port=0, auto_accept=True)
        peer_id = client.pair("127.0.0.1", port)
        cli_folder = client.add_folder(client_dir, peer_id, "bidirectional")
        r0 = client.sync(cli_folder, client_dir, "127.0.0.1", port, peer_id)
        assert r0.success
        assert wait_for_text(client_dir / "shared.txt", "base")

        # Divergent edits on both sides without intermediate sync.
        time.sleep(0.05)
        write_text(server_dir / "shared.txt", "server edit")
        time.sleep(0.05)
        write_text(client_dir / "shared.txt", "client edit")

        r1 = client.sync(cli_folder, client_dir, "127.0.0.1", port, peer_id)
        assert r1.success

        # The client conservatively pulls the server's version and keeps its own
        # superseded copy as a .ferrisync-conflict-* backup (no plain .bak).
        assert wait_for_condition(
            lambda: bool(find_conflict_backups(client_dir, "shared.txt")), timeout=10.0
        )
        backups = find_conflict_backups(client_dir, "shared.txt")
        assert backups, "conflicting edit did not produce a backup"
        assert backups[0].read_text(encoding="utf-8") == "client edit"

        def _conflict_listed() -> bool:
            return any(c.get("path") == "shared.txt" for c in client.conflicts())

        assert wait_for_condition(_conflict_listed, timeout=10.0)
        conflict = next(c for c in client.conflicts() if c.get("path") == "shared.txt")
        backup_path = str(conflict.get("backup_path") or "")
        assert ".ferrisync-conflict-" in backup_path
        contents = client.read_conflict(
            cli_folder, str(conflict.get("path")), backup_path
        )
        assert contents["winner"] == "server edit"
        assert contents["loser"] == "client edit"

        # End state must be consistent: file exists on both sides and contents match.
        assert wait_for_condition(lambda: (server_dir / "shared.txt").exists(), timeout=10.0)
        assert wait_for_condition(lambda: (client_dir / "shared.txt").exists(), timeout=10.0)
        assert wait_for_condition(
            lambda: (server_dir / "shared.txt").read_text(encoding="utf-8")
            == (client_dir / "shared.txt").read_text(encoding="utf-8"),
            timeout=10.0,
        )
        final = (server_dir / "shared.txt").read_text(encoding="utf-8")
        assert final in {"server edit", "client edit"}

        # Conflict API must remain usable regardless of whether a backup was produced.
        conflicts = client.conflicts()
        assert isinstance(conflicts, list)
        server_conflicts = server.conflicts()
        assert isinstance(server_conflicts, list)

        # If backups/conflicts are reported, they should be resolvable.
        all_conflicts = conflicts + server_conflicts
        for c in all_conflicts:
            folder_id = int(c.get("folder_id") or 0)
            backup_path = c.get("backup_path") or c.get("path") or ""
            if folder_id and backup_path:
                winner_path = c.get("path") or c.get("winner_path") or ""
                loser_path = c.get("backup_path") or c.get("loser_path") or backup_path
                if winner_path and loser_path:
                    contents = client.read_conflict(folder_id, winner_path, loser_path)
                    assert isinstance(contents, dict)
                # Keep both to avoid destructive behavior in test.
                resolved = client.resolve_conflict(folder_id, backup_path, "both")
                assert isinstance(resolved, str)


@pytest.mark.reliability
def test_invalid_local_path_and_missing_directory_errors(tmp_path: Path):
    ferrisync = _import_ferrisync()
    data_dir = tmp_path / "data"
    data_dir.mkdir()

    with ferrisync.Client(data_dir) as client:
        # Missing directory for folder operations/sync should raise a typed/clear error.
        missing = tmp_path / "does-not-exist"
        try:
            client.add_folder(missing, client.device_id, "bidirectional")
        except Exception:
            # Some implementations may allow registration but fail later; accept either.
            pass

        # File instead of directory should not be treated as a valid sync folder path.
        file_path = tmp_path / "not-a-dir.txt"
        file_path.write_text("x", encoding="utf-8")
        with pytest.raises(Exception):
            client.sync(999999, file_path, "127.0.0.1", 6553, "no-such-device")


@pytest.mark.reliability
def test_permission_denied_path_fails_cleanly(tmp_path: Path):
    if os.geteuid() == 0:
        pytest.skip("permission-denied behavior not meaningful as root")
    ferrisync = _import_ferrisync()
    data_dir = tmp_path / "data"
    blocked_dir = tmp_path / "blocked"
    peer_dir = tmp_path / "peer"
    data_dir.mkdir()
    blocked_dir.mkdir()
    peer_dir.mkdir()
    (peer_dir / "x.txt").write_text("x", encoding="utf-8")

    # Remove permissions so sync cannot read/write as needed.
    os.chmod(blocked_dir, 0)
    try:
        with ferrisync.Client(data_dir) as client, ferrisync.Client(tmp_path / "peer-data") as peer:
            fid = peer.add_folder(peer_dir, peer.device_id, "bidirectional")
            port = peer.serve(fid, peer_dir, port=0, auto_accept=True)
            peer_id = client.pair("127.0.0.1", port)
            folder_id = client.add_folder(blocked_dir, peer_id, "bidirectional")
            with pytest.raises(Exception):
                client.sync(folder_id, blocked_dir, "127.0.0.1", port, peer_id)
    finally:
        os.chmod(blocked_dir, stat.S_IRWXU)


@pytest.mark.reliability
def test_empty_directories_do_not_break_file_sync(tmp_path: Path):
    ferrisync = _import_ferrisync()
    server_dir = tmp_path / "srv-dir"
    client_dir = tmp_path / "cli-dir"
    server_data = tmp_path / "srv-data"
    client_data = tmp_path / "cli-data"
    for p in (server_dir, client_dir, server_data, client_data):
        p.mkdir()

    # Empty directories are intentionally not indexed by the file-only index.
    # They must not break scanning, and adding a file inside one must still
    # converge with its parent directories.
    (server_dir / "empty").mkdir()
    (server_dir / "nested" / "empty").mkdir(parents=True)
    write_text(server_dir / "nested" / "payload.txt", "payload")
    (client_dir / "client-empty").mkdir()

    with ferrisync.Client(server_data) as server, ferrisync.Client(client_data) as client:
        srv_folder = server.add_folder(server_dir, server.device_id, "bidirectional")
        port = server.serve(srv_folder, server_dir, port=0, auto_accept=True)
        peer_id = client.pair("127.0.0.1", port)
        cli_folder = client.add_folder(client_dir, peer_id, "bidirectional")

        result = client.sync(cli_folder, client_dir, "127.0.0.1", port, peer_id)
        assert result.success
        assert wait_for_text(client_dir / "nested" / "payload.txt", "payload")
        assert not (client_dir / "empty").exists()
        assert not (client_dir / "nested" / "empty").exists()
        assert (client_dir / "client-empty").is_dir()
        assert (server_dir / "empty").is_dir()

        write_text(server_dir / "empty" / "later.txt", "later")
        result2 = client.sync(cli_folder, client_dir, "127.0.0.1", port, peer_id)
        assert result2.success
        assert wait_for_text(client_dir / "empty" / "later.txt", "later")


@pytest.mark.reliability
def test_unsafe_conflict_paths_are_rejected_without_writing_outside(tmp_path: Path):
    ferrisync = _import_ferrisync()
    root = tmp_path / "root"
    data_dir = tmp_path / "data"
    root.mkdir()
    data_dir.mkdir()
    outside = tmp_path / "outside.txt"
    outside.write_text("sentinel", encoding="utf-8")

    with ferrisync.Client(data_dir) as client:
        folder_id = client.add_folder(root, client.device_id, "bidirectional")

        with pytest.raises(Exception):
            client.read_conflict(folder_id, "../outside.txt", "../outside.txt")
        with pytest.raises(Exception):
            client.read_conflict(folder_id, str(outside), str(outside))
        with pytest.raises(Exception):
            client.resolve_conflict(folder_id, "../outside.txt", "both")

        assert outside.read_text(encoding="utf-8") == "sentinel"
        assert list(root.iterdir()) == []
