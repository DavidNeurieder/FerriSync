import ferrisync


def main() -> None:
    with ferrisync.Client() as client:
        peers = client.discover(timeout=5)
        for p in peers:
            print(f"{p.name} ({p.id}) at {p.ip}:{p.port}")


if __name__ == "__main__":
    main()
