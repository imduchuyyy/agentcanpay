pub mod error;
pub mod metadata;
pub mod store;

pub use error::KeystoreError;
pub use metadata::{Account, Backend, Kdf, METADATA_VERSION, Source, WalletMetadata};
pub use store::{FileStore, KeychainStore, SecretStore, store_for};

use std::path::{Path, PathBuf};
use zeroize::Zeroizing;

const METADATA_FILE: &str = "wallet.json";
const HOME_ENV: &str = "AGENTCANPAY_HOME";

/// The `~/.agentcanpay` directory and the operations over it.
pub struct Keystore {
    dir: PathBuf,
}

impl Keystore {
    /// `$AGENTCANPAY_HOME`, else `$HOME/.agentcanpay`.
    pub fn open_default() -> Result<Self, KeystoreError> {
        let dir = if let Some(d) = std::env::var_os(HOME_ENV) {
            PathBuf::from(d)
        } else {
            let home = std::env::var_os("HOME").ok_or(KeystoreError::NoHome)?;
            PathBuf::from(home).join(".agentcanpay")
        };
        Ok(Self::with_dir(dir))
    }

    pub fn with_dir(dir: PathBuf) -> Self {
        Self { dir }
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    pub fn metadata_path(&self) -> PathBuf {
        self.dir.join(METADATA_FILE)
    }

    pub fn exists(&self) -> bool {
        self.metadata_path().exists()
    }

    /// Reads wallet metadata. Never touches the credential store, so this
    /// path cannot prompt the user.
    pub fn load(&self) -> Result<WalletMetadata, KeystoreError> {
        let raw = match std::fs::read_to_string(self.metadata_path()) {
            Ok(r) => r,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return Err(KeystoreError::NoWallet);
            }
            Err(e) => return Err(KeystoreError::Io(e)),
        };

        let meta: WalletMetadata = serde_json::from_str(&raw)?;
        if meta.version != METADATA_VERSION {
            return Err(KeystoreError::UnsupportedVersion(meta.version));
        }
        Ok(meta)
    }

    /// Persists secret first, then metadata.
    ///
    /// Order matters: metadata is what every other command keys off, so it
    /// must never exist while the secret behind it does not.
    pub fn save(
        &self,
        meta: &WalletMetadata,
        phrase: &str,
        force: bool,
    ) -> Result<(), KeystoreError> {
        if self.exists() && !force {
            return Err(KeystoreError::WalletExists);
        }

        std::fs::create_dir_all(&self.dir)?;
        store_for(meta.backend, self.dir.clone()).set(&meta.id, phrase)?;

        let json = serde_json::to_vec_pretty(meta)?;
        store::write_private(&self.metadata_path(), &json)?;
        Ok(())
    }

    /// Reads the phrase. This is the only path that can prompt for an
    /// unlock, so keep it off the read-only commands.
    pub fn phrase(&self, meta: &WalletMetadata) -> Result<Zeroizing<String>, KeystoreError> {
        store_for(meta.backend, self.dir.clone()).get(&meta.id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::OffsetDateTime;

    fn meta(backend: Backend) -> WalletMetadata {
        WalletMetadata {
            version: METADATA_VERSION,
            id: "0xabc".into(),
            backend,
            source: Source::Generated,
            authorized_by: None,
            kdf: None,
            accounts: vec![Account {
                chain: "evm".into(),
                path: "m/44'/60'/0'/0/0".into(),
                address: "0xabc".into(),
            }],
            created_at: OffsetDateTime::UNIX_EPOCH,
        }
    }

    #[test]
    fn round_trips_metadata_and_secret_via_file_backend() {
        let tmp = tempfile::tempdir().unwrap();
        let ks = Keystore::with_dir(tmp.path().to_owned());

        assert!(matches!(ks.load(), Err(KeystoreError::NoWallet)));

        let m = meta(Backend::File);
        ks.save(&m, "some phrase here", false).unwrap();

        let loaded = ks.load().unwrap();
        assert_eq!(loaded.id, "0xabc");
        assert_eq!(loaded.account("evm").unwrap().address, "0xabc");
        assert_eq!(*ks.phrase(&loaded).unwrap(), "some phrase here");
    }

    #[test]
    fn refuses_to_overwrite_without_force() {
        let tmp = tempfile::tempdir().unwrap();
        let ks = Keystore::with_dir(tmp.path().to_owned());
        let m = meta(Backend::File);

        ks.save(&m, "first", false).unwrap();
        assert!(matches!(
            ks.save(&m, "second", false),
            Err(KeystoreError::WalletExists)
        ));
        assert_eq!(*ks.phrase(&m).unwrap(), "first");

        ks.save(&m, "second", true).unwrap();
        assert_eq!(*ks.phrase(&m).unwrap(), "second");
    }

    #[cfg(unix)]
    #[test]
    fn secrets_are_not_group_or_world_readable() {
        use std::os::unix::fs::PermissionsExt;

        let tmp = tempfile::tempdir().unwrap();
        let ks = Keystore::with_dir(tmp.path().join("nested"));
        ks.save(&meta(Backend::File), "phrase", false).unwrap();

        for f in ["wallet.json", "wallet.key"] {
            let mode = std::fs::metadata(ks.dir().join(f))
                .unwrap()
                .permissions()
                .mode();
            assert_eq!(mode & 0o077, 0, "{f} is readable by others: {mode:o}");
        }
    }

    #[test]
    fn rejects_future_metadata_versions() {
        let tmp = tempfile::tempdir().unwrap();
        let ks = Keystore::with_dir(tmp.path().to_owned());
        let mut m = meta(Backend::File);
        m.version = 99;
        ks.save(&m, "p", false).unwrap();

        assert!(matches!(
            ks.load(),
            Err(KeystoreError::UnsupportedVersion(99))
        ));
    }
}
