//! An endpoint an extension proposes for a new storage (spec 038 FR-009b, FR-017, research R8),
//! checked before holzi shows its dialog: its form, the permission `remoteStorage`/`add` for its
//! host, and that all its addresses lie in one scope holzi reaches (`http` only to a local one).
//! The scope found here is what the dialog shows and what the new connection must keep. AWS by
//! region needs `add` for its regional host as well ([`check_aws`]). A WebDAV address (spec 050
//! US6) follows the same rules; its folder path is kept and ends in `/`.

use std::time::Duration;

use reqwest::Url;
use url::Host;

use super::bridge::blocking::block_on;
use super::bridge::dispatch::CallContext;
use super::error::BridgeError;
use super::permissions::{Action, RequestTarget};
use super::remote_storage::{check, grants, invalid, provider};
use crate::remote_storage::address::{self, AddressError};
use crate::remote_storage::model::ConnectionView;
use crate::remote_storage::{EndpointScope, Protocol};

/// How long holzi waits for the addresses of a proposed endpoint before the dialog.
const RESOLVE_TIME: Duration = Duration::from_secs(10);

/// A proposed endpoint after its checks.
pub(super) struct Proposed {
    pub url: Url,
    pub scope: EndpointScope,
}

impl Proposed {
    /// Checks `endpoint` of `protocol` (see the module): 3001 for a form or an address holzi
    /// never reaches, 1004/1002 without the `add` permission for its host (port from the scheme
    /// when absent), 2002 `network` when it does not resolve.
    pub fn check(
        ctx: &CallContext,
        endpoint: &str,
        protocol: Protocol,
    ) -> Result<Self, BridgeError> {
        let mut url =
            address::check_endpoint(endpoint).map_err(|_| invalid("endpoint not allowed"))?;
        if protocol == Protocol::WebDav && !url.path().ends_with('/') {
            let path = format!("{}/", url.path());
            url.set_path(&path);
        }
        let host = match url.host() {
            Some(Host::Domain(name)) => name.to_ascii_lowercase(),
            Some(Host::Ipv4(ip)) => ip.to_string(),
            Some(Host::Ipv6(ip)) => format!("[{ip}]"),
            None => return Err(invalid("endpoint not allowed")),
        };
        let port = url
            .port_or_known_default()
            .ok_or_else(|| invalid("endpoint not allowed"))?;
        let asked = format!("{host}:{port}");
        may_add(ctx, host, port, &asked)?;
        let resolver = ctx.host.storage.resolver();
        let scope = block_on(async {
            tokio::time::timeout(RESOLVE_TIME, address::scope_of(&url, resolver.as_ref())).await
        });
        match scope {
            Ok(Ok(scope)) => Ok(Self { url, scope }),
            Ok(Err(AddressError::Invalid | AddressError::NotAllowed)) => {
                Err(invalid("endpoint not allowed"))
            }
            Ok(Err(AddressError::Unresolved)) | Err(_) => Err(provider("network")),
        }
    }

    /// Whether the user may pick `connection` instead of new credentials: it speaks `protocol`
    /// to this endpoint in `region` (empty for WebDAV), in the scope the dialog shows.
    pub fn fits(&self, connection: &ConnectionView, protocol: Protocol, region: &str) -> bool {
        connection.protocol == protocol
            && Url::parse(&connection.endpoint).is_ok_and(|own| own == self.url)
            && connection.region == region
            && connection.endpoint_scope == self.scope
    }
}

/// The permission `remoteStorage`/`add` for `host` and `port`; without it 1004 naming `asked`
/// (state "ask") or 1002.
fn may_add(ctx: &CallContext, host: String, port: u16, asked: &str) -> Result<(), BridgeError> {
    check(
        ctx,
        &grants(ctx)?,
        Action::Add,
        RequestTarget::Endpoint { host, port },
        asked,
    )
}

/// An AWS proposal by `region` alone: the region must name a regional host, and the extension
/// needs `add` for `s3.<region>.amazonaws.com` (or `*`) like for an own endpoint (FR-009b).
pub(super) fn check_aws(ctx: &CallContext, region: &str) -> Result<(), BridgeError> {
    let url = address::aws_endpoint(region).map_err(|_| invalid("invalid region"))?;
    let host = url
        .host_str()
        .ok_or_else(|| invalid("invalid region"))?
        .to_owned();
    may_add(ctx, host.clone(), 443, &host)
}
