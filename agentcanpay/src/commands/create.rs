use acp_keystore::{Keystore, Source};
use acp_wallet::{chain, create_wallet};

use super::{CommandError, store};
use crate::{cli::CreateArgs, output::Output};

pub fn run(args: &CreateArgs, out: &Output) -> Result<(), CommandError> {
    let ks = Keystore::open_default()?;
    if ks.exists() && !args.force {
        return Err(acp_keystore::KeystoreError::WalletExists.into());
    }

    let wallet = create_wallet(args.words.into(), chain::SUPPORTED)?;
    store::persist(
        &ks,
        &wallet,
        Source::Generated,
        args.keystore.into(),
        args.force,
    )?;

    // A generated phrase has no other copy anywhere, so it is shown once,
    // here, and never again by any other command.
    out.secret_record(
        &wallet.accounts,
        Some(&wallet.phrase),
        args.keystore.into(),
        Source::Generated,
    );
    Ok(())
}
