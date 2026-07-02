# Manual test: Android phone ↔ Linux sync

## Prerequisites

- Android phone with **USB debugging** enabled (Developer options)
- Phone connected via USB (or same Wi-Fi network for wireless adb)
- Android SDK at `/home/mr/Android/Sdk`

## Setup

```bash
# 1. Verify the phone is connected
adb devices
# Should show: <serial>  device

# 2. Build and install the APK
make build-android-apk
adb install -r ferrisync-flutter/build/app/outputs/flutter-apk/app-debug.apk
# Or: cd ferrisync-flutter && flutter run  (builds + installs + launches)

# 3. Start the CLI serve on the host (in another terminal)
make serve-linux
# Or manually:
#   mkdir -p /tmp/ferrisync-serve-folder
#   printf 'host_file' > /tmp/ferrisync-serve-folder/from_host.txt
#   cargo run -p ferrisync-cli -- --data-dir /tmp/ferrisync-serve-data serve --port 9847 /tmp/ferrisync-serve-folder
```

### Connection options

**Option A — USB (adb reverse):** forward the phone's port to the host
```bash
adb reverse tcp:9847 tcp:9847
```
Phone connects to `127.0.0.1:9847`. Works over USB or wireless adb.

**Option B — Wi-Fi (LAN IP):** phone connects directly to the host's IP
```bash
HOST_IP=$(ip -4 -o addr show | awk '!/ lo /{print $4; exit}' | cut -d/ -f1)
echo "Host IP: $HOST_IP"
```
Phone connects to `$HOST_IP:9847`. Requires both devices on the same network.

## Test the sync

1. Open the FerriSync app on the phone
2. **Pair:** Devices tab → **+** → enter IP and port → **Pair**
   - USB: `127.0.0.1:9847`
   - Wi-Fi: `<host-lan-ip>:9847`
3. **Add folder:** Folders tab → **+** → pick any directory on the phone
   - The folder syncs bidirectionally with the host's serve folder
4. **Dashboard** shows sync status, device ID, and paired devices
5. **Verify:**
   - Files created on the phone appear in `/tmp/ferrisync-serve-folder`
   - Files placed in `/tmp/ferrisync-serve-folder` appear on the phone

## Debugging

| Symptom | Fix |
|---------|-----|
| `adb: error: no devices/emulators found` | Enable USB debugging, check `adb usb` |
| `flutter: ADB rejected` | Accept the RSA key prompt on the phone |
| Connection refused | Ensure `serve-linux` is running and port is correct |
| `adb reverse` fails | USB debugging not authorized or cable issue |
| Sync doesn't start | Check Dashboard for error status; verify pairing |

## Automated integration test

```bash
make test-android-flutter
```

Starts the emulator headless — **not applicable for a physical phone**. To run
the integration test on a physical phone, connect it, then:

```bash
cd ferrisync-flutter && flutter test integration_test/sync_test.dart -d "$(adb devices | awk 'NR==2{print $1}')"
```
