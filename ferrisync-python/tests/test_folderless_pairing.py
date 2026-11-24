import inspect
import tempfile
import threading
from pathlib import Path

import pytest
from helpers.assertions import wait_for_condition


def _import_ferrisync():
    try:
        import ferrisync  # type: ignore

        return ferrisync
    except Exception as e:
        pytest.skip(f"ferrisync extension not available: {e}")


def test_pair_port_default_is_auto():
    ferrisync = _import_ferrisync()
    assert inspect.signature(ferrisync.Client.pair).parameters["port"].default is None


def test_pair_uses_control_port_without_any_folders(monkeypatch):
    ferrisync = _import_ferrisync()
    # Isolate this test from other clients in the pytest process that may
    # already hold the well-known control port.
    monkeypatch.setenv("FERRISYNC_CONTROL_PORT", "0")

    with (
        tempfile.TemporaryDirectory(prefix="fs-py-control-server-") as server_data,
        tempfile.TemporaryDirectory(prefix="fs-py-control-client-") as client_data,
    ):
        with (
            ferrisync.Client(Path(server_data)) as server,
            ferrisync.Client(Path(client_data)) as client,
        ):
            assert server.folders() == []
            assert client.folders() == []

            client_id = client.device_id
            server_id = server.device_id
            server_control_port = server.control_port
            assert server_control_port > 0
            result: dict[str, str] = {}

            def do_pair() -> None:
                # Explicit control port avoids collisions when other clients in
                # this process already hold 9846; this still exercises the
                # folder-independent listener that pair() uses by default.
                result["paired"] = client.pair("127.0.0.1", server_control_port)

            thread = threading.Thread(target=do_pair, daemon=True)
            thread.start()
            try:
                assert wait_for_condition(
                    lambda: any(
                        pending_id == client_id
                        for _, pending_id in server.pending_pairings()
                    ),
                    timeout=10.0,
                ), "server never saw the folder-less pairing request"
                server.approve_pairing(client_id, "test-client")
            finally:
                thread.join(timeout=20.0)

            assert not thread.is_alive(), "pair() did not finish after approval"
            assert server_id in result["paired"]
            assert any(d["id"] == server_id for d in client.devices())
            assert any(d["id"] == client_id for d in server.devices())
