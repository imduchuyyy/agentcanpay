# agentcanpay
![build](https://img.shields.io/github/actions/workflow/status/imduchuyyy/agentcanpay/pr.yml?branch=main)
![License](https://img.shields.io/badge/license-MIT-blue)

`agentcanpay` is a Rust CLI that gives an AI agent a crypto wallet it can use on its own: one command to set it up, one to read what it holds, one to send value, and an output contract stable enough to parse.

You are not the one who runs it. Your agent runs it, reads stdout, and branches on the exit code, which is why every command answers with a single value or a single JSON object and keeps its chatter on stderr. The one thing the agent never gets is the recovery phrase.

## Overview
Six commands, and the identifiers one prints are the ones the next takes as input.

```shell
agentcanpay create     # Set up the wallet. The user picks new-or-import in a browser page.
agentcanpay address    # Print the wallet address. Reads no secret, so it can never prompt.
agentcanpay balance    # List what the wallet holds, across chains, with USD values.
agentcanpay chains     # List the chains this wallet can act on.
agentcanpay transfer   # Sign and broadcast a transfer of tokens or native currency.
agentcanpay reveal     # Show the recovery phrase to the user, in a browser page.
```

One BIP-39 phrase, one account at `m/44'/60'/0'/0/0`, so the same address on every EVM chain. Non-EVM chains are the honest limitation: the balance API happily lists Solana, Bitcoin, Tron, Stellar and Sui, and an Ethereum-style wallet can do nothing with any of them, so `chains` filters them out unless you ask for `--all`.

## The Agent and the Human
The split that shapes everything else: the CLI is for the agent, and the browser page is for the human.

An agent calling `create` cannot know whether the user wants a fresh wallet or one they already have, and it must not guess. So it never has to. `create` opens a page on loopback, the human chooses new-or-import there and picks their phrase length, and the CLI gets back an address. That is why there is no `import` command and no `--words` flag: a flag would put the agent back in the position of deciding something only the user can decide.

The pages are server-rendered Askama templates swapped in by htmx, with the assets vendored into the binary. No bundler, no npm tree, no CDN — a page that displays a recovery phrase is the last place to put a third-party script tag.

`reveal` works the same way and goes further: the phrase is sent to the page only when the user clicks to see it, and Hide re-renders the page without it rather than styling it out of view. A page left open on a screen holds nothing.

## Where the Phrase Lives
Two backends, chosen at `create` time and recorded in the wallet metadata so a later run cannot silently downgrade:

| | Stored in | Use when |
| :--- | :--- | :--- |
| **`--keystore keychain`** | OS credential store | there is a desktop session (the default) |
| **`--keystore file`** | `~/.agentcanpay/wallet.key`, mode 0600 | headless Linux, containers, CI |

Secrets are written through a temp file and an atomic rename, so the mode is 0600 before any content reaches the filesystem, never with a plain `fs::write` that would leave a window at the default umask.

The metadata and the secret are deliberately separate. `wallet.json` holds the address, the derivation path and the backend; the phrase lives in the credential store. `address` reads only the former, which is what keeps the command an agent calls constantly from ever raising an unlock prompt. Only `reveal` and `transfer` touch the secret, and both read it as late as they can, after every failure that does not need it has already happened.

The phrase is never a CLI argument either, because argv is world-readable through `ps`.

## Reading and Sending
Reading a wallet needs no chain access at all. Balances arrive from the Socket API together with the token list, so there is no RPC endpoint to configure and no on-chain call to make. Listing every supported chain takes about thirteen seconds and a megabyte, which is why `balance` takes `--chain`.

Sending is the part that has to reach a node, and `transfer` does several things before it signs anything:

| Check | Why |
| :--- | :--- |
| the endpoint's chain id matches `--chain` | a transaction built for one chain is a valid one to submit on another |
| decimals are read from the token itself | assuming 18 sends a thousand times too much of a 6-decimal stablecoin |
| the balance covers the amount | an ERC-20 overdraw reverts, costs gas, and explains nothing |
| the balance covers the amount **plus estimated gas**, for native value | spending native currency competes with the gas that spends it |

Amounts are the human kind — `1.5`, not `1500000` — and more precision than the token has is an error rather than a silent truncation. Negative amounts are rejected explicitly, because the underlying parser turns them into an enormous unsigned integer rather than a complaint.

Endpoints come from a hand-maintained table of public RPC URLs, one per chain, each verified with `eth_chainId` before it was added. They rate-limit, and there is nothing to be done about that except `--rpc-url`, which overrides any of them. A chain that is not in the table is not unsupported; it just needs a URL passed in.

## Getting Started
Grab a binary from the [releases](https://github.com/imduchuyyy/agentcanpay/releases). Every release carries Linux, macOS and Windows builds with a `.sha256` beside each one, which is worth checking for a program that signs transactions:

```shell
gh release download --repo imduchuyyy/agentcanpay --pattern '*aarch64-apple-darwin*'
shasum -a 256 -c ./*.sha256
tar xzf ./*.tar.gz && install ./agentcanpay-*/agentcanpay ~/.local/bin/
```

Then teach your agent to use it. [SKILL.md](SKILL.md) is a drop-in skill file — what each command and flag means, what every exit code means, and what to reach for on each kind of request:

```shell
mkdir -p .claude/skills/agentcanpay
curl -fsSL https://raw.githubusercontent.com/imduchuyyy/agentcanpay/main/SKILL.md \
  -o .claude/skills/agentcanpay/SKILL.md
```

From there a session looks like this. `create` opens your browser and waits:

```shell
$ agentcanpay create
Opening your browser to set up the wallet.
Create a new recovery phrase or import one you already have.
  address: 0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266
  stored:  keychain

0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266
```

The address on the last line is the only thing on stdout. Everything above it went to stderr, so `$(agentcanpay address)` needs no stripping. Send some USDC to that address on Base, and:

```shell
$ agentcanpay balance --chain base
Checking 1 chain(s) for 0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266…
CHAIN ID  CHAIN  TOKEN   AMOUNT           USD  TOKEN ADDRESS
    8453  Base   USDC        25        $25.00  0x833589fCD6eDb6E08f4c7C32D4f71b54bdA02913
    8453  Base   ETH      0.004         $9.85  0xeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee  (native)
          TOTAL                        $34.85

$ agentcanpay transfer --chain base --amount 5 \
    --token 0x833589fCD6eDb6E08f4c7C32D4f71b54bdA02913 \
    --to 0x70997970C51812dc3A010C7d01b50e0d17dc79C8
Sending 5 on Base via https://base-rpc.publicnode.com…
  sent:    5 USDC
  to:      0x70997970C51812dc3A010C7d01b50e0d17dc79C8
  chain:   Base (8453)
  status:  success

0x9b39581b040e0712277ab8cadea8f20c6d4bddbee206d938c3c6f2d75b41c008
```

Token addresses are printed in full, in both output modes, precisely so the second command can be built from the first. An abbreviated address looks usable and is not.

## Output and Exit Codes
Add `--json` anywhere and stdout becomes one object, with errors as `{"error", "kind"}` on stderr. That is the mode an agent should use for anything it parses. Amounts stay strings there, because token balances routinely exceed what a double holds exactly — the table truncates for display, the JSON never does.

The exit code says what happened without anyone parsing prose:

| Code | Meaning |
| :--- | :--- |
| **2** | no wallet yet |
| **3** | no usable phrase: cancelled, timed out, or mistyped |
| **4** | keystore unavailable, or its secret is missing |
| **5** | a wallet already exists |
| **6** | the API or the RPC endpoint did not cooperate — retryable |
| **7** | the transfer did not complete |

A reverted transfer is the interesting one: it prints its result object, hash included, and *then* exits 7. The money did not move but gas was spent, and the caller needs the hash to go and look.

## Testing
```shell
make verify
```

That is fmt, clippy with `-D warnings`, and the test suite — the same three things CI runs. Both browser flows have headless drivers, so `create` can be exercised end to end without a wallet extension:

```shell
cargo run -p acp-connect --example fake_setup -- <url> import "<phrase>"
```

Transfers are tested against a local fork, which is the only way to prove the sending path without spending anything:

```shell
anvil --fork-url https://ethereum-rpc.publicnode.com --chain-id 1
agentcanpay transfer --chain 1 --to 0x… --amount 1.5 --rpc-url http://127.0.0.1:8545
```

Keep anvil's `--chain-id` matching the chain you forked, or the transfer is refused — which is the chain-id guard doing its job.

## License
MIT. See [LICENSE](LICENSE).
