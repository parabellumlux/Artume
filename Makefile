.PHONY: all build test clean run-core run-cli-search

all: build test

build:
	cargo build

test:
	cargo test --workspace

clean:
	cargo clean
	rm -f /tmp/aetherfs.sock

run-core:
	cargo run --bin aetherfs-core

run-cli-search:
	cargo run --bin aetherfs-cli -- search "transcribed"
