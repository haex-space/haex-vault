use serde_json::json;

use super::filters::{request_info, RequestInfo, TrackerFilter};

fn info(url: &str, source: &str, kind: &str) -> RequestInfo {
    RequestInfo {
        url: url.to_owned(),
        source: source.to_owned(),
        kind: kind.to_owned(),
        main_frame: false,
        method: "GET".to_owned(),
    }
}

fn filter(list: &str) -> TrackerFilter {
    TrackerFilter::new([list.to_owned()])
}

#[test]
fn a_network_filter_blocks_its_requests_and_an_exception_lets_them_through() {
    let filter = filter("||ads.example^\n@@||ads.example/allowed.js^\n");
    assert!(filter.blocks(&info(
        "https://ads.example/banner.png",
        "https://news.example/",
        "Image"
    )));
    assert!(!filter.blocks(&info(
        "https://ads.example/allowed.js",
        "https://news.example/",
        "Script"
    )));
    assert!(!filter.blocks(&info(
        "https://news.example/logo.png",
        "https://news.example/",
        "Image"
    )));
}

#[test]
fn options_of_a_filter_apply_resource_type_and_third_party() {
    let filter = filter("||metrics.example^$script,third-party\n");
    assert!(filter.blocks(&info(
        "https://metrics.example/m.js",
        "https://shop.example/",
        "Script"
    )));
    assert!(!filter.blocks(&info(
        "https://metrics.example/m.png",
        "https://shop.example/",
        "Image"
    )));
    assert!(
        !filter.blocks(&info(
            "https://metrics.example/m.js",
            "https://metrics.example/",
            "Script"
        )),
        "the website's own request is first party"
    );
}

#[test]
fn only_network_filters_count() {
    // A cosmetic filter and a scriptlet can hide or change a page; neither blocks a request.
    let filter = filter("example.com##.banner\nexample.com##+js(abort-on-property-read, x)\n");
    assert!(!filter.blocks(&info(
        "https://example.com/page",
        "https://example.com/",
        "Document"
    )));
}

#[test]
fn a_redirect_rule_stops_its_request_without_a_stand_in() {
    // uBlock Origin blocks the request and serves a stand-in; holzi has no stand-ins, so the
    // request is only blocked, and nothing else of the website is touched.
    let filter = filter("||cdn.example/tag.js$script,redirect=noopjs\n");
    assert!(filter.blocks(&info(
        "https://cdn.example/tag.js",
        "https://shop.example/",
        "Script"
    )));
    assert!(!filter.blocks(&info(
        "https://cdn.example/app.js",
        "https://shop.example/",
        "Script"
    )));
}

#[test]
fn a_document_of_the_tabs_own_frame_is_a_document_and_a_frame_in_it_a_subdocument() {
    let filter = filter("||frames.example^$subdocument\n||pages.example^$document\n");
    let frame = info(
        "https://frames.example/",
        "https://shop.example/",
        "Document",
    );
    assert!(filter.blocks(&frame));
    let mut main = info(
        "https://frames.example/",
        "https://frames.example/",
        "Document",
    );
    main.main_frame = true;
    assert!(
        !filter.blocks(&main),
        "a subdocument rule leaves a page alone"
    );
    let mut page = info(
        "https://pages.example/",
        "https://pages.example/",
        "Document",
    );
    page.main_frame = true;
    assert!(filter.blocks(&page));
}

#[test]
fn a_request_without_a_readable_address_is_not_blocked() {
    let filter = filter("||ads.example^\n");
    assert!(!filter.blocks(&info("not an address", "", "Image")));
}

fn paused(kind: &str, url: &str, referer: Option<&str>) -> serde_json::Value {
    let headers = referer.map_or_else(|| json!({}), |referer| json!({ "Referer": referer }));
    json!({
        "resourceType": kind,
        "request": { "url": url, "method": "GET", "headers": headers }
    })
}

#[test]
fn a_paused_request_comes_from_the_page_in_its_frame() {
    // An image a third-party stylesheet loads names the stylesheet as `Referer`; it still comes
    // from the page, as uBlock Origin judges it.
    let params = paused(
        "Image",
        "https://metrics.example/m.gif",
        Some("https://metrics.example/style.css"),
    );
    let request = request_info(&params, false, Some("https://shop.example/a")).expect("a request");
    assert_eq!(request.url, "https://metrics.example/m.gif");
    assert_eq!(request.source, "https://shop.example/a");
    assert_eq!(request.kind, "Image");
    assert!(filter("||metrics.example^$third-party\n").blocks(&request));
    assert!(request_info(&json!({}), false, None).is_none());
}

#[test]
fn without_a_known_page_the_referer_is_the_source() {
    let script = paused(
        "Script",
        "https://metrics.example/m.js",
        Some("https://shop.example/"),
    );
    for page in [None, Some("about:blank")] {
        let request = request_info(&script, false, page).expect("a request");
        assert_eq!(request.source, "https://shop.example/", "{page:?}");
    }
}

#[test]
fn a_page_without_referer_still_names_its_website_to_the_lists() {
    // `Referrer-Policy: no-referrer` leaves no `Referer`; rules bound to the website still apply.
    let filter = filter("||cdn.example/widget.js$domain=shop.example\n");
    let script = paused("Script", "https://cdn.example/widget.js", None);
    let request = request_info(&script, false, Some("https://shop.example/")).expect("a request");
    assert!(filter.blocks(&request));
}

#[test]
fn a_page_is_its_own_source_and_a_frame_comes_from_its_referer() {
    let page = paused(
        "Document",
        "https://news.example/a",
        Some("https://search.example/"),
    );
    let request = request_info(&page, true, Some("https://old.example/")).expect("a request");
    assert_eq!(request.source, "https://news.example/a");
    let frame = paused(
        "Document",
        "https://frames.example/",
        Some("https://news.example/a"),
    );
    let request =
        request_info(&frame, false, Some("https://frames.example/old")).expect("a request");
    assert_eq!(request.source, "https://news.example/a");
}
