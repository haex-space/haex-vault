//! The answer of a `PROPFIND` (RFC 4918 §9.1, spec 050 research R2): a `multistatus` with one
//! `response` per resource. Only elements of the namespace `DAV:` count, whatever prefix a server
//! gives them; properties in a `propstat` whose status is not 200 are ignored. Each `href` is
//! decoded and must lie at or below the requested path, otherwise the entry is dropped and logged
//! (a server must not lead holzi to other paths).

use std::borrow::Cow;

use percent_encoding::percent_decode_str;
use quick_xml::escape::resolve_predefined_entity;
use quick_xml::events::Event;
use quick_xml::name::ResolveResult;
use quick_xml::NsReader;
use reqwest::Url;

use crate::remote_storage::StorageError;

const DAV: &[u8] = b"DAV:";

/// One resource of a `PROPFIND` answer, relative to the requested path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Entry {
    /// The path below the requested one, decoded, without a `/` at either end; empty for the
    /// requested resource itself.
    pub name: String,
    pub is_dir: bool,
    pub size: u64,
    /// `getlastmodified` as sent (an HTTP date).
    pub modified: Option<String>,
    /// `getetag` without quotes and without a weak mark.
    pub etag: Option<String>,
    /// `quota-used-bytes` (RFC 4331), when sent.
    pub quota_used: Option<u64>,
    /// `quota-available-bytes` (RFC 4331) as sent: negative where a server means "unlimited" or
    /// "unknown" (Nextcloud sends -3).
    pub quota_available: Option<i64>,
}

/// The DAV element a text belongs to.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Field {
    Href,
    Status,
    Length,
    Modified,
    Etag,
    QuotaUsed,
    QuotaAvailable,
}

/// What one `response` collected so far.
#[derive(Default)]
struct Response {
    href: String,
    is_dir: bool,
    size: Option<u64>,
    modified: Option<String>,
    etag: Option<String>,
    quota_used: Option<u64>,
    quota_available: Option<i64>,
}

/// The properties of one `propstat`, kept only when its status is 200.
#[derive(Default)]
struct PropStat {
    status: String,
    is_dir: bool,
    size: Option<u64>,
    modified: Option<String>,
    etag: Option<String>,
    quota_used: Option<u64>,
    quota_available: Option<i64>,
}

/// The decoded path of `href` (a path or a full address).
fn href_path(href: &str) -> Option<String> {
    let href = href.trim();
    let path = if href.starts_with('/') {
        href.to_owned()
    } else {
        Url::parse(href).ok()?.path().to_owned()
    };
    percent_decode_str(&path)
        .decode_utf8()
        .ok()
        .map(Cow::into_owned)
}

/// `etag` without the weak mark `W/` and the quotes.
fn bare_etag(etag: &str) -> String {
    let etag = etag.trim();
    let etag = etag.strip_prefix("W/").unwrap_or(etag);
    etag.trim_matches('"').to_owned()
}

