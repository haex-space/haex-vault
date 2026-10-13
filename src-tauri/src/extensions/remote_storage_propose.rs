//! An extension proposes a new storage (spec 038 US3, FR-009b, FR-013; spec 050 US6,
//! contracts/bridge.md): on the connection of a storage it may read (`sameProviderAs`, same
//! protocol only), on an S3 endpoint or AWS by region, or on a WebDAV address. Every proposal goes
//! through holzi's dialog; credentials are typed only in holzi's window over the whole app. A
//! missing folder of a WebDAV storage is made only when the user says so in that dialog (050
//! FR-006).

use serde_json::{json, Map, Value};

use super::bridge::blocking::block_on;
use super::bridge::dispatch::CallContext;
use super::error::BridgeError;
use super::permissions::Action;
use super::remote_storage::{check_storage, invalid, overview, service};
use super::remote_storage_dialog::{ask, open, DialogKind};
use super::remote_storage_endpoint::{check_aws, Proposed};
use super::remote_storage_manage::{
    answer_item, cancelled, config, confirmed, extension_name, from_service, grant, optional_text,
    place_of, request, with_credentials,
};
use crate::remote_storage::address;
use crate::remote_storage::model::{ConnectionInput, StorageInput};
use crate::remote_storage::service::StorageService;
use crate::remote_storage::{Addressing, EndpointScope, Protocol, ProviderKind};

pub const MODULE: &str = module_path!();

/// `{request: {name, type, config, sameProviderAs?}}` → the new storage as in the list. `type` is
/// `s3` (`config: {endpoint?, region?, bucket, pathStyle?}`) or `webdav` (`config: {url,
/// folder}`); with `sameProviderAs` the config holds only `bucket` or `folder`.
pub fn add_backend(ctx: &CallContext, params: &Value) -> Result<Value, BridgeError> {
    let request = request(params)?;
    let name = optional_text(request, "name")?.ok_or_else(|| invalid("name must be a string"))?;
    let protocol = match optional_text(request, "type")? {
        Some("s3") => Protocol::S3,
        Some("webdav") => Protocol::WebDav,
        _ => return Err(invalid("type must be s3 or webdav")),
    };
    let service = service(ctx);
    let storage_id = match (optional_text(request, "sameProviderAs")?, protocol) {
        (Some(same), _) => add_on_same_provider(ctx, &service, request, name, same, protocol)?,
        (None, Protocol::S3) => add_with_endpoint(ctx, &service, request, name)?,
        (None, _) => add_webdav(ctx, &service, request, name)?,
    };
    grant(ctx, &storage_id)?;
    answer_item(&service, &storage_id)
}

/// A new bucket or folder on the connection of a storage the extension may read.
fn add_on_same_provider(
    ctx: &CallContext,
    service: &StorageService,
    request: &Map<String, Value>,
    name: &str,
    same: &str,
    protocol: Protocol,
) -> Result<String, BridgeError> {
    let place = place_of(protocol);
    let config = config(request, &[place])?;
    let bucket = optional_text(&config, place)?
        .ok_or_else(|| invalid(&format!("{place} must be a string")))?;
    check_storage(ctx, Action::Read, same)?;
    let base = block_on(service.storage(same)).map_err(from_service)?;
    let connection = block_on(service.connection(&base.connection_id)).map_err(from_service)?;
    if connection.protocol != protocol {
        return Err(invalid("sameProviderAs is a storage of another type"));
    }
    let answer = confirmed(ask(
        ctx,
        DialogKind::Add,
        &extension_name(ctx),
        json!({
            "name": name,
            "bucket": bucket,
            "protocol": protocol.as_str(),
            "providerName": connection.provider_name,
            "endpoint": connection.endpoint,
            "region": connection.region,
            "scope": connection.endpoint_scope.as_str(),
            "connectionId": connection.id,
            "sameProvider": true,
        }),
        &[],
    )?)?;
    let saved = block_on(service.save_storage(StorageInput {
        id: None,
        connection_id: connection.id,
        name: answer.name.unwrap_or_else(|| name.to_owned()),
        bucket: answer.bucket.unwrap_or_else(|| bucket.to_owned()),
        create_folder: answer.create_folder,
    }))
    .map_err(from_service)?;
    Ok(saved.id)
}

/// Where a proposal on an endpoint puts the new storage.
struct Target {
    protocol: Protocol,
    kind: ProviderKind,
    /// The checked endpoint; `None` for AWS by region.
    proposed: Option<Proposed>,
    region: String,
    addressing: Addressing,
    bucket: String,
}

/// An S3 storage on a proposed endpoint, or on AWS by region.
fn add_with_endpoint(
    ctx: &CallContext,
    service: &StorageService,
    request: &Map<String, Value>,
    name: &str,
) -> Result<String, BridgeError> {
    let config = config(request, &["endpoint", "region", "bucket", "pathStyle"])?;
    let bucket =
        optional_text(&config, "bucket")?.ok_or_else(|| invalid("bucket must be a string"))?;
    let region =
        optional_text(&config, "region")?.ok_or_else(|| invalid("region must be a string"))?;
    let path_style = match config.get("pathStyle") {
        None | Some(Value::Null) => None,
        Some(Value::Bool(path)) => Some(*path),
        Some(_) => return Err(invalid("pathStyle must be a boolean")),
    };
    let endpoint = optional_text(&config, "endpoint")?.filter(|e| !e.trim().is_empty());
    let (kind, proposed) = match endpoint {
        Some(endpoint) => (
            ProviderKind::Other,
            Some(Proposed::check(ctx, endpoint, Protocol::S3)?),
        ),
        None => {
            check_aws(ctx, region)?;
            (ProviderKind::Aws, None)
        }
    };
    let addressing = match path_style.unwrap_or(proposed.is_some()) {
        true => Addressing::Path,
        false => Addressing::Virtual,
    };
    add_on_endpoint(
        ctx,
        service,
        name,
        Target {
            protocol: Protocol::S3,
            kind,
            proposed,
            region: region.to_owned(),
            addressing,
            bucket: bucket.to_owned(),
        },
    )
}

