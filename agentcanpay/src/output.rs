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

    pub fn record(&self, json: &serde_json::Value, human: &str) {
        if self.json {
            println!("{json}");
        } else {
            println!("{human}");
        }
    }

    /// Progress goes to stderr so it never pollutes a parsed stdout.
    pub fn note(&self, msg: &str) {
        if !self.json {
            eprintln!("{msg}");
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
