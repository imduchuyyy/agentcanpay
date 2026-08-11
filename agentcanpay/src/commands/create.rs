use std::time::Duration;

use acp_connect::{
    ConnectOptions,
    setup::{SetupKind, SetupOutcome},
};
use acp_keystore::{Keystore, Source};
use acp_wallet::{chain, derive_from_phrase};

use super::{CommandError, store};
use crate::{cli::CreateArgs, output::Output};

/// Runs wallet setup in the user's browser.
///
/// The phrase is never returned to this process's stdout or stderr: the
/// caller is an agent, and anything printed here is read by it and lands in
/// its logs. The user sees the phrase in the browser or not at all.
pub async fn run(args: &CreateArgs, out: &Output) -> Result<(), CommandError> {
    let ks = Keystore::open_default()?;

    // Fail before opening a browser rather than after the user has copied
    // twenty-four words onto paper.
    if ks.exists() && !args.force {
        return Err(acp_keystore::KeystoreError::WalletExists.into());
    }

    out.note(
        "Opening your browser to set up the wallet.\n\
         Create a new recovery phrase or import one you already have.",
    );

    let SetupOutcome { phrase, kind } = acp_connect::setup::run(
        ConnectOptions {
            timeout: Duration::from_secs(args.timeout),
            open_browser: !args.print_url,
        },
        |url| {
            if args.print_url {
                out.value("url", url);
            } else {
                out.note(&format!("Waiting for {url}"));
            }
        },
    )
    .await?;

    let source = match kind {
        SetupKind::Generated => Source::Generated,
        SetupKind::Imported => Source::Imported,
    };

    let wallet = derive_from_phrase(phrase, chain::SUPPORTED)?;
    store::persist(&ks, &wallet, source, args.keystore.into(), args.force)?;

    out.wallet(&wallet.accounts, args.keystore.into(), source);
    Ok(())
}
