//! Tests for the origin rule (spec 049, T037): browser extensions in, everything else out.

use super::origin::extension_origin;

#[test]
fn browser_extensions_are_accepted() {
    assert_eq!(
        extension_origin(Some("chrome-extension://abcdefghijklmnop")),
        Some("chrome-extension://abcdefghijklmnop")
    );
    assert_eq!(
        extension_origin(Some("moz-extension://8c2f3e5a-1b2c-4d5e-9f00-123456789abc")),
        Some("moz-extension://8c2f3e5a-1b2c-4d5e-9f00-123456789abc")
    );
}

#[test]
fn web_pages_and_everything_else_are_refused() {
    for header in [
        "https://evil.example",
        "http://localhost",
        "http://localhost:19455",
        "null",
        "",
        "chrome-extension:",
        "chrome-extension://",
        "Chrome-Extension://abc",
        "file://",
        "chrome-extension://abc/path",
        "moz-extension://abc@evil.example",
        " chrome-extension://abc",
    ] {
        assert_eq!(extension_origin(Some(header)), None, "{header:?}");
    }
    assert_eq!(extension_origin(None), None);
}
