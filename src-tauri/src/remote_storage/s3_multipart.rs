//! Uploads in parts over S3 (spec 044 FR-020, research R5): create, one PUT per part, complete with
//! the parts' ETags, or abort, so a cancelled or failed upload leaves no object and no parts.

use reqwest::Method;
use rusty_s3::actions::CreateMultipartUpload;
use rusty_s3::S3Action;
use tokio::time::Instant;

use super::s3::{bucket, read_body, signing, S3Store, SIGNED_FOR};
use super::{Access, StorageError};

/// The most of an answer of `CreateMultipartUpload` holzi reads.
const MAX_CREATE_ANSWER: usize = 64 * 1024;

pub(super) async fn create(
    store: &S3Store,
    access: &Access,
    key: &str,
    deadline: Instant,
) -> Result<String, StorageError> {
    let bucket = bucket(access)?;
    let credentials = signing(&access.credentials)?;
    let url = bucket
        .create_multipart_upload(Some(&credentials), key)
        .sign(SIGNED_FOR);
    let response = store
        .send(access, Method::POST, url, None, &[], deadline)
        .await?;
    let body = read_body(response, MAX_CREATE_ANSWER, deadline).await?;
    let text = std::str::from_utf8(&body).map_err(|_| StorageError::Network)?;
    let created = CreateMultipartUpload::parse_response(text).map_err(|_| {
        log::warn!("remote storage: a multipart answer did not parse");
        StorageError::Network
    })?;
    Ok(created.upload_id().to_owned())
}

pub(super) async fn upload_part(
    store: &S3Store,
    access: &Access,
    key: &str,
    upload_id: &str,
    number: u16,
    body: Vec<u8>,
    deadline: Instant,
) -> Result<String, StorageError> {
    let bucket = bucket(access)?;
    let credentials = signing(&access.credentials)?;
    let url = bucket
        .upload_part(Some(&credentials), key, number, upload_id)
        .sign(SIGNED_FOR);
    let response = store
        .send(access, Method::PUT, url, Some(body), &[], deadline)
        .await?;
    response
        .headers()
        .get("etag")
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned)
        .ok_or_else(|| {
            log::warn!("remote storage: a part came back without an ETag");
            StorageError::Network
        })
}

pub(super) async fn complete(
    store: &S3Store,
    access: &Access,
    key: &str,
    upload_id: &str,
    etags: &[String],
    deadline: Instant,
) -> Result<(), StorageError> {
    let bucket = bucket(access)?;
    let credentials = signing(&access.credentials)?;
    let action = bucket.complete_multipart_upload(
        Some(&credentials),
        key,
        upload_id,
        etags.iter().map(String::as_str),
    );
    let url = action.sign(SIGNED_FOR);
    let body = action.body().into_bytes();
    store
        .send(access, Method::POST, url, Some(body), &[], deadline)
        .await
        .map(drop)
}

pub(super) async fn abort(
    store: &S3Store,
    access: &Access,
    key: &str,
    upload_id: &str,
    deadline: Instant,
) -> Result<(), StorageError> {
    let bucket = bucket(access)?;
    let credentials = signing(&access.credentials)?;
    let url = bucket
        .abort_multipart_upload(Some(&credentials), key, upload_id)
        .sign(SIGNED_FOR);
    store
        .send(access, Method::DELETE, url, None, &[], deadline)
        .await
        .map(drop)
}
