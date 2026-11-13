//! Spec 065 T126 (FR-063): which browser tools a model without images is offered.

use serde_json::json;

use super::*;

fn spec(name: &str) -> ToolSpec {
    ToolSpec {
        name: name.to_owned(),
        description: String::new(),
        input_schema: json!({}),
    }
}

fn names(tools: &[ToolSpec]) -> Vec<&str> {
    tools.iter().map(|tool| tool.name.as_str()).collect()
}

fn browser_tools() -> Vec<ToolSpec> {
    [
        "browser_page_read",
        "browser_page_screenshot",
        "browser_click",
        "browser_click_at",
        "browser_drag",
        "browser_hover",
        "browser_press_key",
        "contacts_search",
    ]
    .into_iter()
    .map(spec)
    .collect()
}

#[test]
fn a_model_without_images_works_with_numbers() {
    let mut tools = browser_tools();
    retain_usable(&mut tools, false);
    assert_eq!(
        names(&tools),
        [
            "browser_page_read",
            "browser_click",
            "browser_hover",
            "browser_press_key",
            "contacts_search"
        ]
    );
}

#[test]
fn a_model_with_images_keeps_every_tool() {
    let mut tools = browser_tools();
    retain_usable(&mut tools, true);
    assert_eq!(tools.len(), 8);
}

#[test]
fn the_gated_names_are_the_catalog_ids_of_the_position_tools() {
    for id in [
        "browser.page.screenshot",
        "browser.click_at",
        "browser.drag",
    ] {
        assert!(NEED_IMAGES.contains(&id.replace('.', "_").as_str()), "{id}");
    }
}
