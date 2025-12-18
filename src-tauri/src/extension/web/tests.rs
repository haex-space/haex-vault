// src-tauri/src/extension/web/tests.rs
//!
//! Tests for extension web operations
//!

#[cfg(test)]
mod tests {
    use crate::extension::web::types::{WebFetchRequest, WebFetchResponse};
    use std::collections::HashMap;

    // ============================================================================
    // WebFetchRequest Tests
    // ============================================================================

    #[test]
    fn test_web_fetch_request_minimal() {
        let json = r#"{"url": "https://example.com"}"#;
        let request: WebFetchRequest = serde_json::from_str(json).unwrap();

        assert_eq!(request.url, "https://example.com");
        assert!(request.method.is_none());
        assert!(request.headers.is_none());
        assert!(request.body.is_none());
        assert!(request.timeout.is_none());
    }

    #[test]
    fn test_web_fetch_request_full() {
        let json = r#"{
            "url": "https://api.example.com/data",
            "method": "POST",
            "headers": {"Content-Type": "application/json", "Authorization": "Bearer token"},
            "body": "eyJrZXkiOiAidmFsdWUifQ==",
            "timeout": 5000
        }"#;
        let request: WebFetchRequest = serde_json::from_str(json).unwrap();

        assert_eq!(request.url, "https://api.example.com/data");
        assert_eq!(request.method.as_deref(), Some("POST"));
        assert!(request.headers.is_some());
        let headers = request.headers.unwrap();
        assert_eq!(headers.get("Content-Type"), Some(&"application/json".to_string()));
        assert_eq!(headers.get("Authorization"), Some(&"Bearer token".to_string()));
        assert_eq!(request.body.as_deref(), Some("eyJrZXkiOiAidmFsdWUifQ=="));
        assert_eq!(request.timeout, Some(5000));
    }

    #[test]
    fn test_web_fetch_request_all_methods() {
        let methods = ["GET", "POST", "PUT", "DELETE", "PATCH", "HEAD", "OPTIONS"];

        for method in methods {
            let json = format!(r#"{{"url": "https://example.com", "method": "{}"}}"#, method);
            let request: WebFetchRequest = serde_json::from_str(&json).unwrap();
            assert_eq!(request.method.as_deref(), Some(method));
        }
    }

    #[test]
    fn test_web_fetch_request_empty_headers() {
        let json = r#"{"url": "https://example.com", "headers": {}}"#;
        let request: WebFetchRequest = serde_json::from_str(json).unwrap();

        assert!(request.headers.is_some());
        assert!(request.headers.unwrap().is_empty());
    }

    #[test]
    fn test_web_fetch_request_timeout_values() {
        let timeouts = [0, 1000, 30000, 60000, 120000];

        for timeout in timeouts {
            let json = format!(r#"{{"url": "https://example.com", "timeout": {}}}"#, timeout);
            let request: WebFetchRequest = serde_json::from_str(&json).unwrap();
            assert_eq!(request.timeout, Some(timeout));
        }
    }

    // ============================================================================
    // WebFetchResponse Tests
    // ============================================================================

    #[test]
    fn test_web_fetch_response_serialization() {
        let mut headers = HashMap::new();
        headers.insert("content-type".to_string(), "application/json".to_string());
        headers.insert("x-custom".to_string(), "value".to_string());

        let response = WebFetchResponse {
            status: 200,
            status_text: "OK".to_string(),
            headers,
            body: "eyJzdWNjZXNzIjogdHJ1ZX0=".to_string(), // base64
            url: "https://example.com/api".to_string(),
        };

        let json = serde_json::to_string(&response).unwrap();

        assert!(json.contains("\"status\":200"));
        assert!(json.contains("\"status_text\":\"OK\""));
        assert!(json.contains("\"body\":\"eyJzdWNjZXNzIjogdHJ1ZX0=\""));
        assert!(json.contains("\"url\":\"https://example.com/api\""));
    }

    #[test]
    fn test_web_fetch_response_common_status_codes() {
        let status_codes = [
            (200, "OK"),
            (201, "Created"),
            (204, "No Content"),
            (301, "Moved Permanently"),
            (302, "Found"),
            (400, "Bad Request"),
            (401, "Unauthorized"),
            (403, "Forbidden"),
            (404, "Not Found"),
            (500, "Internal Server Error"),
            (502, "Bad Gateway"),
            (503, "Service Unavailable"),
        ];

        for (code, text) in status_codes {
            let response = WebFetchResponse {
                status: code,
                status_text: text.to_string(),
                headers: HashMap::new(),
                body: String::new(),
                url: "https://example.com".to_string(),
            };

            let json = serde_json::to_string(&response).unwrap();
            assert!(json.contains(&format!("\"status\":{}", code)));
            assert!(json.contains(&format!("\"status_text\":\"{}\"", text)));
        }
    }

    #[test]
    fn test_web_fetch_response_empty_body() {
        let response = WebFetchResponse {
            status: 204,
            status_text: "No Content".to_string(),
            headers: HashMap::new(),
            body: String::new(),
            url: "https://example.com".to_string(),
        };

        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("\"body\":\"\""));
    }

    #[test]
    fn test_web_fetch_response_redirect_url() {
        // When following redirects, url should be the final URL
        let response = WebFetchResponse {
            status: 200,
            status_text: "OK".to_string(),
            headers: HashMap::new(),
            body: "data".to_string(),
            url: "https://example.com/final-destination".to_string(),
        };

        assert_eq!(response.url, "https://example.com/final-destination");
    }

    #[test]
    fn test_web_fetch_response_special_headers() {
        let mut headers = HashMap::new();
        headers.insert("set-cookie".to_string(), "session=abc123; Path=/; HttpOnly".to_string());
        headers.insert("content-encoding".to_string(), "gzip".to_string());
        headers.insert("cache-control".to_string(), "no-cache".to_string());
        headers.insert("access-control-allow-origin".to_string(), "*".to_string());

        let response = WebFetchResponse {
            status: 200,
            status_text: "OK".to_string(),
            headers: headers.clone(),
            body: String::new(),
            url: "https://api.example.com".to_string(),
        };

        let json = serde_json::to_string(&response).unwrap();

        for (key, _) in &headers {
            assert!(json.contains(key));
        }
    }

    // ============================================================================
    // URL Validation Tests (URL parsing logic from commands.rs)
    // ============================================================================

    #[test]
    fn test_valid_http_urls() {
        let valid_urls = [
            "http://example.com",
            "https://example.com",
            "https://example.com/path",
            "https://example.com/path?query=value",
            "https://example.com:8080/path",
            "https://user:pass@example.com/path",
            "https://sub.domain.example.com",
            "https://192.168.1.1/api",
            "https://[::1]/api",
        ];

        for url_str in valid_urls {
            let parsed = url::Url::parse(url_str);
            assert!(parsed.is_ok(), "URL should be valid: {}", url_str);

            let url = parsed.unwrap();
            let scheme = url.scheme();
            assert!(
                scheme == "http" || scheme == "https",
                "Scheme should be http or https for: {}",
                url_str
            );
        }
    }

    #[test]
    fn test_invalid_url_schemes() {
        let invalid_schemes = [
            "file:///etc/passwd",
            "ftp://ftp.example.com",
            "javascript:alert(1)",
            "data:text/html,<script>alert(1)</script>",
            "tel:+1234567890",
            "mailto:test@example.com",
        ];

        for url_str in invalid_schemes {
            let parsed = url::Url::parse(url_str);
            if let Ok(url) = parsed {
                let scheme = url.scheme();
                assert!(
                    scheme != "http" && scheme != "https",
                    "Non-http(s) scheme should be rejected: {}",
                    url_str
                );
            }
        }
    }

    #[test]
    fn test_invalid_url_format() {
        let invalid_urls = [
            "not-a-url",
            "://missing-scheme",
            "http://",
            "",
            "   ",
            "http:// invalid.com",
        ];

        for url_str in invalid_urls {
            let parsed = url::Url::parse(url_str);
            assert!(parsed.is_err(), "URL should be invalid: '{}'", url_str);
        }
    }

    // ============================================================================
    // Base64 Body Encoding Tests
    // ============================================================================

    #[test]
    fn test_base64_body_encoding_text() {
        use base64::{engine::general_purpose::STANDARD, Engine as _};

        let original = "Hello, World!";
        let encoded = STANDARD.encode(original);
        let decoded = STANDARD.decode(&encoded).unwrap();

        assert_eq!(String::from_utf8(decoded).unwrap(), original);
    }

    #[test]
    fn test_base64_body_encoding_json() {
        use base64::{engine::general_purpose::STANDARD, Engine as _};

        let original = r#"{"key": "value", "number": 42}"#;
        let encoded = STANDARD.encode(original);
        let decoded = STANDARD.decode(&encoded).unwrap();

        assert_eq!(String::from_utf8(decoded).unwrap(), original);
    }

    #[test]
    fn test_base64_body_encoding_binary() {
        use base64::{engine::general_purpose::STANDARD, Engine as _};

        let original: Vec<u8> = (0..=255).collect();
        let encoded = STANDARD.encode(&original);
        let decoded = STANDARD.decode(&encoded).unwrap();

        assert_eq!(decoded, original);
    }

    #[test]
    fn test_base64_body_encoding_empty() {
        use base64::{engine::general_purpose::STANDARD, Engine as _};

        let original = "";
        let encoded = STANDARD.encode(original);
        let decoded = STANDARD.decode(&encoded).unwrap();

        assert!(decoded.is_empty());
    }

    #[test]
    fn test_base64_body_encoding_unicode() {
        use base64::{engine::general_purpose::STANDARD, Engine as _};

        let original = "Hello 世界 🌍 Привет";
        let encoded = STANDARD.encode(original);
        let decoded = STANDARD.decode(&encoded).unwrap();

        assert_eq!(String::from_utf8(decoded).unwrap(), original);
    }

    // ============================================================================
    // HTTP Method Handling Tests
    // ============================================================================

    #[test]
    fn test_method_case_insensitive() {
        let methods = [
            ("get", "GET"),
            ("Get", "GET"),
            ("GET", "GET"),
            ("post", "POST"),
            ("Post", "POST"),
            ("POST", "POST"),
        ];

        for (input, expected) in methods {
            assert_eq!(input.to_uppercase(), expected);
        }
    }

    #[test]
    fn test_default_method_is_get() {
        let json = r#"{"url": "https://example.com"}"#;
        let request: WebFetchRequest = serde_json::from_str(json).unwrap();

        // When method is None, the default should be GET
        let method = request.method.as_deref().unwrap_or("GET");
        assert_eq!(method, "GET");
    }

    // ============================================================================
    // Header Handling Tests
    // ============================================================================

    #[test]
    fn test_multiple_headers() {
        let json = r#"{
            "url": "https://example.com",
            "headers": {
                "Accept": "application/json",
                "Accept-Language": "en-US",
                "User-Agent": "TestClient/1.0",
                "X-Custom-Header": "custom-value"
            }
        }"#;

        let request: WebFetchRequest = serde_json::from_str(json).unwrap();
        let headers = request.headers.unwrap();

        assert_eq!(headers.len(), 4);
        assert_eq!(headers.get("Accept"), Some(&"application/json".to_string()));
        assert_eq!(headers.get("Accept-Language"), Some(&"en-US".to_string()));
        assert_eq!(headers.get("User-Agent"), Some(&"TestClient/1.0".to_string()));
        assert_eq!(headers.get("X-Custom-Header"), Some(&"custom-value".to_string()));
    }

    #[test]
    fn test_header_with_special_characters() {
        let json = r#"{
            "url": "https://example.com",
            "headers": {
                "Authorization": "Bearer eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiIxMjM0NTY3ODkwIn0.dozjgNryP4J3jVmNHl0w5N_XgL0n3I9PlFUP0THsR8U"
            }
        }"#;

        let request: WebFetchRequest = serde_json::from_str(json).unwrap();
        let headers = request.headers.unwrap();

        assert!(headers.get("Authorization").unwrap().starts_with("Bearer "));
    }
}
