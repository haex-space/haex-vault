//! The cookies of the agent browser's profile, for "KI → Browser" (spec 065 FR-046, research R17):
//! listed, read, changed and deleted through the running browser (`Network.getAllCookies`,
//! `setCookie` and `deleteCookies` in a session of a page target), so the values come as the
//! browser holds them and no database file is parsed. A blank tab is opened for the call and
//! closed after it. Changes are part of the profile and are saved when the browser ends, like
//! cookies a page set. Only the window calls this; no agent can, and a value never reaches a
//! log, a reply to an agent or the list of cookies.

use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use ts_rs::TS;

use super::cdp::{target, Connection};
use super::error::{BrowserError, Result};

/// One cookie as the browser reports it; the fields holzi uses.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Cookie {
    pub name: String,
    pub value: String,
    /// As the browser holds it: with a leading dot a cookie of the whole domain, without one a
    /// cookie of exactly this host.
    pub domain: String,
    pub path: String,
    /// Seconds since 1970; `-1` for a cookie of the session.
    #[serde(default)]
    pub expires: f64,
    #[serde(default)]
    pub secure: bool,
    #[serde(default)]
    pub http_only: bool,
    #[serde(default)]
    pub same_site: Option<String>,
    /// CDP reports more attributes than the settings view displays. Keep the rest so a change
    /// does not silently turn a partitioned cookie into an unpartitioned one or drop its source.
    #[serde(flatten)]
    pub(crate) attributes: serde_json::Map<String, Value>,
}

/// What names one cookie: the browser keeps one per name, domain (as listed) and path.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub struct CookieRef {
    pub name: String,
    pub domain: String,
    pub path: String,
}

/// A cookie as the window lists it: everything but its value, which is read on a click.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub struct CookieView {
    pub name: String,
    pub domain: String,
    /// The website the cookie belongs to: its domain without the leading dot.
    pub site: String,
    pub path: String,
    /// Seconds since 1970; `None` for a cookie of the session.
    #[ts(optional = nullable, type = "number | null")]
    pub expires: Option<i64>,
    pub secure: bool,
    pub http_only: bool,
    #[ts(optional = nullable)]
    pub same_site: Option<String>,
}

/// What to delete: one cookie, or every cookie of a website.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub enum CookieTarget {
    Cookie(CookieRef),
    /// The website: a domain without the leading dot.
    Site(String),
}

/// The website a cookie domain belongs to.
pub fn site_of(domain: &str) -> &str {
    domain.trim_start_matches('.')
}

impl Cookie {
    pub fn matches(&self, key: &CookieRef) -> bool {
        self.name == key.name && self.domain == key.domain && self.path == key.path
    }

    pub fn view(&self) -> CookieView {
        CookieView {
            name: self.name.clone(),
            domain: self.domain.clone(),
            site: site_of(&self.domain).to_owned(),
            path: self.path.clone(),
            expires: (self.expires > 0.0).then_some(self.expires as i64),
            secure: self.secure,
            http_only: self.http_only,
            same_site: self.same_site.clone(),
        }
    }

    /// The `Network.setCookie` parameters that write this cookie again with `value` and `expires`
    /// (`None`: of the session) and every other attribute kept. A cookie of a host is written by
    /// the address it belongs to, so it stays a cookie of exactly that host.
    pub fn set_params(&self, value: &str, expires: Option<i64>) -> Value {
        let mut params = json!({
            "name": self.name,
            "value": value,
            "path": self.path,
            "secure": self.secure,
            "httpOnly": self.http_only,
        });
        if self.domain.starts_with('.') {
            params["domain"] = json!(self.domain);
        } else {
            let scheme = if self.secure { "https" } else { "http" };
            params["url"] = json!(format!("{scheme}://{}{}", self.domain, self.path));
        }
        if let Some(same_site) = &self.same_site {
            params["sameSite"] = json!(same_site);
        }
        for key in [
            "priority",
            "sameParty",
            "sourceScheme",
            "sourcePort",
            "partitionKey",
        ] {
            if let Some(value) = self.attributes.get(key) {
                params[key] = value.clone();
            }
        }
        if let Some(expires) = expires {
            params["expires"] = json!(expires);
        }
        params
    }
}

