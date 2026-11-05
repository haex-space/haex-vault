//! Downloading a filter list (spec 065 FR-045, research R16): over `https` only, bounded in time
//! and size, and only text that looks like a list. A trait, so tests answer without a network.

use std::time::Duration;

use async_trait::async_trait;

/// How long one download may take.
const TIMEOUT: Duration = Duration::from_secs(30);
/// The largest list holzi takes; the biggest default list is about 13 MB.
const MAX_BYTES: usize = 64 * 1024 * 1024;

/// Where the texts of filter lists come from.
#[async_trait]
pub trait ListFetcher: Send + Sync {
    /// The text at `url`, or why it cannot be had.
    async fn get(&self, url: &str) -> Result<String, String>;
}

/// The real thing: `https` with the system's trust.
pub struct HttpFetcher {
    client: reqwest::Client,
}

impl HttpFetcher {
    pub fn new() -> Result<Self, String> {
        let client = reqwest::Client::builder()
            .user_agent(concat!("holzi/", env!("CARGO_PKG_VERSION")))
            .timeout(TIMEOUT)
            .build()
            .map_err(|error| error.to_string())?;
        Ok(Self { client })
    }
}

#[async_trait]
impl ListFetcher for HttpFetcher {
    async fn get(&self, url: &str) -> Result<String, String> {
        require_https(url)?;
        let mut response = self
            .client
            .get(url)
            .send()
            .await
            .map_err(|error| error.without_url().to_string())?;
        // A redirect must not lead back to an unencrypted address.
        require_https(response.url().as_str())?;
        if !response.status().is_success() {
            return Err(format!("the server answered {}", response.status()));
        }
        let mut bytes = Vec::new();
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|error| error.without_url().to_string())?
        {
            if bytes.len() + chunk.len() > MAX_BYTES {
                return Err("the list is larger than 64 MiB".to_owned());
            }
            bytes.extend_from_slice(&chunk);
        }
        looks_like_list(String::from_utf8_lossy(&bytes).into_owned())
    }
}

/// Only an `https` address is a source of a list.
pub fn require_https(url: &str) -> Result<(), String> {
    match url::Url::parse(url) {
        Ok(parsed) if parsed.scheme() == "https" && parsed.host_str().is_some() => Ok(()),
        Ok(_) => Err("only https addresses are allowed".to_owned()),
        Err(_) => Err("not an address".to_owned()),
    }
}

/// `text` when it can be a list: not empty and not a web page.
pub fn looks_like_list(text: String) -> Result<String, String> {
    let start = text.trim_start();
    if start.is_empty() {
        return Err("the list is empty".to_owned());
    }
    let head: String = start.chars().take(15).collect::<String>().to_lowercase();
    if head.starts_with("<!doctype") || head.starts_with("<html") {
        return Err("the address leads to a web page, not to a filter list".to_owned());
    }
    Ok(text)
}
