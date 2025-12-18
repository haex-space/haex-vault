// src-tauri/src/extension/web/types.rs
//!
//! Types for extension web operations
//!

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Request structure matching the SDK's WebRequestOptions
#[derive(Debug, Deserialize)]
pub struct WebFetchRequest {
    pub url: String,
    #[serde(default)]
    pub method: Option<String>,
    #[serde(default)]
    pub headers: Option<HashMap<String, String>>,
    #[serde(default)]
    pub body: Option<String>, // Base64 encoded
    #[serde(default)]
    pub timeout: Option<u64>, // milliseconds
}

/// Response structure matching the SDK's WebResponse
#[derive(Debug, Serialize)]
pub struct WebFetchResponse {
    pub status: u16,
    pub status_text: String,
    pub headers: HashMap<String, String>,
    pub body: String, // Base64 encoded
    pub url: String,
}
