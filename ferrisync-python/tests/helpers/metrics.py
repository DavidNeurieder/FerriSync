"""Small, dependency-free resource snapshots for opt-in stress tests.

Set ``FERRISYNC_STRESS_METRICS`` to a JSONL file path to record measurements;
without that variable the helpers are inert. Values are deliberately portable:
unavailable counters are reported as ``None`` rather than guessed.
"""

from __future__ import annotations

import json
import os
import threading
import time
from dataclasses import dataclass
from pathlib import Path
from typing import Any

FERRISYNC_THREAD_PREFIX = "ferrisync-"
SHORT_LIVED_PREFIXES = ("ferrisync-cancel", "ferrisync-events", "ferrisync-async")


@dataclass(frozen=True)
class ResourceSnapshot:
    threads_total: int
    ferrisync_threads: dict[str, int]
    rss_kb: int | None
    cpu_seconds: float
    open_fds: int | None


def _proc_int(path: str, key: str) -> int | None:
    try:
        for line in Path(path).read_text(encoding="utf-8").splitlines():
            if line.startswith(key):
                return int(line.split()[1])
    except (OSError, ValueError, IndexError):
        pass
    return None


def take_snapshot() -> ResourceSnapshot:
    """Capture the counters that are cheaply available on the current host."""
    names = [thread.name for thread in threading.enumerate()]
    ferrisync: dict[str, int] = {}
    for name in names:
        if name.startswith(FERRISYNC_THREAD_PREFIX):
            ferrisync[name] = ferrisync.get(name, 0) + 1
    try:
        open_fds = len(list(Path("/proc/self/fd").iterdir()))
    except OSError:  # pragma: no cover - non-Linux
        open_fds = None
    return ResourceSnapshot(
        threads_total=len(names),
        ferrisync_threads=ferrisync,
        rss_kb=_proc_int("/proc/self/status", "VmRSS:"),
        cpu_seconds=time.process_time(),
        open_fds=open_fds,
    )


def diff(before: ResourceSnapshot, after: ResourceSnapshot) -> dict[str, Any]:
    """Return deltas useful for comparing the same workload across runs."""
    return {
        "threads_total": after.threads_total - before.threads_total,
        "ferrisync_threads": {
            name: count - before.ferrisync_threads.get(name, 0)
            for name, count in after.ferrisync_threads.items()
        },
        "rss_kb": (
            None
            if before.rss_kb is None or after.rss_kb is None
            else after.rss_kb - before.rss_kb
        ),
        "cpu_seconds": round(after.cpu_seconds - before.cpu_seconds, 4),
        "open_fds": (
            None
            if before.open_fds is None or after.open_fds is None
            else after.open_fds - before.open_fds
        ),
    }


def short_lived_threads() -> dict[str, int]:
    """Workers owned by a client lifetime (the Tokio pool outlives a client)."""
    counts: dict[str, int] = {}
    for thread in threading.enumerate():
        if thread.name.startswith(SHORT_LIVED_PREFIXES):
            counts[thread.name] = counts.get(thread.name, 0) + 1
    return counts


def wait_for_thread_baseline(
    baseline: dict[str, int], timeout: float = 3.0
) -> dict[str, int]:
    """Poll until FerriSync worker threads return to (or fall below) baseline."""
    deadline = time.monotonic() + timeout
    current: dict[str, int] = {}
    while time.monotonic() < deadline:
        current = take_snapshot().ferrisync_threads
        if all(current.get(name, 0) <= count for name, count in baseline.items()) and not (
            set(current) - set(baseline)
        ):
            break
        time.sleep(0.05)
    return current


def record(name: str, metrics: dict[str, Any]) -> None:
    """Append a measurement when ``FERRISYNC_STRESS_METRICS`` is configured."""
    path = os.environ.get("FERRISYNC_STRESS_METRICS")
    if not path:
        return
    target = Path(path)
    target.parent.mkdir(parents=True, exist_ok=True)
    entry = {"test": name, **metrics}
    with target.open("a", encoding="utf-8") as handle:
        handle.write(json.dumps(entry, sort_keys=True) + "\n")
