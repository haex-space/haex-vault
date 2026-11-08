//! Websites as the rights of spec 065 name them (data-model.md, research R9): an origin of scheme,
//! host and port, written `https://shop.example.de` with the host in lower case (IDN as punycode)
//! and the port only when it is not the default. Addresses with user info, other schemes and
//! hosts the browser cannot reach are refused, so a right never names one.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use serde::{Deserialize, Serialize};
use url::Url;

use crate::mcp_server::grants::{BrowserRights, SiteGrant, SiteLevel};

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
    /// Never: the code an agent sees (`blocked_address`, `site_not_allowed`, `site_blocked`).
    Refuse(&'static str),
}

/// Blank pages and the documents of `srcdoc` frames, which load nothing from anywhere.
const BLANK: [&str; 2] = ["about:blank", "about:srcdoc"];

/// Whether the user blocked `site` (FR-047).
fn blocked(site: &Site, sites: &BTreeMap<Site, SiteGrant>) -> bool {
    sites.get(site).is_some_and(|grant| !grant.level.opens())
}

/// Whether the website of `address` is one the user blocked (FR-047), for the requests a page
/// sends besides its documents. An address that is no website is not blocked here.
pub fn is_blocked(address: &str, sites: &BTreeMap<Site, SiteGrant>) -> bool {
    Site::parse(address).is_ok_and(|site| blocked(&site, sites))
}

/// `address` as a web address: one without a scheme that names a host ("serienstream.to",
/// "example.com/x", "localhost:3000") gets `https://`, anything else stays as it is. A model told
/// to open a site often passes the bare name.
pub fn with_scheme(address: &str) -> String {
    let address = address.trim();
    if address.contains("://") || address.contains(char::is_whitespace) {
        return address.to_owned();
    }
    let host = address.split(['/', '?', '#']).next().unwrap_or_default();
    let (name, port) = host
        .split_once(':')
        .map_or((host, None), |(n, p)| (n, Some(p)));
    let name_ok = !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '.')
        && (name.contains('.') || name.eq_ignore_ascii_case("localhost"));
    let port_ok = port.is_none_or(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_digit()));
    if name_ok && port_ok {
        format!("https://{address}")
    } else {
        address.to_owned()
    }
}

/// The verdict for a document at `address` (a page, a redirect or a frame alike): a website of
/// `sites` that is not blocked, one allowed once in this session, or any website when `all` allows
/// reading or operating (FR-043) loads; a blocked website (FR-047) never does; any other website
/// asks, or is refused in Plan (`ask` false); other schemes, internal pages and addresses with
/// user info never load.
pub fn check_document(
    address: &str,
    sites: &BTreeMap<Site, SiteGrant>,
    all: Option<SiteLevel>,
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
    // A blocked website never loads, not even when the user allowed it once (FR-047).
    if blocked(&site, sites) {
        return Verdict::Refuse("site_blocked");
    }
    let all_opens = all.is_some_and(SiteLevel::opens);
    if sites.contains_key(&site) || once.contains(&site) || all_opens {
        Verdict::Go
    } else if ask {
        Verdict::Ask(site)
    } else {
        Verdict::Refuse("site_not_allowed")
    }
}

/// Whether an agent may operate the page at `address` (FR-008): its website is allowed for
/// operating, or allowed once in this session, or it is not listed and `all` allows operating
/// (FR-043); a website over `http` is only ever read (R9). A listed website is an exception: its
/// own level counts, whatever `all` says.
pub fn may_operate(
    address: &str,
    sites: &BTreeMap<Site, SiteGrant>,
    all: Option<SiteLevel>,
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
        None => once.contains(&site) || all == Some(SiteLevel::Operate),
    }
}

/// "Für diese Website merken" of a fill (FR-021): only a website the agent may operate keeps a
/// fill (data-model.md). One it operates through the level for all websites (FR-043) becomes an
/// exception of its own at operating, so the remembered fill stays whatever `all` becomes later.
/// Whether `rights` changed.
pub fn remember_fill(rights: &mut BrowserRights, site: &Site) -> bool {
    if !may_operate(site.as_str(), &rights.sites, rights.all, &BTreeSet::new()) {
        return false;
    }
    let grant = rights.sites.entry(site.clone()).or_insert(SiteGrant {
        level: SiteLevel::Operate,
        fill_remembered: false,
    });
    let changed = !grant.fill_remembered;
    grant.fill_remembered = true;
    changed
}
