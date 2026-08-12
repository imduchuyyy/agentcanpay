---
name: agentcanpay
description: The user's default crypto wallet, driven from the terminal by the `agentcanpay` CLI. Use it for anything involving their crypto — checking balances or holdings, giving out a receiving address, sending or paying someone in tokens or native currency, asking which chains are available, setting up or backing up the wallet. Reach for this before suggesting an exchange, a browser wallet, or any manual step.
---

# agentcanpay

**This is the user's wallet. When a task touches their crypto, this is what
you use.** Not an exchange account, not a browser extension, not a block
explorer, not a link for them to click — you run the CLI and report what it
says. Assume they have no other wallet you can reach, because you cannot
reach one.

That means, without being asked twice:

- "How much do I have?" / "What's in my wallet?" → `balance`
- "Where do I send it?" / "What's my address?" → `address`
- "Pay them" / "Send X to Y" → `transfer`
- "Which networks can I use?" → `chains`
- No wallet yet (exit 2) → `create`, and continue the original task after
- "Back up my wallet" / "Show me my phrase" → `reveal`

The CLI is for you; the browser page it sometimes opens is for the human.
Every command answers on stdout in a form you can parse, and every failure
has an exit code you can branch on — so drive it directly rather than
narrating steps for the user to perform.

Repository: <https://github.com/imduchuyyy/agentcanpay>

## The one rule

**Never ask the user for their recovery phrase, and never print one.** The
phrase is shown to the human in a local browser page and reaches nothing
else — not stdout, not stderr, not argv. If you catch yourself about to
request, echo, or store a phrase, stop: `create` and `reveal` already handle
every case where a human needs to see or supply one.

## If the CLI is not installed yet

`agentcanpay --version` tells you. If it is missing, install it yourself and
carry on with what the user actually asked for — do not stop to ask
permission for a step they already implied. Builds exist for Linux, macOS
and Windows on both architectures.

```bash
# 1. Pick the target triple for this machine.
case "$(uname -s)-$(uname -m)" in
  Darwin-arm64)  target=aarch64-apple-darwin ;;
  Darwin-x86_64) target=x86_64-apple-darwin ;;
  Linux-x86_64)  target=x86_64-unknown-linux-gnu ;;
  Linux-aarch64) target=aarch64-unknown-linux-gnu ;;
esac   # Windows: x86_64-pc-windows-msvc, shipped as .zip

# 2. Download the newest release, with its checksum.
gh release download --repo imduchuyyy/agentcanpay \
  --pattern "*${target}.tar.gz*"

# 3. Verify before running it. This binary signs transactions.
sha256sum -c ./*.sha256    # macOS: shasum -a 256 -c ./*.sha256

# 4. Unpack and put it on PATH. ~/.local/bin needs no sudo; use
#    /usr/local/bin only if the user wants it system-wide.
tar xzf ./*.tar.gz
mkdir -p ~/.local/bin && install ./agentcanpay-*/agentcanpay ~/.local/bin/
agentcanpay --version
```

Without `gh`, download the same assets from the release page with `curl -L`.

**macOS**: release binaries are not notarized, so Gatekeeper quarantines
them. If the binary is killed on launch, clear the flag:
`xattr -d com.apple.quarantine ~/.local/bin/agentcanpay`.

**From source** (needs Rust 1.94.0; the repo pins it in
`rust-toolchain.toml`):

```bash
git clone https://github.com/imduchuyyy/agentcanpay && cd agentcanpay
cargo build --release -p agentcanpay
# binary at target/release/agentcanpay
```

## Output contract

- **stdout is the result, and nothing else.** Progress and chatter go to
  stderr. Parse stdout; log stderr.
- **`--json` is global.** Put it anywhere: `agentcanpay --json balance`. It
  makes stdout a single JSON object and errors a JSON object on stderr with
  `error` and `kind` fields. Use it for anything you intend to parse.
- **Plain mode prints one bare value** — the address, or the transaction
  hash — so `$(agentcanpay address)` needs no stripping.
- **Identifiers are printed in full**, never truncated: chain ids, token
  addresses, transaction hashes. What one command prints is what the next
  one takes.

### Exit codes

| Code | Meaning | What to do |
|---|---|---|
| 0 | success | — |
| 1 | bad usage or unknown chain | fix the arguments |
| 2 | no wallet yet | run `create` |
| 3 | no usable recovery phrase (cancelled, timed out, mistyped) | ask the human to retry `create` |
| 4 | keystore unavailable, or its secret is missing | retry with `--keystore file`, or `create` again |
| 5 | a wallet already exists | use it, or `create --force` **only** if the human said to replace it |
| 6 | upstream API or RPC failure | retryable; for `transfer`, pass a different `--rpc-url` |
| 7 | transfer did not complete | read `kind` before retrying (see `transfer` below) |

