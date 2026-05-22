.PHONY = build run clean test

build:
	@echo "Building Kagami..."
	@cargo build --release

run: build
	@echo "Running Kagami..."
	@sudo env "PATH=$$PATH" cargo run --bin compositor

test: build
	@echo "Testing Kagami..."
	@sudo env "PATH=$$PATH" cargo run --bin test-client

clean:
	@echo "Cleaning Kagami... (bytheway, why dont use cargo clean? lol)"
	@cargo clean