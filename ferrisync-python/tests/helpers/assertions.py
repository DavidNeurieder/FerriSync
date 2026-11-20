from __future__ import annotations

import hashlib
import time
from pathlib import Path
from typing import Any, Callable


def wait_for_file(path: Path, timeout: float = 10.0, interval: float = 0.1) -> bool:
    deadline = time.time() + timeout
    while time.time() < deadline:
        if path.exists():
            return True
        time.sleep(interval)
    return path.exists()


def wait_for_text(
    path: Path,
    expected: str,
    timeout: float = 10.0,
    interval: float = 0.1,
    encoding: str = "utf-8",
) -> bool:
    deadline = time.time() + timeout
    while time.time() < deadline:
        try:
            if path.read_text(encoding=encoding) == expected:
                return True
        except FileNotFoundError:
            pass
        time.sleep(interval)
    try:
        return path.read_text(encoding=encoding) == expected
    except FileNotFoundError:
        return False


def wait_for_condition(
    predicate: Callable[[], bool], timeout: float = 10.0, interval: float = 0.1
) -> bool:
    deadline = time.time() + timeout
    while time.time() < deadline:
        if predicate():
            return True
        time.sleep(interval)
    return predicate()


def wait_for_event(
    poll_events: Callable[[], list[dict[str, Any]]],
    predicate: Callable[[dict[str, Any]], bool],
    timeout: float = 10.0,
    interval: float = 0.1,
) -> dict[str, Any] | None:
    deadline = time.time() + timeout
    while time.time() < deadline:
        for event in poll_events():
            if predicate(event):
                return event
        time.sleep(interval)
    for event in poll_events():
        if predicate(event):
            return event
    return None


def assert_file_contents(path: Path, expected: str | bytes, encoding: str = "utf-8") -> None:
    if isinstance(expected, bytes):
        actual = path.read_bytes()
    else:
        actual = path.read_text(encoding=encoding)
    assert actual == expected, f"{path}: expected {expected!r}, got {actual!r}"


def file_hash(path: Path) -> str:
    h = hashlib.sha256()
    with path.open("rb") as f:
        for chunk in iter(lambda: f.read(1024 * 1024), b""):
            h.update(chunk)
    return h.hexdigest()


def assert_file_hash(path: Path, expected_hash: str) -> None:
    actual = file_hash(path)
    assert actual == expected_hash, f"{path}: expected hash {expected_hash}, got {actual}"
