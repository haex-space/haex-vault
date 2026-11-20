//! The keys of the own-device sync (spec 024, research R2, R3).
//!
//! - **Vault identity**: one secp256k1 key pair per vault. Its public key is
//!   the synchronized `vault_identity.pubkey`; the secret key lives only on
//!   main devices in `vault_identity_secret_no_sync`.
//! - **Device keys**: per installation, a secp256k1 device key and an ed25519
//!   iroh endpoint key in `device_keys_no_sync`. haex-vault reads only the row of
//!   its own installation: a copied vault keeps the source device's row
//!   untouched and never uses it (FR-006).
//!
//! Secret bytes stay in [`Zeroizing`] buffers and never appear in `Debug`.

use std::fmt;

use haex_crdt::rusqlite::params;
use haex_crdt::CrdtTransaction;
use hkdf::Hkdf;
use secp256k1::SecretKey;
use sha2::Sha256;
use uuid::Uuid;
use zeroize::Zeroizing;

use crate::storage::query::Query;
use crate::sync::signing::xonly_public_key;

/// Fresh bytes from the operating system's CSPRNG.
pub fn random_bytes<const N: usize>() -> Zeroizing<[u8; N]> {
    let mut bytes = Zeroizing::new([0u8; N]);
    getrandom::fill(bytes.as_mut()).expect("the operating system's random source is available");
    bytes
}

/// A fresh secp256k1 secret key.
pub fn random_secret_key() -> Zeroizing<[u8; 32]> {
    loop {
        let bytes = random_bytes::<32>();
        if SecretKey::from_secret_bytes(*bytes).is_ok() {
            return bytes;
        }
    }
}

/// Derives the vault identity from a seed (research R3):
/// `HKDF-SHA256(ikm = seed, salt = "haex", info = "haex/vault-identity/v1" ‖ counter)`,
/// counting up until the output is a valid secp256k1 scalar. Every copy of a
/// vault computes the same identity from the same seed.
pub fn derive_vault_identity(seed: &[u8; 32]) -> Zeroizing<[u8; 32]> {
    let hkdf = Hkdf::<Sha256>::new(Some(b"haex"), seed);
    for counter in 0u32.. {
        let mut info = b"haex/vault-identity/v1".to_vec();
        info.extend_from_slice(&counter.to_be_bytes());
        let mut okm = Zeroizing::new([0u8; 32]);
        hkdf.expand(&info, okm.as_mut())
            .expect("32 bytes are a valid HKDF-SHA256 output length");
        if SecretKey::from_secret_bytes(*okm).is_ok() {
            return okm;
        }
    }
    unreachable!("a valid scalar turns up long before the counter runs out")
}

/// The keys of this installation in this vault.
#[derive(Clone)]
pub struct DeviceKeys {
    pub device_secret: Zeroizing<[u8; 32]>,
    pub device_pubkey: [u8; 32],
    pub endpoint_secret: Zeroizing<[u8; 32]>,
    pub endpoint_id: [u8; 32],
}

impl fmt::Debug for DeviceKeys {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DeviceKeys")
            .field("device_pubkey", &hex(&self.device_pubkey))
            .field("endpoint_id", &hex(&self.endpoint_id))
            .finish_non_exhaustive()
    }
}

impl DeviceKeys {
    /// Fresh device and endpoint keys.
    pub fn generate() -> Self {
        let device_secret = random_secret_key();
        let device_pubkey =
            xonly_public_key(&device_secret).expect("a freshly drawn scalar is a secret key");
        let endpoint_secret = random_bytes::<32>();
        let endpoint_id = *iroh::SecretKey::from_bytes(&endpoint_secret)
            .public()
            .as_bytes();
        Self {
            device_secret,
            device_pubkey,
            endpoint_secret,
            endpoint_id,
        }
    }
}

/// This installation's keys, or `None` if it has none in this vault yet.
pub fn load_device_keys(
    q: &mut impl Query,
    installation_uuid: Uuid,
) -> haex_crdt::Result<Option<DeviceKeys>> {
    let row = q.query_row(
        "SELECT device_secret, device_pubkey, endpoint_secret, endpoint_id \
         FROM device_keys_no_sync WHERE installation_uuid = ?1",
        params![installation_uuid.to_string()],
        |r| {
            Ok((
                r.get::<_, Vec<u8>>(0)?,
                r.get::<_, Vec<u8>>(1)?,
                r.get::<_, Vec<u8>>(2)?,
                r.get::<_, Vec<u8>>(3)?,
            ))
        },
    )?;
    let Some((device_secret, device_pubkey, endpoint_secret, endpoint_id)) = row else {
        return Ok(None);
    };
    let device_secret = Zeroizing::new(device_secret);
    let endpoint_secret = Zeroizing::new(endpoint_secret);
    Ok(Some(DeviceKeys {
        device_secret: Zeroizing::new(array(&device_secret, "device_secret")?),
        device_pubkey: array(&device_pubkey, "device_pubkey")?,
        endpoint_secret: Zeroizing::new(array(&endpoint_secret, "endpoint_secret")?),
        endpoint_id: array(&endpoint_id, "endpoint_id")?,
    }))
}

