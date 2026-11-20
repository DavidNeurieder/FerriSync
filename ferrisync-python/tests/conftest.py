from __future__ import annotations

import sys
from pathlib import Path

import pytest

# Ensure "helpers" package inside tests/ is importable when running pytest from
# the repository root or ferrisync-python/.
_TESTS_DIR = Path(__file__).resolve().parent
if str(_TESTS_DIR) not in sys.path:
    sys.path.insert(0, str(_TESTS_DIR))

from helpers.processes import ensure_cli_binary, find_free_port  # noqa: E402


def pytest_configure(config):
    config.addinivalue_line(
        "markers", "interop: tests that run against the Rust CLI or other frontends"
    )
    config.addinivalue_line(
        "markers", "reliability: reliability and failure-matrix integration tests"
    )
    config.addinivalue_line("markers", "slow: slow integration tests")
    config.addinivalue_line("markers", "flutter: tests that require Flutter/Linux desktop tooling")


@pytest.fixture
def ferrisync_module():
    try:
        import ferrisync  # type: ignore

        return ferrisync
    except Exception as e:
        pytest.skip(f"ferrisync extension not available: {e}")


@pytest.fixture
def isolated_data_dir(tmp_path: Path, monkeypatch) -> Path:
    data_dir = tmp_path / "ferrisync-data"
    data_dir.mkdir(parents=True, exist_ok=True)
    # Keep frontend data isolated (CLI/app conventions may use XDG dirs).
    xdg = tmp_path / "xdg-data"
    xdg.mkdir(parents=True, exist_ok=True)
    monkeypatch.setenv("XDG_DATA_HOME", str(xdg))
    return data_dir


@pytest.fixture
def free_port() -> int:
    return find_free_port()


@pytest.fixture(scope="session")
def cli_binary() -> Path:
    return ensure_cli_binary()


@pytest.fixture
def python_server(ferrisync_module, tmp_path: Path):
    data_dir = tmp_path / "server-data"
    data_dir.mkdir(parents=True, exist_ok=True)
    client = ferrisync_module.Client(data_dir)
    try:
        yield client
    finally:
        client.close()


@pytest.fixture
def python_client(ferrisync_module, tmp_path: Path):
    data_dir = tmp_path / "client-data"
    data_dir.mkdir(parents=True, exist_ok=True)
    client = ferrisync_module.Client(data_dir)
    try:
        yield client
    finally:
        client.close()