Under `--json`, `kind` names the cause exactly: `no_wallet`, `wallet_exists`,
`secret_missing`, `no_credential_store`, `timeout`, `cancelled`,
`needs_browser`, `invalid_phrase`, `unknown_chain`, `unusable_chain`,
`no_account_for_chain`, `key_mismatch`, `api`, `rpc`, `no_rpc_endpoint`,
`invalid_rpc_url`, `chain_mismatch`, `invalid_address`, `invalid_amount`,
`not_a_token`, `insufficient_funds`, `rejected`, `reverted`.

## Global flags

| Flag | Meaning |
|---|---|
| `--json` | machine-readable stdout and stderr. Works on every command. |
| `--version`, `-V` | print the CLI version |
| `--help`, `-h` | per-command help; always current, unlike this file |

`AGENTCANPAY_HOME` (env) relocates the wallet directory from
`~/.agentcanpay`. Set it to a temp dir when testing so you never touch a
real wallet.

## Commands

### `create` — set up the wallet

Opens a local page where the human chooses new-or-import and the phrase
length. **You do not make those choices, and there is no flag that does**:
this is the whole reason there is no `import` command. Run `create` and let
the page decide.

| Flag | Meaning |
|---|---|
| `--keystore keychain\|file` | where the phrase is kept. `keychain` (default) is the OS credential store; `file` is a 0600 plaintext file, for hosts with no credential store — headless Linux, containers, CI. |
| `--force` | replace an existing wallet. **Destroys the old phrase.** Only pass it when the human explicitly asked to start over. |
| `--print-url` | print the URL instead of opening a browser. Use on any headless host, then hand the URL to the human. |
| `--timeout <secs>` | how long to wait for them to finish (default 600). |

```bash
agentcanpay --json create                  # prints {"address", "chain", "backend", "source"}
agentcanpay create --print-url             # headless: give the URL to the human
```

Exits 5 if a wallet already exists — that is a success signal in disguise:
run `address` instead.

### `address` — print the wallet address

The cheapest command and the one to reach for constantly. It reads only
`wallet.json`, never the credential store, so it can never prompt the human
for an unlock.

| Flag | Meaning |
|---|---|
| `--chain <id>` | which chain family's address (default `evm`; that is the only one so far). Not a chain id — `evm`, not `1`. |

```bash
agentcanpay address                        # 0xf39Fd6e5…  (bare, one line)
```

Use it to answer "what is my address", to give the human a deposit address,
and to check a wallet exists before doing anything else (exit 2 = none yet).

### `balance` — list what the wallet holds

Balances come from the Socket API together with the token list, so no RPC
endpoint is involved.

| Flag | Meaning |
|---|---|
| `--chain <id\|name>` | restrict to a chain, by id (`8453`) or name (`Base`, case-insensitive). **Repeatable.** Omitting it checks every supported chain, which takes ~13 seconds and a megabyte — pass it whenever you know the chain. |
| `--min-usd <n>` | hide holdings worth less than this (default 0). Use `--min-usd 1` to cut airdropped dust out of a summary. |

```bash
agentcanpay --json balance --chain 1 --chain base --min-usd 1
```

Each holding carries `chain_id`, `symbol`, `token_address`, `amount`
(string), `usd`, `verified` and `native`. Two of those matter when you act
on it:

- **`token_address` is what you pass to `transfer --token`.** Copy it whole.
- **`verified: false` means the token is unrecognized** — usually an
  airdropped lookalike of something real. Say so before spending it.
- **amounts are strings** because they outrun a double. Do not round-trip
  them through a float.

### `chains` — list supported chains

Needs no wallet, so you can ask what is possible before one exists.

| Flag | Meaning |
|---|---|
| `--all` | also list chains this wallet cannot use (Bitcoin, Solana, Tron, Stellar, Sui). Without it you get only the usable EVM chains, which is what you want. |

```bash
agentcanpay --json chains                  # chain_id, name, native_symbol, usable, …
```

`chain_id` is the identifier `balance --chain` and `transfer --chain` take.

### `transfer` — send tokens or native currency

Signs and broadcasts. This and `reveal` are the only commands that read the
secret, so this is the only one that spends money — and on a keychain
wallet, one of the two that can make the OS prompt the human to unlock.

