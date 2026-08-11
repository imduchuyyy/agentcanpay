# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project

`agentcanpay` — a Rust CLI that lets an AI agent hold and use a crypto wallet.
Commands: `create` (set up the wallet), `address` (print the address),
`reveal` (show the recovery phrase to the user in a browser page),
`balance` (list holdings).

**The CLI is for the agent; the browser page is for the human.** The agent
cannot know whether the user wants a new wallet or an existing one, so it
never has to: it calls `create`, and the user chooses new-or-import, and the
phrase length, in the page. Any decision only a human can make belongs in the
page, never as a CLI flag or a second subcommand. This is why there is no
`import` command and no `--words`.

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
| `crates/connect` (`acp-connect`) | loopback browser flows: `setup` (used by `create`), `reveal`, `authorize` |
| `crates/api` (`acp-api`) | HTTP client for the Socket.tech API — chains, token lists, balances; swap and bridge land here |

`acp-connect::authorize` and `acp-wallet::kdf` implement an alternative flow
where the wallet is derived from an external wallet's signature. They are
complete and tested but nothing in the CLI calls them; keep or delete as a
unit. `acp-connect::setup` is what `create` runs.

## Invariants worth knowing before editing

- **`kdf::root_entropy` is consensus-critical.** Its HKDF salt, info string,
  low-s folding and exclusion of the recovery id all determine which wallet a
  signature maps to. `kdf::tests::entropy_is_stable` pins the output; if it
  fails, every wallet created by the old code has been orphaned. Change it
  only alongside a `KDF_ALG` version bump.
- **`address` must never touch the credential store.** It reads `wallet.json`
  only, which is what keeps the common agent path free of unlock prompts.
  Adding a secret read there would break that. `reveal` is the only command
  that reads the secret, and so the only one that can prompt.
- **`reveal` sends the phrase to the page only when the user asks.** The
  landing page has never seen it, and Hide re-renders without it rather than
  styling it out of view, so a page left open holds nothing.
- **stdout is an API.** stdout carries the command's result and nothing
  else; progress and human chatter go to stderr. Under `--json` stdout is a
  single JSON object. Exit codes: 2 no wallet, 3 bad/absent phrase input,
  4 keystore unavailable, 5 wallet exists, 6 upstream API failure.
- **Token amounts stay strings in JSON.** They routinely exceed what an IEEE
  double holds exactly; the table truncates for display, the JSON does not.
- **The recovery phrase must never reach stdout or stderr.** The caller is an
  AI agent that reads and logs this process's output, so the phrase is shown
  only in the browser. `Output::wallet` deliberately takes no phrase
  parameter — keep it that way, so printing one requires adding a code path
  rather than passing an argument. It is likewise never a CLI argument,
  because argv is world-readable via `ps`.
- **`create` is the single entry point for wallet setup.** Adding a flag or
  subcommand that presets the user's choice puts the agent back in the
  position of guessing.
- **Secrets are written 0600 before content reaches the file**, via
  `store::write_private` (temp file + atomic rename). Do not write secrets
  with plain `fs::write`.

## Browser UI

Screens are Askama templates in `crates/connect/templates/`, rendered
server-side and swapped in by htmx. There is no client-side model of the
flow, no bundler, and no npm dependency tree — deliberate, because these
pages display recovery phrases.

- **Add a screen** by adding a template plus a handler returning `frag(&T)`.
  Templates are compile-time checked, so a bad variable fails `cargo build`.
- **Validation errors re-render their own screen** with a message, returned
  with a 4xx status; htmx is configured to swap 4xx bodies. Keep the status
  honest rather than returning 200 to make the swap work.
- **Every state-changing route checks `state.authorized(&headers)`.** htmx
  sends the session token via `hx-headers` on the root element; a new route
  that forgets the check is reachable by any local process.
- **Assets are vendored and served from the binary** (`/htmx.js`, `/app.css`).
  Never reference a CDN: it would put a third party in front of a phrase.
  See `crates/connect/assets/VENDORED.md` for versions and digests.

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

## Talking to the API

`acp-api` wraps `https://public-backend.socket.tech`. No API key is needed.
Balances arrive with the token list (`userAddress` parameter), so there is
no RPC endpoint to configure and no on-chain call to make.

- Use `list=trending` for anything user-facing: `full` is ~50k tokens per
  chain against ~900 for trending.
- Listing all ~39 supported chains takes roughly 13 seconds and returns
  about a megabyte, which is why `balance` accepts `--chain`.
- The supported list includes non-EVM chains (Solana, Bitcoin, Tron), which
  an EVM wallet will simply hold nothing on.
- Decoding is deliberately lenient: unknown fields are ignored and nullable
  fields are `Option`, because upstream returns `null` for unranked or
  unpriced tokens and adds fields without warning.
- Tests decode recorded fixtures — never live calls. See
  `crates/api/tests/fixtures/README.md` to re-record.

## Testing without a browser

Both browser flows have headless drivers, so `create` can be exercised
end-to-end without a wallet extension:

```
cargo run -p acp-connect --example fake_setup   -- <url> new
cargo run -p acp-connect --example fake_setup   -- <url> import "<phrase>"
cargo run -p acp-connect --example fake_browser -- <url>   # authorize flow
```

Start the CLI with `--print-url` to get the URL to pass them.
