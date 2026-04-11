//! Peer-side logic: connecting to leader, sending/receiving sync data.

use super::error::DeliveryError;
use super::protocol::{self, Request, Response};

/// A connected peer session with the leader.
pub struct PeerSession {
    conn: iroh::endpoint::Connection,
}

impl PeerSession {
    /// Connect to a leader and announce our identity.
    pub async fn connect(
        iroh_endpoint: &iroh::Endpoint,
        leader_endpoint_id: &str,
        leader_relay_url: Option<&str>,
        space_id: &str,
        our_did: &str,
        our_endpoint_id: &str,
        label: Option<&str>,
    ) -> Result<Self, DeliveryError> {
        let remote_id: iroh::EndpointId =
            leader_endpoint_id
                .parse()
                .map_err(|e| DeliveryError::ConnectionFailed {
                    reason: format!("invalid endpoint id: {e}"),
                })?;

        let relay = leader_relay_url
            .and_then(|s| s.parse::<iroh::RelayUrl>().ok());

        let addr = match relay {
            Some(url) => iroh::EndpointAddr::new(remote_id).with_relay_url(url),
            None => iroh::EndpointAddr::new(remote_id),
        };

        let conn = iroh_endpoint
            .connect(addr, protocol::ALPN)
            .await
            .map_err(|e| DeliveryError::ConnectionFailed {
                reason: e.to_string(),
            })?;

        let session = Self { conn };

        // Send Announce request
        let req = Request::Announce {
            did: our_did.to_string(),
            endpoint_id: our_endpoint_id.to_string(),
            space_id: space_id.to_string(),
            label: label.map(|s| s.to_string()),
            claims: None,
        };

        let resp = session.request(req).await?;
        match resp {
            Response::Ok => Ok(session),
            Response::Error { message } => Err(DeliveryError::ProtocolError { reason: message }),
            _ => Err(DeliveryError::ProtocolError {
                reason: "unexpected response to Announce".to_string(),
            }),
        }
    }

    /// Send a request and read the response.
    async fn request(&self, req: Request) -> Result<Response, DeliveryError> {
        let (mut send, mut recv) = self
            .conn
            .open_bi()
            .await
            .map_err(|e| DeliveryError::ConnectionFailed {
                reason: e.to_string(),
            })?;

        let bytes =
            protocol::encode(&req).map_err(|e| DeliveryError::ProtocolError {
                reason: e.to_string(),
            })?;

        send.write_all(&bytes)
            .await
            .map_err(|e| DeliveryError::ConnectionFailed {
                reason: e.to_string(),
            })?;

        send.finish()
            .map_err(|e| DeliveryError::ConnectionFailed {
                reason: e.to_string(),
            })?;

        protocol::read_response(&mut recv)
            .await
            .map_err(|e| DeliveryError::ProtocolError {
                reason: e.to_string(),
            })
    }

    /// Push local CRDT changes to the leader.
    pub async fn push_changes(
        &self,
        space_id: &str,
        changes: serde_json::Value,
    ) -> Result<(), DeliveryError> {
        let req = Request::SyncPush {
            space_id: space_id.to_string(),
            changes,
        };
        match self.request(req).await? {
            Response::Ok => Ok(()),
            Response::Error { message } => Err(DeliveryError::ProtocolError { reason: message }),
            _ => Err(DeliveryError::ProtocolError {
                reason: "unexpected response to SyncPush".to_string(),
            }),
        }
    }

    /// Pull CRDT changes from the leader.
    pub async fn pull_changes(
        &self,
        space_id: &str,
        after_timestamp: Option<&str>,
    ) -> Result<serde_json::Value, DeliveryError> {
        let req = Request::SyncPull {
            space_id: space_id.to_string(),
            after_timestamp: after_timestamp.map(|s| s.to_string()),
        };
        match self.request(req).await? {
            Response::SyncChanges { changes } => Ok(changes),
            Response::Error { message } => Err(DeliveryError::ProtocolError { reason: message }),
            _ => Err(DeliveryError::ProtocolError {
                reason: "unexpected response to SyncPull".to_string(),
            }),
        }
    }

    /// Fetch MLS messages from the leader after a given ID.
    pub async fn fetch_mls_messages(
        &self,
        space_id: &str,
        after_id: Option<i64>,
    ) -> Result<Vec<super::protocol::MlsMessageEntry>, DeliveryError> {
        let req = Request::MlsFetchMessages {
            space_id: space_id.to_string(),
            after_id,
        };
        match self.request(req).await? {
            Response::Messages { messages } => Ok(messages),
            Response::Error { message } => Err(DeliveryError::ProtocolError { reason: message }),
            _ => Err(DeliveryError::ProtocolError {
                reason: "unexpected response to MlsFetchMessages".to_string(),
            }),
        }
    }