/// A WebDAV storage on a proposed address (spec 050 US6): `config: {url, folder}`.
fn add_webdav(
    ctx: &CallContext,
    service: &StorageService,
    request: &Map<String, Value>,
    name: &str,
) -> Result<String, BridgeError> {
    let config = config(request, &["url", "folder"])?;
    let url = optional_text(&config, "url")?.ok_or_else(|| invalid("url must be a string"))?;
    let folder =
        optional_text(&config, "folder")?.ok_or_else(|| invalid("folder must be a string"))?;
    let proposed = Proposed::check(ctx, url, Protocol::WebDav)?;
    add_on_endpoint(
        ctx,
        service,
        name,
        Target {
            protocol: Protocol::WebDav,
            kind: ProviderKind::Other,
            proposed: Some(proposed),
            region: String::new(),
            addressing: Addressing::Path,
            bucket: folder.to_owned(),
        },
    )
}

/// The new storage of `target`: on an existing connection the user picks, or on a new one with
/// credentials typed in holzi's window.
fn add_on_endpoint(
    ctx: &CallContext,
    service: &StorageService,
    name: &str,
    target: Target,
) -> Result<String, BridgeError> {
    let Target {
        protocol,
        kind,
        proposed,
        region,
        addressing,
        bucket,
    } = target;
    let fits = |connection: &crate::remote_storage::model::ConnectionView| {
        proposed
            .as_ref()
            .is_some_and(|p| p.fits(connection, protocol, &region))
    };
    let endpoint_text = proposed
        .as_ref()
        .map_or(String::new(), |p| p.url.to_string());
    let connections = overview(service)?.connections;
    let reusable: Vec<Value> = connections
        .iter()
        .filter(|c| fits(c))
        .map(|c| json!({ "id": c.id, "providerName": c.provider_name }))
        .collect();
    let provider_name = proposed
        .as_ref()
        .and_then(|p| p.url.host_str().map(str::to_owned))
        .unwrap_or_else(|| "AWS".to_owned());
    let mut dialog = open(
        ctx,
        DialogKind::Add,
        &extension_name(ctx),
        json!({
            "name": name,
            "bucket": bucket,
            "protocol": protocol.as_str(),
            "providerName": provider_name,
            "endpoint": endpoint_text,
            "region": region,
            "insecure": proposed.as_ref().is_some_and(|p| address::is_insecure(&p.url)),
            "scope": proposed.as_ref().map_or("public", |p| p.scope.as_str()),
            "connections": reusable,
            "sameProvider": false,
        }),
        &[],
    )?;
    let answer = confirmed(dialog.answer()?)?;
    let storage_name = answer.name.clone().unwrap_or_else(|| name.to_owned());
    let storage_bucket = answer.bucket.clone().unwrap_or(bucket);
    if let Some(connection_id) = answer.connection_id {
        let chosen = connections
            .iter()
            .find(|c| c.id == connection_id)
            .filter(|c| fits(c))
            .ok_or_else(|| invalid("connection does not fit the proposal"))?;
        let saved = block_on(service.save_storage(StorageInput {
            id: None,
            connection_id: chosen.id.clone(),
            name: storage_name,
            bucket: storage_bucket,
            create_folder: answer.create_folder,
        }))
        .map_err(from_service)?;
        return Ok(saved.id);
    }
    let credentials = answer.credentials.ok_or_else(cancelled)?;
    // The scope the dialog showed; AWS by region is public.
    let scope = proposed.map_or(EndpointScope::Public, |p| p.scope);
    let first = (credentials, answer.create_folder);
    let connection = with_credentials(&mut dialog, first, |credentials, create_folder| {
        block_on(service.save_proposed_connection(
            ConnectionInput {
                id: None,
                protocol,
                provider_name: provider_name.clone(),
                provider_kind: kind,
                endpoint: Some(endpoint_text.clone()),
                region: region.clone(),
                addressing,
                credentials: Some(credentials),
                bucket_for_test: storage_bucket.clone(),
                create_folder,
            },
            scope,
        ))
    })?;
    drop(dialog);
    match block_on(service.save_storage(StorageInput {
        id: None,
        connection_id: connection.id.clone(),
        name: storage_name,
        bucket: storage_bucket,
        create_folder: false,
    })) {
        Ok(saved) => Ok(saved.id),
        Err(error) => {
            // Nothing stays from a proposal that did not end in a storage.
            if let Err(cleanup) = block_on(service.remove_connection(&connection.id)) {
                log::warn!("remote storage: connection of a failed proposal stays: {cleanup}");
            }
            Err(from_service(error))
        }
    }
}

#[cfg(test)]
#[path = "remote_storage_propose_tests.rs"]
mod tests;
