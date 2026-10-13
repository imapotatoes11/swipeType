.PHONY: web-build web-serve mac-build mac-dmg mac-run mac-install mac-xcode rust-build version-sync version-bump release clean

# Paths
VERSION_FILE := VERSION
BUILD_NUMBER_FILE := BUILD_NUMBER
MAC_APP_DIR := apps/mac
RUST_SRC := crates/swipe-engine
RUST_TARGETS := aarch64-apple-darwin x86_64-apple-darwin
RUST_LIB_NAME := libswipe_engine.a
MACOS_DEPLOYMENT_TARGET := 13.0
# Code signing identity. The default ad-hoc signature changes on every build,
# which makes macOS forget the Accessibility grant. Pass a stable identity to
# keep it, e.g. make mac-build SIGN_IDENTITY="Apple Development" DEVELOPMENT_TEAM=ABCDE12345
SIGN_IDENTITY ?= -
DEVELOPMENT_TEAM ?=

# --- Web ---
web-build:
	cd apps/web && wasm-pack build --target web
	mkdir -p build
	cp apps/web/pkg/swipe_web.js apps/web/pkg/swipe_web_bg.wasm build/
	cp apps/web/www/index.html build/

web-serve: web-build
	cd build && python3 -m http.server 8000

# --- Versioning ---
version-sync:
	./scripts/sync_version.sh

version-bump:
	@if [ -f $(BUILD_NUMBER_FILE) ]; then \
		BUILD=$$(cat $(BUILD_NUMBER_FILE) | tr -d '[:space:]'); \
		NEW_BUILD=$$(($$BUILD + 1)); \
		echo $$NEW_BUILD > $(BUILD_NUMBER_FILE); \
		echo "Bumped build number to $$NEW_BUILD"; \
	else \
		echo "1" > $(BUILD_NUMBER_FILE); \
		echo "Created BUILD_NUMBER file with 1"; \
	fi

# --- Rust ---
rust-build:
	@if command -v rustup >/dev/null 2>&1; then rustup target add $(RUST_TARGETS); fi
	cd $(RUST_SRC) && for target in $(RUST_TARGETS); do \
		MACOSX_DEPLOYMENT_TARGET=$(MACOS_DEPLOYMENT_TARGET) cargo build --release --features ffi --target $$target || exit 1; \
	done
	mkdir -p $(MAC_APP_DIR)/Rust
	lipo -create $(foreach target,$(RUST_TARGETS),target/$(target)/release/$(RUST_LIB_NAME)) \
		-output $(MAC_APP_DIR)/Rust/$(RUST_LIB_NAME)

# --- Mac ---
mac-xcode: version-sync
	cd $(MAC_APP_DIR) && xcodegen generate

mac-build: version-bump mac-xcode rust-build
	cd $(MAC_APP_DIR) && xcodebuild -project SwipeTypeMac.xcodeproj -scheme SwipeTypeMac -configuration Release -derivedDataPath BuildData \
		CODE_SIGN_IDENTITY="$(SIGN_IDENTITY)" $(if $(DEVELOPMENT_TEAM),DEVELOPMENT_TEAM=$(DEVELOPMENT_TEAM)) build
	rm -rf $(MAC_APP_DIR)/build/Release/SwipeType.app
	mkdir -p $(MAC_APP_DIR)/build/Release
	cp -R $(MAC_APP_DIR)/BuildData/Build/Products/Release/SwipeType.app $(MAC_APP_DIR)/build/Release/

# Launch through LaunchServices: running the binary from a shell makes the
# terminal the "responsible" app, so macOS checks the terminal's permissions
# instead of SwipeType's and the hotkey never works.
mac-run:
	-pkill -x SwipeType
	open -n $(MAC_APP_DIR)/build/Release/SwipeType.app

mac-install: mac-build
	rm -rf /Applications/SwipeType.app
	cp -R $(MAC_APP_DIR)/build/Release/SwipeType.app /Applications/

mac-dmg: mac-build
	@echo "Packaging into DMG..."
	mkdir -p $(MAC_APP_DIR)/build/dmg
	rm -rf $(MAC_APP_DIR)/build/dmg/*
	cp -R $(MAC_APP_DIR)/build/Release/SwipeType.app $(MAC_APP_DIR)/build/dmg/
	ln -s /Applications $(MAC_APP_DIR)/build/dmg/Applications
	hdiutil create -volname "SwipeType" -srcfolder $(MAC_APP_DIR)/build/dmg -ov -format UDZO $(MAC_APP_DIR)/build/SwipeType.dmg
	@echo "Created $(MAC_APP_DIR)/build/SwipeType.dmg"

# --- Release ---
release:
	@./scripts/release.sh

# --- Cleanup ---
clean:
	rm -rf build
	rm -rf $(MAC_APP_DIR)/build
	rm -rf $(MAC_APP_DIR)/BuildData
	rm -rf $(MAC_APP_DIR)/Rust/*.a
	rm -rf $(MAC_APP_DIR)/SwipeTypeMac.xcodeproj
	cd apps/web && rm -rf pkg target
	cd $(RUST_SRC) && cargo clean
