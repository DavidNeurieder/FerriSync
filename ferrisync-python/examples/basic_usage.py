import ferrisync


def main() -> None:
    # Use an explicit data dir to avoid touching your real FerriSync identity.
    with ferrisync.Client(data_dir="./.ferrisync-python-demo") as client:
        print("version:", ferrisync.__version__)
        print("device id:", client.device_id)
        print("device name:", client.device_name)
        print("folders:", client.folders())
        print("devices:", client.devices())
        print("health:", client.status()["health"])


if __name__ == "__main__":
    main()
