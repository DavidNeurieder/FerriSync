PROJECT_ROOT    := $(shell pwd)
TARGET          ?= x86_64-linux-android
ANDROID_TARGETS ?= x86_64-linux-android aarch64-linux-android
ANDROID_PLATFORM ?= 24
ABI             := $(subst armv7-linux-androideabi,armeabi-v7a,$(subst aarch64-linux-android,arm64-v8a,$(subst x86_64-linux-android,x86_64,$(subst i686-linux-android,x86,$(TARGET)))))
FLUTTER_ROOT    := $(PROJECT_ROOT)/ferrisync-flutter
RUST_CORE       := $(PROJECT_ROOT)/ferrisync-core
ANDROID_DEVICE_SERIAL_FILE ?= /tmp/ferrisync-android-device-serial
RELEASE_VERSION ?= 0.1.0
RELEASE_DIR     ?= $(PROJECT_ROOT)/release/v$(RELEASE_VERSION)
# Prefer a static musl CLI when musl-gcc is installed; otherwise fall back to
# the host glibc target (still usable, but not fully static).
LINUX_RELEASE_TARGET ?= $(shell command -v musl-gcc >/dev/null 2>&1 && echo x86_64-unknown-linux-musl || echo x86_64-unknown-linux-gnu)
CLI_BIN         := target/debug/ferrisync
ANDROID_CLI     := target/$(TARGET)/release/ferrisync
# ferrisync-core is a workspace member: cargo puts artifacts in the
# WORKSPACE-ROOT target/. The .so name must match the loader stem that
# flutter_rust_bridge generates (lib/gen/frb_generated.dart).
ANDROID_SO      := $(PROJECT_ROOT)/target/$(TARGET)/release/libferrisync_core.so
JNILIB_SO       := $(FLUTTER_ROOT)/android/app/src/main/jniLibs/$(ABI)/libferrisync_core.so

# Map a Rust target triple to Android ABI name
target_to_abi = $(subst armv7-linux-androideabi,armeabi-v7a,$(subst aarch64-linux-android,arm64-v8a,$(subst x86_64-linux-android,x86_64,$(subst i686-linux-android,x86,$(1)))))
# Map a Rust target triple to the ABI name cargo-ndk expects
target_to_ndk = $(call target_to_abi,$(1))

.PHONY: all build-all build-linux-cli build-android-cli check-cargo-ndk
.PHONY: build-android-so build-android-so-universal build-android-apk build-android-apk-universal
.PHONY: test-rust test-flutter test-android-cli test-android-flutter test-all full-test
.PHONY: test-android-instrumented test-linux-flutter ensure-android-device
.PHONY: run-linux serve-linux serve-android codegen clean help install-phone
.PHONY: fdroid-check release-all

all: build-linux-cli

# ── Build ──

build-linux-cli:
	cargo build -p ferrisync

check-cargo-ndk:
	@command -v cargo-ndk >/dev/null 2>&1 || { \
	  echo 'error: cargo-ndk not found. Run: cargo install cargo-ndk --locked'; exit 1; }

# Plain `cargo build --target <android triple>` needs a NDK clang in PATH,
# which is easy to miss. cargo-ndk wires up CC/AR from ANDROID_HOME for us.
build-android-cli: check-cargo-ndk
	cargo ndk -t $(call target_to_ndk,$(TARGET)) --platform $(ANDROID_PLATFORM) \
	  build -p ferrisync --release

build-android-so: check-cargo-ndk
	cd $(RUST_CORE) && cargo ndk -t $(call target_to_ndk,$(TARGET)) --platform $(ANDROID_PLATFORM) \
	  build --release --lib
	@mkdir -p $(dir $(JNILIB_SO))
	cp $(ANDROID_SO) $(JNILIB_SO)

build-android-apk-x86_64:
	$(MAKE) build-android-apk TARGET=x86_64-linux-android

build-android-apk-arm64:
	$(MAKE) build-android-apk TARGET=aarch64-linux-android

build-android-apk: build-android-so
	cd $(FLUTTER_ROOT) && flutter build apk --debug

