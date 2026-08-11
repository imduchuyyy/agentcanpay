use std::io::{IsTerminal, Read};

use acp_keystore::{Keystore, Source};
use acp_wallet::{chain, derive_from_phrase, normalize_phrase};
use zeroize::Zeroizing;

use super::{CommandError, store};
use crate::{cli::ImportArgs, output::Output};

pub fn run(args: &ImportArgs, out: &Output) -> Result<(), CommandError> {
    let ks = Keystore::open_default()?;
    if ks.exists() && !args.force {
        return Err(acp_keystore::KeystoreError::WalletExists.into());
    }

    let phrase = read_phrase(out)?;
    let wallet = derive_from_phrase(phrase, chain::SUPPORTED)?;
    store::persist(
        &ks,
        &wallet,
        Source::Imported,
        args.keystore.into(),
        args.force,
    )?;

    out.wallet(&wallet.accounts, args.keystore.into(), Source::Imported);
    Ok(())
}

/// Reads the phrase from stdin only.
///
/// Deliberately not a command-line argument: argv is visible to every other
/// process on the machine via `ps`, and lands in shell history.
fn read_phrase(out: &Output) -> Result<Zeroizing<String>, CommandError> {
    let mut stdin = std::io::stdin();
    if stdin.is_terminal() {
        out.note("Paste your recovery phrase, then press Enter and Ctrl-D:");
    }

    let mut raw = Zeroizing::new(String::new());
    stdin
        .read_to_string(&mut raw)
        .map_err(CommandError::ReadPhrase)?;

    if raw.trim().is_empty() {
        return Err(CommandError::EmptyPhrase);
    }
    Ok(normalize_phrase(&raw))
}
