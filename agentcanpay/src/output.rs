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

    /// Reports a newly stored wallet, optionally including the phrase.
    ///
    /// In plain mode stdout stays a bare address so `create` pipes exactly
    /// like `address`, and the phrase goes to stderr where a human reads it
    /// but `$(...)` does not capture it.
    pub fn secret_record(
        &self,
        accounts: &[DerivedAccount],
        phrase: Option<&str>,
        backend: Backend,
        source: Source,
    ) {
        let Some(primary) = accounts.first() else {
            return;
        };

        if self.json {
            let mut obj = serde_json::json!({
                "address": primary.address,
                "chain": primary.chain,
                "backend": backend.as_str(),
                "source": source.as_str(),
            });
            if let Some(p) = phrase {
                obj["phrase"] = serde_json::Value::String(p.to_owned());
            }
            println!("{obj}");
            return;
        }

        if let Some(p) = phrase {
            eprintln!(
                "\n  Write down this recovery phrase and store it offline.\n  \
                 It is the only copy. Anyone who has it controls this wallet,\n  \
                 and it will not be shown again.\n"
            );
            for (i, line) in phrase_lines(p).iter().enumerate() {
                eprintln!("    {:>2}. {line}", i * 4 + 1);
            }
            eprintln!();
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

/// Groups the phrase four words to a line so it can be transcribed by hand
/// without losing your place.
fn phrase_lines(phrase: &str) -> Vec<String> {
    phrase
        .split_whitespace()
        .collect::<Vec<_>>()
        .chunks(4)
        .map(|c| c.join(" "))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn groups_a_24_word_phrase_into_six_lines() {
        let phrase = "abandon ".repeat(23) + "art";
        let lines = phrase_lines(&phrase);
        assert_eq!(lines.len(), 6);
        assert_eq!(lines[0].split(' ').count(), 4);
        assert!(lines[5].ends_with("art"));
    }

    #[test]
    fn groups_a_12_word_phrase_into_three_lines() {
        let phrase = "abandon ".repeat(11) + "about";
        assert_eq!(phrase_lines(&phrase).len(), 3);
    }
}
