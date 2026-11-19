# FerriSync Python

Experimental Python bindings for [FerriSync](https://github.com/DavidNeurieder/FerriSync),
a LAN-first peer-to-peer folder synchronization engine.

This package wraps the existing `ferrisync-core` Rust engine via PyO3/maturin.
It is a **developer preview (0.1.0 MVP)** and the API may change.

## Status and limitations (0.1.0)

- Synchronous `Client` API with discovery, pairing, device/folder management,
  hosting, sync, health, and conflict helpers.
- `AsyncClient` wrapper using `asyncio.to_thread`.
- Events are available via polling (`client.poll_events()`); callback/watch APIs
  are planned for 0.2.
- **One active FerriSync server/client lifecycle per process is recommended.**
  The underlying core currently uses a global server registry; concurrent clients
  in the same process may interfere. This will be addressed by core lifecycle
  refactoring (see `docs/python-library-plan.md`).
- `serve()` uses the core's confirm policy (unknown peers must be approved via
  `pending_pairings()` / `approve_pairing()` / `deny_pairing()`). Unattended
  auto-accept is not exposed in 0.1.
- Requires the same data-directory identity model as other FerriSync frontends
  (`metadata.db`, `identity.crt`, `identity.key`).

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
