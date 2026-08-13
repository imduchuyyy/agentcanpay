# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project

`agentcanpay` — a Rust CLI that lets an AI agent hold and use a crypto wallet.
Commands: `create` (set up the wallet), `address` (print the address),
`reveal` (show the recovery phrase to the user in a browser page),
`balance` (list holdings), `chains` (list supported chains),
`transfer` (send tokens or native currency to another address).

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

## SKILL.md is part of the CLI's surface

`SKILL.md` in the repo root is what other people install into their own
agents to teach them this CLI. **Update it in the same commit that changes
the surface it documents** — a new command, a new or renamed flag, a changed
default, a new exit code or error `kind`. It is the one doc that is wrong by
default: `--help` regenerates itself, `SKILL.md` does not.

It documents what an agent needs to *use the wallet*, which is not the whole
surface. `setup` is deliberately absent: it installs this file, `install.sh`
runs it, and an agent paying someone has no reason to call it — documenting
it would spend context on a command that never comes up. Its exit code and
`kind` are left out for the same reason. Keep that split when adding a
command: if an agent doing a wallet task would never run it, it belongs in
`--help` only.

The file is embedded in the binary with `include_str!`, so editing it
changes what `setup` installs. There is no separate copy to update.

## Releasing

Publishing a GitHub release triggers `.github/workflows/release.yml`, which
builds `agentcanpay` for Linux, macOS and Windows and attaches an archive
plus a `.sha256` to that release. Bump `version` in the workspace
`Cargo.toml` **before** tagging: the workflow refuses to build when the tag
and the workspace version disagree, so a binary can never report a version
its release does not. Re-run a failed target with the workflow's
`workflow_dispatch` input rather than re-cutting the release.

## Layout

| Crate | Owns |
|---|---|
| `agentcanpay` | clap parsing, output rendering, exit codes |
| `crates/wallet` (`acp-wallet`) | phrase generation, BIP-39/44 derivation, `ChainAccount` seam |
| `crates/keystore` (`acp-keystore`) | secret backends, wallet metadata, atomic writes |
| `crates/connect` (`acp-connect`) | loopback browser flows: `setup` (used by `create`), `reveal`, `authorize` |
| `crates/api` (`acp-api`) | HTTP client for the Socket.tech API — chains, token lists, balances; swap and bridge land here |
| `crates/tx` (`acp-tx`) | the only crate that talks to a chain: RPC endpoints, amount scaling, signing and broadcasting transfers |

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
  Adding a secret read there would break that. Only `reveal` and `transfer`
  read the secret, and so only they can prompt: `reveal` to show the phrase,
  `transfer` because signing needs the key. Both read it as late as possible,
  after every failure that does not need it has already happened.
- **`reveal` sends the phrase to the page only when the user asks.** The
  landing page has never seen it, and Hide re-renders without it rather than
  styling it out of view, so a page left open holds nothing.
- **stdout is an API.** stdout carries the command's result and nothing
  else; progress and human chatter go to stderr. Under `--json` stdout is a
  single JSON object. Exit codes: 2 no wallet, 3 bad/absent phrase input,
  4 keystore unavailable, 5 wallet exists, 6 upstream API or RPC failure,
  7 transfer not completed. `transfer` prints its result object before
  judging the status, so a reverted transaction still hands the caller its
  hash on stdout and then exits 7.
- **Token amounts stay strings in JSON.** They routinely exceed what an IEEE
  double holds exactly; the table truncates for display, the JSON does not.
- **Every listing prints the identifier a later command takes as input**, in
  full and in both output modes: `chain_id` for chains, the token address for
  balances. Never abbreviate an address for display — a truncated one looks
  usable and is not. Native value is the `0xeeee…eeee` sentinel
  (`acp_api::NATIVE_TOKEN_ADDRESS`), which is itself a valid swap input, and
  is flagged `native` so a caller knows it cannot be approved like an ERC-20.
