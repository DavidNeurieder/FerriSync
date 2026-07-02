PROJECT_ROOT   := $(shell pwd)
TARGET         ?= x86_64-linux-android
ABI            := $(subst armv7-linux-androideabi,armeabi-v7a,$(subst aarch64-linux-android,arm64-v8a,$(subst x86_64-linux-android,x86_64,$(subst i686-linux-android,x86,$(TARGET)))))
FLUTTER_ROOT   := $(PROJECT_ROOT)/ferrisync-flutter
RUST_FLUTTER   := $(FLUTTER_ROOT)/rust
CLI_BIN        := target/debug/ferrisync-cli
ANDROID_CLI    := target/$(TARGET)/release/ferrisync-cli
ANDROID_SO     := $(RUST_FLUTTER)/target/$(TARGET)/release/libferrisync_flutter.so
JNILIB_SO      := $(FLUTTER_ROOT)/android/app/src/main/jniLibs/$(ABI)/libferrisync_flutter.so

.PHONY: all build-all build-linux-cli build-android-cli build-android-so build-android-apk build-android-apk-x86_64 build-android-apk-arm64
.PHONY: test-rust test-flutter test-android-cli test-android-flutter test-all
.PHONY: run-linux serve-linux serve-android codegen clean help

all: build-linux-cli

# ── Build ──

build-linux-cli:
	cargo build -p ferrisync-cli

build-android-cli:
	cargo build -p ferrisync-cli --target $(TARGET) --release

build-android-so: $(JNILIB_SO)

$(JNILIB_SO): $(ANDROID_SO)
	@mkdir -p $(dir $@)
	cp $< $@

$(ANDROID_SO):
	cd $(RUST_FLUTTER) && cargo build --target $(TARGET) --release

build-android-apk-x86_64:
	$(MAKE) build-android-apk TARGET=x86_64-linux-android

build-android-apk-arm64:
	$(MAKE) build-android-apk TARGET=aarch64-linux-android

build-android-apk: build-android-so
	cd $(FLUTTER_ROOT) && flutter build apk --debug

build-all: build-linux-cli build-android-cli build-android-apk

# ── Test ──

test-rust:
	cargo test -p ferrisync-core

test-flutter:
	cd $(FLUTTER_ROOT) && flutter test

test-android-cli: build-android-cli build-linux-cli
	scripts/test_android_cli_sync.sh

test-android-flutter: build-android-apk build-linux-cli
	scripts/test_android_flutter_sync.sh

test-all: test-rust test-flutter

# ── Run / Serve ──

run-linux: build-linux-cli
	cd $(FLUTTER_ROOT) && flutter run -d linux

serve-linux: build-linux-cli
	@mkdir -p /tmp/ferrisync-serve-folder
	$(CLI_BIN) --data-dir /tmp/ferrisync-serve-data serve \
	  --port 9847 /tmp/ferrisync-serve-folder

serve-android: build-linux-cli build-android-cli
	adb push $(ANDROID_CLI) /data/local/tmp/ferrisync-cli
	adb shell "mkdir -p /data/local/tmp/ferrisync-serve-folder /data/local/tmp/fsd"
	adb shell "nohup /data/local/tmp/ferrisync-cli \
	  --data-dir /data/local/tmp/fsd serve \
	  --port 9847 /data/local/tmp/ferrisync-serve-folder &"

# ── Codegen ──

codegen:
	cd $(FLUTTER_ROOT) && flutter_rust_bridge_codegen generate
	cd $(FLUTTER_ROOT) && sed -i \
	  "s/stem: 'ferrisync_core'/stem: 'ferrisync_flutter'/; \
	   s|ioDirectory: '../ferrisync-core/target/release/'|ioDirectory: 'rust/target/release/'|" \
	  lib/gen/frb_generated.dart

# ── Clean ──

clean:
	cargo clean
	cd $(FLUTTER_ROOT) && flutter clean
	rm -rf $(FLUTTER_ROOT)/android/app/src/main/jniLibs

# ── Help ──

help:
	@echo 'Targets:'
	@echo '  build-linux-cli        — Build Linux CLI debug          (cargo build)'
	@echo '  build-android-cli      — Cross-compile CLI for Android  (cargo build --target)'
	@echo '  build-android-so       — Build libferrisync_flutter.so  (for APK)'
	@echo '  build-android-apk      — Build Flutter APK (debug, uses $(TARGET))'
	@echo '  build-android-apk-x86_64 — Build for emulator (x86_64)'
	@echo '  build-android-apk-arm64  — Build for physical phone (arm64)'
	@echo '  build-all              — All of the above'
	@echo '  test-rust              — cargo test (Rust)'
	@echo '  test-flutter           — flutter test (Linux desktop)'
	@echo '  test-android-cli       — CLI sync test on emulator'
	@echo '  test-android-flutter   — Flutter sync test on emulator'
	@echo '  test-all               — All tests'
	@echo '  run-linux              — flutter run -d linux'
	@echo '  serve-linux            — Start serve on Linux host'
	@echo '  serve-android          — Push + start serve on emulator'
	@echo '  codegen                — FRB codegen + re-patch loader'
	@echo '  clean                  — Remove build artifacts'
	@echo ''
	@echo 'Variables:'
	@echo '  TARGET=x86_64-linux-android — Rust target triple (default: x86_64-linux-android)'
	@echo '  ABI=arm64-v8a         — Android ABI (derived from TARGET)'
