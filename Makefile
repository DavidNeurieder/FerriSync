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
# Flutter packages its own copy of the Rust cdylib. A changing Dart define
# forces its build step to re-run on every invocation, so a Rust/Dart change
# cannot be masked by Flutter/Gradle considering the app up to date.
FORCE_REBUILD_DEFINE := FERRISYNC_BUILD_STAMP=$(shell date +%s%N)
# Generated FRB bindings must match the Rust API before any app is built. Use
# SKIP_CODEGEN=1 only when you know lib/gen and frb_generated.rs are current.
FRB_CODEGEN     ?= $(HOME)/.cargo/bin/flutter_rust_bridge_codegen
CODEGEN_TARGET  := $(if $(filter 1,$(SKIP_CODEGEN)),,codegen)
DEVICE_SERIAL   ?=
ADB             := adb$(if $(DEVICE_SERIAL), -s $(DEVICE_SERIAL))
ANDROID_PACKAGE := io.github.davidneurieder.ferrisync
ANDROID_APK     := $(FLUTTER_ROOT)/build/app/outputs/flutter-apk/app-debug.apk
# Gradle caches merged native libs; dropping these forces it to repackage the
# freshly copied .so files from jniLibs.
ANDROID_NATIVE_INTERMEDIATES := \
  $(FLUTTER_ROOT)/build/app/intermediates/merged_jni_libs/debug \
  $(FLUTTER_ROOT)/build/app/intermediates/merged_native_libs/debug \
  $(FLUTTER_ROOT)/build/app/intermediates/stripped_native_libs/debug
CLI_BIN         := target/debug/ferrisync
# Use the local venv when present so pytest/ruff/mypy/maturin tools are found.
PYTHON          ?= $(shell test -x .venv/bin/python && echo .venv/bin/python || echo python3)
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

.PHONY: all build-all build-linux-cli build-linux-core build-linux-flutter-release build-android-cli check-cargo-ndk
.PHONY: build-android-so build-android-so-universal build-android-apk build-android-apk-universal
.PHONY: test-rust test-flutter test-python test-python-unit test-python-lint test-python-integration test-python-interop test-python-reliability test-python-all typecheck-python
.PHONY: test-android-cli test-android-flutter test-all full-test
.PHONY: test-android-instrumented test-linux-flutter ensure-android-device
.PHONY: run-linux serve-linux run-cli serve-android codegen clean help install-phone
.PHONY: check-frb-codegen verify-android-device
.PHONY: fdroid-check release-all build-python develop-python

all: build-linux-cli

# ── Build ──

build-linux-cli:
	cargo build -p ferrisync

# The Linux Flutter app links libferrisync_core.so, not the CLI binary. Build
# the cdylib explicitly so `make run-linux` cannot launch against a stale .so
# when Flutter skips its CMake build step.
build-linux-core:
	cargo build -p ferrisync-core --lib

build-linux-flutter-release:
	cd $(FLUTTER_ROOT) && flutter build linux --release

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

build-android-apk: $(CODEGEN_TARGET) build-android-so
	rm -rf $(ANDROID_NATIVE_INTERMEDIATES)
	cd $(FLUTTER_ROOT) && flutter build apk --debug --dart-define=$(FORCE_REBUILD_DEFINE)

build-android-so-universal:
	@for target in $(ANDROID_TARGETS); do \
	  abi=$(call target_to_abi,$$target); \
	  $(MAKE) build-android-so TARGET=$$target; \
	done

build-android-apk-universal: $(CODEGEN_TARGET) build-android-so-universal
	rm -rf $(ANDROID_NATIVE_INTERMEDIATES)
	cd $(FLUTTER_ROOT) && flutter build apk --debug --dart-define=$(FORCE_REBUILD_DEFINE)

verify-android-device:
	@if [ -n "$(DEVICE_SERIAL)" ]; then \
	  $(ADB) get-state >/dev/null 2>&1 || { \
	    echo "error: device $(DEVICE_SERIAL) is not available" >&2; exit 1; }; \
	else \
	  count=$$($(ADB) devices | awk 'NR > 1 && $$2 == "device" {n++} END {print n+0}'); \
	  [ "$$count" -eq 1 ] || { \
	    echo "error: expected exactly one connected Android device, found $$count (set DEVICE_SERIAL=<serial>)" >&2; \
	    exit 1; }; \
	fi

