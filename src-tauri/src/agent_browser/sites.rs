//! Websites as the rights of spec 065 name them (data-model.md, research R9): an origin of scheme,
//! host and port, written `https://shop.example.de` with the host in lower case (IDN as punycode)
//! and the port only when it is not the default. Addresses with user info, other schemes and
//! hosts the browser cannot reach are refused, so a right never names one.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use serde::{Deserialize, Serialize};
use url::Url;

use crate::mcp_server::grants::{SiteGrant, SiteLevel};

/// One website. Ordered and compared by its written form.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Site(String);

/// Why an address is no website.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SiteError {
    #[error("not an address")]
    Unparsable,
    #[error("only https and http websites")]
    Scheme,
    #[error("an address with a user name or password")]
    UserInfo,
    #[error("an address without a host")]
    NoHost,
}

impl Site {
    /// The website of `address`; a path, query or fragment is dropped.
    pub fn parse(address: &str) -> Result<Self, SiteError> {
        let url = Url::parse(address.trim()).map_err(|_| SiteError::Unparsable)?;
        Self::of(&url)
    }

    /// The website of a parsed address.
    pub fn of(url: &Url) -> Result<Self, SiteError> {
        if !matches!(url.scheme(), "https" | "http") {
            return Err(SiteError::Scheme);
        }
        if !url.username().is_empty() || url.password().is_some() {
            return Err(SiteError::UserInfo);
        }
        let host = url
            .host_str()
            .filter(|host| !host.is_empty())
            .ok_or(SiteError::NoHost)?;
        let port = url
            .port()
            .map(|port| format!(":{port}"))
            .unwrap_or_default();
        Ok(Self(format!("{}://{host}{port}", url.scheme())))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Over an unencrypted connection; such a website can only be read (research R9).
    pub fn is_plain_http(&self) -> bool {
        self.0.starts_with("http://")
    }
}

impl TryFrom<String> for Site {
    type Error = SiteError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::parse(&value)
    }
}

impl From<Site> for String {
    fn from(site: Site) -> Self {
        site.0
    }
}

impl fmt::Display for Site {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// What happens with a document an agent tab is about to load (research R9).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    /// Load it.
    Go,
    /// Ask the user whether the agent may open this website (FR-007).
    Ask(Site),
    /// Never: the code an agent sees (`blocked_address`, `site_not_allowed`).
    Refuse(&'static str),
}

/// Blank pages and the documents of `srcdoc` frames, which load nothing from anywhere.
const BLANK: [&str; 2] = ["about:blank", "about:srcdoc"];

/// The verdict for a document at `address` (a page, a redirect or a frame alike): a website of
/// `sites`, or one allowed once in this session, loads; any other website asks, or is refused in
/// Plan (`ask` false); other schemes, internal pages and addresses with user info never load.
pub fn check_document(
    address: &str,
    sites: &BTreeMap<Site, SiteGrant>,
    once: &BTreeSet<Site>,
    ask: bool,
) -> Verdict {
    if BLANK.contains(&address) {
        return Verdict::Go;
    }
    let site = match Url::parse(address)
        .map_err(|_| SiteError::Unparsable)
        .and_then(|url| Site::of(&url))
    {
        Ok(site) => site,
        Err(_) => return Verdict::Refuse("blocked_address"),
    };
    if sites.contains_key(&site) || once.contains(&site) {
        Verdict::Go
    } else if ask {
        Verdict::Ask(site)
    } else {
        Verdict::Refuse("site_not_allowed")
    }
}

/// Whether an agent may operate the page at `address` (FR-008): its website is allowed for
/// operating, or allowed once in this session; a website over `http` is only ever read (R9).
pub fn may_operate(
    address: &str,
    sites: &BTreeMap<Site, SiteGrant>,
    once: &BTreeSet<Site>,
) -> bool {
    let Ok(site) = Site::parse(address) else {
        return false;
    };
    if site.is_plain_http() {
        return false;
    }
    match sites.get(&site) {
        Some(grant) => grant.level == SiteLevel::Operate,
        None => once.contains(&site),
    }
}
