//! The file browser's S3 calls besides objects as a whole (spec 044 US5): one level of a bucket
//! with `delimiter=/` and a server-side copy. Kept apart so `s3.rs` stays short.

use reqwest::Method;
use rusty_s3::actions::ListObjectsV2;
use rusty_s3::S3Action;
use tokio::time::Instant;

use super::s3::{bucket, read_body, signing, S3Store, MAX_LIST_PAGE, PAGE, SIGNED_FOR};
use super::{Access, DirListing, ObjectInfo, StorageError};

/// One level below `prefix` with `delimiter=/`, following pages up to `max` entries.
pub(super) async fn list_dir(
    store: &S3Store,
    access: &Access,
    prefix: &str,
    max: usize,
    deadline: Instant,
) -> Result<DirListing, StorageError> {
    let bucket = bucket(access)?;
    let credentials = signing(&access.credentials);
    let mut listing = DirListing::default();
    let mut token: Option<String> = None;
    loop {
        let mut action = bucket.list_objects_v2(Some(&credentials));
        action.with_prefix(prefix.to_owned());
        action.with_delimiter("/");
        action.with_max_keys(PAGE);
        if let Some(token) = token.take() {
            action.with_continuation_token(token);
        }
        let url = action.sign(SIGNED_FOR);
        let response = store
            .send(access, Method::GET, url, None, &[], deadline)
            .await?;
        let body = read_body(response, MAX_LIST_PAGE, deadline).await?;
        let text = std::str::from_utf8(&body).map_err(|_| StorageError::Network)?;
        let page = ListObjectsV2::parse_response(text).map_err(|_| {
            log::warn!("remote storage: a listing did not parse");
            StorageError::Network
        })?;
        listing.objects.extend(
            page.contents
                .into_iter()
                .filter(|object| object.key != prefix)
                .map(|object| ObjectInfo {
                    key: object.key,
                    size: object.size,
                    last_modified: object.last_modified,
                    etag: object.etag,
                }),
        );
        listing
            .prefixes
            .extend(page.common_prefixes.into_iter().map(|common| common.prefix));
        if listing.objects.len() + listing.prefixes.len() > max {
            return Err(StorageError::TooLarge);
        }
        match page.next_continuation_token {
            Some(next) if !next.is_empty() => token = Some(next),
            _ => return Ok(listing),
        }
    }
}

/// A server-side copy: a signed PUT of `to` with a signed `x-amz-copy-source` (rusty-s3 0.10 has
/// no `CopyObject` of its own).
pub(super) async fn copy(
    store: &S3Store,
    access: &Access,
    from: &str,
    to: &str,
    deadline: Instant,
) -> Result<(), StorageError> {
    let bucket = bucket(access)?;
    let credentials = signing(&access.credentials);
    let source = copy_source(&access.location.bucket, from);
    let mut action = bucket.put_object(Some(&credentials), to);
    action
        .headers_mut()
        .insert("x-amz-copy-source", source.clone());
    let url = action.sign(SIGNED_FOR);
    store
        .send(
            access,
            Method::PUT,
            url,
            None,
            &[("x-amz-copy-source", source)],
            deadline,
        )
        .await
        .map(drop)
}

/// The `x-amz-copy-source` of `key` in `bucket`: `/bucket/key`, each part percent-encoded, the
/// slashes of the key kept.
pub fn copy_source(bucket: &str, key: &str) -> String {
    use percent_encoding::{utf8_percent_encode, AsciiSet, NON_ALPHANUMERIC};
    const PART: &AsciiSet = &NON_ALPHANUMERIC
        .remove(b'-')
        .remove(b'_')
        .remove(b'.')
        .remove(b'~')
        .remove(b'/');
    format!(
        "/{}/{}",
        utf8_percent_encode(bucket, PART),
        utf8_percent_encode(key, PART)
    )
}
