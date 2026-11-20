#!/usr/bin/env python3
from __future__ import annotations

import argparse
import json
import re
import sys
from pathlib import Path

try:
    import tomllib  # Python 3.11+
except Exception:  # pragma: no cover
    import tomli as tomllib  # type: ignore


def read_toml(path: Path) -> dict:
    with path.open("rb") as f:
        return tomllib.load(f)


def read_text(path: Path) -> str:
    return path.read_text(encoding="utf-8")


def main() -> int:
    parser = argparse.ArgumentParser(description="Check FerriSync Python/version consistency")
    parser.add_argument("--expected", help="Expected version (e.g. 0.1.0)")
    parser.add_argument(
        "--skip-import",
        action="store_true",
        help="Skip importing ferrisync.__version__ (useful in source-only checks)",
    )
    args = parser.parse_args()

    root = Path(__file__).resolve().parents[1]
    errors: list[str] = []

    workspace = read_toml(root / "Cargo.toml")
    ws_version = workspace.get("workspace", {}).get("package", {}).get("version")
    if not ws_version:
        errors.append("workspace Cargo.toml missing workspace.package.version")

    py_cargo = read_toml(root / "ferrisync-python" / "Cargo.toml")
    py_pkg = py_cargo.get("package", {})
    py_cargo_version = py_pkg.get("version")
    py_cargo_ws = py_pkg.get("version", None)
    # version.workspace = true is represented as {"workspace": true} in tomllib
    if not (py_cargo_version == {"workspace": True} or ws_version):
        pass

    pyproject = read_toml(root / "ferrisync-python" / "pyproject.toml")
    pyproject_version = pyproject.get("project", {}).get("version")
    if not pyproject_version:
        errors.append("ferrisync-python/pyproject.toml missing project.version")

    # CHANGELOG top released heading: ## [X.Y.Z] - ...
    # Skip the Keep-a-Changelog "Unreleased" section; it is not a release.
    changelog = read_text(root / "CHANGELOG.md")
    m = re.search(
        r"^## \[(?!Unreleased\])([^\]]+)\]", changelog, flags=re.MULTILINE
    )
    changelog_version = m.group(1) if m else None
    if not changelog_version:
        errors.append("CHANGELOG.md missing a top released version heading")

    wheels = read_text(root / ".github" / "workflows" / "python-wheels.yml")
    # Informational workflow input default
    wm = re.search(r"version:\n(?:.*\n)*?\s+default:\s*'([^']+)'", wheels)
    wheels_default = wm.group(1) if wm else None
    if not wheels_default:
        errors.append("python-wheels.yml missing workflow_dispatch.inputs.version.default")

    imported_version = None
    if not args.skip_import:
        try:
            import ferrisync  # type: ignore

            imported_version = getattr(ferrisync, "__version__", None)
        except Exception as e:
            errors.append(f"could not import ferrisync.__version__: {e}")

    versions = {
        "workspace Cargo.toml": ws_version,
        "ferrisync-python pyproject.toml": pyproject_version,
        "CHANGELOG.md": changelog_version,
        "python-wheels.yml default": wheels_default,
    }
    if imported_version is not None:
        versions["ferrisync.__version__"] = imported_version

    # ferrisync-python inherits workspace version
    if py_pkg.get("version") != {"workspace": True}:
        errors.append("ferrisync-python/Cargo.toml should use version.workspace = true")

    expected = args.expected or ws_version
    for name, value in versions.items():
        if expected and value != expected:
            errors.append(f"{name} is {value!r}, expected {expected!r}")

    print(json.dumps({"expected": expected, "versions": versions, "errors": errors}, indent=2))

    if errors:
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
