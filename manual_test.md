# Manual test: Android phone ↔ Linux sync

## Prerequisites

- Android phone with **USB debugging** enabled (Developer options)
- Phone connected via USB (or same Wi-Fi network for wireless adb)
- Android SDK at `/home/mr/Android/Sdk`
- Rust target installed: `rustup target add aarch64-linux-android`

## Setup

```bash
# 1. Verify the phone is connected
adb devices
# Should show: <serial>  device

# 2. Build and install the APK
make build-android-apk-arm64
adb install -r ferrisync-flutter/build/app/outputs/flutter-apk/app-debug.apk

# 3. Start the CLI serve on the host (acts as sync server)
make serve-linux
# Or manually:
#   mkdir -p /tmp/ferrisync-serve-folder
#   printf 'host_file' > /tmp/ferrisync-serve-folder/from_host.txt
#   cargo run -p ferrisync-cli -- --data-dir /tmp/ferrisync-serve-data serve --port 9847 /tmp/ferrisync-serve-folder
```

The serve process now **advertises itself via mDNS** so the phone can discover it.

## Connection options

**Option A — USB (adb reverse):** forward the phone's port to the host
```bash
adb reverse tcp:9847 tcp:9847
```
Phone connects to `127.0.0.1:9847`.

**Option B — Discovery (mDNS scan):** phone finds the server automatically
1. Open the FerriSync app on the phone
2. Devices tab → tap the **Scan** button (magnifying glass icon)
3. After a few seconds, the serve server appears in the list
4. Tap **Pair** next to the discovered server

**Option C — Wi-Fi (LAN IP):** phone connects directly to the host's IP
```bash
HOST_IP=$(ip -4 -o addr show | awk '!/ lo /{print $4; exit}' | cut -d/ -f1)
echo "Host IP: $HOST_IP"
```
Phone connects to `$HOST_IP:9847`.

## Pairing flow

1. Open the FerriSync app on the phone
2. **Devices tab** → tap **+** or **Scan**
3. Enter IP:port manually, or scan, or tap a discovered device
4. Tap **Pair** — the serve process accepts via TOFU
5. The paired device appears in the list

## Adding a sync folder

1. **Folders tab** → tap **+**
2. Pick a directory on the phone
3. Select the paired device from the list
4. The folder is registered for bidirectional sync

## Test the sync

Once paired and a folder is configured:

- Files created on the phone appear in `/tmp/ferrisync-serve-folder`
- Files placed in `/tmp/ferrisync-serve-folder` appear on the phone
- Conflicts: the file with the newer mtime wins; the older version is saved as `.bak`

## Debugging

| Symptom | Fix |
|---------|-----|
| `adb: error: no devices/emulators found` | Enable USB debugging, check `adb usb` |
| `flutter: ADB rejected` | Accept the RSA key prompt on the phone |
| Connection refused | Ensure `serve-linux` is running and port is correct |
| `adb reverse` fails | USB debugging not authorized or cable issue |
| Pairing fails silently | Check `serve-linux` terminal for TLS or deserialization errors |
| Scan finds nothing | Both devices must be on the same LAN; check firewall (port 5353 for mDNS) |

## Automated integration test

```bash
make test-android-flutter
```

Starts the emulator headless — **not applicable for a physical phone**. To run
the integration test on a physical phone, connect it, then:

```bash
cd ferrisync-flutter && flutter test integration_test/sync_test.dart -d "$(adb devices | awk 'NR==2{print $1}')"
```