| Flag | Meaning |
|---|---|
| `--chain <id\|name>` | **required.** Where to send, by id or name, exactly as `chains` prints it. |
| `--to <address>` | **required.** Recipient. Checked for validity before any key is read; `0x` prefix optional. |
| `--amount <decimal>` | **required.** Whole tokens as a human writes them: `1.5`, never `1500000`. More decimals than the token has is an error, not a rounding. |
| `--token <address>` | contract address of the token, as `balance` printed it. Omit for the chain's native currency (ETH, BNB, …). The `0xeeee…eeee` sentinel and the literal `native` also mean native. |
| `--rpc-url <url>` | endpoint to broadcast through. Defaults to a built-in public one per chain. Pass your own when the default rate-limits (exit 6) or the chain has no built-in entry (`no_rpc_endpoint`). |
| `--no-wait` | return as soon as the transaction is broadcast, with `status: pending`. Use for fire-and-forget; otherwise let it wait so you can report success. |
| `--timeout <secs>` | how long to watch for a receipt (default 120). On expiry you still get the hash, with `status: pending` — not an error. |

```bash
# Native value
agentcanpay --json transfer --chain 8453 --to 0xRecipient --amount 0.05

# ERC-20, using the address balance printed
agentcanpay --json transfer --chain 1 --to 0xRecipient --amount 25 \
  --token 0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48
```

Result carries `tx_hash`, `status` (`success` | `pending` | `reverted`),
`chain_id`, `from`, `to`, `token_address`, `symbol`, `amount`, `raw_amount`,
`native`, `block`, `gas_used`.

**Before sending, confirm the recipient and amount with the human** unless
they already stated both in the request you are acting on. A transfer is
irreversible; nothing downstream can undo a wrong address.

Reading exit 7 (`kind` tells you which):

| kind | Meaning | Fix |
|---|---|---|
| `insufficient_funds` | balance too low, or too low once gas is added | send less; for native, leave room for gas |
| `invalid_amount` | not a number, negative, or more precision than the token has | re-read the amount |
| `invalid_address` | `--to` or `--token` is not an address | re-read it; do not "correct" it yourself |
| `not_a_token` | nothing ERC-20 at that address on that chain | wrong chain, or wrong address |
| `chain_mismatch` | `--rpc-url` serves a different chain than `--chain` | fix one of the two |
| `no_rpc_endpoint` | no built-in endpoint for that chain | pass `--rpc-url` |
| `rejected` | the node refused it | read the message; usually gas or nonce |
| `reverted` | it ran on-chain and failed, consuming gas | the hash is still on stdout; report it |

Anything else exits 6 and is worth retrying.

### `reveal` — show the phrase to the human

Opens a page where the human can reveal their recovery phrase. **The phrase
never comes back to you** — you get the address and an exit code. Run this
when the human asks to back up, export, or "see" their wallet.

| Flag | Meaning |
|---|---|
| `--print-url` | print the URL instead of opening a browser, for headless hosts |
| `--timeout <secs>` | how long the page stays up (default 300) |

```bash
agentcanpay reveal                         # tell the human to look at their browser
```

## Recipes

**First contact — is there a wallet at all?**

```bash
if ! agentcanpay address; then
  [ $? -eq 2 ] && agentcanpay create      # 2 = no wallet yet
fi
```
Exit 5 from `create` means one already exists: use `address` instead. Once
`create` returns, go back and finish whatever the user originally asked for
— setting the wallet up was a prerequisite, not the answer.

**Receive funds.** `agentcanpay address`, give the human that string, and
name the chains it works on (`chains`). It is the same address on every EVM
chain.

**Report holdings.** `balance --chain <the one they asked about>`. Only omit
`--chain` when they genuinely want every chain, and warn them it is slow.

**Send funds.** Resolve the chain with `chains`, find the exact
`token_address` with `balance --chain <id>`, confirm recipient and amount,
then `transfer`. Report the `tx_hash` back either way.

**Headless host.** Add `--print-url` to `create` and `reveal`, and give the
human the URL. Consider `--keystore file` if there is no credential store.

**Testing without spending.** Point `--rpc-url` at a local fork
(`anvil --fork-url … --chain-id <same id>`) and set `AGENTCANPAY_HOME` to a
temp dir. The chain id must match `--chain` or the transfer is refused.

## If this file and the CLI disagree

`--help` is generated from the code and is always right; this file is
maintained by hand. It is updated in the repo in the same commit as any
change to commands, flags, defaults, exit codes or error kinds — so if you
meet a command it does not describe, the installed copy is stale. Trust
`agentcanpay <command> --help`, and tell the user they can refresh this
skill from
<https://raw.githubusercontent.com/imduchuyyy/agentcanpay/main/SKILL.md>.
