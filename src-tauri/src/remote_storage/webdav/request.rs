//! The requests of the WebDAV client (spec 050 research R2): addresses below the folder of a
//! storage, Basic authentication, the pinned client without redirects, and the status codes as the
//! few errors of [`StorageError`]. The log gets the method and the status, never an address, a
//! header or an answer.

use reqwest::{Method, StatusCode, Url};
use tokio::time::Instant;

use super::super::address::Resolver;
use super::super::http::pinned_client;
use super::super::{Access, Credentials, Location, StorageError};

/// The properties every `PROPFIND` asks for.
pub(super) const PROPFIND_BODY: &str = "<?xml version=\"1.0\" encoding=\"utf-8\"?>\
<d:propfind xmlns:d=\"DAV:\"><d:prop><d:resourcetype/><d:getcontentlength/>\
<d:getlastmodified/><d:getetag/></d:prop></d:propfind>";

/// The `PROPFIND` that asks for the quota of a folder (RFC 4331).
pub(super) const QUOTA_BODY: &str = "<?xml version=\"1.0\" encoding=\"utf-8\"?>\
<d:propfind xmlns:d=\"DAV:\"><d:prop><d:quota-used-bytes/><d:quota-available-bytes/>\
</d:prop></d:propfind>";

/// A WebDAV method by name.
pub(super) fn method(name: &'static str) -> Method {
    Method::from_bytes(name.as_bytes()).expect("WebDAV method names are valid tokens")
}

/// The address of `key` below the folder of `location`: each part percent-encoded, a key ending
/// in `/` (or an empty key, the folder itself) with a `/` at the end, so no server redirects. A
/// part `.` or `..` would lead out of the folder and reaches nothing ([`StorageError::NotFound`]);
/// callers refuse such keys before (spec 038 research R5).
pub(super) fn url_of(location: &Location, key: &str) -> Result<Url, StorageError> {
    let dots = |part: &str| part == "." || part == "..";
    if location.bucket.split('/').any(dots) || key.split('/').any(dots) {
        log::warn!("remote storage: a WebDAV key with `.` or `..` was refused");
        return Err(StorageError::NotFound);
    }
    let mut url = Url::parse(&location.endpoint).map_err(|_| StorageError::Network)?;
    let folder = location.bucket.split('/').filter(|part| !part.is_empty());
    let parts = key.split('/').filter(|part| !part.is_empty());
    {
        let mut segments = url.path_segments_mut().map_err(|_| StorageError::Network)?;
        segments.pop_if_empty();
        segments.extend(folder);
        segments.extend(parts);
        if key.is_empty() || key.ends_with('/') {
            segments.push("");
        }
    }
    Ok(url)
}

/// The decoded path of `url`, as [`super::multistatus::parse`] compares it.
pub(super) fn decoded_path(url: &Url) -> String {
    percent_encoding::percent_decode_str(url.path())
        .decode_utf8_lossy()
        .into_owned()
}

/// The error of a status that is not a success (research R2).
pub(super) fn status_error(method: &Method, status: StatusCode) -> StorageError {
    log::warn!(
        "remote storage: WebDAV {method} answered {}",
        status.as_u16()
    );
    match status.as_u16() {
        401 => StorageError::AccessDenied,
        403 | 423 => StorageError::MissingRight,
        404 | 409 | 410 => StorageError::NotFound,
        405 => StorageError::Protocol,
        413 => StorageError::TooLarge,
        507 => StorageError::QuotaExceeded,
        _ => StorageError::Network,
    }
}

/// One request to the server of `access`, before `deadline`. Every answer but a redirect comes
/// back, so the caller can look at statuses like 405 or 409 itself; a redirect is
/// [`StorageError::Protocol`] (no redirect is followed).
pub(super) async fn send_raw(
    resolver: &dyn Resolver,
    access: &Access,
    method: Method,
    url: Url,
    headers: &[(&str, String)],
    body: Option<reqwest::Body>,
    deadline: Instant,
) -> Result<reqwest::Response, StorageError> {
    let Credentials::Password(login) = &access.credentials else {
        log::warn!("remote storage: WebDAV got credentials of another kind");
        return Err(StorageError::AccessDenied);
    };
    let client = tokio::time::timeout_at(
        deadline,
        pinned_client(&url, access.location.endpoint_scope, resolver),
    )
    .await
    .map_err(|_| StorageError::TimedOut)??;
    let mut request = client
        .request(method.clone(), url)
        .basic_auth(&login.username, Some(login.password.as_str()));
    for (name, value) in headers {
        request = request.header(*name, value);
    }
    if let Some(body) = body {
        request = request.body(body);
    }
    let response = tokio::time::timeout_at(deadline, request.send())
        .await
        .map_err(|_| StorageError::TimedOut)?
        .map_err(|error| {
            log::warn!(
                "remote storage: WebDAV {method} failed: {}",
                error.without_url()
            );
            StorageError::Network
        })?;
    if response.status().is_redirection() {
        log::warn!("remote storage: WebDAV {method} was redirected; holzi follows no redirect");
        return Err(StorageError::Protocol);
    }
    Ok(response)
}

/// [`send_raw`] that only lets a success through.
pub(super) async fn send(
    resolver: &dyn Resolver,
    access: &Access,
    method: Method,
    url: Url,
    headers: &[(&str, String)],
    body: Option<reqwest::Body>,
    deadline: Instant,
) -> Result<reqwest::Response, StorageError> {
    let response = send_raw(
        resolver,
        access,
        method.clone(),
        url,
        headers,
        body,
        deadline,
    )
    .await?;
    if response.status().is_success() {
        Ok(response)
    } else {
        Err(status_error(&method, response.status()))
    }
}
