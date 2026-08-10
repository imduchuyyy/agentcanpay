use acp_keystore::Keystore;

use super::CommandError;
use crate::{cli::AddressArgs, output::Output};

/// Reads metadata only — never the credential store — so an agent calling
/// this can never be blocked on an unlock prompt.
pub fn run(args: &AddressArgs, out: &Output) -> Result<(), CommandError> {
    let meta = Keystore::open_default()?.load()?;

    let account = meta
        .account(&args.chain)
        .ok_or_else(|| CommandError::NoAccountForChain(args.chain.clone()))?;

    out.value("address", &account.address);
    Ok(())
}