/// The cookies in the reply of `Network.getAllCookies`; one that cannot be read is left out.
pub fn parse(reply: &Value) -> Vec<Cookie> {
    reply["cookies"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|cookie| serde_json::from_value(cookie.clone()).ok())
        .collect()
}

/// A new expiry must lie ahead: one in the past would make the browser drop the cookie, and
/// deleting is its own action.
pub fn check_expiry(expires: Option<i64>, now: i64) -> std::result::Result<(), String> {
    match expires {
        Some(at) if at <= now => {
            Err("the expiry is in the past; delete the cookie instead".to_owned())
        }
        _ => Ok(()),
    }
}

fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.as_secs() as i64)
}

/// Runs `work` in a session of a fresh blank tab and closes the tab afterwards, whatever happened.
async fn in_blank_tab<T, F>(connection: &Connection, work: F) -> Result<T>
where
    F: for<'a> FnOnce(
        &'a Connection,
        &'a str,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<T>> + Send + 'a>,
    >,
{
    let tab = target::create(connection).await?;
    let outcome = async {
        let session = target::attach(connection, &tab).await?;
        let outcome = work(connection, &session).await;
        let _ = target::detach(connection, &session).await;
        outcome
    }
    .await;
    let _ = target::close(connection, &tab).await;
    outcome
}

async fn all(connection: &Connection, session: &str) -> Result<Vec<Cookie>> {
    let reply = connection
        .call("Network.getAllCookies", json!({}), Some(session))
        .await?;
    Ok(parse(&reply))
}

/// The cookies of the profile, sorted by website, name and path.
pub async fn list(connection: &Connection) -> Result<Vec<Cookie>> {
    let mut cookies = in_blank_tab(connection, |c, s| Box::pin(all(c, s))).await?;
    cookies.sort_by(|a, b| {
        (site_of(&a.domain), &a.name, &a.path).cmp(&(site_of(&b.domain), &b.name, &b.path))
    });
    Ok(cookies)
}

/// The value of one cookie, for the window only.
pub async fn value_of(connection: &Connection, key: CookieRef) -> Result<String> {
    list(connection)
        .await?
        .into_iter()
        .find(|cookie| cookie.matches(&key))
        .map(|cookie| cookie.value)
        .ok_or_else(|| BrowserError::Protocol("the cookie is gone".to_owned()))
}

/// Writes `value` and `expires` (`None`: of the session) into the cookie `key`, all else kept.
pub async fn change(
    connection: &Connection,
    key: CookieRef,
    value: String,
    expires: Option<i64>,
) -> Result<()> {
    check_expiry(expires, now()).map_err(BrowserError::Protocol)?;
    in_blank_tab(connection, move |c, s| {
        Box::pin(async move {
            let cookie = all(c, s)
                .await?
                .into_iter()
                .find(|cookie| cookie.matches(&key))
                .ok_or_else(|| BrowserError::Protocol("the cookie is gone".to_owned()))?;
            let reply = c
                .call(
                    "Network.setCookie",
                    cookie.set_params(&value, expires),
                    Some(s),
                )
                .await?;
            if reply["success"] == false {
                return Err(BrowserError::Protocol(
                    "the browser did not accept the cookie".to_owned(),
                ));
            }
            Ok(())
        })
    })
    .await
}

/// Deletes one cookie or every cookie of a website; answers how many went.
pub async fn delete(connection: &Connection, what: CookieTarget) -> Result<usize> {
    in_blank_tab(connection, move |c, s| {
        Box::pin(async move {
            let doomed: Vec<Cookie> = all(c, s)
                .await?
                .into_iter()
                .filter(|cookie| match &what {
                    CookieTarget::Cookie(key) => cookie.matches(key),
                    CookieTarget::Site(site) => site_of(&cookie.domain) == site_of(site),
                })
                .collect();
            for cookie in &doomed {
                c.call(
                    "Network.deleteCookies",
                    json!({ "name": cookie.name, "domain": cookie.domain, "path": cookie.path }),
                    Some(s),
                )
                .await?;
            }
            Ok(doomed.len())
        })
    })
    .await
}
