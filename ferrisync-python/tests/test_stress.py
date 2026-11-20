"""Opt-in scale/resource stress coverage.

All tests are marked ``reliability`` and ``slow``. They are intentionally
bounded so the existing CI reliability job can run them; set
``FERRISYNC_STRESS_METRICS=/path/to/metrics.jsonl`` to record measurements.
"""

from __future__ import annotations

import asyncio
import concurrent.futures
import random
import socket
import subprocess
import sys
import threading
import time
from contextlib import contextmanager
from pathlib import Path
from typing import Any

import pytest
from helpers.metrics import (
    diff,
    record,
    short_lived_threads,
    take_snapshot,
)

pytestmark = [pytest.mark.reliability, pytest.mark.slow]


def _import_ferrisync():
    try:
        import ferrisync  # type: ignore

        return ferrisync
    except Exception as e:
        pytest.skip(f"ferrisync extension not available: {e}")


@contextmanager
def _paired(ferrisync, root: Path):
    srv_dir = root / "srv"
    cli_dir = root / "cli"
    srv_data = root / "srv-data"
    cli_data = root / "cli-data"
    for path in (srv_dir, cli_dir, srv_data, cli_data):
        path.mkdir()
    (srv_dir / "a.txt").write_text("from-server", encoding="utf-8")

    with ferrisync.Client(srv_data) as server, ferrisync.Client(cli_data) as client:
        folder_id = server.add_folder(srv_dir, server.device_id, "bidirectional")
        port = server.serve(folder_id, srv_dir, port=0, auto_accept=True)
        peer = client.pair("127.0.0.1", port)
        client_folder = client.add_folder(cli_dir, peer, "bidirectional")
        yield server, client, client_folder, port, peer, cli_dir, cli_data


class _Blackhole:
    """TCP listener that accepts connections and holds them open."""

    def __init__(self) -> None:
        self._listener = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        self._listener.bind(("127.0.0.1", 0))
        self._listener.listen(32)
        self._listener.settimeout(0.1)
        self.port = int(self._listener.getsockname()[1])
        self._release = threading.Event()
        self._accepted: list[Any] = []
        self._thread = threading.Thread(
            target=self._serve, name="ferrisync-test-blackhole", daemon=True
        )
        self._thread.start()

    def _serve(self) -> None:
        while not self._release.is_set():
            try:
                conn, _ = self._listener.accept()
            except TimeoutError:
                continue
            except OSError:
                break
            self._accepted.append(conn)
        for conn in self._accepted:
            conn.close()
        self._listener.close()

    def close(self) -> None:
        self._release.set()
        self._thread.join(timeout=3.0)


def _settle_workers(baseline: dict[str, int]) -> dict[str, int]:
    deadline = time.monotonic() + 3.0
    current = short_lived_threads()
    while time.monotonic() < deadline and (
        set(current) - set(baseline)
        or any(count > baseline.get(name, 0) for name, count in current.items())
    ):
        time.sleep(0.05)
        current = short_lived_threads()
    return current


def test_many_clients_concurrent_distinct_dirs(tmp_path: Path):
    ferrisync = _import_ferrisync()

    # Initialize the process-wide runtime before taking the baseline.
    with ferrisync.Client(tmp_path / "warmup") as warmup:
        assert warmup.device_id
    baseline = short_lived_threads()
    before = take_snapshot()

    def worker(index: int) -> str | None:
        data = tmp_path / f"data-{index}"
        folder = tmp_path / f"folder-{index}"
        data.mkdir()
        folder.mkdir()
        (folder / "f.txt").write_text(str(index), encoding="utf-8")
        try:
            with ferrisync.Client(data) as client:
                folder_id = client.add_folder(folder, client.device_id, "bidirectional")
                assert client.status()["health"]
                assert client.devices() == []
                assert client.poll_events() == []
                assert folder_id > 0
                return str(client.device_id)
        except BaseException as exc:  # collected below, never silently ignored
            errors.append(exc)
            return None

    errors: list[BaseException] = []
    start = time.perf_counter()
    with concurrent.futures.ThreadPoolExecutor(max_workers=6) as executor:
        ids = list(executor.map(worker, range(12)))
    duration = time.perf_counter() - start
    after = take_snapshot()
    settled = _settle_workers(baseline)

    record(
        "many_clients_concurrent",
        {
            "clients": 12,
            "duration_s": round(duration, 4),
            "resources": diff(before, after),
            "short_lived_threads": settled,
        },
    )
    assert not errors, errors
    assert len({i for i in ids if i is not None}) == 12
    assert not (set(settled) - set(baseline))
    assert all(count <= baseline.get(name, 0) for name, count in settled.items())


