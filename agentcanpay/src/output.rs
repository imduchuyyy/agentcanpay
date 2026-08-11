use acp_api::Chain;
use acp_keystore::{Backend, Source};
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
                        "token": h.address,
                        "amount": h.amount,
                        "usd": h.usd,
                        "verified": h.verified,
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

        println!(
            "{:<chain_width$}  {:<symbol_width$}  {:>amount_width$}  {:>14}",
            "CHAIN", "TOKEN", "AMOUNT", "USD"
        );
        for (h, amount) in holdings.iter().zip(&shown) {
            // An unverified token in a wallet is usually an airdropped
            // lookalike, so it is marked rather than silently trusted.
            let flag = if h.verified { "" } else { "  (unverified)" };
            println!(
                "{:<chain_width$}  {:<symbol_width$}  {:>amount_width$}  {:>14}{flag}",
                h.chain,
                h.symbol,
                amount,
                format!("${:.2}", h.usd),
            );
        }
        println!(
            "{:<chain_width$}  {:<symbol_width$}  {:>amount_width$}  {:>14}",
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
