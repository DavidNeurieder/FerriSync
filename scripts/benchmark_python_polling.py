#!/usr/bin/env python3
"""Baseline measurement helper for Python cancellation bridge/polling overhead.

Run manually in a development environment with the extension installed:

    .venv/bin/python scripts/benchmark_python_polling.py

This is intentionally not a CI test; it records indicative local timings only.
"""

from __future__ import annotations

import tempfile
import threading
import time
from pathlib import Path

import ferrisync


def _paired(tmp: Path):
    srv_dir = tmp / "srv"
    cli_dir = tmp / "cli"
    srv_data = tmp / "srv-data"
    cli_data = tmp / "cli-data"
    for p in (srv_dir, cli_dir, srv_data, cli_data):
        p.mkdir()
    (srv_dir / "a.txt").write_text("a", encoding="utf-8")
    server = ferrisync.Client(srv_data)
    client = ferrisync.Client(cli_data)
    folder_id = server.add_folder(srv_dir, server.device_id, "bidirectional")
    port = server.serve(folder_id, srv_dir, port=0, auto_accept=True)
    peer = client.pair("127.0.0.1", port)
    client_folder = client.add_folder(cli_dir, peer, "bidirectional")
    return server, client, client_folder, cli_dir, port, peer


def main() -> None:
    with tempfile.TemporaryDirectory(prefix="fs-bench-") as d:
        tmp = Path(d)
        server, client, folder_id, local, port, peer = _paired(tmp)
        try:
            # Baseline: sync without cancel event.
            start = time.perf_counter()
            for _ in range(10):
                client.sync(folder_id, local, "127.0.0.1", port, peer)
            no_cancel = time.perf_counter() - start

            # Pre-set cancel event: immediate cancellation path.
            cancel = threading.Event()
            cancel.set()
            start = time.perf_counter()
            for _ in range(50):
                try:
                    client.sync(folder_id, local, "127.0.0.1", port, peer, cancel=cancel)
                except ferrisync.FerriSyncCancelledError:
                    pass
            pre_cancelled = time.perf_counter() - start

            # Unset cancel event: bridge polls but never fires.
            cancel = threading.Event()
            start = time.perf_counter()
            for _ in range(10):
                client.sync(folder_id, local, "127.0.0.1", port, peer, cancel=cancel)
            with_cancel_unset = time.perf_counter() - start

            print(f"10 syncs without cancel event:      {no_cancel:.4f}s")
            print(f"10 syncs with unset cancel event:   {with_cancel_unset:.4f}s")
            print(f"50 pre-cancelled sync attempts:     {pre_cancelled:.4f}s")
        finally:
            client.close()
            server.close()


if __name__ == "__main__":
    main()
