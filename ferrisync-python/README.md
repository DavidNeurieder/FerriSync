# FerriSync Python

Experimental Python bindings for [FerriSync](https://github.com/DavidNeurieder/FerriSync),
a LAN-first peer-to-peer folder synchronization engine.

This package wraps the existing `ferrisync-core` Rust engine via PyO3/maturin.
It is a **developer preview (0.1.0 MVP)** and the API may change.

## Status and guarantees (0.1.0 MVP)

- Synchronous `Client` API with discovery, pairing, device/folder management,
  hosting, sync, background folder watches (`watch()`/`Watcher`), health, and
  conflict helpers.
- `AsyncClient` wrapper using a bounded thread-pool executor (cooperative async
  API). Sync/watch operations accept cancellation signals; cancellation is
  cooperative and checked at API/boundary points. Native work already in
  progress may continue briefly, but cancelled awaitables raise
  `asyncio.CancelledError`/`FerriSyncCancelledError` and their results should be
  discarded.
- Events are available via polling (`client.poll_events()`), iterator helpers
  (`client.events()`), and polling-based callback subscriptions
  (`client.subscribe(...)`).
- Each client owns its engine instance, folder servers, and background watches.
  `close()` / context-manager exit stops background activity owned by that
  instance. Integration tests cover Python↔Python sync, CLI interop, selected
  reliability scenarios, and a primary Linux Flutter desktop interop path
  (Python server ↔ Flutter client). Reverse-direction Flutter-server ↔
  Python-client automation is not yet covered by the Flutter interop script.
- `serve()` defaults to the confirm policy (unknown peers must be approved via
  `pending_pairings()` / `approve_pairing()` / `deny_pairing()`). Pass
  `auto_accept=True` only on trusted private networks — it grants read/write
  access to any device that can reach the host.
- Requires the same data-directory identity model as other FerriSync frontends
  (`metadata.db`, `identity.crt`, `identity.key`).
- MSRV/build compatibility: PyO3 is pinned to `=0.27.2` with `abi3-py310` to
  remain compatible with the workspace Rust MSRV (1.80). Blocking native calls
  release the Python GIL while running Rust/Tokio work.

## Install (development)

```bash
cd ferrisync-python
python3 -m venv .venv
source .venv/bin/activate
pip install maturin pytest
maturin develop
```

## Quick start

```python
import ferrisync

with ferrisync.Client() as client:
    peers = client.discover(timeout=5)
    if peers:
        peer = peers[0]
        device_id = client.pair(peer.ip, peer.port)
        folder_id = client.add_folder("/path/to/local", device_id=device_id)
        result = client.sync(
            folder_id=folder_id,
            local_path="/path/to/local",
            remote_ip=peer.ip,
            remote_port=peer.port,
            device_id=device_id,
        )
        print(result)
```

## Async usage

```python
import asyncio
import ferrisync

async def main():
    async with ferrisync.AsyncClient() as client:
        peers = await client.discover(timeout=5)
        print(peers)

asyncio.run(main())
```

## Notes

- FerriSync is LAN-first and uses TOFU pairing with explicit consent for unknown
  devices.
- The package is licensed under AGPL-3.0-only, consistent with FerriSync.
- See the repository root for build, security, and contribution details.

## Platform support

| Platform | Wheel build target | Install/smoke validation |
| --- | --- | --- |
| Linux | `x86_64` (manylinux via CI) | Clean-venv install + loopback sync smoke in wheel CI |
| macOS | `x86_64` (`macos-13`) and `arm64` (`macos-14`) | Clean-venv install + loopback sync smoke per target |
| Windows | `x86_64` | Clean-venv install + loopback sync smoke in wheel CI |

> Note: Linux aarch64 wheel builds are not yet validated in the current workflow matrix.

## Install

```bash
pip install ferrisync
```

## Building wheels and TestPyPI

Build wheels locally with [maturin](https://www.maturin.rs/):

```bash
cd ferrisync-python
python -m pip install --upgrade maturin
maturin build --release
# artifacts: ferrisync-python/target/wheels/*.whl
```

CI workflow: `.github/workflows/python-wheels.yml` builds wheels/sdist, verifies
each wheel in a clean virtual environment (import, version consistency, `pip check`,
and loopback sync smoke), and can publish to TestPyPI via `workflow_dispatch`.
Enable `publish_testpypi` to publish and run post-publish install smoke tests.

Version sync: keep `Cargo.toml` workspace `version`, `ferrisync-python` (inherits
workspace version), `ferrisync-python/pyproject.toml` `project.version`,
`CHANGELOG.md`, and workflow defaults aligned. Use
`scripts/check_python_version.py` to verify consistency.