install-phone: verify-android-device build-android-apk-universal
	$(ADB) install -r $(ANDROID_APK)
	@echo '==> Verifying the installed APK matches the one just built'
	@local_sha=$$(sha256sum "$(ANDROID_APK)" | awk '{print $$1}'); \
	  remote_path=$$($(ADB) shell pm path $(ANDROID_PACKAGE) | sed 's/^package://' | tr -d '\r' | head -n 1); \
	  [ -n "$$remote_path" ] || { echo 'error: installed APK not found on device' >&2; exit 1; }; \
	  tmp=$$(mktemp /tmp/ferrisync-installed.XXXXXX.apk); \
	  $(ADB) exec-out cat "$$remote_path" > "$$tmp"; \
	  remote_sha=$$(sha256sum "$$tmp" | awk '{print $$1}'); \
	  rm -f "$$tmp"; \
	  [ "$$local_sha" = "$$remote_sha" ] || { \
	    echo "error: installed APK differs from the one just built" >&2; exit 1; }; \
	  echo "Verified installed APK: $$local_sha"

build-all: build-linux-cli build-android-cli build-android-apk

# ── Test ──

test-rust:
	cargo test -p ferrisync-core

test-flutter:
	cd $(FLUTTER_ROOT) && flutter test

test-python: test-python-unit
	@$(PYTHON) -c "import ferrisync" >/dev/null 2>&1 && $(PYTHON) -m pytest ferrisync-python/tests -q -m "not interop and not reliability and not slow and not flutter" || echo 'Python extension not installed; ran static checks only. Build with: make develop-python'

test-python-unit: test-python-lint
	cargo check -p ferrisync-python
	$(PYTHON) -m py_compile \
	  ferrisync-python/python/ferrisync/__init__.py \
	  ferrisync-python/examples/basic_usage.py ferrisync-python/examples/async_usage.py ferrisync-python/examples/discovery.py ferrisync-python/examples/two_instances_sync.py \
	  ferrisync-python/tests/conftest.py \
	  ferrisync-python/tests/helpers/__init__.py ferrisync-python/tests/helpers/assertions.py ferrisync-python/tests/helpers/metrics.py ferrisync-python/tests/helpers/processes.py ferrisync-python/tests/helpers/scenarios.py \
	  ferrisync-python/tests/test_smoke.py ferrisync-python/tests/test_two_client_integration.py ferrisync-python/tests/test_lifecycle_and_multi_client.py ferrisync-python/tests/test_cli_interop.py ferrisync-python/tests/test_reliability_matrix.py ferrisync-python/tests/test_restart_recovery.py ferrisync-python/tests/test_cancellation.py ferrisync-python/tests/test_concurrency.py ferrisync-python/tests/test_watch_lifecycle.py ferrisync-python/tests/test_stress.py ferrisync-python/tests/test_flutter_interop.py ferrisync-python/tests/test_docs_examples.py ferrisync-python/tests/test_exceptions.py ferrisync-python/tests/test_folderless_pairing.py

test-python-lint:
	$(PYTHON) -m ruff check ferrisync-python/python ferrisync-python/tests ferrisync-python/examples

test-python-integration: test-python-unit
	@$(PYTHON) -c "import ferrisync" >/dev/null 2>&1 && $(PYTHON) -m pytest ferrisync-python/tests -q -m "not interop and not reliability and not slow and not flutter" || echo 'Python extension not installed; ran static checks only. Build with: make develop-python'

test-python-interop: test-python-unit
	@$(PYTHON) -c "import ferrisync" >/dev/null 2>&1 && $(PYTHON) -m pytest ferrisync-python/tests -q -m "interop and not flutter" || echo 'Python extension not installed; ran static checks only. Build with: make develop-python'

test-python-reliability: test-python-unit
	@$(PYTHON) -c "import ferrisync" >/dev/null 2>&1 && $(PYTHON) -m pytest ferrisync-python/tests -q -m "reliability" || echo 'Python extension not installed; ran static checks only. Build with: make develop-python'

test-python-all: test-python-unit
	@$(PYTHON) -c "import ferrisync" >/dev/null 2>&1 && $(PYTHON) -m pytest ferrisync-python/tests -q || echo 'Python extension not installed; ran static checks only. Build with: make develop-python'

# Ensure an Android device is online. If none is connected, this starts the
# configured AVD with a VISIBLE emulator window and waits for it to boot.
# The emulator is left running for the rest of the test run.
ensure-android-device:
	@ANDROID_DEVICE_SERIAL_FILE="$(ANDROID_DEVICE_SERIAL_FILE)" scripts/ensure_android_device.sh

test-android-cli: build-linux-cli ensure-android-device
	@export ANDROID_SERIAL="$$(cat "$(ANDROID_DEVICE_SERIAL_FILE)")"; scripts/test_android_cli_sync.sh

test-android-flutter: build-linux-cli ensure-android-device
	@export DEVICE_SERIAL="$$(cat "$(ANDROID_DEVICE_SERIAL_FILE)")"; scripts/test_android_flutter_sync.sh

