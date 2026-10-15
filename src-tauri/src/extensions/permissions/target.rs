//! Targets of permissions and how a request matches them (contracts/permissions.md §Arten).
//!
//! Every kind has its own target form. A path matches by whole path components and a domain at a
//! label boundary, so `/a/docs` never covers `/a/docs-private` and `example.org` never covers
//! `badexample.org`. A request target is always the resolved one (a canonical path, a parsed URL);
//! resolving is the caller's job.

use std::path::{Component, Path, PathBuf};

use super::model::PermissionKind;
use crate::extensions::ids::{ExtensionName, ExtensionTable, PublicKey, TablePrefix};
use crate::passwords::ids::fold;

/// The target of a stored permission.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
    /// `*`: everything of this kind (not for `database`).
    Any,
    /// All tables of one extension: `<publicKey>__<name>__*`.
    ExtensionTables(TablePrefix),
    /// One table of one extension.
    ExtensionTable(ExtensionTable),
    /// A file, or a folder with everything below it.
    Path(PathBuf),
    /// `scheme://host[:port]/path[*]`.
    Url(UrlPattern),
    /// A domain without scheme: the domain itself and its subdomains, or with `*.` only the
    /// subdomains.
    Domain(HostPattern),
    /// A mail server, on one port or on all.
    MailServer { host: String, port: Option<u16> },
    /// A program, by its canonical path.
    Program(PathBuf),
    /// A tag of the password manager or a group of the address book, by name, compared in its
    /// folded form.
    Tag(String),
    /// A remote storage, by its id; for the action `add` the host of an endpoint
    /// (`host:port`, `host`, an IPv6 address as `[addr]` or `[addr]:port`), matched like a mail
    /// server (spec 038 FR-009b).
    StorageId(String),
    /// One encrypted folder: `<storage_id>/<folder_id>` (spec 048, contracts/bridge.md). Never a
    /// wildcard: a grant names exactly one folder.
    EncryptedFolder(FolderRef),
    /// An agent tool, by its manifest name (`^[a-z][a-z0-9_]{0,31}$`).
    Tool(String),
}

/// An encrypted folder of a storage: the storage's id and the Base32 id of the folder.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct FolderRef {
    pub storage_id: String,
    pub folder_id: String,
}

impl FolderRef {
    /// `<storage_id>/<folder_id>`: a storage id (`device:/` holds a `/` itself) and a folder id in
    /// the spelling of [`encode_id`](crate::files::encrypted::format::encode_id).
    pub fn parse(value: &str) -> Option<Self> {
        // The folder id never holds a `/`; a storage id may (`device:/`, spec 048 US7).
        let (storage_id, folder_id) = value.rsplit_once('/')?;
        let valid_storage = !storage_id.is_empty()
            && storage_id.len() <= 128
            && storage_id.chars().all(|c| !c.is_control() && c != '*');
        (valid_storage && crate::files::encrypted::format::decode_id(folder_id).is_some()).then(
            || Self {
                storage_id: storage_id.to_owned(),
                folder_id: folder_id.to_owned(),
            },
        )
    }

    /// The stored form, the inverse of [`FolderRef::parse`].
    pub fn target(&self) -> String {
        format!("{}/{}", self.storage_id, self.folder_id)
    }
}

/// How a host part of a target matches.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HostPattern {
    /// Exactly this host.
    Exact(String),
    /// Only subdomains of this domain (`*.example.org`).
    Subdomains(String),
    /// The domain and its subdomains (`example.org`).
    DomainAndSubdomains(String),
}

/// A URL pattern of a `web` permission.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UrlPattern {
    pub scheme: String,
    pub host: HostPattern,
    /// `None`: only the default port of the scheme.
    pub port: Option<u16>,
    pub path: String,
    /// `true`: the pattern ended in `*` and matches every path starting with [`Self::path`].
    pub path_prefix: bool,
}

/// What a request is about, already resolved.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RequestTarget {
    /// For kinds without targets (`notifications`).
    Any,
    Table(ExtensionTable),
    Path(PathBuf),
    Url(WebRequest),
    MailServer {
        host: String,
        port: u16,
    },
    Program(PathBuf),
    Tag(String),
    StorageId(String),
    /// The host of a storage endpoint an extension proposes (spec 038 FR-009b).
    Endpoint {
        host: String,
        port: u16,
    },
    EncryptedFolder(FolderRef),
}

/// A parsed http(s) request URL.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WebRequest {
    pub scheme: String,
    /// Lower case, without a trailing dot.
    pub host: String,
    /// `None` for the default port of the scheme.
    pub port: Option<u16>,
    pub path: String,
}

