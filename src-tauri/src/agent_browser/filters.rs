//! Blocking requests the way an ad blocker does (spec 065 FR-045, research R16): Brave's `adblock`
//! engine reads filter lists in the Adblock Plus and uBlock Origin syntax and decides, per request,
//! whether it is blocked. Only the network filters of a list are loaded, so a list can stop
//! requests and nothing else: cosmetic filters and scriptlets never run, and no redirect resource
//! is there, so a `$redirect` rule stops its request as uBlock Origin does, without the stand-in
//! uBlock serves in its place.

use std::sync::{Arc, PoisonError, RwLock};

use adblock::lists::{FilterSet, ParseOptions, RuleTypes};
use adblock::request::Request;
use adblock::Engine;
use serde_json::Value;

/// The engine over the lists that are switched on.
pub struct TrackerFilter {
    engine: Engine,
}

/// The engine every tab asks per request: built once over the lists, replaced in place when they
/// are built anew, so a tab opened before the first download or a refresh gets the new lists.
#[derive(Clone, Default)]
pub struct LiveFilter(Arc<RwLock<Option<Arc<TrackerFilter>>>>);

impl LiveFilter {
    /// The engine now; `None` while no list is there.
    pub fn get(&self) -> Option<Arc<TrackerFilter>> {
        self.0
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    pub fn set(&self, filter: Option<Arc<TrackerFilter>>) {
        *self.0.write().unwrap_or_else(PoisonError::into_inner) = filter;
    }
}

/// What the engine needs to know about one request a page sends.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RequestInfo {
    pub url: String,
    /// The website of the page the request comes from; empty when unknown.
    pub source: String,
    /// The browser's resource type (`Image`, `Script`, `XHR`, …).
    pub kind: String,
    /// The request comes from the tab's own frame; a document of it is the tab's page.
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
        Request::new(&request.url, &request.source, kind, &request.method)
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
/// and the page it comes from. That is `frame_page`, the address of the document in the request's
/// frame, as uBlock Origin takes it; without a website there, the request's `Referer`. A page of
/// the tab's own frame is its own source, a frame's document comes from its `Referer`, since the
/// frame's old document is not where it comes from.
pub fn request_info(
    params: &Value,
    main_frame: bool,
    frame_page: Option<&str>,
) -> Option<RequestInfo> {
    let request = &params["request"];
    let url = request["url"].as_str()?.to_owned();
    let kind = params["resourceType"]
        .as_str()
        .unwrap_or_default()
        .to_owned();
    let referer = request["headers"]["Referer"].as_str().unwrap_or_default();
    let source = match (kind.as_str(), main_frame) {
        ("Document", true) => url.clone(),
        ("Document", false) => referer.to_owned(),
        _ => frame_page
            .filter(|page| is_website(page))
            .unwrap_or(referer)
            .to_owned(),
    };
    Some(RequestInfo {
        url,
        source,
        kind,
        main_frame,
        method: request["method"].as_str().unwrap_or("GET").to_owned(),
    })
}

/// Whether `address` is a page of a website (not `about:blank`, `data:` or the like).
fn is_website(address: &str) -> bool {
    url::Url::parse(address).is_ok_and(|url| matches!(url.scheme(), "http" | "https"))
}
