use std::{fs, path::PathBuf};

use zeroize::Zeroizing;

use crate::{error::KeystoreError, metadata::Backend};

const SERVICE: &str = "agentcanpay";
const SECRET_FILE: &str = "wallet.key";

/// Where a wallet phrase lives at rest.
pub trait SecretStore {
    fn set(&self, id: &str, phrase: &str) -> Result<(), KeystoreError>;
    fn get(&self, id: &str) -> Result<Zeroizing<String>, KeystoreError>;
    fn delete(&self, id: &str) -> Result<(), KeystoreError>;
}

/// Platform credential store: Keychain, Credential Manager, Secret Service.
pub struct KeychainStore;

impl KeychainStore {
    fn entry(id: &str) -> Result<keyring::Entry, KeystoreError> {
        keyring::Entry::new(SERVICE, id).map_err(map_keyring)
    }
}

impl SecretStore for KeychainStore {
    fn set(&self, id: &str, phrase: &str) -> Result<(), KeystoreError> {
        Self::entry(id)?.set_password(phrase).map_err(map_keyring)
    }

    fn get(&self, id: &str) -> Result<Zeroizing<String>, KeystoreError> {
        Self::entry(id)?
            .get_password()
            .map(Zeroizing::new)
            .map_err(map_keyring)
    }

    fn delete(&self, id: &str) -> Result<(), KeystoreError> {
        Self::entry(id)?.delete_credential().map_err(map_keyring)
    }
}

fn map_keyring(e: keyring::Error) -> KeystoreError {
    match e {
        keyring::Error::NoEntry => KeystoreError::SecretMissing,
        keyring::Error::NoDefaultStore => KeystoreError::NoCredentialStore,
        other => KeystoreError::Keychain(other.to_string()),
    }
}

/// Plaintext fallback for hosts with no credential store.
///
/// Only ever selected explicitly, never as a silent downgrade from the
/// keychain — a wallet quietly landing in plaintext is exactly the kind of
/// thing nobody notices.
pub struct FileStore {
    dir: PathBuf,
}

impl FileStore {
    pub fn new(dir: PathBuf) -> Self {
        Self { dir }
    }

    fn path(&self) -> PathBuf {
        self.dir.join(SECRET_FILE)
    }
}

impl SecretStore for FileStore {
    fn set(&self, _id: &str, phrase: &str) -> Result<(), KeystoreError> {
        write_private(&self.path(), phrase.as_bytes())
    }

    fn get(&self, _id: &str) -> Result<Zeroizing<String>, KeystoreError> {
        match fs::read_to_string(self.path()) {
            Ok(s) => Ok(Zeroizing::new(s.trim().to_owned())),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Err(KeystoreError::SecretMissing),
            Err(e) => Err(KeystoreError::Io(e)),
        }
    }

    fn delete(&self, _id: &str) -> Result<(), KeystoreError> {
        match fs::remove_file(self.path()) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(KeystoreError::Io(e)),
        }
    }
}

pub fn store_for(backend: Backend, dir: PathBuf) -> Box<dyn SecretStore> {
    match backend {
        Backend::Keychain => Box::new(KeychainStore),
        Backend::File => Box::new(FileStore::new(dir)),
    }
}

/// Writes `bytes` to `path` atomically, never leaving a readable window.
///
/// The temp file is created 0600 before any content reaches it, so the
/// secret is never briefly world-readable; the rename is atomic so a crash
/// mid-write cannot truncate an existing wallet.
pub fn write_private(path: &std::path::Path, bytes: &[u8]) -> Result<(), KeystoreError> {
    use std::io::Write;

    let dir = path.parent().ok_or(KeystoreError::BadPath)?;
    fs::create_dir_all(dir)?;
    restrict_dir(dir)?;

    let tmp = path.with_extension("tmp");
    let mut opts = fs::OpenOptions::new();
    opts.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o600);
    }

    let mut f = opts.open(&tmp)?;
    f.write_all(bytes)?;
    f.sync_all()?;
    drop(f);

    fs::rename(&tmp, path)?;
    Ok(())
}

#[cfg(unix)]
fn restrict_dir(dir: &std::path::Path) -> Result<(), KeystoreError> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(dir, fs::Permissions::from_mode(0o700))?;
    Ok(())
}

#[cfg(not(unix))]
fn restrict_dir(_dir: &std::path::Path) -> Result<(), KeystoreError> {
    Ok(())
}
