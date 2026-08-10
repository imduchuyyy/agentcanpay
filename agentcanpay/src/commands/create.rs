use std::time::Duration;

use acp_connect::{ConnectOptions, typed_data};
use acp_keystore::{Account, Kdf, Keystore, METADATA_VERSION, WalletMetadata};
use acp_wallet::{chain, derive_from_signature};
use time::OffsetDateTime;

use super::CommandError;
use crate::{cli::CreateArgs, output::Output};

pub async fn run(args: &CreateArgs, out: &Output) -> Result<(), CommandError> {
    let ks = Keystore::open_default()?;

    // Fail before opening a browser rather than after the user has signed.
    if ks.exists() && !args.force {
        return Err(acp_keystore::KeystoreError::WalletExists.into());
    }

    out.note(&format!(
        "Open your browser and connect the wallet you want to authorise.\n\
         You will be asked to sign:\n\n  {}\n",
        typed_data::PURPOSE
    ));

    let handshake = acp_connect::run(
        ConnectOptions {
            index: args.index,
            timeout: Duration::from_secs(args.timeout),
            open_browser: !args.print_url,
        },
        |url| {
            if args.print_url {
                // stdout in plain mode: the user must be able to copy this.
                out.value("url", url);
            } else {
                out.note(&format!("Waiting for {url}"));
            }
        },
    )
    .await?;

    let derived = derive_from_signature(
        &handshake.signature,
        handshake.address,
        args.index,
        chain::SUPPORTED,
    )?;

    let primary = derived
        .accounts
        .first()
        .ok_or_else(|| CommandError::NoAccountForChain("evm".into()))?;

    let meta = WalletMetadata {
        version: METADATA_VERSION,
        id: primary.address.to_lowercase(),
        backend: args.keystore.into(),
        authorized_by: handshake.address.to_string(),
        kdf: Kdf {
            alg: acp_wallet::KDF_ALG.to_owned(),
            index: args.index,
        },
        accounts: derived
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

    ks.save(&meta, &derived.phrase, args.force)?;

    out.record(
        &serde_json::json!({
            "address": primary.address,
            "chain": primary.chain,
            "backend": meta.backend.as_str(),
            "authorized_by": meta.authorized_by,
            "index": args.index,
        }),
        &format!(
            "Wallet created.\n  address:    {}\n  authorised: {}\n  phrase:     stored in {}\n\
             \nRe-signing the same message with the same wallet and index \
             re-derives this wallet.",
            primary.address,
            meta.authorized_by,
            meta.backend.as_str(),
        ),
    );

    Ok(())
}
