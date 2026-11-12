// `extension_header_set` as a call from a frame (spec 090, contracts/extension-header.md): what it
// emits, which pictures it reads and sends, and what it never lets through. The installed
// extension is the `notes-like` fixture, whose bundle holds `icon.svg`.

use std::sync::Arc;

use base64::Engine;
use serde_json::{json, Value};

use super::dispatch::tests::{setup, Setup};
use super::dispatch::{call, CallContext, Emit};
use super::header::{set, SET_EVENT};
use crate::extensions::host::ExtensionHost;

fn events(s: &Setup) -> Vec<Value> {
    s.recorded
        .0
        .lock()
        .unwrap()
        .iter()
        .filter(|(event, _)| event == SET_EVENT)
        .map(|(_, payload)| payload.clone())
        .collect()
}

fn with_icon(icon: &str) -> Value {
    json!({"end": [{"type": "button", "id": "a", "label": "L", "icon": icon}]})
}

/// The same extension in a development version: no stored bundle, inline pictures only.
fn dev_frame(s: &Setup) -> CallContext {
    CallContext {
        db: s.ctx.db.clone(),
        host: Arc::new(ExtensionHost::default()),
        session: s
            .ctx
            .host
            .frames
            .open_dev(s.ctx.session.extension_id, "tab-dev"),
        device: s.ctx.device,
        emitter: Arc::clone(&s.recorded) as Arc<dyn Emit>,
    }
}

fn svg_url(body: &str) -> String {
    format!(
        "data:image/svg+xml;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(body)
    )
}

#[test]
fn a_valid_description_is_emitted_for_the_calling_frame_only() {
    let s = setup();
    let params = json!({"start": [{"type": "toggle", "id": "t", "label": "T", "text": "T", "pressed": true}]});
    assert_eq!(
        call(&s.ctx, "extension_header_set", &params).unwrap(),
        Value::Null
    );
    let sent = events(&s);
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0]["frame"], s.ctx.session.frame);
    assert_eq!(sent[0]["seq"], 1);
    assert_eq!(sent[0]["header"], params);
}

#[test]
fn a_frame_or_tab_named_in_the_params_changes_nothing() {
    let s = setup();
    let params = json!({"frame": "other", "tab": "tab-2", "tabId": "t", "end": []});
    call(&s.ctx, "extension_header_set", &params).unwrap();
    let sent = events(&s);
    assert_eq!(sent[0]["frame"], s.ctx.session.frame);
    assert_eq!(sent[0]["header"], json!({"end": []}));
}

#[test]
fn a_refused_description_emits_nothing_and_the_old_one_stays() {
    let s = setup();
    call(&s.ctx, "extension_header_set", &json!({"end": []})).unwrap();
    let refused = call(
        &s.ctx,
        "extension_header_set",
        &json!({"end": [{"type": "button", "id": "a"}]}),
    );
    assert_eq!(refused.unwrap_err().code.as_u16(), 3001);
    assert_eq!(events(&s).len(), 1);
    // The next one still counts on from the last that was emitted.
    call(&s.ctx, "extension_header_set", &json!({})).unwrap();
    assert_eq!(events(&s)[1]["seq"], 2);
}

#[test]
fn a_bundle_picture_is_read_and_sent_once_per_frame() {
    let s = setup();
    call(&s.ctx, "extension_header_set", &with_icon("icon.svg")).unwrap();
    let first = &events(&s)[0];
    let url = first["header"]["icons"]["icon.svg"].as_str().unwrap();
    let encoded = url
        .strip_prefix("data:image/svg+xml;base64,")
        .expect("a data URL of an SVG");
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .unwrap();
    assert!(String::from_utf8(bytes).unwrap().starts_with("<svg"));
    assert_eq!(first["header"]["end"][0]["icon"], "icon.svg");

    // The same file again, also on another control: nothing new to send.
    call(&s.ctx, "extension_header_set", &with_icon("icon.svg")).unwrap();
    assert!(events(&s)[1]["header"].get("icons").is_none());
}