def test_many_async_tasks_randomized_cancellation(tmp_path: Path):
    ferrisync = _import_ferrisync()

    with _paired(ferrisync, tmp_path) as (_server, _client, folder_id, port, peer, cli_dir, _):
        data = tmp_path / "async-data"
        data.mkdir()
        blackhole = _Blackhole()
        try:
            before = take_snapshot()

            async def run_tasks() -> tuple[list[float], int]:
                async with ferrisync.AsyncClient(data, max_workers=4) as client:
                    await client.add_folder(cli_dir, peer, "bidirectional")
                    rng = random.Random(20261120)
                    delays = [rng.uniform(0.005, 0.08) for _ in range(12)]
                    events = [asyncio.Event() for _ in delays]
                    set_at: list[float] = [0.0] * len(delays)
                    tasks = [
                        asyncio.create_task(
                            client.sync(
                                folder_id,
                                cli_dir,
                                "127.0.0.1",
                                blackhole.port,
                                peer,
                                cancel=event,
                            )
                        )
                        for event in events
                    ]

                    async def trigger(index: int, delay: float) -> None:
                        await asyncio.sleep(delay)
                        set_at[index] = time.perf_counter()
                        events[index].set()

                    await asyncio.gather(
                        *(trigger(i, delay) for i, delay in enumerate(delays))
                    )
                    results = await asyncio.gather(*tasks, return_exceptions=True)
                    latencies = [
                        time.perf_counter() - set_at[i]
                        for i, result in enumerate(results)
                        if isinstance(result, asyncio.CancelledError)
                    ]
                    assert await client.status()
                    return latencies, len(results)

            start = time.perf_counter()
            latencies, total = asyncio.run(run_tasks())
            duration = time.perf_counter() - start
            after = take_snapshot()
            record(
                "async_randomized_cancellation",
                {
                    "tasks": total,
                    "cancelled": len(latencies),
                    "max_latency_s": round(max(latencies, default=0.0), 4),
                    "duration_s": round(duration, 4),
                    "resources": diff(before, after),
                },
            )
            assert total == 12
            assert len(latencies) == total
            assert max(latencies) < 5.0
        finally:
            blackhole.close()


def test_repeated_watch_start_stop_and_client_reopen(tmp_path: Path):
    ferrisync = _import_ferrisync()

    with _paired(ferrisync, tmp_path) as (
        _server,
        client,
        folder_id,
        port,
        peer,
        cli_dir,
        cli_data,
    ):
        baseline = short_lived_threads()
        before = take_snapshot()
        start = time.perf_counter()
        watch_ids: set[int] = set()
        for _ in range(8):
            watcher = client.watch(folder_id, cli_dir, "127.0.0.1", port, peer)
            assert watcher.id > 0
            watch_ids.add(watcher.id)
            assert [w["id"] for w in client.list_watches()] == [watcher.id]
            watcher.stop()
            assert watcher.closed
            assert client.list_watches() == []

        client.close()
        with ferrisync.Client(cli_data) as reopened:
            reopened_folder = reopened.add_folder(cli_dir, peer, "bidirectional")
            watcher = reopened.watch(reopened_folder, cli_dir, "127.0.0.1", port, peer)
            # Watch IDs are scoped to a client, so reuse after reopening is valid.
            assert [w["id"] for w in reopened.list_watches()] == [watcher.id]
            watcher.stop()
            assert reopened.list_watches() == []
        duration = time.perf_counter() - start
        settled = _settle_workers(baseline)
        after = take_snapshot()
        record(
            "watch_start_stop_reopen",
            {
                "cycles": 8,
                "duration_s": round(duration, 4),
                "resources": diff(before, after),
                "short_lived_threads": settled,
            },
        )
        assert len(watch_ids) == 8
        assert not (set(settled) - set(baseline))


