from __future__ import annotations

import asyncio
import tempfile
from pathlib import Path


def _import_ferrisync():
    try:
        import ferrisync  # type: ignore

        return ferrisync
    except Exception as e:
        import pytest

        pytest.skip(f"ferrisync extension not available: {e}")


def test_documented_sync_example_smoke():
    ferrisync = _import_ferrisync()

    with tempfile.TemporaryDirectory() as a, tempfile.TemporaryDirectory() as b, tempfile.TemporaryDirectory() as da, tempfile.TemporaryDirectory() as db:
        sa, sb = Path(a), Path(b)
        (sa / "from_server.txt").write_text("server", encoding="utf-8")
        (sb / "from_client.txt").write_text("client", encoding="utf-8")

        with ferrisync.Client(da) as server, ferrisync.Client(db) as client:
            folder_id = server.add_folder(sa, server.device_id, "bidirectional")
            port = server.serve(folder_id, sa, port=0, auto_accept=True)
            device_id = client.pair("127.0.0.1", port)
            client_folder_id = client.add_folder(sb, device_id, "bidirectional")
            result = client.sync(
                folder_id=client_folder_id,
                local_path=sb,
                remote_ip="127.0.0.1",
                remote_port=port,
                device_id=device_id,
            )
            assert result.success
            assert (sb / "from_server.txt").read_text(encoding="utf-8") == "server"
            assert (sa / "from_client.txt").read_text(encoding="utf-8") == "client"


def test_documented_async_example_smoke():
    ferrisync = _import_ferrisync()

    async def run():
        with tempfile.TemporaryDirectory() as a, tempfile.TemporaryDirectory() as b, tempfile.TemporaryDirectory() as da, tempfile.TemporaryDirectory() as db:
            sa, sb = Path(a), Path(b)
            (sa / "s.txt").write_text("s", encoding="utf-8")
            with ferrisync.Client(da) as server:
                fid = server.add_folder(sa, server.device_id, "bidirectional")
                port = server.serve(fid, sa, port=0, auto_accept=True)
                async with ferrisync.AsyncClient(db, max_workers=2) as client:
                    peer = await client.pair("127.0.0.1", port)
                    cf = await client.add_folder(sb, peer, "bidirectional")
                    res = await client.sync(cf, sb, "127.0.0.1", port, peer)
                    assert res.success
                    assert (sb / "s.txt").read_text(encoding="utf-8") == "s"

    asyncio.run(run())
