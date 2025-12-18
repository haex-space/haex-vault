// src-tauri/src/extension/web/helpers.rs
//!
//! Helper functions for extension web operations
//!

use crate::extension::error::ExtensionError;
use crate::extension::web::types::{WebFetchRequest, WebFetchResponse};
use base64::{engine::general_purpose::STANDARD, Engine as _};
use std::collections::HashMap;
use std::time::Duration;
use tauri_plugin_http::reqwest;

/// Performs the actual HTTP request without CORS restrictions
pub async fn fetch_web_request(request: WebFetchRequest) -> Result<WebFetchResponse, ExtensionError> {
    let method_str = request.method.as_deref().unwrap_or("GET");
    let timeout_ms = request.timeout.unwrap_or(30000);

    // Build reqwest client with timeout
    let client = reqwest::Client::builder()
        .timeout(Duration::from_millis(timeout_ms))
        .build()
        .map_err(|e| ExtensionError::WebError {
            reason: format!("Failed to create HTTP client: {}", e),
        })?;

    // Build request
    let mut req_builder = match method_str.to_uppercase().as_str() {
        "GET" => client.get(&request.url),
        "POST" => client.post(&request.url),
        "PUT" => client.put(&request.url),
        "DELETE" => client.delete(&request.url),
        "PATCH" => client.patch(&request.url),
        "HEAD" => client.head(&request.url),
        "OPTIONS" => client.request(reqwest::Method::OPTIONS, &request.url),
        _ => {
            return Err(ExtensionError::WebError {
                reason: format!("Unsupported HTTP method: {}", method_str),
            })
        }
    };

    // Add headers
    if let Some(headers) = request.headers {
        for (key, value) in headers {
            req_builder = req_builder.header(key, value);
        }
    }

    // Add body if present (decode from base64)
    if let Some(body_base64) = request.body {
        let body_bytes = STANDARD
            .decode(&body_base64)
            .map_err(|e| ExtensionError::WebError {
                reason: format!("Failed to decode request body from base64: {}", e),
            })?;
        req_builder = req_builder.body(body_bytes);
    }

    // Execute request
    let response = req_builder.send().await.map_err(|e| {
        if e.is_timeout() {
            ExtensionError::WebError {
                reason: format!("Request timeout after {}ms", timeout_ms),
            }
        } else {
            ExtensionError::WebError {
                reason: format!("Request failed: {}", e),
            }
        }
    })?;

    // Extract response data
    let status = response.status().as_u16();
    let status_text = response
        .status()
        .canonical_reason()
        .unwrap_or("")
        .to_string();
    let final_url = response.url().to_string();

    // Extract headers
    let mut response_headers = HashMap::new();
    for (key, value) in response.headers() {
        if let Ok(value_str) = value.to_str() {
            response_headers.insert(key.to_string(), value_str.to_string());
        }
    }

    // Read body and encode to base64
    let body_bytes = response
        .bytes()
        .await
        .map_err(|e| ExtensionError::WebError {
            reason: format!("Failed to read response body: {}", e),
        })?;

    let body_base64 = STANDARD.encode(&body_bytes);

    Ok(WebFetchResponse {
        status,
        status_text,
        headers: response_headers,
        body: body_base64,
        url: final_url,
    })
}
