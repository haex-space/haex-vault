//! The client of the Abacus sign-in service: the device-code flow HaexVault runs on behalf of the
//! user, so no terminal and no variable is involved (spec 096 FR-005, research §3).
//!
//! The service is private and not documented by the vendor. Two `POST` calls with a JSON body
//! (data-model §3a): `start` hands over the e-mail address and a random device id and gets the
//! address the user opens and the code to enter there; `poll` asks every few seconds until the
//! user has confirmed and the device code comes back. Neither the e-mail address, nor the user
//! code, nor the device code, nor a body or an answer ever reaches a log line, a `Debug` output or
//! an error text: only fixed words do.

use std::fmt;
use std::time::Duration;

use serde::{Deserialize, Deserializer};
use serde_json::json;
use thiserror::Error;
use tokio::time::Instant;
use tokio_util::sync::CancellationToken;
use zeroize::Zeroizing;

#[cfg(test)]
#[path = "abacus_signin_tests.rs"]
mod tests;

/// The fixed address of the sign-in service.
const BASE: &str = "https://apps.abacus.ai";
const START_PATH: &str = "/api/_authenticateDeviceForCodeLlm";
const POLL_PATH: &str = "/api/_pollDeviceCode";

/// Our own name, not the vendor's. The check of quickstart §3.0 decides whether the service
/// needs another one.
const USER_AGENT: &str = concat!("haex-vault/", env!("CARGO_PKG_VERSION"));
const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

/// Used when the service names no interval, or a useless one.
const DEFAULT_INTERVAL: Duration = Duration::from_secs(5);
const MIN_INTERVAL_SECS: f64 = 1.0;
const MAX_INTERVAL_SECS: f64 = 60.0;
/// The tool's own limit: the code is good for ten minutes.
const WAIT_LIMIT: Duration = Duration::from_secs(10 * 60);
/// A flaky connection must not end a ten-minute wait: this many answers in a row may fail.
const TOLERATED_POLL_FAILURES: u32 = 3;
/// The vendor's words are shown to the user; a runaway text is cut.
const MAX_VENDOR_TEXT_CHARS: usize = 300;
const REFUSED_FALLBACK: &str = "no reason given";

/// A value that is secret or personal: redacted `Debug` and `Display`, zeroized on drop. There is
/// no `Serialize`: it cannot be written anywhere by accident.
#[derive(Clone)]
pub struct Secret(Zeroizing<String>);

impl Secret {
    pub fn new(value: String) -> Self {
        Self(Zeroizing::new(value))
    }

    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Secret([redacted])")
    }
}

impl fmt::Display for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("[redacted]")
    }
}

impl<'de> Deserialize<'de> for Secret {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        String::deserialize(deserializer).map(Secret::new)
    }
}

/// What the user needs on screen: where to go and which code to enter there.
pub struct Prompt {
    pub uri: String,
    pub code: Secret,
    pub interval: Duration,
}

