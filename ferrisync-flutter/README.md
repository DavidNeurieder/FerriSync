# FerriSync — Flutter client

Flutter frontend for [FerriSync](../README.md), targeting Android and Linux
desktop. It talks to the shared `ferrisync-core` Rust crate through
[flutter_rust_bridge](https://cjycode.com/flutter_rust_bridge/).

## Prerequisites

- Flutter (stable channel) with the Android toolchain (`flutter doctor`)
- Rust 1.80+ installed via [`rustup`](https://rustup.rs)
- Android SDK and NDK (or `ANDROID_NDK_HOME` pointing at an NDK)
- [`cargo-ndk`](https://github.com/bbqsrc/cargo-ndk) for the Android native build

```bash
rustup target add aarch64-linux-android x86_64-linux-android
cargo install cargo-ndk --locked

export ANDROID_HOME="$HOME/Android/Sdk"
# Optional: if the NDK is not installed under $ANDROID_HOME/ndk/<version>
export ANDROID_NDK_HOME="$ANDROID_HOME/ndk/28.2.13676358"
```

Gradle rebuilds the Rust core with `cargo ndk` on every Android build
(`android/app/build.gradle.kts`) and fails with a clear error if `cargo`,
`cargo-ndk`, or the NDK is missing.

## Setup and run

```bash
flutter pub get
flutter run          # connected Android device or emulator
flutter run -d linux # Linux desktop
```

The generated Flutter Rust Bridge bindings under `lib/gen/` are checked in, so
codegen is only required after changing the Rust API:

```bash
make codegen
```

## QR pairing

QR codes are scanned with [`flutter_zxing`](https://pub.dev/packages/flutter_zxing)
(ZXing-C++, MIT licensed). This intentionally avoids the proprietary ML Kit /
Google Play Services stack used by `mobile_scanner`. The app declares only the
`CAMERA` permission for scanning; scanning is optional and manual IP entry still
works without it.

## Tests and checks

```bash
flutter analyze
flutter test
```

From the repository root, `make fdroid-check` runs the full local F-Droid gate:
prerequisite checks, `cargo test --workspace`, `flutter analyze`, `flutter test`,
and a clean unsigned `flutter build apk --release` verified to contain both
`arm64-v8a` and `x86_64` native libraries.

## Release builds

```bash
flutter build apk --release
```

The APK is written to `build/app/outputs/flutter-apk/app-release.apk`. Release
signing is configured only when `android/key.properties` exists; otherwise the
APK is left unsigned so F-Droid (or your own release pipeline) can sign it.
Fastlane/F-Droid metadata lives at the repository root in `fastlane/metadata/`.
