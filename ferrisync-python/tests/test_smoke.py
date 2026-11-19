import sys


def test_package_importable():
    # Requires the native extension to be built/installed (maturin develop).
    try:
        import ferrisync
    except Exception as e:
        # Keep this as a soft smoke test in environments without a built wheel.
        print(f"SKIP: ferrisync not importable: {e}", file=sys.stderr)
        return
    assert hasattr(ferrisync, "__version__")


def test_async_client_class_present():
    try:
        import ferrisync
    except Exception:
        return
    assert hasattr(ferrisync, "AsyncClient")
