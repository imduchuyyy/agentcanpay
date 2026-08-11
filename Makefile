.PHONY: fmt fmt-check check build lint test verify run clean-wallet

# Wallet data lives outside the repo, so `cargo clean` never touches it.
ACP_HOME ?= $(or $(AGENTCANPAY_HOME),$(HOME)/.agentcanpay)
ACP_SERVICE ?= agentcanpay

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

# Wipes every trace of a wallet so `create` can be exercised from scratch.
# Clears the credential store as well as the keystore directory: deleting
# the directory alone strands the phrase in the OS keychain, and repeated
# `create --force` runs leave an entry behind each time.
#
#   make clean-wallet            prompts before deleting
#   make clean-wallet YES=1      no prompt, for scripts
#   make clean-wallet ACP_HOME=/tmp/acp ACP_SERVICE=acp-test
clean-wallet:
	@echo "About to delete wallet data. Recovery phrases are NOT recoverable."; \
	echo "  keystore dir:     $(ACP_HOME)"; \
	echo "  credential store: service '$(ACP_SERVICE)' (all entries)"; \
	if [ "$(YES)" != "1" ]; then \
		printf "Type 'delete' to continue: "; \
		read -r reply; \
		[ "$$reply" = "delete" ] || { echo "aborted"; exit 1; }; \
	fi; \
	if command -v security >/dev/null 2>&1; then \
		while security find-generic-password -s "$(ACP_SERVICE)" >/dev/null 2>&1; do \
			security delete-generic-password -s "$(ACP_SERVICE)" >/dev/null 2>&1 || break; \
			echo "  removed a keychain entry"; \
		done; \
	elif command -v secret-tool >/dev/null 2>&1; then \
		secret-tool clear service "$(ACP_SERVICE)" 2>/dev/null || true; \
	else \
		echo "  no supported credential store CLI found; skipped"; \
	fi; \
	rm -rf "$(ACP_HOME)"; \
	echo "done"