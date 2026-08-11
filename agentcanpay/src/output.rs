use acp_keystore::{Backend, Source};
use acp_wallet::DerivedAccount;

use crate::commands::CommandError;

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
