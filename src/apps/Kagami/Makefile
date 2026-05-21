.PHONY = build run test clean
.DEFAULT_GOAL := build

build:
	@echo "Building Kagami..."
	@cargo build --target x86_64-unknown-linux-gnu -Z build-std=

run: build
	@echo "Running Kagami..."
	@sudo ~/.cargo/bin/cargo run --bin=compositor --target x86_64-unknown-linux-gnu -Z build-std=std=

test:
	@echo "Running tests for Kagami..."
	@~/.cargo/bin/cargo run --bin=test-client --target x86_64-unknown-linux-gnu -Z build-std=std=