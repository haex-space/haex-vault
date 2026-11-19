//! Messages of `haex-link/1` (contracts/sync-protocol.md, research R11).
//!
//! Frames are those of the sync protocol (`u32 BE length ‖ postcard`), with
//! the 64 KiB limit until both proofs checked out and 4 MiB after.

use std::fmt;

use serde::{Deserialize, Serialize};
use uuid::Uuid;
use zeroize::Zeroize;

use crate::sync::change::Page;
use crate::sync::content_keys::StoredEnvelope;
use crate::sync::wire::{SchemaVersion, Signature};

/// Why a side ends the link on purpose.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AbortReason {
    /// The user on the main device declined.
    Rejected,
    /// The two sides' protocol or schema versions differ (FR-029).
    Incompatible,
    /// The user cancelled, or the wait for confirmation ran out.
    Cancelled,
}

/// The private key of the vault identity while it crosses the link. Never
/// printed, wiped when dropped (FR-002).
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SecretBytes(pub [u8; 32]);

impl fmt::Debug for SecretBytes {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("SecretBytes(..)")
    }
}

impl Drop for SecretBytes {
    fn drop(&mut self) {
        self.0.zeroize();
    }
}

/// Every message of `haex-link/1`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum LinkMessage {
    /// H → N: the first message; the code counts as used once it is sent.
    Hello {
        nonce_h: [u8; 32],
        endpoint_h: [u8; 32],
        device_h: [u8; 32],
    },
    /// N → H: who N is, with its proof of the code.
    Proof {
        nonce_n: [u8; 32],
        endpoint_n: [u8; 32],
        device_n: [u8; 32],
        /// The device id of N's vault (the origin of its changes), which
        /// the new list names it by.
        vault_device_uuid: Uuid,
        name: String,
        schema: SchemaVersion,
        mac_n: [u8; 32],
    },
    /// H → N: H's proof of the code.
    HostProof { mac_h: [u8; 32] },
    /// H → N: the user's decision, sent after H showed N's name.
    Decision { accepted: bool },
    /// H → N: a page of the snapshot, as in the sync protocol, from
    /// "nothing" for every origin.
    Page(Page),
    /// H → N: after the last page, everything else N needs.
    Transfer {
        link_id: [u8; 32],
        /// Every content key generation, wrapped for N under the new list.
        envelopes: Vec<StoredEnvelope>,
        /// The new device list (canonical bytes), not published yet.
        list_payload: Vec<u8>,
        list_signature: Signature,
        /// Only when the user chose the main device role (FR-038).
        vault_secret: Option<SecretBytes>,
    },
    /// N → H: N stored everything and now waits for the new list.
    Done { link_id: [u8; 32] },
    /// H → N: resumes a link after a break, authenticated with the session
    /// secret instead of the used-up code.
    Resume {
        link_id: [u8; 32],
        state: String,
        mac: [u8; 32],
    },
    /// Either side: the link ends here.
    Abort { reason: AbortReason },
}
