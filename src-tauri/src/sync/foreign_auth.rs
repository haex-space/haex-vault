//! A device of another vault proves who it is (spec 057 research R4; shared with spec 056).
//!
//! On the first bi-stream of a connection, opened by the dialing side:
//!
//! ```text
//! dialing   → Hello { v, vault, device_pubkey, list_payload, list_sig, nonce }
//! accepting → Hello { … its own values … }
//! dialing   → Proof { v, sig }
//! accepting → Proof { v, sig }
//! ```
//!
//! Each side signs `lp(nonce_dial) ‖ lp(nonce_accept) ‖ lp(own endpoint) ‖ lp(other endpoint) ‖
//! lp(vault) ‖ lp(alpn)` with its device key, in a [`Domain`] the protocol picks; the other
//! endpoint is the connection's `remote_id()`, so a proof only holds for the endpoint the
//! connection really comes from, and the ALPN keeps a proof for one protocol from serving another.
//! Each side then checks the other: its device list is signed by its vault, names the device with
//! exactly this endpoint and does not remove it. For the own vault the own valid list decides, not
//! the presented one. A vault whose card is stored and not blocked is a contact, as long as the
//! list is not older than the card's or one seen before in this session; a blocked vault is
//! refused; any other vault is not verified, and the protocol decides what that may do.
//!
//! Every refusal is the same to the other side: the connection ends without a reason.

use std::collections::HashMap;
use std::sync::{Mutex, PoisonError};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tokio::io::{AsyncRead, AsyncWrite};

use crate::contacts::service::{Caller, ContactsService, VaultCard};
use crate::sync::device_list::{self, SignedList};
use crate::sync::keys::{self, DeviceKeys};
use crate::sync::signing::{self, lp, Domain};
use crate::sync::wire::{expect_value, write_value, Signature, WireError, HANDSHAKE_FRAME_LIMIT};
use crate::vault_gate::VaultDb;

/// The version of [`Hello`] and [`Proof`].
const VERSION: u8 = 1;
/// How long the whole exchange may take (FR-032 of spec 057).
pub const HANDSHAKE_LIMIT: Duration = Duration::from_secs(5);
/// Most vaults whose highest list generation this session remembers.
const MAX_SEEN_VAULTS: usize = 1024;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Hello {
    v: u8,
    vault: [u8; 32],
    device_pubkey: [u8; 32],
    list_payload: Vec<u8>,
    list_sig: Signature,
    nonce: [u8; 32],
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Proof {
    v: u8,
    sig: Signature,
}

/// How far this device trusts the other one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Trust {
    /// A listed device of this device's own vault.
    OwnVault,
    /// A device of a vault whose card is stored on a contact and that is not blocked.
    Contact {
        contact_id: String,
        /// The name the stored card carries.
        card_name: String,
    },
    /// A device of another vault that proved its key and list, nothing more.
    Unverified,
}

/// Both nonces of an exchange; a protocol may bind further proofs to them (a join's secret).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Nonces {
    pub dial: [u8; 32],
    pub accept: [u8; 32],
}

/// The other device after a successful exchange.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Verified {
    pub device_pubkey: [u8; 32],
    pub vault: [u8; 32],
    pub endpoint_id: [u8; 32],
    pub trust: Trust,
    pub nonces: Nonces,
}

/// What the proofs of one protocol are bound to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Purpose {
    pub alpn: &'static [u8],
    pub domain: Domain,
}

/// The highest device-list generation seen per foreign vault in this session (056 FR-003), so a
/// removed device cannot come back with a list from before its removal once a newer one was seen.
///
/// ponytail: kept in memory only, at most [`MAX_SEEN_VAULTS`] vaults (further ones are not
/// remembered); after a restart only the card's generation is the floor. Upgrade: a `_no_sync`
/// table of the highest generation per vault (spec 056 adopted this rule instead of its own table).
#[derive(Debug, Default)]
pub struct SeenGenerations(Mutex<HashMap<[u8; 32], u64>>);

impl SeenGenerations {
    /// Notes `generation` for `vault`; `false` when a higher one was seen before.
    pub fn note(&self, vault: [u8; 32], generation: u64) -> bool {
        let mut seen = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        match seen.get_mut(&vault) {
            Some(highest) if *highest > generation => false,
            Some(highest) => {
                *highest = generation;
                true
            }
            None => {
                if seen.len() < MAX_SEEN_VAULTS {
                    seen.insert(vault, generation);
                }
                true
            }
        }
    }

    /// Forgets everything, at the end of the vault session.
    pub fn clear(&self) {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clear();
    }
}