#[test]
fn the_same_file_on_many_controls_is_sent_once() {
    let s = setup();
    let params = json!({"start": [
        {"type": "button", "id": "a", "label": "A", "icon": "icon.svg"},
        {"type": "button", "id": "b", "label": "B", "icon": "./icon.svg"},
    ], "end": [{"type": "menu", "id": "m", "label": "M", "icon": "icon.svg",
        "items": [{"type": "item", "id": "i", "label": "I", "icon": "icon.svg"}]}]});
    call(&s.ctx, "extension_header_set", &params).unwrap();
    let icons = events(&s)[0]["header"]["icons"].clone();
    let keys: Vec<&String> = icons.as_object().unwrap().keys().collect();
    // `./icon.svg` is its own name; the file behind it is read once for each name.
    assert_eq!(keys, ["./icon.svg", "icon.svg"]);
}

#[test]
fn a_picture_a_description_no_longer_names_is_forgotten_and_sent_again_when_used_again() {
    let s = setup();
    call(&s.ctx, "extension_header_set", &with_icon("icon.svg")).unwrap();
    call(&s.ctx, "extension_header_set", &json!({"end": []})).unwrap();
    call(&s.ctx, "extension_header_set", &with_icon("icon.svg")).unwrap();
    let sent = events(&s);
    assert!(sent[0]["header"]["icons"]["icon.svg"].is_string());
    assert!(sent[1]["header"].get("icons").is_none());
    assert!(sent[2]["header"]["icons"]["icon.svg"].is_string());
}

#[test]
fn a_new_frame_session_gets_the_pictures_again() {
    let s = setup();
    call(&s.ctx, "extension_header_set", &with_icon("icon.svg")).unwrap();
    let reloaded = CallContext {
        db: s.ctx.db.clone(),
        host: Arc::clone(&s.ctx.host),
        session: s.ctx.host.frames.open(
            s.ctx.session.extension_id,
            s.ctx.session.source.bundle().unwrap(),
            "tab-1",
        ),
        device: s.ctx.device,
        emitter: Arc::clone(&s.recorded) as Arc<dyn Emit>,
    };
    call(&reloaded, "extension_header_set", &with_icon("icon.svg")).unwrap();
    let sent = events(&s);
    assert!(sent[1]["header"]["icons"]["icon.svg"].is_string());
    assert_eq!(sent[1]["seq"], 1, "the count is per frame session");
}

#[test]
fn a_missing_or_unreadable_bundle_file_refuses_the_whole_description() {
    let s = setup();
    call(&s.ctx, "extension_header_set", &with_icon("icon.svg")).unwrap();
    let message = call(
        &s.ctx,
        "extension_header_set",
        &with_icon("icons/missing.svg"),
    )
    .unwrap_err();
    assert_eq!(message.code.as_u16(), 3001);
    assert!(message.message.contains("icons/missing.svg"));
    assert_eq!(events(&s).len(), 1);
    // The refused call changed nothing the next one relies on: the old picture is still "sent".
    call(&s.ctx, "extension_header_set", &with_icon("icon.svg")).unwrap();
    assert!(events(&s)[1]["header"].get("icons").is_none());
}

#[test]
fn a_file_that_is_not_a_picture_is_refused() {
    let s = setup();
    for icon in [
        "index.html",
        "haextension/manifest.json",
        "favicon.ico",
        "../icon.svg",
    ] {
        let error = call(&s.ctx, "extension_header_set", &with_icon(icon)).unwrap_err();
        assert_eq!(error.code.as_u16(), 3001, "{icon}");
    }
    assert!(events(&s).is_empty());
}

