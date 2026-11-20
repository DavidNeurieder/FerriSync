from __future__ import annotations

import socket
import tempfile

import pytest


def _import_ferrisync():
    try:
        import ferrisync  # type: ignore

        return ferrisync
    except Exception as e:
        pytest.skip(f"ferrisync extension not available: {e}")


def _closed_port() -> int:
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as s:
        s.bind(("127.0.0.1", 0))
        return int(s.getsockname()[1])


def test_closed_client_raises_client_closed_error():
    ferrisync = _import_ferrisync()
    with tempfile.TemporaryDirectory(prefix="fs-exc-closed-") as data_dir:
        client = ferrisync.Client(data_dir)
        client.close()
        with pytest.raises(ferrisync.ClientClosedError):
            client.device_id
        with pytest.raises(ferrisync.ClientClosedError):
            client.sync(1, data_dir, "127.0.0.1", 9847, "missing")


def test_invalid_device_name_raises_invalid_argument_error():
    ferrisync = _import_ferrisync()
    with tempfile.TemporaryDirectory(prefix="fs-exc-name-") as data_dir:
        with ferrisync.Client(data_dir) as client:
            with pytest.raises(ferrisync.InvalidArgumentError):
                client.rename("")
            with pytest.raises(ferrisync.InvalidArgumentError):
                client.rename("bad\x00name")


def test_unknown_resources_raise_not_found_error():
    ferrisync = _import_ferrisync()
    with tempfile.TemporaryDirectory(prefix="fs-exc-missing-") as data_dir:
        with ferrisync.Client(data_dir) as client:
            with pytest.raises(ferrisync.NotFoundError):
                client.stop_watch(999999)
            with pytest.raises(ferrisync.NotFoundError):
                client.deny_pairing("00000000-0000-0000-0000-000000000000")


def test_connection_refused_raises_connection_error():
    ferrisync = _import_ferrisync()
    port = _closed_port()
    with (
        tempfile.TemporaryDirectory(prefix="fs-exc-conn-data-") as data_dir,
        tempfile.TemporaryDirectory(prefix="fs-exc-conn-dir-") as folder,
    ):
        with ferrisync.Client(data_dir) as client:
            folder_id = client.add_folder(folder, "missing-device", "bidirectional")
            with pytest.raises(ferrisync.ConnectionError):
                client.sync(folder_id, folder, "127.0.0.1", port, "missing-device")


def test_exception_hierarchy_is_stable():
    ferrisync = _import_ferrisync()
    assert issubclass(ferrisync.ClientClosedError, ferrisync.FerriSyncError)
    assert issubclass(ferrisync.FerriSyncCancelledError, ferrisync.FerriSyncError)
    assert issubclass(ferrisync.InvalidArgumentError, ferrisync.FerriSyncError)
    assert issubclass(ferrisync.ConfigurationError, ferrisync.FerriSyncError)
    assert issubclass(ferrisync.NotFoundError, ferrisync.FerriSyncError)
    assert issubclass(ferrisync.ConnectionError, ferrisync.FerriSyncError)
    assert issubclass(ferrisync.SyncError, ferrisync.FerriSyncError)
    assert issubclass(ferrisync.FerriSyncError, RuntimeError)