build-android-so-universal:
	@for target in $(ANDROID_TARGETS); do \
	  abi=$(call target_to_abi,$$target); \
	  $(MAKE) build-android-so TARGET=$$target; \
	done

build-android-apk-universal: build-android-so-universal
	cd $(FLUTTER_ROOT) && flutter build apk --debug

install-phone: build-android-apk-universal
	adb install -r $(FLUTTER_ROOT)/build/app/outputs/flutter-apk/app-debug.apk

build-all: build-linux-cli build-android-cli build-android-apk

# ── Test ──

test-rust:
	cargo test -p ferrisync-core

test-flutter:
	cd $(FLUTTER_ROOT) && flutter test

# Ensure an Android device is online. If none is connected, this starts the
# configured AVD with a VISIBLE emulator window and waits for it to boot.
# The emulator is left running for the rest of the test run.
ensure-android-device:
	@ANDROID_DEVICE_SERIAL_FILE="$(ANDROID_DEVICE_SERIAL_FILE)" scripts/ensure_android_device.sh

test-android-cli: build-linux-cli ensure-android-device
	@export ANDROID_SERIAL="$$(cat "$(ANDROID_DEVICE_SERIAL_FILE)")"; scripts/test_android_cli_sync.sh

test-android-flutter: build-linux-cli ensure-android-device
	@export DEVICE_SERIAL="$$(cat "$(ANDROID_DEVICE_SERIAL_FILE)")"; scripts/test_android_flutter_sync.sh

test-all: test-rust test-flutter

# Native instrumented tests (NotificationsControllerTest) on a connected
# device/emulator. Runs twice: granted and revoked POST_NOTIFICATIONS.
test-android-instrumented: ensure-android-device
	@serial=$$(cat "$(ANDROID_DEVICE_SERIAL_FILE)"); \
	echo "Using Android device $$serial for instrumented tests"; \
	export ANDROID_SERIAL="$$serial"; \
	cd "$(FLUTTER_ROOT)/android"; \
	./gradlew :app:installDebug && \
	(adb -s "$$serial" shell pm grant com.example.ferrisync android.permission.POST_NOTIFICATIONS || true) && \
	./gradlew :app:connectedDebugAndroidTest && \
	(adb -s "$$serial" shell pm revoke com.example.ferrisync android.permission.POST_NOTIFICATIONS || true) && \
	./gradlew :app:installDebug && \
	./gradlew :app:connectedDebugAndroidTest

test-linux-flutter: build-linux-cli
	scripts/test_linux_flutter_sync.sh

# Runs every available test target. Android targets start a visible emulator
# automatically when no device is already online. Other integration tests still
# require their host/network setup and tooling.
full-test: test-rust test-flutter test-linux-flutter test-android-instrumented test-android-cli test-android-flutter

# ── F-Droid ──

# Optional pre-submission gate: verifies the documented Rust/NDK prerequisites
# and reproduces the clean build F-Droid performs (tests + unsigned release APK).
# Run locally before tagging a release or updating F-Droid metadata.
fdroid-check:
	@echo '==> Checking build prerequisites'
	@command -v cargo >/dev/null 2>&1 || { \
	  echo 'error: cargo not found. Install Rust: https://rustup.rs'; exit 1; }
	@command -v cargo-ndk >/dev/null 2>&1 || { \
	  echo 'error: cargo-ndk not found. Run: cargo install cargo-ndk --locked'; exit 1; }
	@if command -v rustup >/dev/null 2>&1; then \
	  for target in aarch64-linux-android x86_64-linux-android; do \
	    rustup target list --installed | grep -qx $$target || { \
	      echo "error: missing Rust target $$target. Run: rustup target add aarch64-linux-android x86_64-linux-android"; \
	      exit 1; }; \
	  done; \
	fi
	@test -n "$$ANDROID_HOME" || test -n "$$ANDROID_SDK_ROOT" || { \
	  echo 'error: set ANDROID_HOME (or ANDROID_SDK_ROOT) to your Android SDK'; exit 1; }
	@echo '==> Cleaning previous build artifacts'
	$(MAKE) clean
	@echo '==> Rust tests'
	cargo test --workspace
	@echo '==> Flutter dependencies, analysis, and tests'
	cd $(FLUTTER_ROOT) && flutter pub get
	cd $(FLUTTER_ROOT) && flutter analyze
	cd $(FLUTTER_ROOT) && flutter test
	@echo '==> Building unsigned release APK (arm64-v8a + x86_64)'
	cd $(FLUTTER_ROOT) && flutter build apk --release
	@if command -v unzip >/dev/null 2>&1; then \
	  for abi in arm64-v8a x86_64; do \
	    unzip -l $(FLUTTER_ROOT)/build/app/outputs/flutter-apk/app-release.apk \
	      | grep -q "lib/$$abi/libferrisync_core.so" || { \
	        echo "error: release APK is missing lib/$$abi/libferrisync_core.so"; exit 1; }; \
	  done; \
	fi
	@echo '==> F-Droid check passed'