def test_network_interruption_during_active_sync(tmp_path: Path):
    ferrisync = _import_ferrisync()

    with _paired(ferrisync, tmp_path) as (
        _server,
        client,
        folder_id,
        port,
        peer,
        cli_dir,
        _,
    ):
        (cli_dir / "reply.txt").write_text("from-client", encoding="utf-8")
        baseline = short_lived_threads()
        durations: list[float] = []
        for _ in range(5):
            blackhole = _Blackhole()
            outcome: list[str] = []

            def do_sync() -> None:
                try:
                    client.sync(folder_id, cli_dir, "127.0.0.1", blackhole.port, peer)
                except (ferrisync.ConnectionError, ferrisync.SyncError):
                    outcome.append("failed")
                else:
                    outcome.append("succeeded")

            start = time.perf_counter()
            with concurrent.futures.ThreadPoolExecutor(max_workers=1) as executor:
                future = executor.submit(do_sync)
                time.sleep(0.05)
                blackhole.close()
                future.result(timeout=15)
            durations.append(time.perf_counter() - start)
            assert outcome == ["failed"]

        # The real peer remains usable after repeated interrupted sessions.
        result = client.sync(folder_id, cli_dir, "127.0.0.1", port, peer)
        assert result.success
        settled = _settle_workers(baseline)
        record(
            "network_interruption",
            {
                "cycles": 5,
                "max_duration_s": round(max(durations), 4),
                "short_lived_threads": settled,
            },
        )
        assert not (set(settled) - set(baseline))


_SHUTDOWN_SCRIPT = """
import asyncio
import tempfile
import threading
from pathlib import Path
import ferrisync

root = Path(tempfile.mkdtemp(prefix="fs-shutdown-"))
sdir, cdir, sdata, cdata, adata = (root / n for n in
    ("s", "c", "sdata", "cdata", "adata"))
for path in (sdir, cdir, sdata, cdata, adata):
    path.mkdir()
(sdir / "a.txt").write_text("a", encoding="utf-8")
server = ferrisync.Client(sdata)
client = ferrisync.Client(cdata)
fid = server.add_folder(sdir, server.device_id, "bidirectional")
port = server.serve(fid, sdir, port=0, auto_accept=True)
peer = client.pair("127.0.0.1", port)
cfid = client.add_folder(cdir, peer, "bidirectional")
client.watch(cfid, cdir, "127.0.0.1", port, peer)
client.subscribe(lambda event: None, poll_interval=0.01)

async def background():
    async_client = ferrisync.AsyncClient(adata)
    await async_client.__aenter__()
    await async_client.subscribe(lambda event: None, poll_interval=0.01)

asyncio.run(background())
print("SHUTDOWN_OK")
# Deliberately do not close clients: interpreter shutdown must still be clean.
"""


def test_interpreter_shutdown_with_active_background_work(tmp_path: Path):
    _import_ferrisync()
    start = time.perf_counter()
    completed = subprocess.run(
        [sys.executable, "-c", _SHUTDOWN_SCRIPT],
        capture_output=True,
        text=True,
        timeout=45,
        cwd=str(Path(__file__).resolve().parents[2]),
    )
    duration = time.perf_counter() - start
    record(
        "interpreter_shutdown",
        {
            "duration_s": round(duration, 4),
            "returncode": completed.returncode,
            "stdout": completed.stdout.strip()[-200:],
            "stderr": completed.stderr.strip()[-500:],
        },
    )
    assert completed.returncode == 0, completed.stderr
    assert "SHUTDOWN_OK" in completed.stdout
    assert "Traceback" not in completed.stderr
    assert "Exception ignored" not in completed.stderr


def test_reentrant_callback_during_concurrent_close(tmp_path: Path):
    ferrisync = _import_ferrisync()
    data = tmp_path / "data"
    data.mkdir()
    with ferrisync.Client(data) as client:
        errors: list[BaseException] = []

        def callback(_event: dict[str, Any]) -> None:
            try:
                client.status()
                client.poll_events()
            except ferrisync.ClientClosedError:
                pass
            except BaseException as exc:
                errors.append(exc)

        stop = client.subscribe(callback, poll_interval=0.01)
        time.sleep(0.1)
        closer = threading.Thread(target=client.close, name="test-close", daemon=True)
        start = time.perf_counter()
        closer.start()
        closer.join(timeout=5.0)
        close_duration = time.perf_counter() - start
        stop.set()
        record(
            "reentrant_callback_close",
            {
                "close_duration_s": round(close_duration, 4),
                "errors": len(errors),
            },
        )
        assert not closer.is_alive()
        assert not errors, errors
        assert close_duration < 5.0
