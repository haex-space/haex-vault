//! S3 over `rusty-s3` and `reqwest` (spec 038, research R1, R7, R8).
//!
//! `rusty-s3` signs presigned addresses (SigV4) and reads the XML of a listing; `reqwest` with
//! rustls sends them. Each call resolves and checks the host first ([`address::pin`], within the
//! call's deadline) and builds a client that connects to exactly the checked addresses, with no
//! proxy and no redirects (an S3 provider answers with an error, not a redirect, so a redirect is
//! [`StorageError::Network`]).
//! Answers are read in chunks up to their limit and within the one deadline of the call, as in
//! `extensions/web.rs`. Errors name what went wrong, never the endpoint, a header or the provider's
//! text; the log gets the status and the provider's error code.

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use reqwest::{Method, StatusCode, Url};
use rusty_s3::actions::ListObjectsV2;
use rusty_s3::{Bucket, S3Action, UrlStyle};
use tokio::io::AsyncRead;
use tokio::time::Instant;

use super::address::{self, Resolver, SystemResolver};
pub(super) use super::http::read_body;
use super::{
    Access, Addressing, Credentials, DirListing, ObjectHead, ObjectInfo, ObjectStore, StorageError,
};

pub use super::s3_ext::copy_source;

/// How long a signed address stays valid; longer than any deadline of a call.
pub(super) const SIGNED_FOR: Duration = Duration::from_secs(15 * 60);

/// The largest page of a listing holzi reads (1000 keys of at most 1024 bytes and their data).
pub(super) const MAX_LIST_PAGE: usize = 8 * 1024 * 1024;

/// The part of an error answer holzi reads for its code.
const MAX_ERROR_BODY: usize = 64 * 1024;

/// Keys per page holzi asks for (the S3 maximum).
pub(super) const PAGE: usize = 1000;

/// The S3 implementation of [`ObjectStore`].
pub struct S3Store {
    resolver: Arc<dyn Resolver>,
}

impl S3Store {
    /// Creates an S3 provider using `resolver` to check and pin request destinations.
    pub fn new(resolver: Arc<dyn Resolver>) -> Self {
        Self { resolver }
    }

    /// With the resolver of the operating system.
    pub fn system() -> Self {
        Self::new(Arc::new(SystemResolver))
    }

    /// Sends one signed request and returns the answer when its status is a success. `headers`
    /// must be the ones the address was signed with.
    pub(super) async fn send(
        &self,
        access: &Access,
        method: Method,
        url: Url,
        body: Option<Vec<u8>>,
        headers: &[(&str, String)],
        deadline: Instant,
    ) -> Result<reqwest::Response, StorageError> {
        let client = tokio::time::timeout_at(
            deadline,
            super::http::pinned_client(
                &url,
                access.location.endpoint_scope,
                self.resolver.as_ref(),
            ),
        )
        .await
        .map_err(|_| StorageError::TimedOut)??;
        let mut request = client.request(method.clone(), url);
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
                    "remote storage: {method} failed: {}",
                    if error.is_timeout() {
                        "timed out"
                    } else if error.is_connect() {
                        "could not connect"
                    } else {
                        "network error"
                    }
                );
                StorageError::Network
            })?;
        if response.status().is_success() {
            Ok(response)
        } else {
            Err(failure(&method, response, deadline).await)
        }
    }
}

/// Builds the signing bucket from the validated endpoint, region and addressing style.
/// Invalid endpoint or bucket configuration becomes a network error.
pub(super) fn bucket(access: &Access) -> Result<Bucket, StorageError> {
    let location = &access.location;
    let endpoint = address::endpoint_url(location).map_err(|_| StorageError::Network)?;
    let style = match location.addressing {
        Addressing::Path => UrlStyle::Path,
        Addressing::Virtual => UrlStyle::VirtualHost,
    };
    Bucket::new(
        endpoint,
        style,
        location.bucket.clone(),
        location.region.clone(),
    )
    .map_err(|_| StorageError::Network)
}

/// Builds signing credentials, including the session token when present. Credentials of another
/// protocol never reach S3 ([`StorageError::AccessDenied`]).
pub(super) fn signing(credentials: &Credentials) -> Result<rusty_s3::Credentials, StorageError> {
    let Credentials::S3(credentials) = credentials else {
        log::warn!("remote storage: S3 got credentials of another protocol");
        return Err(StorageError::AccessDenied);
    };
    let key = credentials.access_key_id.clone();
    let secret = credentials.secret_access_key.as_str();
    Ok(match &credentials.session_token {
        Some(token) => rusty_s3::Credentials::new_with_token(key, secret, token.as_str()),
        None => rusty_s3::Credentials::new(key, secret),
    })
}

