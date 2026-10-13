//! SSH keys of SFTP connections (spec 050 research R4, R5): holzi generates an Ed25519 key whose
//! private half never leaves the process before it is saved (FR-008), reads the keys a user gives
//! (OpenSSH, PKCS#1, PKCS#8, PuTTY), and names host keys by their OpenSSH line and their
//! `SHA256:` fingerprint, as OpenSSH shows them.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

use russh::keys::ssh_key::private::Ed25519Keypair;
use russh::keys::ssh_key::LineEnding;
use russh::keys::{HashAlg, PrivateKey, PublicKey};
use tokio::time::Instant;
use uuid::Uuid;
use zeroize::Zeroizing;

/// The comment of a key holzi generates.
const COMMENT: &str = "holzi";

/// How long a generated key waits for its connection to be saved.
const PENDING_FOR: Duration = Duration::from_secs(30 * 60);

/// A key holzi could not generate: the system gave no randomness.
#[derive(Debug, thiserror::Error)]
#[error("no SSH key could be generated")]
pub struct KeyError;

/// A new Ed25519 key: the private key in OpenSSH form and the public key as one line
/// (`ssh-ed25519 AAAA… holzi`).
pub fn generate() -> Result<(Zeroizing<String>, String), KeyError> {
    let mut seed = Zeroizing::new([0u8; 32]);
    getrandom::fill(seed.as_mut()).map_err(|_| KeyError)?;
    let mut key = PrivateKey::from(Ed25519Keypair::from_seed(&seed));
    key.set_comment(COMMENT);
    let private = key.to_openssh(LineEnding::LF).map_err(|_| KeyError)?;
    let public = key.public_key().to_openssh().map_err(|_| KeyError)?;
    Ok((private, public))
}

/// Reads a private key the user gave, with its passphrase when it has one; `None` when it is no
/// key holzi can read or the passphrase does not open it.
pub fn decode(private_key: &str, passphrase: Option<&str>) -> Option<PrivateKey> {
    russh::keys::decode_secret_key(private_key, passphrase.filter(|p| !p.is_empty())).ok()
}

/// The OpenSSH line of a host key, without a comment (`ssh-ed25519 AAAA…`).
pub fn host_key_line(key: &PublicKey) -> String {
    let mut key = key.clone();
    key.set_comment("");
    key.to_openssh().unwrap_or_default().trim().to_owned()
}

/// The host key of an OpenSSH line; `None` for anything else.
pub fn parse_host_key(line: &str) -> Option<PublicKey> {
    PublicKey::from_openssh(line.trim()).ok()
}

/// The fingerprint of a host key line as OpenSSH shows it (`SHA256:…`).
pub fn fingerprint(line: &str) -> Option<String> {
    parse_host_key(line).map(|key| key.fingerprint(HashAlg::Sha256).to_string())
}

/// Keys holzi generated whose connection is not saved yet: by an id the window keeps, the
/// private key only here. A key goes when it is taken, after 30 minutes, and when the vault locks.
#[derive(Default)]
pub struct PendingKeys {
    keys: Mutex<HashMap<String, (Instant, Zeroizing<String>)>>,
}

impl PendingKeys {
    /// Generates a key; returns its id and its public key line.
    pub fn generate(&self) -> Result<(String, String), KeyError> {
        let (private, public) = generate()?;
        let id = Uuid::new_v4().to_string();
        let mut keys = self.keys.lock().expect("pending keys");
        keys.retain(|_, (at, _)| at.elapsed() < PENDING_FOR);
        keys.insert(id.clone(), (Instant::now(), private));
        Ok((id, public))
    }

    /// The private key of `id`, which stays in the store (a save may fail and be tried again);
    /// `None` when it is unknown or too old.
    pub fn get(&self, id: &str) -> Option<Zeroizing<String>> {
        let keys = self.keys.lock().expect("pending keys");
        let (at, key) = keys.get(id)?;
        (at.elapsed() < PENDING_FOR).then(|| key.clone())
    }

    /// The private key of `id`, which leaves the store; `None` when it is unknown or too old.
    pub fn take(&self, id: &str) -> Option<Zeroizing<String>> {
        let (at, key) = self.keys.lock().expect("pending keys").remove(id)?;
        (at.elapsed() < PENDING_FOR).then_some(key)
    }

    /// Drops every waiting key (the vault locks).
    pub fn clear(&self) {
        self.keys.lock().expect("pending keys").clear();
    }
}

/// The keys of this process that wait for their connection (one vault per process).
pub fn pending() -> &'static PendingKeys {
    static PENDING: OnceLock<PendingKeys> = OnceLock::new();
    PENDING.get_or_init(PendingKeys::default)
}