impl WebRequest {
    /// Parses an `http` or `https` URL; anything else is not a web request.
    pub fn parse(value: &str) -> Option<Self> {
        let url = reqwest::Url::parse(value).ok()?;
        if !matches!(url.scheme(), "http" | "https") {
            return None;
        }
        Some(Self {
            scheme: url.scheme().to_owned(),
            host: canonical_host(url.host_str()?),
            port: url.port(),
            path: url.path().to_owned(),
        })
    }
}

impl Target {
    /// Parses the target of `kind`. A target the kind does not accept is absent (FR-022); a
    /// `database` target that is not an extension prefix or table is never valid.
    pub fn parse(kind: PermissionKind, value: &str) -> Option<Self> {
        use PermissionKind as K;
        match kind {
            K::Database => parse_database(value),
            K::Filesystem if value == "*" => Some(Self::Any),
            K::Filesystem => {
                let path = value.strip_suffix("/*").unwrap_or(value);
                clean_absolute(path).map(Self::Path)
            }
            K::Web if value == "*" => Some(Self::Any),
            K::Web if value.contains("://") => parse_url_pattern(value).map(Self::Url),
            K::Web => parse_domain(value).map(Self::Domain),
            K::Notifications => (value == "*").then_some(Self::Any),
            K::Passwords if value == "*" => Some(Self::Any),
            K::Passwords => {
                let tag = fold(value);
                (!tag.is_empty()).then_some(Self::Tag(tag))
            }
            K::RemoteStorage if value == "*" => Some(Self::Any),
            K::RemoteStorage => (!value.is_empty()).then(|| Self::StorageId(value.to_owned())),
            K::Mail if value == "*" => Some(Self::Any),
            K::Mail => parse_mail_server(value),
            K::Shell if value == "*" => Some(Self::Any),
            K::Shell => clean_absolute(value).map(Self::Program),
            K::EncryptedFolder => FolderRef::parse(value).map(Self::EncryptedFolder),
            K::Contacts if value == "*" => Some(Self::Any),
            K::Contacts => {
                let group = fold(value);
                (!group.is_empty()).then_some(Self::Tag(group))
            }
            K::AgentTool if value == "*" => Some(Self::Any),
            K::AgentTool => is_tool_name(value).then(|| Self::Tool(value.to_owned())),
        }
    }

    /// Whether `request` falls under this target. Targets and requests of different kinds never
    /// match.
    pub fn matches(&self, request: &RequestTarget) -> bool {
        match (self, request) {
            (Self::Any, RequestTarget::Table(_) | RequestTarget::EncryptedFolder(_)) => false,
            (Self::Any, _) => true,
            (Self::ExtensionTables(prefix), RequestTarget::Table(table)) => &table.prefix == prefix,
            (Self::ExtensionTable(own), RequestTarget::Table(table)) => own == table,
            (Self::Path(root), RequestTarget::Path(path)) => path.starts_with(root),
            (Self::Url(pattern), RequestTarget::Url(url)) => pattern.matches(url),
            (Self::Domain(host), RequestTarget::Url(url)) => host.matches(&url.host),
            (
                Self::MailServer { host, port },
                RequestTarget::MailServer {
                    host: asked,
                    port: asked_port,
                },
            ) => {
                host.eq_ignore_ascii_case(&canonical_host(asked))
                    && port.is_none_or(|port| port == *asked_port)
            }
            (Self::Program(program), RequestTarget::Program(asked)) => program == asked,
            (Self::Tag(tag), RequestTarget::Tag(asked)) => *tag == fold(asked),
            (Self::StorageId(id), RequestTarget::StorageId(asked)) => id == asked,
            (Self::EncryptedFolder(own), RequestTarget::EncryptedFolder(asked)) => own == asked,
            (Self::StorageId(host), RequestTarget::Endpoint { host: asked, port }) => {
                parse_endpoint_host(host).is_some_and(|server| {
                    server.matches(&RequestTarget::MailServer {
                        host: asked.clone(),
                        port: *port,
                    })
                })
            }
            _ => false,
        }
    }
}

/// A tool name as the manifest's `tools` block allows it: `^[a-z][a-z0-9_]{0,31}$`.
fn is_tool_name(value: &str) -> bool {
    let mut bytes = value.bytes();
    bytes.next().is_some_and(|b| b.is_ascii_lowercase())
        && value.len() <= 32
        && bytes.all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
}

impl HostPattern {
    pub fn matches(&self, host: &str) -> bool {
        let host = canonical_host(host);
        let below = |domain: &str| {
            host.strip_suffix(domain)
                .is_some_and(|rest| rest.ends_with('.') && rest.len() > 1)
        };
        match self {
            Self::Exact(exact) => host == *exact,
            Self::Subdomains(domain) => below(domain),
            Self::DomainAndSubdomains(domain) => host == *domain || below(domain),
        }
    }
}