# ── Release artifacts (locally buildable subset) ──

# Builds the release assets that this Linux host can produce: the CLI and the
# universal unsigned Android APK. macOS and Windows binaries cannot be built
# here (Apple SDK / MinGW toolchain), so they are intentionally skipped; use
# the release-assets workflow for those.
release-all: check-cargo-ndk
	@mkdir -p "$(RELEASE_DIR)"
	@rm -f "$(RELEASE_DIR)/ferrisync-$(RELEASE_VERSION)-*" "$(RELEASE_DIR)/SHA256SUMS"
	@echo '==> Building Linux CLI ($(LINUX_RELEASE_TARGET))'
ifeq ($(LINUX_RELEASE_TARGET),x86_64-unknown-linux-musl)
	cargo build -p ferrisync --release --target x86_64-unknown-linux-musl
	cp target/x86_64-unknown-linux-musl/release/ferrisync \
	  "$(RELEASE_DIR)/ferrisync-$(RELEASE_VERSION)-linux-x86_64"
else
	@echo '    musl-gcc not found; using glibc target (not fully static)'
	cargo build -p ferrisync --release --target x86_64-unknown-linux-gnu
	cp target/x86_64-unknown-linux-gnu/release/ferrisync \
	  "$(RELEASE_DIR)/ferrisync-$(RELEASE_VERSION)-linux-x86_64"
endif
	@echo '==> Building universal unsigned Android APK'
	cd $(FLUTTER_ROOT) && flutter build apk --release
	@if command -v unzip >/dev/null 2>&1; then \
	  apk="$(FLUTTER_ROOT)/build/app/outputs/flutter-apk/app-release.apk"; \
	  for abi in arm64-v8a x86_64; do \
	    unzip -l "$$apk" | grep -q "lib/$$abi/libferrisync_core.so" || { \
	      echo "error: release APK is missing lib/$$abi/libferrisync_core.so" >&2; \
	      exit 1; \
	    }; \
	  done; \
	fi
	cp $(FLUTTER_ROOT)/build/app/outputs/flutter-apk/app-release.apk \
	  "$(RELEASE_DIR)/ferrisync-$(RELEASE_VERSION)-android-universal-unsigned.apk"
	@echo '==> Skipping macOS and Windows assets (cannot be built on this host)'
	@echo '    Build them with: gh workflow run release-assets.yml -f version=$(RELEASE_VERSION)'
	@echo '==> Generating SHA256SUMS'
	cd "$(RELEASE_DIR)" && sha256sum ferrisync-$(RELEASE_VERSION)-* > SHA256SUMS
	@echo '==> Release artifacts in $(RELEASE_DIR)'
	@ls -lh "$(RELEASE_DIR)"

# ── Run / Serve ──

run-linux: build-linux-cli
	cd $(FLUTTER_ROOT) && flutter run -d linux

serve-linux: build-linux-cli
	@mkdir -p /tmp/ferrisync-serve-folder
	$(CLI_BIN) --data-dir /tmp/ferrisync-serve-data serve \
	  --port 9847 /tmp/ferrisync-serve-folder

