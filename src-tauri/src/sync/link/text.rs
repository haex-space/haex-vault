//! The link text a main device shows as a QR code and as a link to copy, and the one parser that
//! reads it (spec 085, contracts/link-text.md).
//!
//! `haex-vault://link?v=1&c=<code>[&n=<vault name>][&nr=<url>]…[&ir=<url>]…[&off=<url>]…`
//!
//! The code is the only secret in it. The servers are the ones the user added and the ones
//! switched off; the built-in servers are implied. A plain code, as typed or shown by an older
//! main device, reads as a text without servers.

use url::Url;

use crate::sync::link::code::LinkCode;
use crate::sync::servers::{self, ServerConfig, MAX_SERVERS};

/// The version of the text this build writes and reads.
pub const TEXT_VERSION: u32 = 1;
/// What every link text starts with, scheme and host.
const PREFIX: &str = "haex-vault://link";
/// Longest text the parser looks at: enough for 60 URLs of 256 UTF-8 bytes after percent-encoding,
/// the name and the link syntax.
const MAX_TEXT_BYTES: usize = 49_152;
/// Longest vault name a text carries, in characters.
pub const MAX_NAME_CHARS: usize = 64;
/// Most switched-off servers a text carries (both kinds, built-in or added).
const MAX_OFF: usize = 4 * MAX_SERVERS;
/// Longest text that is shown as a QR code: above it the modules get too small to read from a
/// laptop screen (research R7). The text itself is never shortened.
pub const QR_MAX_TEXT_BYTES: usize = 1_200;

/// Why a text is not a link text this build can read.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum LinkTextError {
    #[error("this is not the code of a vault")]
    NotALinkCode,
    #[error("this text comes from a newer version")]
    UnsupportedVersion,
}

/// A link text read in.
#[derive(Clone)]
pub struct ParsedLinkText {
    pub code: LinkCode,
    /// A suggestion for the name of the new vault.
    pub vault_name: Option<String>,
    /// `None` when the text names no server: the built-in ones stay.
    pub servers: Option<ServerConfig>,
    /// Servers left out because they were no valid address or over a limit.
    pub dropped_servers: u32,
}

impl std::fmt::Debug for ParsedLinkText {
    /// Formats without the code.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ParsedLinkText")
            .field("vault_name", &self.vault_name)
            .field("servers", &self.servers)
            .field("dropped_servers", &self.dropped_servers)
            .finish_non_exhaustive()
    }
}

/// Whether `text` is small enough to show as a QR code.
pub fn fits_qr(text: &str) -> bool {
    text.len() <= QR_MAX_TEXT_BYTES
}

/// The QR code of `text` as SVG, or `None` when the text is too long to scan reliably: the view
/// then offers the link to copy instead. The renderer is the contact card's, so there is one.
pub fn qr_svg(text: &str) -> Option<String> {
    if !fits_qr(text) {
        return None;
    }
    crate::contacts::card::format::qr_svg(text).ok()
}

/// The link text of `code`, the vault's name (cut to [`MAX_NAME_CHARS`] at a character
/// boundary) and the servers of `config`. Nothing else goes in: no passphrase, no key.
pub fn build(code: &LinkCode, vault_name: &str, config: &ServerConfig) -> String {
    let mut query = url::form_urlencoded::Serializer::new(String::new());
    query.append_pair("v", &TEXT_VERSION.to_string());
    query.append_pair("c", &code.display().replace('-', ""));
    let name: String = vault_name.chars().take(MAX_NAME_CHARS).collect();
    if !name.is_empty() {
        query.append_pair("n", &name);
    }
    for (key, urls) in [
        ("nr", &config.nostr_relays),
        ("ir", &config.iroh_relays),
        ("off", &config.disabled),
    ] {
        for url in urls {
            query.append_pair(key, url);
        }
    }
    format!("{PREFIX}?{}", query.finish())
}

