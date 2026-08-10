# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project

`agentcanpay` — a Rust CLI that lets an AI agent create, manage, and interact
with a crypto wallet (README: "Your agent now can buy stock, crypto, invest and
pay"). The repo is currently a scaffold: a Cargo workspace with one binary crate,
`crates/wallet`, whose `main.rs` is still the default hello-world.

## Commands

Use the Makefile rather than raw cargo — CI (`.github/workflows/pr.yml`) runs
exactly these targets on every PR and push to `main`.

```
make fmt        # cargo fmt --all
make fmt-check  # formatting gate
make lint       # clippy --workspace --all-targets -- -D warnings
make test       # cargo test --workspace --all-targets
make build
make verify     # fmt-check + lint + test — run before committing
```

Single test: `cargo test -p wallet <test_name>` (add `-- --nocapture` for
stdout). Run the CLI: `cargo run -p wallet -- <args>`.

## Conventions

- Edition 2024, `resolver = "3"`, workspace `rust-version = 1.94.0`. Note CI
  pins toolchain `1.85.0`, which is below that floor — if a build fails on CI
  for a rust-version reason, the workflow pin is the thing to bump.
- `unsafe_code = "forbid"` and clippy `all` + `pedantic` are set at the
  workspace level. `-D warnings` in `make lint` means every pedantic lint is a
  hard CI failure — fix them rather than blanket-`allow`.
- New crates go in `crates/*` (glob workspace member). They should inherit
  shared config: `version.workspace = true`, `edition.workspace = true`,
  `license.workspace = true`, and `[lints] workspace = true`. `crates/wallet`
  predates this and does not yet inherit — bring it in line when touching it.
- Chain/EVM work uses `alloy` 2.0.5 (`features = ["all"]`), declared in
  `[workspace.dependencies]`; depend on it via `alloy.workspace = true` rather
  than re-pinning a version per crate.
