from __future__ import annotations

import os
import shutil
import subprocess

import pytest
from helpers.processes import repo_root


@pytest.mark.interop
@pytest.mark.flutter
@pytest.mark.slow
def test_python_flutter_interop_script():
    if shutil.which("flutter") is None:
        pytest.skip("flutter not available")
    if shutil.which("cargo") is None:
        pytest.skip("cargo not available")

    script = repo_root() / "scripts" / "test_python_flutter_interop.sh"
    assert script.exists(), f"missing script: {script}"

    env = os.environ.copy()
    # Keep test isolated and avoid polluting user dirs; script manages its own temps.
    result = subprocess.run(
        ["bash", str(script)],
        cwd=str(repo_root()),
        env=env,
        text=True,
        capture_output=True,
        timeout=60 * 30,
    )
    if result.returncode != 0:
        raise AssertionError(
            "Python<->Flutter interop script failed\n"
            f"exit={result.returncode}\n"
            f"--- stdout ---\n{result.stdout[-8000:]}\n"
            f"--- stderr ---\n{result.stderr[-8000:]}"
        )
