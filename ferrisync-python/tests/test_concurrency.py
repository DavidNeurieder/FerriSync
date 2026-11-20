from __future__ import annotations

import asyncio
from pathlib import Path

import pytest


def _import_ferrisync():
    try:
        import ferrisync  # type: ignore

        return ferrisync
    except Exception as e:
        pytest.skip(f"ferrisync extension not available: {e}")


@pytest.mark.reliability
def test_async_bounded_executor_concurrent_calls(tmp_path: Path):
    ferrisync = _import_ferrisync()

    async def run():
        data = tmp_path / "data"
        data.mkdir()
        async with ferrisync.AsyncClient(data, max_workers=2) as client:
            # Mix of lightweight operations; all must complete without starving the loop.
            async def probe(i: int):
                # Interleave operations to exercise bounded executor.
                if i % 2 == 0:
                    _ = await client.status()
                else:
                    _ = await client.devices()
                await asyncio.sleep(0)  # event loop responsiveness check
                return i

            start = asyncio.get_running_loop().time()
            results = await asyncio.gather(*(probe(i) for i in range(20)))
            elapsed = asyncio.get_running_loop().time() - start

            assert sorted(results) == list(range(20))
            # Sanity bound: should complete quickly on a healthy system.
            assert elapsed < 30.0

            # Client still usable after concurrent load.
            _ = await client.status()
            _ = await client.poll_events()

    asyncio.run(run())
