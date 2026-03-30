//! Peer-side logic: connecting to leader, sending/receiving sync data.

use super::error::DeliveryError;
use super::protocol::{self, MlsMessageEntry, Request, Response};

/// A connected peer session with the leader.
pub struct PeerSession {
    conn: iroh::endpoint::Connection,
    our_did: String,
    our_endpoint_id: String,
}

impl PeerSession {
    /// Connect to a leader and announce our identity.
    pub async fn connect(
        iroh_endpoint: &iroh::Endpoint,
        leader_endpoint_id: &str,
        leader_relay_url: Option<&str>,
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

        let session = Self {
            conn,
            our_did: our_did.to_string(),
            our_endpoint_id: our_endpoint_id.to_string(),
        };

        // Send Announce request
        let req = Request::Announce {
            did: our_did.to_string(),
            endpoint_id: our_endpoint_id.to_string(),
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

    /// Upload MLS key packages.
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

    /// Fetch a key package for a target DID.
    pub async fn fetch_key_package(
        &self,
        space_id: &str,
        target_did: &str,
    ) -> Result<String, DeliveryError> {
        let req = Request::MlsFetchKeyPackage {
            space_id: space_id.to_string(),
            target_did: target_did.to_string(),
        };
        match self.request(req).await? {
            Response::KeyPackage { package } => Ok(package),
            Response::Error { message } => Err(DeliveryError::ProtocolError { reason: message }),
            _ => Err(DeliveryError::ProtocolError {
                reason: "unexpected response to MlsFetchKeyPackage".to_string(),
            }),
        }
    }

    /// Send an MLS message.
    pub async fn send_message(
        &self,
        space_id: &str,
        message: String,
        message_type: &str,
    ) -> Result<i64, DeliveryError> {
        let req = Request::MlsSendMessage {
            space_id: space_id.to_string(),
            message,
            message_type: message_type.to_string(),
        };
        match self.request(req).await? {
            Response::MessageStored { message_id } => Ok(message_id),
            Response::Error { message } => Err(DeliveryError::ProtocolError { reason: message }),
            _ => Err(DeliveryError::ProtocolError {
                reason: "unexpected response to MlsSendMessage".to_string(),
            }),
        }
    }

    /// Fetch MLS messages after a given ID.
    pub async fn fetch_messages(
        &self,
        space_id: &str,
        after_id: Option<i64>,
    ) -> Result<Vec<MlsMessageEntry>, DeliveryError> {
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

    /// Send a welcome message.
    pub async fn send_welcome(
        &self,
        space_id: &str,
        recipient_did: &str,
        welcome: String,
    ) -> Result<(), DeliveryError> {
        let req = Request::MlsSendWelcome {
            space_id: space_id.to_string(),
            recipient_did: recipient_did.to_string(),
            welcome,
        };
        match self.request(req).await? {
            Response::Ok => Ok(()),
            Response::Error { message } => Err(DeliveryError::ProtocolError { reason: message }),
            _ => Err(DeliveryError::ProtocolError {
                reason: "unexpected response to MlsSendWelcome".to_string(),
            }),
        }
    }

    /// Fetch welcome messages.
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

    /// Our DID.
    pub fn our_did(&self) -> &str {
        &self.our_did
    }

    /// Our endpoint ID.
    pub fn our_endpoint_id(&self) -> &str {
        &self.our_endpoint_id
    }

    /// Close the connection gracefully.
    pub fn close(&self) {
        self.conn.close(0u32.into(), b"done");
    }
}