impl UrlPattern {
    pub fn matches(&self, url: &WebRequest) -> bool {
        self.scheme == url.scheme
            && self.host.matches(&url.host)
            && self.port == url.port
            && if self.path_prefix {
                url.path.starts_with(&self.path)
            } else {
                url.path == self.path
            }
    }
}

/// `<publicKey>__<name>__*` or one table of an extension.
fn parse_database(value: &str) -> Option<Target> {
    if let Some(base) = value.strip_suffix("__*") {
        let (key, name) = base.split_once("__")?;
        if name.contains("__") {
            return None;
        }
        let public_key = PublicKey::parse_case_insensitive(key).ok()?;
        let name = ExtensionName::parse(&name.to_ascii_lowercase()).ok()?;
        return Some(Target::ExtensionTables(TablePrefix { public_key, name }));
    }
    ExtensionTable::parse(value)
        .ok()
        .map(Target::ExtensionTable)
}

/// An absolute path without `.` or `..` components.
fn clean_absolute(value: &str) -> Option<PathBuf> {
    let path = Path::new(value);
    let clean = path.is_absolute()
        && path
            .components()
            .all(|c| !matches!(c, Component::CurDir | Component::ParentDir));
    clean.then(|| path.to_path_buf())
}

/// A requested host as it is compared: lower case, without the trailing dot of a fully qualified
/// name, so `example.org.` (the same server) never escapes a rule for `example.org`.
fn canonical_host(host: &str) -> String {
    let host = host.to_ascii_lowercase();
    match host.strip_suffix('.') {
        Some(stripped) => stripped.to_owned(),
        None => host,
    }
}

/// A host name: lower-case letters, digits, `-` and `.`, no empty label.
fn valid_host(host: &str) -> bool {
    !host.is_empty()
        && host.split('.').all(|label| {
            !label.is_empty()
                && label
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        })
}

fn parse_host_pattern(value: &str, bare: bool) -> Option<HostPattern> {
    let value = value.to_ascii_lowercase();
    if let Some(domain) = value.strip_prefix("*.") {
        return valid_host(domain).then(|| HostPattern::Subdomains(domain.to_owned()));
    }
    if !valid_host(&value) {
        return None;
    }
    Some(if bare {
        HostPattern::DomainAndSubdomains(value)
    } else {
        HostPattern::Exact(value)
    })
}

fn parse_domain(value: &str) -> Option<HostPattern> {
    parse_host_pattern(value, true)
}

fn parse_url_pattern(value: &str) -> Option<UrlPattern> {
    let (scheme, rest) = value.split_once("://")?;
    if !matches!(scheme, "http" | "https") {
        return None;
    }
    let (authority, path) = match rest.find('/') {
        Some(index) => (&rest[..index], &rest[index..]),
        None => (rest, "/"),
    };
    let (host, port) = match authority.rsplit_once(':') {
        Some((host, port)) => (host, Some(port.parse::<u16>().ok()?)),
        None => (authority, None),
    };
    let default_port = if scheme == "https" { 443 } else { 80 };
    let (path, path_prefix) = match path.strip_suffix('*') {
        Some(prefix) => (prefix, true),
        None => (path, false),
    };
    Some(UrlPattern {
        scheme: scheme.to_owned(),
        host: parse_host_pattern(host, false)?,
        port: port.filter(|port| *port != default_port),
        path: path.to_owned(),
        path_prefix,
    })
}

/// The host of a storage endpoint (`add`): like a mail server, or an IPv6 address in brackets
/// (`[fd00::1]`, `[fd00::1]:9000`) in the form the request names it.
fn parse_endpoint_host(value: &str) -> Option<Target> {
    let Some(rest) = value.strip_prefix('[') else {
        return parse_mail_server(value);
    };
    let (ip, port) = rest.split_once(']')?;
    let port = match port {
        "" => None,
        port => Some(port.strip_prefix(':')?.parse::<u16>().ok()?),
    };
    let ip: std::net::Ipv6Addr = ip.parse().ok()?;
    Some(Target::MailServer {
        host: format!("[{ip}]"),
        port,
    })
}

fn parse_mail_server(value: &str) -> Option<Target> {
    let (host, port) = match value.rsplit_once(':') {
        Some((host, port)) => (host, Some(port.parse::<u16>().ok()?)),
        None => (value, None),
    };
    let host = host.to_ascii_lowercase();
    valid_host(&host).then_some(Target::MailServer { host, port })
}
