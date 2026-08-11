.PHONY: fmt fmt-check check build lint test verify

fmt:
	cargo fmt --all

fmt-check:
	cargo fmt --all -- --check

check:
	cargo check --workspace --all-targets

build:
	cargo build --workspace --all-targets

lint:
	cargo clippy --workspace --all-targets -- -D warnings

test:
	cargo test --workspace --all-targets

verify: fmt-check lint test

run:
	cargo run --bin agentcanpay