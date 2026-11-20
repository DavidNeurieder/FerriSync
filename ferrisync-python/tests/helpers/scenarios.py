from __future__ import annotations

import hashlib
from pathlib import Path


def write_text(path: Path, content: str, encoding: str = "utf-8") -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(content, encoding=encoding)


def write_bytes(path: Path, content: bytes) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(content)


def deterministic_bytes(size: int, seed: bytes = b"ferrisync") -> bytes:
    out = bytearray()
    counter = 0
    while len(out) < size:
        h = hashlib.sha256(seed + counter.to_bytes(8, "little")).digest()
        out.extend(h)
        counter += 1
    return bytes(out[:size])


def sha256_file(path: Path) -> str:
    h = hashlib.sha256()
    with path.open("rb") as f:
        for chunk in iter(lambda: f.read(1024 * 1024), b""):
            h.update(chunk)
    return h.hexdigest()


def find_conflict_backups(root: Path, relative_path: str) -> list[Path]:
    """Return ``.ferrisync-conflict-*`` backups for ``relative_path``."""
    target = root / relative_path
    return sorted(target.parent.glob(f"{target.name}.ferrisync-conflict-*"))


def make_nested_files(root: Path) -> dict[str, str]:
    files = {
        "a.txt": "alpha",
        "nested/b.txt": "bravo",
        "nested/deeper/c.txt": "charlie",
    }
    for rel, content in files.items():
        write_text(root / rel, content)
    return files
