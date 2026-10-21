//! Which connections may open a WebSocket at all (spec 049, FR-005, research R2): only a browser
//! extension, recognised by the `Origin` header the browser sets itself. A web page cannot forge
//! it; a local program can, which the spec accepts (Assumptions).

const EXTENSION_SCHEMES: [&str; 2] = ["chrome-extension://", "moz-extension://"];

/// The origin of a browser extension, or `None` for a web page, `null`, a missing header or
/// anything else.
pub fn extension_origin(header: Option<&str>) -> Option<&str> {
    let origin = header?;
    let host = EXTENSION_SCHEMES
        .iter()
        .find_map(|scheme| origin.strip_prefix(scheme))?;
    let valid_host = !host.is_empty()
        && host
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
    valid_host.then_some(origin)
}
