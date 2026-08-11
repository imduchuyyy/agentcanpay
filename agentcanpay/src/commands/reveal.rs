use std::time::Duration;

use acp_connect::ConnectOptions;
use acp_keystore::Keystore;

use super::CommandError;
use crate::{cli::RevealArgs, output::Output};

/// Shows the stored recovery phrase to the user, in their browser.
///
/// This is the only command that reads the credential store, so it is the
/// only one that can prompt for an unlock. The phrase goes to the browser
/// and nowhere else: the agent that invoked this sees an address and an
/// exit code.
pub async fn run(args: &RevealArgs, out: &Output) -> Result<(), CommandError> {
    let ks = Keystore::open_default()?;
    let meta = ks.load()?;

    let account = meta
        .account("evm")
        .ok_or_else(|| CommandError::NoAccountForChain("evm".into()))?;
    let address = account.address.clone();

    // Read the secret only once the wallet is known to be intact, so a
    // broken wallet fails before the user is asked to unlock anything.
    let phrase = ks.phrase(&meta)?;

    out.note("Opening your browser to show the recovery phrase.");

    acp_connect::reveal::run(
        address.clone(),
        phrase,
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

    out.value("address", &address);
    Ok(())
}
