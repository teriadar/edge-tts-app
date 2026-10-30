RUST_DIR    := rust-core
ANDROID_DIR := android-app
JNILIBS_DIR := ../$(ANDROID_DIR)/app/src/main/jniLibs
APK_DEBUG   := $(ANDROID_DIR)/app/build/outputs/apk/debug/app-debug.apk

ifeq ($(OS),Windows_NT)
    GRADLEW := gradlew.bat
else
    GRADLEW := ./gradlew
endif

.PHONY: help test check clippy rust-android rust-arm64 apk-debug apk-release install clean

help:
	@echo "Pagina Reader Build Targets:"
	@echo "  make test          - Run Rust unit and integration tests"
	@echo "  make check         - Run cargo check on rust-core"
	@echo "  make clippy        - Run cargo clippy linter on rust-core"
	@echo "  make rust-android  - Cross-compile libpagina_core.so for all Android ABIs (arm64-v8a, armeabi-v7a, x86_64)"
	@echo "  make rust-arm64    - Cross-compile libpagina_core.so for arm64-v8a only (fast dev build)"
	@echo "  make apk-debug     - Build Rust native libs and assemble the Android debug APK"
	@echo "  make apk-release   - Build Rust native libs and assemble the Android release APK"
	@echo "  make install       - Build and install the debug APK onto a connected device via adb"
	@echo "  make clean         - Clean both Cargo and Gradle build artifacts"

test:
	cd $(RUST_DIR) && cargo test

check:
	cd $(RUST_DIR) && cargo check

clippy:
	cd $(RUST_DIR) && cargo clippy --all-targets

rust-android:
	cd $(RUST_DIR) && cargo ndk \
		-t arm64-v8a \
		-t armeabi-v7a \
		-t x86_64 \
		-o $(JNILIBS_DIR) \
		build --release

rust-arm64:
	cd $(RUST_DIR) && cargo ndk \
		-t arm64-v8a \
		-o $(JNILIBS_DIR) \
		build --release

apk-debug: rust-android
	cd $(ANDROID_DIR) && $(GRADLEW) assembleDebug

apk-release: rust-android
	cd $(ANDROID_DIR) && $(GRADLEW) assembleRelease

install: apk-debug
	adb install -r $(APK_DEBUG)

clean:
	cd $(RUST_DIR) && cargo clean
	cd $(ANDROID_DIR) && $(GRADLEW) clean
