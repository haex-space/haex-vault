//! Blocking requests the way an ad blocker does (spec 065 FR-045, research R16): Brave's `adblock`
//! engine reads filter lists in the Adblock Plus and uBlock Origin syntax and decides, per request,
//! whether it is blocked. Only the network filters of a list are loaded, so a list can stop
//! requests and nothing else: cosmetic filters, scriptlets and redirects never run.

use adblock::lists::{FilterSet, ParseOptions, RuleTypes};
use adblock::request::Request;
use adblock::Engine;
use serde_json::Value;

/// The engine over the lists that are switched on.
pub struct TrackerFilter {
    engine: Engine,
}

/// What the engine needs to know about one request a page sends.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RequestInfo {
    pub url: String,
    /// The website of the page the request comes from; empty when unknown.
    pub source: String,
    /// The browser's resource type (`Image`, `Script`, `XHR`, …).
    pub kind: String,
    /// The request loads a document into the tab's own frame.
    pub main_frame: bool,
    pub method: String,
}

impl TrackerFilter {
    /// The engine over the texts of `lists`; a line that is no network filter is ignored.
    pub fn new(lists: impl IntoIterator<Item = String>) -> Self {
        let mut set = FilterSet::new(false);
        let options = ParseOptions {
            rule_types: RuleTypes::NetworkOnly,
            ..ParseOptions::default()
        };
        for list in lists {
            set.add_filter_list(list, options);
        }
        Self {
            engine: Engine::new_with_filter_set(set),
        }
    }

    /// Whether the lists block `request`. A request the engine cannot read is not blocked.
    pub fn blocks(&self, request: &RequestInfo) -> bool {
        let kind = engine_type(&request.kind, request.main_frame);
        let source = if request.source.is_empty() && kind == "document" {
            request.url.as_str()
        } else {
            request.source.as_str()
        };
        Request::new(&request.url, source, kind, &request.method)
            .map(|parsed| self.engine.check_network_request(&parsed).should_block())
            .unwrap_or(false)
    }
}

/// The engine's name of a resource type of the browser.
fn engine_type(kind: &str, main_frame: bool) -> &'static str {
    match kind {
        "Document" if main_frame => "document",
        "Document" => "subdocument",
        "Stylesheet" => "stylesheet",
        "Image" => "image",
        "Media" => "media",
        "Font" => "font",
        "Script" => "script",
        "XHR" | "Fetch" => "xhr",
        "Ping" => "ping",
        "WebSocket" => "websocket",
        "CSPViolationReport" => "csp_report",
        _ => "other",
    }
}

/// What a paused request tells about itself (`Fetch.requestPaused`): the address, method and type,
/// and the website it comes from, taken from its `Referer`; a document without one counts as its
/// own source.
pub fn request_info(params: &Value, main_frame: bool) -> Option<RequestInfo> {
    let request = &params["request"];
    Some(RequestInfo {
        url: request["url"].as_str()?.to_owned(),
        source: request["headers"]["Referer"]
            .as_str()
            .unwrap_or_default()
            .to_owned(),
        kind: params["resourceType"]
            .as_str()
            .unwrap_or_default()
            .to_owned(),
        main_frame,
        method: request["method"].as_str().unwrap_or("GET").to_owned(),
    })
}