#[test]
fn an_installed_extension_cannot_send_an_inline_picture() {
    let s = setup();
    let error = call(
        &s.ctx,
        "extension_header_set",
        &with_icon(&svg_url("<svg/>")),
    )
    .unwrap_err();
    assert_eq!(error.code.as_u16(), 3001);
    assert!(events(&s).is_empty());
}

#[test]
fn a_development_version_may_send_an_inline_picture_up_to_256_kib_decoded() {
    let s = setup();
    let dev = dev_frame(&s);
    let url = |size: usize| {
        format!(
            "data:image/png;base64,{}",
            base64::engine::general_purpose::STANDARD.encode(vec![7u8; size])
        )
    };
    set(&dev, &with_icon(&url(256 * 1024))).unwrap();
    let error = set(&dev, &with_icon(&url(256 * 1024 + 1))).unwrap_err();
    assert_eq!(error.code.as_u16(), 3001);
    assert!(error.message.contains("256 KiB"));
    // A refusal is a validation error, not the limit error of notifications (7000).
    let sent = events(&s);
    assert_eq!(sent.len(), 1);
    assert!(sent[0]["header"]["icons"].as_object().unwrap().len() == 1);
}

#[test]
fn an_inline_picture_of_a_wrong_type_or_shape_is_refused() {
    let s = setup();
    let dev = dev_frame(&s);
    for icon in [
        "data:text/html;base64,PGI+",
        "data:image/png,plain",
        "data:image/png;base64,@@@@",
    ] {
        assert_eq!(
            set(&dev, &with_icon(icon)).unwrap_err().code.as_u16(),
            3001,
            "{icon}"
        );
    }
}

#[test]
fn an_svg_with_a_script_or_an_external_reference_passes_as_an_image_only() {
    // It is sent as a data URL and drawn through `<img>`, where a script does not run and nothing
    // is loaded; holzi neither executes nor rewrites it.
    let s = setup();
    let dev = dev_frame(&s);
    let hostile = svg_url(
        "<svg xmlns='http://www.w3.org/2000/svg'><script>alert(1)</script>\
         <image href='https://example.org/x.png'/></svg>",
    );
    set(&dev, &with_icon(&hostile)).unwrap();
    assert_eq!(events(&s)[0]["header"]["icons"][hostile.as_str()], hostile);
}

#[test]
fn a_hundred_pictures_of_a_hundred_kib_are_accepted_in_one_description() {
    // No limit on the number or the total size of pictures (research R3): 64 + 36 entries of two menus.
    let s = setup();
    let dev = dev_frame(&s);
    let url = |n: usize| {
        let mut bytes = vec![1u8; 100 * 1024];
        bytes[..8].copy_from_slice(&(n as u64).to_le_bytes());
        format!(
            "data:image/png;base64,{}",
            base64::engine::general_purpose::STANDARD.encode(bytes)
        )
    };
    let menu = |id: &str, from: usize, count: usize| {
        json!({"type": "menu", "id": id, "label": "M", "text": "M",
            "items": (from..from + count).map(|n| json!({
                "type": "item", "id": format!("i{n}"), "label": "I", "icon": url(n),
            })).collect::<Vec<_>>()})
    };
    set(
        &dev,
        &json!({"end": [menu("m1", 0, 64), menu("m2", 64, 36)]}),
    )
    .unwrap();
    assert_eq!(
        events(&s)[0]["header"]["icons"].as_object().unwrap().len(),
        100
    );
}

#[test]
fn a_table_of_icons_the_extension_sends_is_ignored() {
    let s = setup();
    let params = json!({"end": [], "icons": {"x.png": "data:image/png;base64,AAAA"}});
    call(&s.ctx, "extension_header_set", &params).unwrap();
    assert!(events(&s)[0]["header"].get("icons").is_none());
}

#[test]
fn the_method_is_on_the_allowlist_without_a_permission() {
    let s = setup();
    // No grant was made for this extension; the call needs none (FR-023).
    assert!(call(&s.ctx, "extension_header_set", &json!({})).is_ok());
}