impl fmt::Debug for Prompt {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Prompt")
            .field("uri", &self.uri)
            .field("code", &self.code)
            .field("interval", &self.interval)
            .finish()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum SigninError {
    /// The service said no on the first call. The text is the vendor's own prose.
    #[error("the sign-in service refused the request: {0}")]
    Refused(String),
    #[error("the sign-in service could not be reached")]
    Network,
    #[error("the sign-in was not confirmed in time")]
    Expired,
    #[error("the sign-in was cancelled")]
    Cancelled,
    #[error("the sign-in service answered in an unexpected way")]
    Unexpected,
}

pub struct Client {
    http: reqwest::Client,
    base: String,
    wait_limit: Duration,
}

impl fmt::Debug for Client {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Client")
            .field("base", &self.base)
            .field("wait_limit", &self.wait_limit)
            .finish()
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct StartAnswer {
    #[serde(default)]
    success: bool,
    error: Option<String>,
    result: Option<StartResult>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct StartResult {
    verification_uri: Option<String>,
    user_code: Option<Secret>,
    polling_interval: Option<f64>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PollAnswer {
    #[serde(default)]
    success: bool,
    result: Option<PollResult>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PollResult {
    device_code: Option<Secret>,
}

/// The interval the service asked for: missing, zero or nonsense is five seconds, the rest is
/// kept between one second and one minute.
fn interval_from(seconds: Option<f64>) -> Duration {
    match seconds {
        Some(s) if s.is_finite() && s > 0.0 => {
            Duration::from_secs_f64(s.clamp(MIN_INTERVAL_SECS, MAX_INTERVAL_SECS))
        }
        _ => DEFAULT_INTERVAL,
    }
}

fn vendor_text(error: Option<String>) -> String {
    let text: String = error
        .as_deref()
        .map(str::trim)
        .unwrap_or_default()
        .chars()
        .take(MAX_VENDOR_TEXT_CHARS)
        .collect();
    if text.is_empty() {
        REFUSED_FALLBACK.to_string()
    } else {
        text
    }
}

impl Client {
    pub fn new() -> Result<Client, SigninError> {
        Self::with_base(BASE)
    }

    pub(crate) fn with_base(base: &str) -> Result<Client, SigninError> {
        let http = reqwest::Client::builder()
            .user_agent(USER_AGENT)
            .connect_timeout(CONNECT_TIMEOUT)
            .timeout(REQUEST_TIMEOUT)
            // The e-mail address goes to one address only; an answer that points elsewhere is not
            // followed.
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|_| SigninError::Network)?;
        Ok(Client {
            http,
            base: base.trim_end_matches('/').to_string(),
            wait_limit: WAIT_LIMIT,
        })
    }

    /// A shorter wait than the ten minutes, so a test need not wait that long.
    #[cfg(test)]
    pub(crate) fn with_wait_limit(mut self, limit: Duration) -> Self {
        self.wait_limit = limit;
        self
    }

    /// One `POST` with a JSON body. The error says what went wrong in fixed words only: a
    /// transport failure is `Network`, an answer that is not the JSON we expect is `Unexpected`.
    async fn post<T: for<'de> Deserialize<'de>>(
        &self,
        path: &str,
        body: &serde_json::Value,
    ) -> Result<T, SigninError> {
        let response = self
            .http
            .post(format!("{}{path}", self.base))
            .json(body)
            .send()
            .await
            .map_err(|_| SigninError::Network)?;
        // A refusal may come with an error status and a JSON body that carries the reason, so the
        // status alone does not decide: the body must have the documented shape.
        response.json::<T>().await.map_err(|e| {
            if e.is_decode() {
                SigninError::Unexpected
            } else {
                SigninError::Network
            }
        })
    }

    /// The first call: hands over the e-mail address and the device id, gets the prompt.
    pub async fn start(&self, email: &Secret, device_id: &str) -> Result<Prompt, SigninError> {
        let body = json!({ "email": email.expose(), "deviceId": device_id });
        let answer: StartAnswer = self.post(START_PATH, &body).await?;
        let Some(result) = answer.result.filter(|_| answer.success) else {
            return Err(SigninError::Refused(vendor_text(answer.error)));
        };
        match (result.verification_uri, result.user_code) {
            (Some(uri), Some(code)) if !uri.is_empty() && !code.expose().is_empty() => Ok(Prompt {
                uri,
                code,
                interval: interval_from(result.polling_interval),
            }),
            _ => Err(SigninError::Unexpected),
        }
    }

    /// Asks every `prompt.interval` until the user has confirmed (the device code comes back),
    /// ten minutes have passed or `cancel` fires. A cancel ends it at once, in the wait and in a
    /// request that is under way, and no further request goes out.
    pub async fn poll(
        &self,
        prompt: &Prompt,
        cancel: &CancellationToken,
    ) -> Result<Secret, SigninError> {
        let deadline = Instant::now() + self.wait_limit;
        let body = json!({ "userCode": prompt.code.expose() });
        let mut failures = 0u32;
        loop {
            if cancel.is_cancelled() {
                return Err(SigninError::Cancelled);
            }
            let answer = tokio::select! {
                () = cancel.cancelled() => return Err(SigninError::Cancelled),
                answer = self.post::<PollAnswer>(POLL_PATH, &body) => answer,
            };
            match answer {
                Ok(PollAnswer {
                    success: true,
                    result:
                        Some(PollResult {
                            device_code: Some(code),
                        }),
                }) if !code.expose().is_empty() => return Ok(code),
                // "Not confirmed yet".
                Ok(PollAnswer { success: false, .. }) => failures = 0,
                // `success` without a device code is not the documented shape.
                Ok(_) => failures = note_failure(failures, SigninError::Unexpected)?,
                Err(error) => failures = note_failure(failures, error)?,
            }
            tokio::select! {
                () = cancel.cancelled() => return Err(SigninError::Cancelled),
                () = tokio::time::sleep(prompt.interval) => {}
            }
            if Instant::now() >= deadline {
                return Err(SigninError::Expired);
            }
        }
    }
}

/// Counts one failed answer in a row; the one past the tolerated number ends the wait.
fn note_failure(failures: u32, error: SigninError) -> Result<u32, SigninError> {
    if failures + 1 >= TOLERATED_POLL_FAILURES {
        Err(error)
    } else {
        Ok(failures + 1)
    }
}