/// This installation's keys, created on first use (FR-003). Rows of other
/// installations are never read or changed (FR-006).
pub fn ensure_device_keys(
    tx: &mut CrdtTransaction<'_>,
    installation_uuid: Uuid,
    now_ms: i64,
) -> haex_crdt::Result<DeviceKeys> {
    if let Some(keys) = load_device_keys(tx, installation_uuid)? {
        return Ok(keys);
    }
    let keys = DeviceKeys::generate();
    tx.execute(
        "INSERT INTO device_keys_no_sync \
           (installation_uuid, device_secret, device_pubkey, endpoint_secret, endpoint_id, \
            created_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![
            installation_uuid.to_string(),
            keys.device_secret.as_slice(),
            keys.device_pubkey.as_slice(),
            keys.endpoint_secret.as_slice(),
            keys.endpoint_id.as_slice(),
            now_ms,
        ],
    )?;
    Ok(keys)
}

/// The public key of the vault identity, once it is published.
pub fn vault_pubkey(q: &mut impl Query) -> haex_crdt::Result<Option<[u8; 32]>> {
    let raw: Option<Vec<u8>> =
        q.query_row("SELECT pubkey FROM vault_identity WHERE id = 1", &[], |r| {
            r.get(0)
        })?;
    raw.map(|bytes| array(&bytes, "vault_identity.pubkey"))
        .transpose()
}

/// The secret key of the vault identity; present exactly on main devices.
pub fn vault_secret(q: &mut impl Query) -> haex_crdt::Result<Option<Zeroizing<[u8; 32]>>> {
    let raw: Option<Vec<u8>> = q.query_row(
        "SELECT privkey FROM vault_identity_secret_no_sync WHERE id = 1",
        &[],
        |r| r.get(0),
    )?;
    raw.map(|bytes| {
        let bytes = Zeroizing::new(bytes);
        array(&bytes, "vault_identity_secret_no_sync.privkey").map(Zeroizing::new)
    })
    .transpose()
}

/// Stores the private key of the vault identity on a device that just became
/// a main device by linking (FR-024, FR-038). A device that already holds one
/// keeps it.
pub fn store_vault_secret(
    tx: &mut CrdtTransaction<'_>,
    secret: &[u8; 32],
) -> haex_crdt::Result<()> {
    tx.execute(
        "INSERT OR IGNORE INTO vault_identity_secret_no_sync (id, privkey) VALUES (1, ?1)",
        params![secret.as_slice()],
    )?;
    Ok(())
}

/// Publishes the vault identity if it is not published yet, and returns its
/// public key.
///
/// A vault that is not published yet carries a seed in
/// `vault_identity_secret_no_sync`: the placeholder of a vault from before spec
/// 024 (migration 0021 moved it there) or, with `allow_genesis`, a fresh one for
/// a new vault. The seed is replaced by the identity derived from it (R3) and the
/// public key is written in the same transaction, so a crash cannot leave a
/// derived secret next to an unpublished identity. `None` when there is neither
/// an identity nor a seed and `allow_genesis` is off.
pub fn ensure_vault_identity(
    tx: &mut CrdtTransaction<'_>,
    allow_genesis: bool,
) -> haex_crdt::Result<Option<[u8; 32]>> {
    if let Some(pubkey) = vault_pubkey(tx)? {
        return Ok(Some(pubkey));
    }
    let seed = match vault_secret(tx)? {
        Some(seed) => seed,
        None if allow_genesis => random_secret_key(),
        None => return Ok(None),
    };
    let secret = derive_vault_identity(&seed);
    let pubkey = xonly_public_key(&secret).expect("a derived identity is a valid scalar");
    tx.execute(
        "INSERT OR REPLACE INTO vault_identity_secret_no_sync (id, privkey) VALUES (1, ?1)",
        params![secret.as_slice()],
    )?;
    tx.execute(
        "INSERT INTO vault_identity (id, pubkey) VALUES (1, ?1)",
        params![pubkey.as_slice()],
    )?;
    Ok(Some(pubkey))
}

/// Lower-case hex, for `Debug` output and logs of public values.
pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn array<const N: usize>(bytes: &[u8], column: &str) -> haex_crdt::Result<[u8; N]> {
    bytes.try_into().map_err(|_| {
        haex_crdt::Error::consumer(format!(
            "{column} holds {} bytes, expected {N}",
            bytes.len()
        ))
    })
}

#[cfg(test)]
#[path = "keys_tests.rs"]
mod tests;