serve-android: build-linux-cli build-android-cli
	adb push $(ANDROID_CLI) /data/local/tmp/ferrisync
	adb shell "mkdir -p /data/local/tmp/ferrisync-serve-folder /data/local/tmp/fsd"
	adb shell "nohup /data/local/tmp/ferrisync \
	  --data-dir /data/local/tmp/fsd serve \
	  --port 9847 /data/local/tmp/ferrisync-serve-folder &"

# ── Codegen ──

codegen:
	cd $(FLUTTER_ROOT) && $(HOME)/.cargo/bin/flutter_rust_bridge_codegen generate

# ── Clean ──

clean:
	cargo clean
	cd $(FLUTTER_ROOT) && flutter clean
	rm -rf $(FLUTTER_ROOT)/android/app/src/main/jniLibs

# ── Help ──

help:
	@echo 'Targets:'
	@echo '  build-linux-cli              — Build Linux CLI debug        (cargo build)'
	@echo '  build-android-cli            — Cross-compile CLI for Android (cargo build --target)'
	@echo '  build-android-so             — Build libferrisync_core.so (single ABI, uses TARGET)'
	@echo '  build-android-so-universal   — Build .so for all targets    (x86_64 + arm64)'
	@echo '  build-android-apk            — Build Flutter APK (single ABI, uses TARGET)'
	@echo '  build-android-apk-universal  — Build universal APK          (x86_64 + arm64)'
	@echo '  build-android-apk-x86_64     — Build for emulator (x86_64)'
	@echo '  build-android-apk-arm64      — Build for physical phone (arm64)'
	@echo '  install-phone                — Build universal APK + adb install to connected device'
	@echo '  build-all                    — build-linux-cli + build-android-cli + build-android-apk'
	@echo '  test-rust                    — cargo test (Rust)'
	@echo '  test-flutter                 — flutter test (Linux desktop)'
	@echo '  test-android-cli             — CLI sync test (auto-detects device ABI)'
	@echo '  test-android-flutter         — Flutter integration tests: UI + FRB smoke + sync (universal APK)'
	@echo '  ensure-android-device        — Ensure a device is online; start visible emulator if needed'
	@echo '  test-android-instrumented    — Native notification tests on device (granted + revoked)'
	@echo '  test-linux-flutter           — Flutter integration tests on Linux desktop'
	@echo '  test-all                     — Rust + Flutter tests'
	@echo '  full-test                    — Run every test target (starts a visible emulator if Android device is absent)'
	@echo '  fdroid-check                 — Prereq checks + tests + clean unsigned release APK (F-Droid gate)'
	@echo '  release-all                  — Build Linux CLI + Android APK into release/v<version>'
	@echo '  run-linux                    — flutter run -d linux'
	@echo '  serve-linux                  — Start serve on Linux host'
	@echo '  serve-android                — Push + start serve on device'
	@echo '  codegen                      — FRB codegen (regenerates lib/gen from ferrisync-core)'
	@echo '  clean                        — Remove build artifacts'
	@echo ''
	@echo 'Variables:'
	@echo '  ANDROID_TARGETS=x86_64-linux-android aarch64-linux-android — targets for universal build'
	@echo '  TARGET=x86_64-linux-android       — single Rust target triple'
	@echo '  ANDROID_PLATFORM=24               — Android API level for cargo-ndk builds'
	@echo '  AVD_NAME=test_phone               — AVD started by Android test targets when no device is online'
	@echo '  DEVICE_SERIAL=<serial>            — use a specific connected device instead of auto-selecting'
	@echo '  EMU_HEADLESS=1                    — use headless mode instead of the visible emulator window'
	@echo '  EMU_BOOT_TIMEOUT=240              — seconds to wait for the emulator to finish booting'
	@echo '  RELEASE_VERSION=0.1.0             — version folder (release/v<version>) and asset names'
	@echo '  RELEASE_DIR=<path>                — override the release output directory'