/// The `<Code>` of an S3 error answer, if it is a plain word.
fn error_code(body: &[u8]) -> Option<String> {
    let text = std::str::from_utf8(body).ok()?;
    let start = text.find("<Code>")? + "<Code>".len();
    let end = start + text[start..].find("</Code>")?;
    let code = &text[start..end];
    (!code.is_empty() && code.len() <= 64 && code.bytes().all(|b| b.is_ascii_alphanumeric()))
        .then(|| code.to_owned())
}

/// Maps an answer that is not a success (research R7).
async fn failure(method: &Method, response: reqwest::Response, deadline: Instant) -> StorageError {
    let status = response.status();
    let code = read_body(response, MAX_ERROR_BODY, deadline)
        .await
        .ok()
        .and_then(|body| error_code(&body));
    log::warn!(
        "remote storage: {method} answered {status} ({})",
        code.as_deref().unwrap_or("no code")
    );
    classify(status, code.as_deref())
}

/// The error for an answer with `status` and the provider's error `code`.
pub fn classify(status: StatusCode, code: Option<&str>) -> StorageError {
    match code {
        Some("NoSuchKey" | "NoSuchBucket") => return StorageError::NotFound,
        Some(
            "InvalidAccessKeyId"
            | "SignatureDoesNotMatch"
            | "ExpiredToken"
            | "InvalidToken"
            | "TokenRefreshRequired"
            | "AuthorizationHeaderMalformed"
            | "AuthorizationQueryParametersError",
        ) => return StorageError::AccessDenied,
        Some("AccessDenied") => return StorageError::MissingRight,
        _ => {}
    }
    match status {
        StatusCode::NOT_FOUND => StorageError::NotFound,
        StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => StorageError::AccessDenied,
        _ => StorageError::Network,
    }
}

#[async_trait]
impl ObjectStore for S3Store {
    /// Uploads an object through a signed PUT request using the supplied deadline.
    async fn put(
        &self,
        access: &Access,
        key: &str,
        body: Vec<u8>,
        deadline: Instant,
    ) -> Result<(), StorageError> {
        let bucket = bucket(access)?;
        let credentials = signing(&access.credentials)?;
        let url = bucket.put_object(Some(&credentials), key).sign(SIGNED_FOR);
        self.send(access, Method::PUT, url, Some(body), &[], deadline)
            .await
            .map(drop)
    }

    /// Downloads an object through a signed GET request, bounded by `max_bytes` and `deadline`.
    async fn get(
        &self,
        access: &Access,
        key: &str,
        max_bytes: usize,
        deadline: Instant,
    ) -> Result<Vec<u8>, StorageError> {
        let bucket = bucket(access)?;
        let credentials = signing(&access.credentials)?;
        let url = bucket.get_object(Some(&credentials), key).sign(SIGNED_FOR);
        let response = self
            .send(access, Method::GET, url, None, &[], deadline)
            .await?;
        read_body(response, max_bytes, deadline).await
    }

    /// Follows signed listing pages for `prefix`, rejecting results above `max` or the deadline.
    async fn list(
        &self,
        access: &Access,
        prefix: &str,
        max: usize,
        deadline: Instant,
    ) -> Result<Vec<ObjectInfo>, StorageError> {
        let bucket = bucket(access)?;
        let credentials = signing(&access.credentials)?;
        let mut objects = Vec::new();
        let mut token: Option<String> = None;
        loop {
            let mut action = bucket.list_objects_v2(Some(&credentials));
            action.with_prefix(prefix.to_owned());
            action.with_max_keys(PAGE);
            if let Some(token) = token.take() {
                action.with_continuation_token(token);
            }
            let url = action.sign(SIGNED_FOR);
            let response = self
                .send(access, Method::GET, url, None, &[], deadline)
                .await?;
            let body = read_body(response, MAX_LIST_PAGE, deadline).await?;
            let text = std::str::from_utf8(&body).map_err(|_| StorageError::Network)?;
            let page = ListObjectsV2::parse_response(text).map_err(|_| {
                log::warn!("remote storage: a listing did not parse");
                StorageError::Network
            })?;
            objects.extend(page.contents.into_iter().map(|object| ObjectInfo {
                key: object.key,
                size: object.size,
                last_modified: object.last_modified,
                etag: object.etag,
            }));
            if objects.len() > max {
                return Err(StorageError::TooLarge);
            }
            match page.next_continuation_token {
                Some(next) if !next.is_empty() => token = Some(next),
                _ => return Ok(objects),
            }
        }
    }

    /// Deletes an object through a signed DELETE request using the supplied deadline.
    async fn delete(
        &self,
        access: &Access,
        key: &str,
        deadline: Instant,
    ) -> Result<(), StorageError> {
        let bucket = bucket(access)?;
        let credentials = signing(&access.credentials)?;
        let url = bucket
            .delete_object(Some(&credentials), key)
            .sign(SIGNED_FOR);
        self.send(access, Method::DELETE, url, None, &[], deadline)
            .await
            .map(drop)
    }

