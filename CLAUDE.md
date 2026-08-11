# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project

`agentcanpay` — a Rust CLI that lets an AI agent hold and use a crypto wallet.
Commands: `create` (generate a new recovery phrase), `import` (adopt an
existing one), `address` (print the address).

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

Single test: `cargo test -p acp-wallet <test_name>` (add `-- --nocapture` for
stdout). Run the CLI: `cargo run -p agentcanpay -- <args>`.

Set `AGENTCANPAY_HOME` to a temp dir when exercising the CLI by hand, so you
never touch a real wallet at `~/.agentcanpay`.

## Layout

| Crate | Owns |
|---|---|
| `agentcanpay` | clap parsing, output rendering, exit codes |
| `crates/wallet` (`acp-wallet`) | phrase generation, BIP-39/44 derivation, `ChainAccount` seam |
| `crates/keystore` (`acp-keystore`) | secret backends, wallet metadata, atomic writes |
| `crates/connect` (`acp-connect`) | browser EIP-712 handshake — **not currently wired into the CLI** |

`acp-connect` and `acp-wallet::kdf` implement an alternative flow where the
wallet is derived from an external wallet's signature. They are complete and
tested but nothing in the CLI calls them; keep or delete as a unit.

## Invariants worth knowing before editing

- **`kdf::root_entropy` is consensus-critical.** Its HKDF salt, info string,
  low-s folding and exclusion of the recovery id all determine which wallet a
  signature maps to. `kdf::tests::entropy_is_stable` pins the output; if it
  fails, every wallet created by the old code has been orphaned. Change it
  only alongside a `KDF_ALG` version bump.
- **`address` must never touch the credential store.** It reads `wallet.json`
  only, which is what keeps the common agent path free of unlock prompts.
  Adding a secret read there would break that.
- **stdout is an API.** In plain mode every command prints the bare address
  and nothing else; human text goes to stderr. Under `--json` stdout is a
  single JSON object. Exit codes: 2 no wallet, 3 bad/absent phrase input,
  4 keystore unavailable, 5 wallet exists.
- **The phrase is only ever printed by `create`.** `import` does not echo it,
  and no command reads it back out. It is not accepted as a CLI argument
  anywhere, because argv is world-readable via `ps`.
- **Secrets are written 0600 before content reaches the file**, via
  `store::write_private` (temp file + atomic rename). Do not write secrets
  with plain `fs::write`.

## Conventions

- Edition 2024, toolchain pinned to 1.94.0 in `rust-toolchain.toml`; CI pins
  the same version separately in the workflow, so bump both together.
- `unsafe_code = "forbid"`, clippy `all` + `pedantic` at workspace level with
  `-D warnings` in CI. `missing_errors_doc` and `must_use_candidate` are
  allowed workspace-wide; everything else in pedantic must be fixed, not
  `allow`ed locally.
- New crates go in `crates/*` and inherit shared config: `version.workspace`,
  `edition.workspace`, `license.workspace`, `[lints] workspace = true`.
- Depend on `alloy` via `alloy.workspace = true`. Note `all` is **not** a
  valid alloy feature; the workspace enables an explicit list. Use alloy's
  re-exported `coins_bip39` rather than a direct dependency, since
  `alloy-signer-local` pins `^0.12` and two copies would not unify.

## Testing without a browser

`cargo run -p acp-connect --example fake_browser -- <url>` completes a connect
handshake with a throwaway key, for exercising the `acp-connect` flow.