build-python:
	cargo check -p ferrisync-python

develop-python:
	cd ferrisync-python && maturin develop

typecheck-python:
	$(PYTHON) -m mypy ferrisync-python/python ferrisync-python/tests

test-all: test-rust test-flutter test-python-all typecheck-python

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
full-test: test-rust test-flutter test-python-all typecheck-python test-linux-flutter test-android-instrumented test-android-cli test-android-flutter

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

# Builds the release assets that this Linux host can produce: the CLI, Linux
# Flutter desktop app bundle, and an arm64-only unsigned Android APK. macOS and
# Windows binaries cannot be built here (Apple SDK / MinGW toolchain), so they
# are intentionally skipped; use the release-assets workflow for those.
release-all: check-cargo-ndk
	@mkdir -p "$(RELEASE_DIR)"
	@find "$(RELEASE_DIR)" -maxdepth 1 -type f \
	  \( -name 'ferrisync-$(RELEASE_VERSION)-*' -o -name 'SHA256SUMS' \) -delete
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
	@echo '==> Building Linux Flutter desktop app (release)'
	cd $(FLUTTER_ROOT) && flutter build linux --release
	@test -d "$(FLUTTER_ROOT)/build/linux/x64/release/bundle" || { \
	  echo 'error: Linux Flutter release bundle not found at $(FLUTTER_ROOT)/build/linux/x64/release/bundle' >&2; \
	  exit 1; }
	tar -C "$(FLUTTER_ROOT)/build/linux/x64/release" -czf \
	  "$(RELEASE_DIR)/ferrisync-$(RELEASE_VERSION)-linux-flutter-x86_64.tar.gz" bundle
	@echo '==> Building arm64-only unsigned Android APK'
	cd $(FLUTTER_ROOT) && flutter build apk --release --target-platform android-arm64
	@if command -v unzip >/dev/null 2>&1; then \
	  apk="$(FLUTTER_ROOT)/build/app/outputs/flutter-apk/app-release.apk"; \
	  unzip -l "$$apk" | grep -q 'lib/arm64-v8a/libferrisync_core.so' || { \
	    echo 'error: release APK is missing lib/arm64-v8a/libferrisync_core.so' >&2; \
	    exit 1; \
	  }; \
	  unexpected=$$(unzip -Z1 "$$apk" | grep '^lib/' \
	    | sed 's#^lib/\([^/]*\)/.*#\1#' | sort -u | grep -v '^arm64-v8a$$' || true); \
	  [ -z "$$unexpected" ] || { \
	    echo "error: release APK contains unexpected ABIs: $$unexpected" >&2; \
	    exit 1; \
	  }; \
	fi
	cp $(FLUTTER_ROOT)/build/app/outputs/flutter-apk/app-release.apk \
	  "$(RELEASE_DIR)/ferrisync-$(RELEASE_VERSION)-android-arm64-unsigned.apk"
	@echo '==> Skipping macOS and Windows assets (cannot be built on this host)'
	@echo '    Build them with: gh workflow run release-assets.yml -f version=$(RELEASE_VERSION)'
	@echo '==> Generating SHA256SUMS'
	cd "$(RELEASE_DIR)" && sha256sum ferrisync-$(RELEASE_VERSION)-* > SHA256SUMS
	@echo '==> Release artifacts in $(RELEASE_DIR)'
	@ls -lh "$(RELEASE_DIR)"

# ── Run / Serve ──

run-linux: $(CODEGEN_TARGET) build-linux-cli build-linux-core
	cd $(FLUTTER_ROOT) && flutter run -d linux --dart-define=$(FORCE_REBUILD_DEFINE)

serve-linux: build-linux-cli
	@mkdir -p /tmp/ferrisync-serve-folder
	$(CLI_BIN) --data-dir /tmp/ferrisync-serve-data serve \
	  --port 9847 /tmp/ferrisync-serve-folder

run-cli: build-linux-cli
	$(CLI_BIN) $(ARGS)

serve-android: build-linux-cli build-android-cli
	adb push $(ANDROID_CLI) /data/local/tmp/ferrisync
	adb shell "mkdir -p /data/local/tmp/ferrisync-serve-folder /data/local/tmp/fsd"
	adb shell "nohup /data/local/tmp/ferrisync \
	  --data-dir /data/local/tmp/fsd serve \
	  --port 9847 /data/local/tmp/ferrisync-serve-folder &"

# ── Codegen ──

check-frb-codegen:
	@command -v "$(FRB_CODEGEN)" >/dev/null 2>&1 || { \
	  echo 'error: flutter_rust_bridge_codegen not found. Install it with:' >&2; \
	  echo '  cargo install flutter_rust_bridge_codegen --version 2.12.0 --locked' >&2; \
	  echo 'or set FRB_CODEGEN=<path>, or SKIP_CODEGEN=1 if bindings are already current.' >&2; \
	  exit 1; }

