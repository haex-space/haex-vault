//! The site of a host (spec 049 FR-019, research R7): the registrable domain one label below the
//! public suffix, so `www.example.de` and `login.example.de` are the same site and
//! `bad-example.de` is another. IP addresses and `localhost` have no registrable domain; for them
//! the host itself is the site. The passkey service's origin check uses the same suffix rule
//! (spec 036 R9).

use std::net::IpAddr;

/// Whether a domain is itself a public suffix (`com`, `co.uk`), which belongs to nobody.
pub fn is_public_suffix(domain: &str) -> bool {
    !matches!(psl::suffix_str(domain), Some(suffix) if suffix != domain)
}

/// The site of `host`, lowercased; `None` for an empty host or a public suffix.
pub fn site_of(host: &str) -> Option<String> {
    let host = host
        .trim()
        .trim_start_matches('[')
        .trim_end_matches(']')
        .trim_end_matches('.')
        .to_ascii_lowercase();
    if host.is_empty() {
        return None;
    }
    if host.parse::<IpAddr>().is_ok() || host == "localhost" || host.ends_with(".localhost") {
        return Some(host);
    }
    if is_public_suffix(&host) {
        return None;
    }
    Some(psl::domain_str(&host).unwrap_or(&host).to_owned())
}