    /// Acknowledge successfully processed MLS commits.
    pub async fn ack_commits(
        &self,
        space_id: &str,
        message_ids: Vec<i64>,
    ) -> Result<(), DeliveryError> {
        if message_ids.is_empty() {
            return Ok(());
        }
        let req = Request::MlsAckCommit {
            space_id: space_id.to_string(),
            message_ids,
        };
        match self.request(req).await? {
            Response::Ok => Ok(()),
            Response::Error { message } => Err(DeliveryError::ProtocolError { reason: message }),
            _ => Err(DeliveryError::ProtocolError {
                reason: "unexpected response to MlsAckCommit".to_string(),
            }),
        }
    }

    /// Upload key packages to the leader for this peer's DID.
    pub async fn upload_key_packages(
        &self,
        space_id: &str,
        packages: Vec<String>,
    ) -> Result<(), DeliveryError> {
        let req = Request::MlsUploadKeyPackages {
            space_id: space_id.to_string(),
            packages,
        };
        match self.request(req).await? {
            Response::Ok => Ok(()),
            Response::Error { message } => Err(DeliveryError::ProtocolError { reason: message }),
            _ => Err(DeliveryError::ProtocolError {
                reason: "unexpected response to MlsUploadKeyPackages".to_string(),
            }),
        }
    }

    /// Query key package status: how many the leader has and how many more it needs.
    /// Returns (available, needed).
    pub async fn query_key_package_status(
        &self,
        space_id: &str,
    ) -> Result<(u32, u32), DeliveryError> {
        let req = Request::MlsKeyPackageCount {
            space_id: space_id.to_string(),
        };
        match self.request(req).await? {
            Response::KeyPackageCount { available, needed } => Ok((available, needed)),
            Response::Error { message } => Err(DeliveryError::ProtocolError { reason: message }),
            _ => Err(DeliveryError::ProtocolError {
                reason: "unexpected response to MlsKeyPackageCount".to_string(),
            }),
        }
    }

    /// Fetch unconsumed welcome messages from the leader.
    pub async fn fetch_welcomes(
        &self,
        space_id: &str,
    ) -> Result<Vec<String>, DeliveryError> {
        let req = Request::MlsFetchWelcomes {
            space_id: space_id.to_string(),
        };
        match self.request(req).await? {
            Response::Welcomes { welcomes } => Ok(welcomes),
            Response::Error { message } => Err(DeliveryError::ProtocolError { reason: message }),
            _ => Err(DeliveryError::ProtocolError {
                reason: "unexpected response to MlsFetchWelcomes".to_string(),
            }),
        }
    }

    /// Request rejoin via External Commit. Returns base64-encoded GroupInfo.
    pub async fn request_rejoin(
        &self,
        space_id: &str,
        ucan_token: &str,
    ) -> Result<String, DeliveryError> {
        let req = Request::RequestRejoin {
            space_id: space_id.to_string(),
            ucan_token: ucan_token.to_string(),
        };
        match self.request(req).await? {
            Response::GroupInfo { group_info } => Ok(group_info),
            Response::Error { message } => Err(DeliveryError::ProtocolError { reason: message }),
            _ => Err(DeliveryError::ProtocolError {
                reason: "unexpected response to RequestRejoin".to_string(),
            }),
        }
    }

    /// Submit an External Commit to rejoin a group.
    pub async fn submit_external_commit(
        &self,
        space_id: &str,
        commit_b64: &str,
        ucan_token: &str,
    ) -> Result<(), DeliveryError> {
        let req = Request::SubmitExternalCommit {
            space_id: space_id.to_string(),
            commit: commit_b64.to_string(),
            ucan_token: ucan_token.to_string(),
        };
        match self.request(req).await? {
            Response::Ok => Ok(()),
            Response::Error { message } => Err(DeliveryError::ProtocolError { reason: message }),
            _ => Err(DeliveryError::ProtocolError {
                reason: "unexpected response to SubmitExternalCommit".to_string(),
            }),
        }
    }

    /// Close the connection gracefully.
    pub fn close(&self) {
        self.conn.close(0u32.into(), b"done");
    }
}
