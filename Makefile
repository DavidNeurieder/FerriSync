PROJECT_ROOT   := $(shell pwd)
TARGET         ?= x86_64-linux-android
FLUTTER_ROOT   := $(PROJECT_ROOT)/ferrisync-flutter
RUST_FLUTTER   := $(FLUTTER_ROOT)/rust
CLI_BIN        := target/debug/ferrisync-cli
ANDROID_CLI    := target/$(TARGET)/release/ferrisync-cli
ANDROID_SO     := $(RUST_FLUTTER)/target/$(TARGET)/release/libferrisync_flutter.so
JNILIB_SO      := $(FLUTTER_ROOT)/android/app/src/main/jniLibs/$(TARGET)/libferrisync_flutter.so

.PHONY: all build-all build-cli build-cli-android build-flutter-so build-flutter
.PHONY: test-rust test-flutter test-cli-android test-flutter-android test-all
.PHONY: run serve serve-android codegen clean help

all: build-cli

# ── Build ──

build-cli:
	cargo build -p ferrisync-cli

build-cli-android:
	cargo build -p ferrisync-cli --target $(TARGET) --release

build-flutter-so: $(JNILIB_SO)

$(JNILIB_SO): $(ANDROID_SO)
	@mkdir -p $(dir $@)
	cp $< $@

$(ANDROID_SO):
	cd $(RUST_FLUTTER) && cargo build --target $(TARGET) --release

build-flutter: build-flutter-so
	cd $(FLUTTER_ROOT) && flutter build apk --debug

build-all: build-cli build-cli-android build-flutter

# ── Test ──

test-rust:
	cargo test -p ferrisync-core

test-flutter:
	cd $(FLUTTER_ROOT) && flutter test

test-cli-android: build-cli-android build-cli
	scripts/test_android_cli_sync.sh

test-flutter-android: build-flutter build-cli
	scripts/test_android_flutter_sync.sh

test-all: test-rust test-flutter

# ── Run / Serve ──

run: build-cli
	cd $(FLUTTER_ROOT) && flutter run -d linux

serve: build-cli
	@mkdir -p /tmp/ferrisync-serve-folder
	$(CLI_BIN) --data-dir /tmp/ferrisync-serve-data serve \
	  --port 9847 /tmp/ferrisync-serve-folder

serve-android: build-cli build-cli-android
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
	@echo '  build-cli              — Build host CLI debug'
	@echo '  build-cli-android      — Cross-compile CLI for Android'
	@echo '  build-flutter-so       — Cross-compile Rust lib + copy to jniLibs'
	@echo '  build-flutter          — Build Flutter APK (debug)'
	@echo '  build-all              — All of the above'
	@echo '  test-rust              — cargo test (Rust)'
	@echo '  test-flutter           — flutter test (widget tests)'
	@echo '  test-cli-android       — CLI sync test on emulator'
	@echo '  test-flutter-android   — Flutter sync test on emulator'
	@echo '  test-all               — All tests'
	@echo '  run                    — flutter run -d linux'
	@echo '  serve                  — Start serve on host (port 9847)'
	@echo '  serve-android          — Push + start serve on emulator'
	@echo '  codegen                — FRB codegen + re-patch loader'
	@echo '  clean                  — Remove build artifacts'
	@echo ''
	@echo 'Variables:'
	@echo '  TARGET=arm64-v8a       — Android ABI (default: x86_64-linux-android)'
