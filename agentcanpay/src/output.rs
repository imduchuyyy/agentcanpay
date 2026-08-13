use acp_api::{Chain, NATIVE_TOKEN_ADDRESS};
use acp_keystore::{Backend, Source};
use acp_tx::Sent;
use acp_wallet::DerivedAccount;

use crate::commands::{CommandError, balance::Holding};

/// Renders results for two very different audiences: a human reading a
/// terminal, and an agent parsing stdout.
pub struct Output {
    json: bool,
}

impl Output {
    pub fn new(json: bool) -> Self {
        Self { json }
    }

    /// The one value an agent is most likely to consume, so in plain mode it
    /// is printed bare and alone — no label, no decoration to strip.
    pub fn value(&self, key: &str, value: &str) {
        if self.json {
            println!("{}", serde_json::json!({ key: value }));
        } else {
            println!("{value}");
        }
    }

    /// Progress goes to stderr so it never pollutes a parsed stdout.
    pub fn note(&self, msg: &str) {
        if !self.json {
            eprintln!("{msg}");
        }
    }

    /// Reports a newly stored wallet.
    ///
    /// Takes no phrase, by construction. The recovery phrase is shown to the
    /// user in the browser and nowhere else; giving this function a way to
    /// accept one would put it a single call site away from an agent's log.
    pub fn wallet(&self, accounts: &[DerivedAccount], backend: Backend, source: Source) {
        let Some(primary) = accounts.first() else {
            return;
        };

        if self.json {
            println!(
                "{}",
                serde_json::json!({
                    "address": primary.address,
                    "chain": primary.chain,
                    "backend": backend.as_str(),
                    "source": source.as_str(),
                })
            );
            return;
        }

        eprintln!(
            "  address: {}\n  stored:  {}\n",
            primary.address,
            backend.as_str()
        );
        println!("{}", primary.address);
    }

    /// Renders holdings as a table for a human, or one JSON object for an
    /// agent. Amounts stay strings in JSON: they routinely exceed what an
    /// IEEE double represents exactly.
    pub fn balances(&self, address: &str, holdings: &[Holding], total_usd: f64) {
        if self.json {
            println!(
                "{}",
                serde_json::json!({
                    "address": address,
                    "total_usd": total_usd,
                    "holdings": holdings.iter().map(|h| serde_json::json!({
                        "chain_id": h.chain_id,
                        "chain": h.chain,
                        "symbol": h.symbol,
                        "token_address": h.address,
                        "amount": h.amount,
                        "usd": h.usd,
                        "verified": h.verified,
                        "native": h.native,
                    })).collect::<Vec<_>>(),
                })
            );
            return;
        }

        if holdings.is_empty() {
            eprintln!("  no balances found for {address}");
            return;
        }

        let shown: Vec<String> = holdings.iter().map(|h| short_amount(&h.amount)).collect();
        let chain_width = holdings
            .iter()
            .map(|h| h.chain.len())
            .max()
            .unwrap_or(5)
            .max(5);
        let symbol_width = holdings
            .iter()
            .map(|h| h.symbol.len())
            .max()
            .unwrap_or(6)
            .max(6);
        let amount_width = shown.iter().map(String::len).max().unwrap_or(6).max(6);

        // The chain id and the full token address are printed, not just the
        // human names, because they are what a later transfer or swap takes
        // as input. A truncated address would look usable and not be.
        println!(
            "{:>8}  {:<chain_width$}  {:<symbol_width$}  {:>amount_width$}  {:>12}  TOKEN ADDRESS",
            "CHAIN ID", "CHAIN", "TOKEN", "AMOUNT", "USD"
        );
        for (h, amount) in holdings.iter().zip(&shown) {
            // An unverified token in a wallet is usually an airdropped
            // lookalike, so it is marked rather than silently trusted.
            let mut flags = String::new();
            if h.native {
                flags.push_str("  (native)");
            }
            if !h.verified {
                flags.push_str("  (unverified)");
            }
            println!(
                "{:>8}  {:<chain_width$}  {:<symbol_width$}  {:>amount_width$}  {:>12}  {}{flags}",
                h.chain_id,
                h.chain,
                h.symbol,
                amount,
                format!("${:.2}", h.usd),
                h.address,
            );
        }
        println!(
            "{:>8}  {:<chain_width$}  {:<symbol_width$}  {:>amount_width$}  {:>12}",
            "",
            "TOTAL",
            "",
            "",
            format!("${total_usd:.2}")
        );
    }

