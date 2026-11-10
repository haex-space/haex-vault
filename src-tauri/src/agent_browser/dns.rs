//! DNS over HTTPS for the agent browser (spec 065 FR-048, research R18): the user's choice of mode
//! and provider (kept in the vault, see `choices`), and its effect: before the browser starts, holzi
//! writes Chromium's own secure-DNS keys into the profile's `Local State`. The choice applies to
//! the agent browser only; it takes effect at the browser's next start.

use std::path::Path;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use ts_rs::TS;

use super::error::{BrowserError, Result};

/// The longest address of an own provider.
const MAX_TEMPLATE: usize = 2048;
/// What a URI template of a provider may end with to ask by `GET`; without it the browser asks by
/// `POST`.
const GET_SUFFIX: &str = "{?dns}";

/// How names are looked up.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub enum DnsMode {
    /// Disable DNS over HTTPS and use the system's resolver.
    Off,
    /// The provider first; the system's resolver only when the provider does not answer.
    Automatic,
    /// Only the provider; an unreachable provider ends every page load.
    Secure,
}

/// Whose resolver answers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub enum DnsProvider {
    Quad9,
    Cloudflare,
    Google,
    Mullvad,
    /// An address the user typed.
    Custom,
}

/// A provider holzi knows by itself.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub struct DnsPreset {
    pub id: DnsProvider,
    pub name: &'static str,
    pub url: &'static str,
}

/// What "KI → Browser" shows: the choice and the providers holzi knows by itself.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub struct DnsOverview {
    pub settings: DnsSettings,
    pub presets: Vec<DnsPreset>,
}

impl DnsOverview {
    pub fn of(settings: DnsSettings) -> Self {
        Self {
            settings,
            presets: PRESETS.to_vec(),
        }
    }
}

/// The providers of FR-048; their addresses live in the code, so an update may change them.
pub const PRESETS: [DnsPreset; 4] = [
    DnsPreset {
        id: DnsProvider::Quad9,
        name: "Quad9",
        url: "https://dns.quad9.net/dns-query",
    },
    DnsPreset {
        id: DnsProvider::Cloudflare,
        name: "Cloudflare",
        url: "https://cloudflare-dns.com/dns-query",
    },
    DnsPreset {
        id: DnsProvider::Google,
        name: "Google",
        url: "https://dns.google/dns-query",
    },
    DnsPreset {
        id: DnsProvider::Mullvad,
        name: "Mullvad",
        url: "https://dns.mullvad.net/dns-query",
    },
];

/// What the user chose.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub struct DnsSettings {
    pub mode: DnsMode,
    pub provider: DnsProvider,
    /// The address of an own provider; counts only with [`DnsProvider::Custom`].
    #[serde(default)]
    pub custom: String,
}

impl Default for DnsSettings {
    /// Automatic with Quad9 (operator 2026-11-05).
    fn default() -> Self {
        Self {
            mode: DnsMode::Automatic,
            provider: DnsProvider::Quad9,
            custom: String::new(),
        }
    }
}

impl DnsSettings {
    /// The browser's own default; for tests that must not depend on a network.
    pub fn off() -> Self {
        Self {
            mode: DnsMode::Off,
            ..Self::default()
        }
    }

    /// The address of the provider that counts, `None` while the mode is off.
    pub fn template(&self) -> Option<String> {
        if self.mode == DnsMode::Off {
            return None;
        }
        match self.provider {
            DnsProvider::Custom => Some(self.custom.clone()),
            preset => PRESETS
                .iter()
                .find(|known| known.id == preset)
                .map(|known| known.url.to_owned()),
        }
    }

    /// The choice as it is kept: an own address must be `https` with a host, no user info and at
    /// most 2048 characters; it is dropped when another provider is chosen.
    pub fn validated(mut self) -> std::result::Result<Self, String> {
        if self.provider != DnsProvider::Custom {
            self.custom.clear();
            return Ok(self);
        }
        self.custom = self.custom.trim().to_owned();
        check_template(&self.custom)?;
        Ok(self)
    }
}

/// An own provider's address: `https`, a host, no user info, at most 2048 characters, and braces
/// only as the `{?dns}` that ends it.
pub fn check_template(template: &str) -> std::result::Result<(), String> {
    if template.len() > MAX_TEMPLATE {
        return Err("the address of the provider is too long".to_owned());
    }
    let base = template.strip_suffix(GET_SUFFIX).unwrap_or(template);
    if base.contains(['{', '}']) {
        return Err("only {?dns} may be a template variable".to_owned());
    }
    let url = url::Url::parse(base).map_err(|_| "not an address".to_owned())?;
    if url.scheme() != "https" {
        return Err("only https addresses are allowed".to_owned());
    }
    if url.host_str().is_none_or(str::is_empty) {
        return Err("an address without a host".to_owned());
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err("an address with a user name or password".to_owned());
    }
    Ok(())
}

/// What `dns_over_https` of `Local State` holds for `settings`: the mode the browser knows and the
/// provider's address (empty while off).
pub fn preference(settings: &DnsSettings) -> Value {
    let mode = match settings.mode {
        DnsMode::Off => "off",
        DnsMode::Automatic => "automatic",
        DnsMode::Secure => "secure",
    };
    json!({
        "mode": mode,
        "templates": settings.template().unwrap_or_default(),
    })
}

/// Writes the choice into `Local State` of the profile in `profile` (the browser's user data
/// directory) before the browser starts, and keeps every other key. The file of a saved profile
/// has its own `dns_over_https`; it is replaced. A file that cannot be read is replaced as a whole.
pub fn apply(profile: &Path, settings: &DnsSettings) -> Result<()> {
    let path = profile.join("Local State");
    let mut state: Value = std::fs::read(&path)
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .filter(Value::is_object)
        .unwrap_or_else(|| json!({}));
    state["dns_over_https"] = preference(settings);
    let bytes = serde_json::to_vec(&state).map_err(|e| BrowserError::Profile(e.to_string()))?;
    std::fs::write(&path, bytes).map_err(|e| BrowserError::Profile(e.to_string()))
}
