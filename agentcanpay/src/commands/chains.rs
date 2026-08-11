use acp_api::{Chain, Client};

use super::CommandError;
use crate::{cli::ChainsArgs, output::Output};

/// Lists the chains this CLI can work with.
///
/// Needs no wallet, so an agent can ask what is possible before there is
/// one. Non-EVM chains are filtered out by default: this wallet holds
/// Ethereum-style addresses, so reporting Bitcoin as supported would send
/// an agent down a path that cannot work.
pub async fn run(args: &ChainsArgs, out: &Output) -> Result<(), CommandError> {
    let mut chains = Client::new()?.supported_chains().await?;

    if !args.all {
        chains.retain(Chain::is_evm);
    }
    chains.sort_by(|a, b| a.name.cmp(&b.name));

    out.chains(&chains);
    Ok(())
}