    /// Lists supported chains. `usable` is carried explicitly so an agent
    /// asking with `--all` can still tell which it can act on.
    pub fn chains(&self, chains: &[Chain]) {
        if self.json {
            println!(
                "{}",
                serde_json::json!({
                    "chains": chains.iter().map(|c| serde_json::json!({
                        "chain_id": c.chain_id,
                        "name": c.name,
                        "native_symbol": c.currency.symbol,
                        "native_decimals": c.currency.decimals,
                        "native_token_address": c.currency.address,
                        "usable": c.is_evm(),
                        "sending_enabled": c.sending_enabled,
                        "receiving_enabled": c.receiving_enabled,
                    })).collect::<Vec<_>>(),
                })
            );
            return;
        }

        if chains.is_empty() {
            eprintln!("  no chains available");
            return;
        }

        let name_width = chains
            .iter()
            .map(|c| c.name.len())
            .max()
            .unwrap_or(4)
            .max(4);
        println!(
            "{:>12}  {:<name_width$}  {:<6}",
            "CHAIN ID", "NAME", "NATIVE"
        );
        for c in chains {
            // Only ever printed under --all; the default list is all usable.
            let flag = if c.is_evm() {
                ""
            } else {
                "  (not usable by this wallet)"
            };
            println!(
                "{:>12}  {:<name_width$}  {:<6}{flag}",
                c.chain_id, c.name, c.currency.symbol
            );
        }
    }

    /// Reports a broadcast transfer.
    ///
    /// The hash is what a later lookup takes as input, so in plain mode it
    /// is the only thing on stdout, exactly as `address` is. Native value
    /// is reported under the same `0xeeee…eeee` sentinel `balance` prints,
    /// so the two commands can be chained without translation.
    pub fn transfer(&self, sent: &Sent, chain: &str) {
        let token_address = sent
            .token
            .map_or_else(|| NATIVE_TOKEN_ADDRESS.to_owned(), |t| t.to_string());

        if self.json {
            println!(
                "{}",
                serde_json::json!({
                    "tx_hash": sent.hash,
                    "chain_id": sent.chain_id,
                    "chain": chain,
                    "status": sent.status.as_str(),
                    "from": sent.from.to_string(),
                    "to": sent.to.to_string(),
                    "token_address": token_address,
                    "symbol": sent.symbol,
                    "decimals": sent.decimals,
                    // Strings for the same reason balances are: an amount
                    // in the token's smallest unit outruns a double.
                    "amount": sent.amount,
                    "raw_amount": sent.raw_amount,
                    "native": sent.is_native(),
                    "block": sent.block,
                    "gas_used": sent.gas_used,
                })
            );
            return;
        }

        eprintln!(
            "  sent:    {} {}\n  to:      {}\n  chain:   {chain} ({})\n  status:  {}\n",
            sent.amount,
            sent.symbol,
            sent.to,
            sent.chain_id,
            sent.status.as_str(),
        );
        println!("{}", sent.hash);
    }

    /// Reports the outcome of an update check or install.
    ///
    /// `latest` is on stdout in plain mode — the version now installed, or
    /// the one available under `--check` — so a caller can compare it
    /// without parsing prose, exactly as `address` prints a bare address.
    pub fn update(&self, current: &str, latest: &str, updated: bool, path: &std::path::Path) {
        if self.json {
            println!(
                "{}",
                serde_json::json!({
                    "current": current,
                    "latest": latest,
                    "updated": updated,
                    "update_available": !updated && latest != current,
                    "path": path.display().to_string(),
                })
            );
            return;
        }

        if updated {
            eprintln!(
                "  updated {current} -> {latest}\n  path:    {}\n",
                path.display()
            );
        } else if latest == current {
            eprintln!("  agentcanpay {current} is the newest release\n");
        } else {
            eprintln!("  agentcanpay {current} installed; {latest} is available\n");
        }
        println!("{latest}");
    }

    pub fn error(&self, err: &CommandError) {
        if self.json {
            eprintln!(
                "{}",
                serde_json::json!({ "error": err.to_string(), "kind": err.kind() })
            );
        } else {
            eprintln!("error: {err}");
        }
    }
}

/// Shortens an amount for the table.
///
/// Only for display: the JSON output keeps the exact value, because a
/// balance is a quantity a caller may act on.
fn short_amount(amount: &str) -> String {
    const SHOWN_DECIMALS: usize = 6;
    match amount.split_once('.') {
        Some((whole, frac)) if frac.len() > SHOWN_DECIMALS => {
            let trimmed = frac[..SHOWN_DECIMALS].trim_end_matches('0');
            if trimmed.is_empty() {
                whole.to_owned()
            } else {
                format!("{whole}.{trimmed}")
            }
        }
        _ => amount.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::short_amount;

    #[test]
    fn shortens_long_fractions_without_touching_the_whole_part() {
        assert_eq!(short_amount("3.128598018118078032"), "3.128598");
        assert_eq!(short_amount("3000574.185146692290319902"), "3000574.185146");
    }

    #[test]
    fn leaves_short_amounts_alone() {
        assert_eq!(short_amount("1000"), "1000");
        assert_eq!(short_amount("2.5"), "2.5");
        assert_eq!(short_amount("0"), "0");
    }

    /// Holdings below display precision collapse to "0" in the table. The
    /// exact value is still in the JSON output, which is what a caller
    /// acting on the balance should read.
    #[test]
    fn amounts_below_display_precision_render_as_zero() {
        assert_eq!(short_amount("0.000001234567"), "0.000001");
        assert_eq!(short_amount("0.0000000001"), "0");
    }
}
