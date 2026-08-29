PROJECT_ROOT    := $(shell pwd)
TARGET          ?= x86_64-linux-android
ANDROID_TARGETS ?= x86_64-linux-android aarch64-linux-android
ABI             := $(subst armv7-linux-androideabi,armeabi-v7a,$(subst aarch64-linux-android,arm64-v8a,$(subst x86_64-linux-android,x86_64,$(subst i686-linux-android,x86,$(TARGET)))))
FLUTTER_ROOT    := $(PROJECT_ROOT)/ferrisync-flutter
RUST_CORE       := $(PROJECT_ROOT)/ferrisync-core
CLI_BIN         := target/debug/ferrisync
ANDROID_CLI     := target/$(TARGET)/release/ferrisync
# ferrisync-core is a workspace member: cargo puts artifacts in the
# WORKSPACE-ROOT target/. The .so name must match the loader stem that
# flutter_rust_bridge generates (lib/gen/frb_generated.dart).
ANDROID_SO      := $(PROJECT_ROOT)/target/$(TARGET)/release/libferrisync_core.so
JNILIB_SO       := $(FLUTTER_ROOT)/android/app/src/main/jniLibs/$(ABI)/libferrisync_core.so

# Map a Rust target triple to Android ABI name
target_to_abi = $(subst armv7-linux-androideabi,armeabi-v7a,$(subst aarch64-linux-android,arm64-v8a,$(subst x86_64-linux-android,x86_64,$(subst i686-linux-android,x86,$(1)))))

.PHONY: all build-all build-linux-cli build-android-cli
.PHONY: build-android-so build-android-so-universal build-android-apk build-android-apk-universal
.PHONY: test-rust test-flutter test-android-cli test-android-flutter test-all
.PHONY: test-android-instrumented test-linux-flutter
.PHONY: run-linux serve-linux serve-android codegen clean help install-phone

all: build-linux-cli

# ── Build ──

build-linux-cli:
	cargo build -p ferrisync

build-android-cli:
	cargo build -p ferrisync --target $(TARGET) --release

build-android-so:
	cd $(RUST_CORE) && cargo build --target $(TARGET) --release
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

test-android-cli: build-linux-cli
	scripts/test_android_cli_sync.sh

test-android-flutter: build-linux-cli
	scripts/test_android_flutter_sync.sh

test-all: test-rust test-flutter

# Native instrumented tests (NotificationsControllerTest) on a connected
# device/emulator. Runs twice: granted and revoked POST_NOTIFICATIONS.
test-android-instrumented:
	cd $(FLUTTER_ROOT)/android && ./gradlew :app:installDebug
	adb shell pm grant com.example.ferrisync android.permission.POST_NOTIFICATIONS || true
	cd $(FLUTTER_ROOT)/android && ./gradlew :app:connectedDebugAndroidTest
	adb shell pm revoke com.example.ferrisync android.permission.POST_NOTIFICATIONS || true
	cd $(FLUTTER_ROOT)/android && ./gradlew :app:installDebug
	cd $(FLUTTER_ROOT)/android && ./gradlew :app:connectedDebugAndroidTest

test-linux-flutter: build-linux-cli
	scripts/test_linux_flutter_sync.sh

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
	@echo '  test-android-instrumented    — Native notification tests on device (granted + revoked)'
	@echo '  test-linux-flutter           — Flutter integration tests on Linux desktop'
	@echo '  test-all                     — All tests'
	@echo '  run-linux                    — flutter run -d linux'
	@echo '  serve-linux                  — Start serve on Linux host'
	@echo '  serve-android                — Push + start serve on device'
	@echo '  codegen                      — FRB codegen (regenerates lib/gen from ferrisync-core)'
	@echo '  clean                        — Remove build artifacts'
	@echo ''
	@echo 'Variables:'
	@echo '  ANDROID_TARGETS=x86_64-linux-android aarch64-linux-android — targets for universal build'
	@echo '  TARGET=x86_64-linux-android       — single Rust target triple'
