use acp_keystore::{Account, Backend, Keystore, METADATA_VERSION, Source, WalletMetadata};
use acp_wallet::DerivedWallet;
use time::OffsetDateTime;

use super::CommandError;

/// Shared tail of `create` and `import`: both differ only in where the
/// phrase came from.
pub fn persist(
    ks: &Keystore,
    wallet: &DerivedWallet,
    source: Source,
    backend: Backend,
    force: bool,
) -> Result<(), CommandError> {
    let primary = wallet
        .accounts
        .first()
        .ok_or_else(|| CommandError::NoAccountForChain("evm".into()))?;

    let meta = WalletMetadata {
        version: METADATA_VERSION,
        id: primary.address.to_lowercase(),
        backend,
        source,
        authorized_by: None,
        kdf: None,
        accounts: wallet
            .accounts
            .iter()
            .map(|a| Account {
                chain: a.chain.to_owned(),
                path: a.path.to_owned(),
                address: a.address.clone(),
            })
            .collect(),
        created_at: OffsetDateTime::now_utc(),
    };

    ks.save(&meta, &wallet.phrase, force)?;
    Ok(())
}
