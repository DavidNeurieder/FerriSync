import asyncio

import ferrisync


async def main() -> None:
    async with ferrisync.AsyncClient() as client:
        print(await client.devices())
        print(await client.folders())


if __name__ == "__main__":
    asyncio.run(main())
