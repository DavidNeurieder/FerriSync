from __future__ import annotations

import os
import socket
import subprocess
import time
from pathlib import Path
from typing import Mapping, Sequence


def repo_root() -> Path:
    # ferrisync-python/tests/helpers/processes.py -> repo root
    return Path(__file__).resolve().parents[3]


def cli_debug_binary() -> Path:
    return repo_root() / "target" / "debug" / "ferrisync"


def ensure_cli_binary() -> Path:
    binary = cli_debug_binary()
    subprocess.run(
        ["cargo", "build", "-p", "ferrisync"],
        cwd=repo_root(),
        check=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        text=True,
    )
    if not binary.exists():
        raise RuntimeError(f"CLI binary not found after build: {binary}")
    return binary


def find_free_port(host: str = "127.0.0.1") -> int:
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as s:
        s.bind((host, 0))
        return int(s.getsockname()[1])


def wait_for_port(host: str, port: int, timeout: float = 10.0, interval: float = 0.1) -> bool:
    deadline = time.time() + timeout
    while time.time() < deadline:
        try:
            with socket.create_connection((host, port), timeout=0.2):
                return True
        except OSError:
            time.sleep(interval)
    try:
        with socket.create_connection((host, port), timeout=0.2):
            return True
    except OSError:
        return False


def wait_for_process_exit(
    proc: subprocess.Popen[bytes] | subprocess.Popen[str],
    timeout: float = 10.0,
    interval: float = 0.1,
) -> bool:
    deadline = time.time() + timeout
    while time.time() < deadline:
        if proc.poll() is not None:
            return True
        time.sleep(interval)
    return proc.poll() is not None


def start_cli_process(
    args: Sequence[str],
    *,
    data_dir: Path | str | None = None,
    env: Mapping[str, str] | None = None,
    cwd: Path | str | None = None,
    log_path: Path | str | None = None,
) -> subprocess.Popen[str]:
    binary = ensure_cli_binary()
    cmd = [str(binary)]
    if data_dir is not None:
        cmd.extend(["--data-dir", str(data_dir)])
    cmd.extend([str(a) for a in args])

    proc_env = os.environ.copy()
    if env:
        proc_env.update({str(k): str(v) for k, v in env.items()})

    stdout_target: int | object = subprocess.PIPE
    stderr_target: int | object = subprocess.STDOUT
    log_file = None
    if log_path is not None:
        log_file = open(log_path, "w", encoding="utf-8")
        stdout_target = log_file
        stderr_target = subprocess.STDOUT

    proc = subprocess.Popen(
        cmd,
        cwd=str(cwd) if cwd is not None else str(repo_root()),
        env=proc_env,
        stdin=subprocess.DEVNULL,
        stdout=stdout_target,  # type: ignore[arg-type]
        stderr=stderr_target,  # type: ignore[arg-type]
        text=True,
    )
    # Attach log file handle so callers can close it after process exit.
    setattr(proc, "_ferrisync_log_file", log_file)
    return proc


def stop_process(proc: subprocess.Popen[bytes] | subprocess.Popen[str], timeout: float = 5.0) -> None:
    if proc.poll() is None:
        proc.terminate()
        try:
            proc.wait(timeout=timeout)
        except subprocess.TimeoutExpired:
            proc.kill()
            proc.wait(timeout=timeout)
    log_file = getattr(proc, "_ferrisync_log_file", None)
    if log_file is not None:
        try:
            log_file.close()
        except Exception:
            pass
