//! Websites as the rights of spec 065 name them (data-model.md, research R9): an origin of scheme,
//! host and port, written `https://shop.example.de` with the host in lower case (IDN as punycode)
//! and the port only when it is not the default. Addresses with user info, other schemes and
//! hosts the browser cannot reach are refused, so a right never names one.

use std::fmt;

use serde::{Deserialize, Serialize};
use url::Url;

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
