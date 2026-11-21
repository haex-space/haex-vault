//! The test of a Blossom-Server address in the chat settings (spec 072 FR-080, research R12): asks
//! the server with `HEAD /upload` (BUD-06) whether it takes a file of the largest size, and when it
//! refuses for size, whether it takes a small one. Nothing is uploaded; the request is signed with a
//! key made for the test.

use nostr::key::Keys;
use serde::Serialize;
use sha2::{Digest, Sha256};
use ts_rs::TS;

use super::blossom::{BlossomError, Server};
use super::crypto::{ciphertext_len, MAX_PLAINTEXT};
use crate::remote_storage::address::Resolver;
use crate::sync::keys::hex;

/// What the test found out about a server.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[ts(
    export,
    export_to = "../../src/types/bindings/",
    rename = "ChatBlossomVerdict"
)]
#[serde(rename_all = "camelCase")]
pub enum Verdict {
    /// The server takes a file of the largest size.
    Accepts,
    /// The server takes small files but refuses the largest size.
    OnlySmall,
    /// The server refuses even a small file.
    Refused,
    /// The server did not answer, or not in time.
    Unreachable,
    /// The address is not one a chat file may go to.
    NotAllowed,
}

/// The result of the test: the verdict, and for a refusal the status and the reason the server gave.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(
    export,
    export_to = "../../src/types/bindings/",
    rename = "ChatBlossomCheck"
)]
#[serde(rename_all = "camelCase")]
pub struct Check {
    pub verdict: Verdict,
    pub status: Option<u16>,
    pub reason: Option<String>,
}

impl Check {
    fn of(verdict: Verdict) -> Self {
        Self {
            verdict,
            status: None,
            reason: None,
        }
    }

    fn refused(error: BlossomError, verdict: Verdict) -> Self {
        match error {
            BlossomError::Refused { status, reason } => Self {
                verdict,
                status: Some(status),
                reason,
            },
            BlossomError::NotAllowed => Self::of(Verdict::NotAllowed),
            _ => Self::of(Verdict::Unreachable),
        }
    }
}

/// Tests the server at `address`. The size asked for first is the largest file a message carries.
pub async fn check(address: &str, resolver: &dyn Resolver) -> Check {
    let server = match Server::new(address, resolver).await {
        Ok(server) => server,
        Err(error) => return Check::refused(error, Verdict::NotAllowed),
    };
    let signer = Keys::generate();
    let hash = hex(&Sha256::digest(b"haex blossom check"));
    let largest = ciphertext_len(MAX_PLAINTEXT);
    let first = match server.check_upload(&signer, &hash, largest, resolver).await {
        Ok(()) => return Check::of(Verdict::Accepts),
        Err(error) => error,
    };
    // Only "too large" is worth a second question; any other refusal holds for every size.
    if !matches!(first, BlossomError::Refused { status: 413, .. }) {
        return Check::refused(first, Verdict::Refused);
    }
    match server
        .check_upload(&signer, &hash, ciphertext_len(1), resolver)
        .await
    {
        Ok(()) => Check::refused(first, Verdict::OnlySmall),
        Err(error) => Check::refused(error, Verdict::Refused),
    }
}

#[cfg(test)]
#[path = "probe_tests.rs"]
mod tests;