    /// Size and time of an object through a signed HEAD request.
    async fn head(
        &self,
        access: &Access,
        key: &str,
        deadline: Instant,
    ) -> Result<ObjectHead, StorageError> {
        let bucket = bucket(access)?;
        let credentials = signing(&access.credentials)?;
        let url = bucket.head_object(Some(&credentials), key).sign(SIGNED_FOR);
        let response = self
            .send(access, Method::HEAD, url, None, &[], deadline)
            .await?;
        let header = |name: &str| {
            response
                .headers()
                .get(name)
                .and_then(|value| value.to_str().ok())
                .map(str::to_owned)
        };
        let size = header("content-length")
            .and_then(|text| text.parse().ok())
            .ok_or(StorageError::Network)?;
        Ok(ObjectHead {
            size,
            last_modified: header("last-modified"),
            etag: header("etag"),
        })
    }

    /// A part of an object through a signed GET with a signed `Range`; the body streams.
    async fn get_range(
        &self,
        access: &Access,
        key: &str,
        start: u64,
        len: u64,
        deadline: Instant,
    ) -> Result<Box<dyn AsyncRead + Send + Unpin>, StorageError> {
        if len == 0 {
            return Ok(Box::new(tokio::io::empty()));
        }
        let bucket = bucket(access)?;
        let credentials = signing(&access.credentials)?;
        let range = format!("bytes={start}-{}", start + len - 1);
        let mut action = bucket.get_object(Some(&credentials), key);
        action.headers_mut().insert("range", range.clone());
        let url = action.sign(SIGNED_FOR);
        let response = self
            .send(
                access,
                Method::GET,
                url,
                None,
                &[("range", range)],
                deadline,
            )
            .await?;
        // A provider that ignores the range answers 200 with the whole object; only a part from
        // the start can be cut from that.
        if response.status() == StatusCode::OK && start > 0 {
            log::warn!("remote storage: a range request was answered with the whole object");
            return Err(StorageError::Network);
        }
        let stream = futures::TryStreamExt::map_err(response.bytes_stream(), |_| {
            std::io::Error::other("the provider's answer broke off")
        });
        let reader = tokio_util::io::StreamReader::new(stream);
        Ok(Box::new(tokio::io::AsyncReadExt::take(reader, len)))
    }

    /// One level below `prefix` with `delimiter=/`, following pages up to `max` entries.
    async fn list_dir(
        &self,
        access: &Access,
        prefix: &str,
        max: usize,
        deadline: Instant,
    ) -> Result<DirListing, StorageError> {
        super::s3_ext::list_dir(self, access, prefix, max, deadline).await
    }

    /// A server-side copy (`s3_ext::copy`).
    async fn copy(
        &self,
        access: &Access,
        from: &str,
        to: &str,
        deadline: Instant,
    ) -> Result<(), StorageError> {
        super::s3_ext::copy(self, access, from, to, deadline).await
    }

    /// S3 copies on the provider (`CopyObject`).
    fn copies_on_provider(&self, _access: &Access) -> bool {
        true
    }

    /// Never called for S3: folders are prefixes, moved object by object (spec 050 research R8).
    async fn rename_tree(
        &self,
        _access: &Access,
        _from: &str,
        _to: &str,
        _deadline: Instant,
    ) -> Result<(), StorageError> {
        Err(StorageError::Unsupported)
    }

    /// Never called for S3: folders are prefixes, deleted object by object (spec 050 research R8).
    async fn remove_tree(
        &self,
        _access: &Access,
        _prefix: &str,
        _deadline: Instant,
    ) -> Result<(), StorageError> {
        Err(StorageError::Unsupported)
    }

    async fn begin_upload(
        &self,
        access: &Access,
        key: &str,
        deadline: Instant,
    ) -> Result<String, StorageError> {
        super::s3_multipart::create(self, access, key, deadline).await
    }

    async fn upload_part(
        &self,
        access: &Access,
        key: &str,
        upload_id: &str,
        number: u16,
        body: Vec<u8>,
        deadline: Instant,
    ) -> Result<String, StorageError> {
        super::s3_multipart::upload_part(self, access, key, upload_id, number, body, deadline).await
    }

    async fn finish_upload(
        &self,
        access: &Access,
        key: &str,
        upload_id: &str,
        marks: &[String],
        deadline: Instant,
    ) -> Result<(), StorageError> {
        super::s3_multipart::complete(self, access, key, upload_id, marks, deadline).await
    }

    async fn abort_upload(
        &self,
        access: &Access,
        key: &str,
        upload_id: &str,
        deadline: Instant,
    ) -> Result<(), StorageError> {
        super::s3_multipart::abort(self, access, key, upload_id, deadline).await
    }
}