/// This device, and where it looks the other device up.
pub struct Local<'a> {
    pub keys: &'a DeviceKeys,
    pub vault: [u8; 32],
    pub db: &'a VaultDb,
    /// The holzi function the contact lookup runs as (`Caller::Internal`).
    pub feature: &'static str,
    pub seen: &'a SeenGenerations,
}

/// Why an exchange ended without a verified device.
#[derive(Debug, thiserror::Error)]
pub enum ForeignAuthError {
    #[error("the other device was refused: {0}")]
    Refused(&'static str),
    #[error("the exchange took longer than {HANDSHAKE_LIMIT:?}")]
    TimedOut,
    #[error("this device holds no valid device list")]
    NoDeviceList,
    #[error(transparent)]
    Wire(#[from] WireError),
    #[error(transparent)]
    Signing(#[from] signing::SigningError),
    #[error("this vault could not be read: {0}")]
    Vault(#[from] crate::error::HolziError),
}

/// The dialing side; `remote_endpoint` is the connection's `remote_id()`.
pub async fn dial<W, R>(
    send: &mut W,
    recv: &mut R,
    local: &Local<'_>,
    purpose: Purpose,
    remote_endpoint: [u8; 32],
) -> Result<Verified, ForeignAuthError>
where
    W: AsyncWrite + Unpin + Send,
    R: AsyncRead + Unpin + Send,
{
    within_limit(async {
        let own = own_lists(local).await?;
        let hello = own_hello(local, &own)?;
        write_value(send, &hello, HANDSHAKE_FRAME_LIMIT).await?;
        let theirs: Hello = expect_value(recv, HANDSHAKE_FRAME_LIMIT).await?;
        let nonces = Nonces {
            dial: hello.nonce,
            accept: theirs.nonce,
        };
        let proof = own_proof(local, purpose, &nonces, remote_endpoint)?;
        write_value(send, &proof, HANDSHAKE_FRAME_LIMIT).await?;
        let their_proof: Proof = expect_value(recv, HANDSHAKE_FRAME_LIMIT).await?;
        check(
            local,
            purpose,
            &own,
            &theirs,
            &their_proof,
            nonces,
            remote_endpoint,
        )
        .await
    })
    .await
}

/// The accepting side; `remote_endpoint` is the connection's `remote_id()`. It proves itself only
/// after the dialing side passed every check.
pub async fn accept<W, R>(
    send: &mut W,
    recv: &mut R,
    local: &Local<'_>,
    purpose: Purpose,
    remote_endpoint: [u8; 32],
) -> Result<Verified, ForeignAuthError>
where
    W: AsyncWrite + Unpin + Send,
    R: AsyncRead + Unpin + Send,
{
    within_limit(async {
        let own = own_lists(local).await?;
        let theirs: Hello = expect_value(recv, HANDSHAKE_FRAME_LIMIT).await?;
        let hello = own_hello(local, &own)?;
        write_value(send, &hello, HANDSHAKE_FRAME_LIMIT).await?;
        let nonces = Nonces {
            dial: theirs.nonce,
            accept: hello.nonce,
        };
        let their_proof: Proof = expect_value(recv, HANDSHAKE_FRAME_LIMIT).await?;
        let verified = check(
            local,
            purpose,
            &own,
            &theirs,
            &their_proof,
            nonces,
            remote_endpoint,
        )
        .await?;
        let proof = own_proof(local, purpose, &nonces, remote_endpoint)?;
        write_value(send, &proof, HANDSHAKE_FRAME_LIMIT).await?;
        Ok(verified)
    })
    .await
}

async fn within_limit(
    exchange: impl std::future::Future<Output = Result<Verified, ForeignAuthError>>,
) -> Result<Verified, ForeignAuthError> {
    tokio::time::timeout(HANDSHAKE_LIMIT, exchange)
        .await
        .map_err(|_| ForeignAuthError::TimedOut)?
}

/// The valid lists of this device's own vault.
async fn own_lists(
    local: &Local<'_>,
) -> Result<std::collections::BTreeMap<[u8; 32], SignedList>, ForeignAuthError> {
    let vault = local.vault;
    let rows = local.db.read(|q| device_list::load_all(q)).await?;
    Ok(device_list::valid_lists(&rows, &vault))
}

fn own_hello(
    local: &Local<'_>,
    own: &std::collections::BTreeMap<[u8; 32], SignedList>,
) -> Result<Hello, ForeignAuthError> {
    let effective = device_list::effective(own).ok_or(ForeignAuthError::NoDeviceList)?;
    Ok(Hello {
        v: VERSION,
        vault: local.vault,
        device_pubkey: local.keys.device_pubkey,
        list_payload: effective.payload.clone(),
        list_sig: Signature(effective.signature),
        nonce: *keys::random_bytes::<32>(),
    })
}

fn own_proof(
    local: &Local<'_>,
    purpose: Purpose,
    nonces: &Nonces,
    remote_endpoint: [u8; 32],
) -> Result<Proof, ForeignAuthError> {
    let signed = transcript(
        nonces,
        &local.keys.endpoint_id,
        &remote_endpoint,
        &local.vault,
        purpose.alpn,
    );
    Ok(Proof {
        v: VERSION,
        sig: Signature(signing::sign(
            purpose.domain,
            &signed,
            &local.keys.device_secret,
        )?),
    })
}

/// The signed bytes, `own` being the signer's endpoint and `vault` the signer's vault.
fn transcript(
    nonces: &Nonces,
    own: &[u8; 32],
    other: &[u8; 32],
    vault: &[u8; 32],
    alpn: &[u8],
) -> Vec<u8> {
    [
        lp(&nonces.dial),
        lp(&nonces.accept),
        lp(own),
        lp(other),
        lp(vault),
        lp(alpn),
    ]
    .concat()
}

/// Every check of the other side, cheapest first; the address book is read last.
async fn check(
    local: &Local<'_>,
    purpose: Purpose,
    own: &std::collections::BTreeMap<[u8; 32], SignedList>,
    hello: &Hello,
    proof: &Proof,
    nonces: Nonces,
    remote_endpoint: [u8; 32],
) -> Result<Verified, ForeignAuthError> {
    use ForeignAuthError::Refused;
    if hello.v != VERSION || proof.v != VERSION {
        return Err(Refused("unknown version"));
    }
    let own_vault = hello.vault == local.vault;
    let presented;
    let list = if own_vault {
        if hello.device_pubkey == local.keys.device_pubkey {
            return Err(Refused("this device itself"));
        }
        device_list::effective(own).ok_or(ForeignAuthError::NoDeviceList)?
    } else {
        presented = device_list::check_pushed(&hello.list_payload, &hello.list_sig.0, &hello.vault)
            .map_err(|_| Refused("the device list does not check out"))?;
        &presented
    };
    if list.list.removes(&hello.device_pubkey) {
        return Err(Refused("removed from its vault"));
    }
    let entry = list
        .list
        .device(&hello.device_pubkey)
        .ok_or(Refused("not on its vault's device list"))?;
    if entry.endpoint_id != remote_endpoint {
        return Err(Refused("another endpoint than its list names"));
    }
    let signed = transcript(
        &nonces,
        &remote_endpoint,
        &local.keys.endpoint_id,
        &hello.vault,
        purpose.alpn,
    );
    signing::verify(purpose.domain, &signed, &proof.sig.0, &hello.device_pubkey)
        .map_err(|_| Refused("the proof does not verify"))?;
    let trust = if own_vault {
        Trust::OwnVault
    } else {
        foreign_trust(local, hello.vault, list.list.generation).await?
    };
    Ok(Verified {
        device_pubkey: hello.device_pubkey,
        vault: hello.vault,
        endpoint_id: remote_endpoint,
        trust,
        nonces,
    })
}

/// How far a device of another vault, with a valid list of `generation`, is trusted.
async fn foreign_trust(
    local: &Local<'_>,
    vault: [u8; 32],
    generation: u64,
) -> Result<Trust, ForeignAuthError> {
    use ForeignAuthError::Refused;
    let caller = Caller::Internal {
        feature: local.feature,
    };
    let card = ContactsService::new(local.db.clone())
        .card_of_vault(&caller, vault)
        .await?;
    let trust = match card {
        VaultCard::Blocked => return Err(Refused("blocked")),
        VaultCard::Stored { contact_id, card } => {
            // `card` is the newest stored card of the vault (`received::stored`). Addresses that a
            // share added from other cards only serve to dial; they never lower this threshold, so
            // an older card that still names a removed device cannot let it in (spec 060 R5).
            if generation < card.card.list_generation {
                return Err(Refused("a list older than the stored card's"));
            }
            Trust::Contact {
                contact_id,
                card_name: card.card.name,
            }
        }
        VaultCard::Unknown => Trust::Unverified,
    };
    if !local.seen.note(vault, generation) {
        return Err(Refused("a list older than one seen before"));
    }
    Ok(trust)
}

#[cfg(test)]
#[path = "foreign_auth_tests.rs"]
mod tests;