codegen: check-frb-codegen
	cd $(FLUTTER_ROOT) && "$(FRB_CODEGEN)" generate

# ── Clean ──

clean:
	cargo clean
	cd $(FLUTTER_ROOT) && flutter clean
	rm -rf $(FLUTTER_ROOT)/android/app/src/main/jniLibs

# ── Help ──

help:
	@echo 'Targets:'
	@echo '  build-linux-cli              — Build Linux CLI debug        (cargo build)'
	@echo '  build-linux-core             — Build the Rust cdylib used by the Flutter apps'
	@echo '  build-linux-flutter-release  — Build Linux Flutter desktop app (flutter build linux --release)'
	@echo '  build-android-cli            — Cross-compile CLI for Android (cargo build --target)'
	@echo '  build-android-so             — Build libferrisync_core.so (single ABI, uses TARGET)'
	@echo '  build-android-so-universal   — Build .so for all targets    (x86_64 + arm64)'
	@echo '  build-android-apk            — Build Flutter APK (single ABI, uses TARGET)'
	@echo '  build-android-apk-universal  — Build universal APK          (x86_64 + arm64)'
	@echo '  build-android-apk-x86_64     — Build for emulator (x86_64)'
	@echo '  build-android-apk-arm64      — Build for physical phone (arm64)'
	@echo '  install-phone                — Regenerate bindings, build universal APK, install + verify on device'
	@echo '  build-all                    — build-linux-cli + build-android-cli + build-android-apk'
	@echo '  test-rust                    — cargo test (Rust)'
	@echo '  test-flutter                 — flutter test (Linux desktop)'
	@echo '  test-python                  — Python static checks + fast tests'
	@echo '  test-python-lint             — Ruff check Python sources'
	@echo '  test-python-integration      — Python fast integration subset'
	@echo '  test-python-interop          — Python interop tests (CLI/Flutter)'
	@echo '  test-python-reliability      — Python reliability matrix'
	@echo '  test-python-all              — All Python tests (incl. Flutter interop)'
	@echo '  typecheck-python             — mypy on Python sources/tests'
	@echo '  test-android-cli             — CLI sync test (auto-detects device ABI)'
	@echo '  test-android-flutter         — Flutter integration tests: UI + FRB smoke + sync (universal APK)'
	@echo '  ensure-android-device        — Ensure a device is online; start visible emulator if needed'
	@echo '  test-android-instrumented    — Native notification tests on device (granted + revoked)'
	@echo '  test-linux-flutter           — Flutter integration tests on Linux desktop'
	@echo '  test-all                     — Rust + Flutter + Python tests and typecheck'
	@echo '  full-test                    — Run every test target (starts a visible emulator if Android device is absent)'
	@echo '  fdroid-check                 — Prereq checks + tests + clean unsigned release APK (F-Droid gate)'
	@echo '  release-all                  — Build Linux CLI + Linux Flutter app + arm64 Android APK into release/v<version>'
	@echo '  run-linux                    — flutter run -d linux'
	@echo '  serve-linux                  — Start serve on Linux host'
	@echo '  run-cli                      — Run the Rust CLI (ARGS="status")'
	@echo '  serve-android                — Push + start serve on device'
	@echo '  codegen                      — FRB codegen (regenerates lib/gen from ferrisync-core)'
	@echo '  clean                        — Remove build artifacts'
	@echo ''
	@echo 'Variables:'
	@echo '  PYTHON=<interpreter>          — Python for Python targets (.venv/bin/python when present)'
	@echo '  ANDROID_TARGETS=x86_64-linux-android aarch64-linux-android — targets for universal build'
	@echo '  TARGET=x86_64-linux-android       — single Rust target triple'
	@echo '  ANDROID_PLATFORM=24               — Android API level for cargo-ndk builds'
	@echo '  AVD_NAME=test_phone               — AVD started by Android test targets when no device is online'
	@echo '  DEVICE_SERIAL=<serial>            — target device for install-phone / Android tests (required with multiple devices)'
	@echo '  SKIP_CODEGEN=1                    — skip FRB regeneration when bindings are already current'
	@echo '  FRB_CODEGEN=<path>                — flutter_rust_bridge_codegen executable'
	@echo '  EMU_HEADLESS=1                    — use headless mode instead of the visible emulator window'
	@echo '  EMU_BOOT_TIMEOUT=240              — seconds to wait for the emulator to finish booting'
	@echo '  RELEASE_VERSION=0.1.0             — version folder (release/v<version>) and asset names'
	@echo '  RELEASE_DIR=<path>                — override the release output directory'
