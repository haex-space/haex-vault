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
    // A cosmetic filter, a scriptlet and a redirect rule can hide or change a page; none of
    // them blocks a request here.
    let filter = filter(
        "example.com##.banner\nexample.com##+js(abort-on-property-read, x)\n||cdn.example^$redirect=noopjs\n",
    );
    assert!(!filter.blocks(&info(
        "https://example.com/page",
        "https://example.com/",
        "Document"
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
    let mut main = info("https://frames.example/", "", "Document");
    main.main_frame = true;
    assert!(
        !filter.blocks(&main),
        "a subdocument rule leaves a page alone"
    );
    let mut page = info("https://pages.example/", "", "Document");
    page.main_frame = true;
    assert!(filter.blocks(&page));
}

#[test]
fn a_request_without_a_readable_address_is_not_blocked() {
    let filter = filter("||ads.example^\n");
    assert!(!filter.blocks(&info("not an address", "", "Image")));
}

#[test]
fn a_paused_request_is_read_with_its_referer_as_source() {
    let params = json!({
        "resourceType": "Script",
        "request": {
            "url": "https://metrics.example/m.js",
            "method": "GET",
            "headers": { "Referer": "https://shop.example/" }
        }
    });
    let request = request_info(&params, false).expect("a request");
    assert_eq!(request.url, "https://metrics.example/m.js");
    assert_eq!(request.source, "https://shop.example/");
    assert_eq!(request.kind, "Script");
    assert!(request_info(&json!({}), false).is_none());
}