/// Reads a link text, or a plain code.
pub fn parse(input: &str) -> Result<ParsedLinkText, LinkTextError> {
    let text = input.trim();
    if text.is_empty() || text.len() > MAX_TEXT_BYTES {
        return Err(LinkTextError::NotALinkCode);
    }
    let is_link = text
        .get(..PREFIX.len())
        .is_some_and(|start| start.eq_ignore_ascii_case(PREFIX));
    if !is_link {
        let code = LinkCode::parse(text).map_err(|_| LinkTextError::NotALinkCode)?;
        return Ok(ParsedLinkText {
            code,
            vault_name: None,
            servers: None,
            dropped_servers: 0,
        });
    }
    parse_link(text)
}

fn parse_link(text: &str) -> Result<ParsedLinkText, LinkTextError> {
    let url = Url::parse(text).map_err(|_| LinkTextError::NotALinkCode)?;
    if !url.username().is_empty()
        || url.password().is_some()
        || url.port().is_some()
        || !url.path().is_empty()
        || url.fragment().is_some()
        || !url
            .host_str()
            .is_some_and(|host| host.eq_ignore_ascii_case("link"))
    {
        return Err(LinkTextError::NotALinkCode);
    }
    let pairs: Vec<(String, String)> = url
        .query_pairs()
        .map(|(key, value)| (key.into_owned(), value.into_owned()))
        .collect();
    let all = |key: &str| -> Vec<&str> {
        pairs
            .iter()
            .filter(|(k, _)| k == key)
            .map(|(_, v)| v.as_str())
            .collect()
    };

    // The version first: a text of a later version may differ in everything else.
    let version = match all("v").as_slice() {
        [one] => one.parse::<u32>().ok(),
        _ => None,
    }
    .ok_or(LinkTextError::NotALinkCode)?;
    if version != TEXT_VERSION {
        return Err(LinkTextError::UnsupportedVersion);
    }

    let code = match all("c").as_slice() {
        [one]
            if one.len() == 26
                && one
                    .bytes()
                    .all(|byte| byte.is_ascii_uppercase() || (b'2'..=b'7').contains(&byte)) =>
        {
            LinkCode::parse(one).map_err(|_| LinkTextError::NotALinkCode)?
        }
        _ => return Err(LinkTextError::NotALinkCode),
    };
    let vault_name = all("n")
        .first()
        .map(|name| name.trim())
        .filter(|name| {
            !name.is_empty()
                && name.chars().count() <= MAX_NAME_CHARS
                && !name.chars().any(char::is_control)
        })
        .map(str::to_owned);

    let mut dropped = 0u32;
    let nostr_relays = keep(
        all("nr"),
        MAX_SERVERS,
        servers::is_nostr_relay,
        &mut dropped,
    );
    let iroh_relays = keep(all("ir"), MAX_SERVERS, servers::is_iroh_relay, &mut dropped);
    // Switched-off entries are only compared with, never dialed: any short text will do.
    let disabled = keep(
        all("off"),
        MAX_OFF,
        |url| url.len() <= 256 && !url.chars().any(char::is_whitespace),
        &mut dropped,
    );
    let config = ServerConfig {
        nostr_relays,
        iroh_relays,
        disabled,
    };
    let named = !(config.nostr_relays.is_empty()
        && config.iroh_relays.is_empty()
        && config.disabled.is_empty());
    // What the checks above let through must pass the settings' own check too.
    let servers = (named && servers::validate(&config).is_ok()).then_some(config);
    Ok(ParsedLinkText {
        code,
        vault_name,
        servers,
        dropped_servers: dropped,
    })
}

/// The first `limit` distinct entries of `urls` that `accepts`; the others count as dropped.
fn keep(
    urls: Vec<&str>,
    limit: usize,
    accepts: impl Fn(&str) -> bool,
    dropped: &mut u32,
) -> Vec<String> {
    let mut kept: Vec<String> = Vec::new();
    for url in urls {
        if kept.iter().any(|have| have == url) {
            continue;
        }
        if kept.len() < limit && accepts(url) {
            kept.push(url.to_owned());
        } else {
            *dropped += 1;
        }
    }
    kept
}

#[cfg(test)]
#[path = "text_tests.rs"]
mod tests;
