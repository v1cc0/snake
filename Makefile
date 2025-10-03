.PHONY: build clean install

# Get the actual target directory used by cargo
TARGET_DIR := $(shell cargo metadata --format-version 1 2>/dev/null | grep -o '"target_directory":"[^"]*"' | cut -d'"' -f4 | sed 's:/*$$::')
ifeq ($(TARGET_DIR),)
TARGET_DIR := target
endif

# Build release binary and copy to project root
build:
	cargo build --release
	cp $(TARGET_DIR)/release/snake ./snake
	@echo "✓ Binary built and copied to ./snake"

# Clean build artifacts
clean:
	cargo clean
	rm -f ./snake
	@echo "✓ Cleaned build artifacts"

# Install binary to system (requires sudo)
install: build
	sudo cp ./snake /usr/local/bin/snake
	@echo "✓ Installed to /usr/local/bin/snake"

# Run tests
test:
	cargo test

# Check code
check:
	cargo check
	cargo fmt --check
	cargo clippy --all-targets --all-features

# Format code
fmt:
	cargo fmt
