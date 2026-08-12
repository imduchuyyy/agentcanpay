# Agent can pay
Your agent now can buy stock, crypto, invest and pay

## For agents

[`SKILL.md`](SKILL.md) is a drop-in skill: install it and your agent knows
how to fetch a release binary, set up the wallet, read balances and send
value, including what every flag and exit code means.

```bash
mkdir -p .claude/skills/agentcanpay
curl -fsSL https://raw.githubusercontent.com/imduchuyyy/agentcanpay/main/SKILL.md \
  -o .claude/skills/agentcanpay/SKILL.md
```

## Commands

| Command | Does |
|---|---|
| `create` | set up the wallet — the user picks new-or-import in a browser page |
| `address` | print the wallet address |
| `balance` | list holdings, across chains |
| `chains` | list supported chains |
| `transfer` | send tokens or native currency to another address |
| `reveal` | show the recovery phrase to the user, in a browser page |

The recovery phrase is only ever shown in the local browser page — never on
stdout, stderr, or the command line.

Binaries for Linux, macOS and Windows are attached to every
[release](https://github.com/imduchuyyy/agentcanpay/releases).
