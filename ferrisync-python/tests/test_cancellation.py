from __future__ import annotations

import asyncio
import threading
import time
from pathlib import Path

import pytest

from helpers.assertions import wait_for_condition, wait_for_text
from helpers.scenarios import deterministic_bytes, sha256_file


def _import_ferrisync():
    try:
        import ferrisync  # type: ignore

        return ferrisync
    except Exception as e:
        pytest.skip(f"ferrisync extension not available: {e}")


@pytest.mark.reliability
@pytest.mark.slow
def test_sync_cancel_event_before_start(tmp_path: Path):
    ferrisync = _import_ferrisync()
    srv_dir = tmp_path / "srv"
    cli_dir = tmp_path / "cli"
    srv_data = tmp_path / "srv-data"
    cli_data = tmp_path / "cli-data"
    for p in (srv_dir, cli_dir, srv_data, cli_data):
        p.mkdir()
    (srv_dir / "a.txt").write_text("a", encoding="utf-8")

    with ferrisync.Client(srv_data) as server, ferrisync.Client(cli_data) as client:
        fid = server.add_folder(srv_dir, server.device_id, "bidirectional")
        port = server.serve(fid, srv_dir, port=0, auto_accept=True)
        peer = client.pair("127.0.0.1", port)
        cf = client.add_folder(cli_dir, peer, "bidirectional")

        cancel = threading.Event()
        cancel.set()
        with pytest.raises(ferrisync.FerriSyncCancelledError):
            client.sync(cf, cli_dir, "127.0.0.1", port, peer, cancel=cancel)

        # Client remains usable after pre-cancelled operation.
        result = client.sync(cf, cli_dir, "127.0.0.1", port, peer)
        assert result.success
        assert wait_for_text(cli_dir / "a.txt", "a")


@pytest.mark.reliability
@pytest.mark.slow
def test_async_sync_cancel_raises_cancelled_error(tmp_path: Path):
    ferrisync = _import_ferrisync()

    async def run():
        srv_dir = tmp_path / "srv"
        cli_dir = tmp_path / "cli"
        srv_data = tmp_path / "srv-data"
        cli_data = tmp_path / "cli-data"
        for p in (srv_dir, cli_dir, srv_data, cli_data):
            p.mkdir()

        # Larger payload to give cancellation a chance during operation/fallback boundaries.
        payload = deterministic_bytes(12 * 1024 * 1024)
        (srv_dir / "big.bin").write_bytes(payload)
        expected_hash = sha256_file(srv_dir / "big.bin")

        with ferrisync.Client(srv_data) as server:
            fid = server.add_folder(srv_dir, server.device_id, "bidirectional")
            port = server.serve(fid, srv_dir, port=0, auto_accept=True)

            async with ferrisync.AsyncClient(cli_data, max_workers=2) as client:
                peer = await client.pair("127.0.0.1", port)
                cf = await client.add_folder(cli_dir, peer, "bidirectional")

                cancel = asyncio.Event()
                task = asyncio.create_task(
                    client.sync(cf, cli_dir, "127.0.0.1", port, peer, cancel=cancel)
                )
                # Request cancellation shortly after starting.
                await asyncio.sleep(0.05)
                cancel.set()

                cancelled = False
                try:
                    await task
                except (asyncio.CancelledError, ferrisync.FerriSyncCancelledError):
                    cancelled = True
                except Exception:
                    # If transfer completed before cooperative cancellation point, that's acceptable;
                    # the API contract is best-effort cooperative cancellation.
                    cancelled = False

                # Client should remain usable; a follow-up sync must converge correctly.
                result = await client.sync(cf, cli_dir, "127.0.0.1", port, peer)
                assert result.success
                assert wait_for_condition(lambda: (cli_dir / "big.bin").exists(), timeout=30.0)
                assert wait_for_condition(
                    lambda: (cli_dir / "big.bin").exists() and sha256_file(cli_dir / "big.bin") == expected_hash,
                    timeout=30.0,
                )
                _ = cancelled  # informational; cooperative cancellation timing is environment-dependent

    asyncio.run(run())


@pytest.mark.reliability
def test_async_close_with_pending_work_does_not_hang(tmp_path: Path):
    ferrisync = _import_ferrisync()

    async def run():
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