- **The recovery phrase must never reach stdout or stderr.** The caller is an
  AI agent that reads and logs this process's output, so the phrase is shown
  only in the browser. `Output::wallet` deliberately takes no phrase
  parameter — keep it that way, so printing one requires adding a code path
  rather than passing an argument. It is likewise never a CLI argument,
  because argv is world-readable via `ps`.
- **Amounts are scaled by the token's own decimals, read on-chain.** Guessing
  18 would send a thousand times too much of a 6-decimal stablecoin, so a
  token whose `decimals()` cannot be read is an error rather than a default.
  Native decimals come from the chain listing for the same reason.
  `scale_amount` is stricter than alloy's `parse_units` on purpose: it
  rejects negatives (which convert into an enormous `U256`) and rejects more
  precision than the token has (which `parse_units` silently truncates).
- **`transfer` needs no browser step, and must not grow one.** Recipient,
  token and amount were all given to the agent by the user, so nothing is
  being guessed — the rule is that decisions the agent *cannot* know belong
  in the page, not that spending does.
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
- The supported list includes non-EVM chains (Solana, Bitcoin, Tron, Stellar,
  Sui), which an EVM wallet cannot use. The API exposes **nothing** to tell
  them apart — all but Bitcoin report the same `0xeeee…` sentinel as their
  native currency address — so `Chain::is_evm` consults a hand-maintained
  list in `chains.rs`. Update it when Socket adds a chain, or a new non-EVM
  one will be reported as usable.
- Decoding is deliberately lenient: unknown fields are ignored and nullable
  fields are `Option`, because upstream returns `null` for unranked or
  unpriced tokens and adds fields without warning.
- Tests decode recorded fixtures — never live calls. See
  `crates/api/tests/fixtures/README.md` to re-record.

## Talking to a chain

Reading needs no chain access; sending does. `acp-tx` is the only crate that
opens an RPC connection, and it is where the alloy provider/contract
features are for.

- **Endpoints are a hand-maintained table** in `tx/rpc.rs`, because Socket
  publishes routing data but no RPC URLs. Each entry was verified with
  `eth_chainId` before being added — do the same for a new one. They are
  public endpoints and will rate-limit, which is what `--rpc-url` is for. A
  chain absent from the table is not unsupported; it just needs a URL.
- **The chain id is checked against the endpoint before anything is signed.**
  A transaction built for one chain is a valid transaction to submit on
  another, so a wrong `--rpc-url` would otherwise spend real value on a
  network the caller never named.
- **Balance is checked before broadcasting**, and for native value the
  estimated gas is added to it. An ERC-20 overdraw reverts and costs gas
  while reporting nothing useful; native value competes with gas, so a
  balance that covers the amount alone is still not enough. The gas check is
  best-effort — a chain that prices gas unusually falls through to the
  node's own rejection rather than blocking the transfer.
- **A timeout waiting for a receipt is not a failure.** The transaction is
  already broadcast; the caller gets the hash with status `pending`, which
  is also what `--no-wait` returns.

## Testing without a browser

Both browser flows have headless drivers, so `create` can be exercised
end-to-end without a wallet extension:

```
cargo run -p acp-connect --example fake_setup   -- <url> new
cargo run -p acp-connect --example fake_setup   -- <url> import "<phrase>"
cargo run -p acp-connect --example fake_browser -- <url>   # authorize flow
```

Start the CLI with `--print-url` to get the URL to pass them.

## Testing transfers without spending anything

Fork a chain locally and point `--rpc-url` at it. Keep anvil's `--chain-id`
matching the forked chain, or the chain-id guard will reject the transfer —
which is the guard working:

```
anvil --fork-url https://ethereum-rpc.publicnode.com --chain-id 1
cargo run -p agentcanpay -- create --keystore file --print-url   # then
cargo run -p acp-connect --example fake_setup -- <url> import \
  "test test test test test test test test test test test junk"
cargo run -p agentcanpay -- transfer --chain 1 --to <addr> --amount 1.5 \
  --rpc-url http://127.0.0.1:8545
```

That phrase derives anvil's first prefunded account. For the ERC-20 path,
move tokens in from a whale with `cast rpc anvil_impersonateAccount`.