/// The entries of the `multistatus` `xml`, answered for the decoded path `requested` (with a `/` at
/// the end for a folder). An answer that does not parse is [`StorageError::Protocol`].
pub(super) fn parse(xml: &str, requested: &str) -> Result<Vec<Entry>, StorageError> {
    fn unparsable<E>(_: E) -> StorageError {
        log::warn!("remote storage: a WebDAV listing did not parse");
        StorageError::Protocol
    }
    let mut reader = NsReader::from_str(xml);
    let mut responses = Vec::new();
    let mut response: Option<Response> = None;
    let mut propstat: Option<PropStat> = None;
    let mut in_resourcetype = false;
    let mut field: Option<Field> = None;
    let mut text = String::new();
    loop {
        let (namespace, event) = reader.read_resolved_event().map_err(unparsable)?;
        let dav = matches!(namespace, ResolveResult::Bound(ns) if ns.0 == DAV);
        match event {
            // An empty element carries no value; only `<collection/>` matters among them.
            Event::Empty(start) if dav => {
                if in_resourcetype && start.local_name().as_ref() == b"collection" {
                    if let Some(stat) = propstat.as_mut() {
                        stat.is_dir = true;
                    }
                }
            }
            Event::Start(start) if dav => match start.local_name().as_ref() {
                b"response" => response = Some(Response::default()),
                b"propstat" => propstat = Some(PropStat::default()),
                b"resourcetype" => in_resourcetype = true,
                b"collection" if in_resourcetype => {
                    if let Some(stat) = propstat.as_mut() {
                        stat.is_dir = true;
                    }
                }
                name => {
                    field = match name {
                        b"href" => Some(Field::Href),
                        b"status" => Some(Field::Status),
                        b"getcontentlength" => Some(Field::Length),
                        b"getlastmodified" => Some(Field::Modified),
                        b"getetag" => Some(Field::Etag),
                        b"quota-used-bytes" => Some(Field::QuotaUsed),
                        b"quota-available-bytes" => Some(Field::QuotaAvailable),
                        _ => None,
                    };
                    text.clear();
                }
            },
            Event::Text(t) if field.is_some() => {
                text.push_str(&t.xml10_content().map_err(unparsable)?)
            }
            Event::CData(t) if field.is_some() => {
                text.push_str(&String::from_utf8_lossy(&t.into_inner()))
            }
            Event::GeneralRef(r) if field.is_some() => {
                if let Ok(Some(c)) = r.resolve_char_ref() {
                    text.push(c);
                } else if let Some(resolved) =
                    resolve_predefined_entity(&r.decode().map_err(unparsable)?)
                {
                    text.push_str(resolved);
                }
            }
            Event::End(end) if dav => match end.local_name().as_ref() {
                b"resourcetype" => in_resourcetype = false,
                b"propstat" => {
                    if let (Some(stat), Some(resp)) = (propstat.take(), response.as_mut()) {
                        if stat.status.split_whitespace().nth(1) == Some("200") {
                            resp.is_dir |= stat.is_dir;
                            resp.size = resp.size.or(stat.size);
                            resp.modified = resp.modified.take().or(stat.modified);
                            resp.etag = resp.etag.take().or(stat.etag);
                            resp.quota_used = resp.quota_used.or(stat.quota_used);
                            resp.quota_available = resp.quota_available.or(stat.quota_available);
                        }
                    }
                }
                b"response" => {
                    if let Some(resp) = response.take() {
                        responses.push(resp);
                    }
                }
                _ => {
                    let value = std::mem::take(&mut text);
                    match (field.take(), propstat.as_mut(), response.as_mut()) {
                        (Some(Field::Href), _, Some(resp)) => resp.href = value,
                        (Some(Field::Status), Some(stat), _) => stat.status = value,
                        (Some(Field::Length), Some(stat), _) => {
                            stat.size = value.trim().parse().ok();
                        }
                        (Some(Field::Modified), Some(stat), _) => {
                            stat.modified = Some(value.trim().to_owned());
                        }
                        (Some(Field::Etag), Some(stat), _) => stat.etag = Some(bare_etag(&value)),
                        (Some(Field::QuotaUsed), Some(stat), _) => {
                            stat.quota_used = value.trim().parse().ok();
                        }
                        (Some(Field::QuotaAvailable), Some(stat), _) => {
                            stat.quota_available = value.trim().parse().ok();
                        }
                        _ => {}
                    }
                }
            },
            Event::Eof => break,
            _ => {}
        }
    }
    let requested = requested.trim_end_matches('/');
    let mut entries = Vec::new();
    for resp in responses {
        let Some(path) = href_path(&resp.href) else {
            log::warn!("remote storage: a WebDAV entry without a readable href was dropped");
            continue;
        };
        let path = path.trim_end_matches('/');
        let name = if path == requested {
            ""
        } else if let Some(rest) = path
            .strip_prefix(requested)
            .and_then(|rest| rest.strip_prefix('/'))
        {
            rest
        } else {
            log::warn!("remote storage: a WebDAV entry outside the requested path was dropped");
            continue;
        };
        entries.push(Entry {
            name: name.to_owned(),
            is_dir: resp.is_dir,
            size: resp.size.unwrap_or(0),
            modified: resp.modified,
            etag: resp.etag,
            quota_used: resp.quota_used,
            quota_available: resp.quota_available,
        });
    }
    Ok(entries)
}
