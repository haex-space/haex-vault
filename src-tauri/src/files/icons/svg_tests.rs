use super::*;

fn clean(svg: &str) -> String {
    String::from_utf8(sanitize_svg(svg.as_bytes()).expect("sanitized")).expect("utf-8")
}

const ICON: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16" width="16" height="16"><path d="M1 1h14v14H1z" fill="#336699" stroke="none"/><g opacity="0.5"><circle cx="8" cy="8" r="3"/></g></svg>"##;

#[test]
fn a_plain_icon_keeps_its_shapes_and_attributes() {
    let out = clean(ICON);
    assert!(out.contains(r#"d="M1 1h14v14H1z""#));
    assert!(out.contains(r##"fill="#336699""##));
    assert!(out.contains(r#"<circle cx="8" cy="8" r="3"/>"#));
    assert!(out.contains(r#"viewBox="0 0 16 16""#));
    assert!(out.contains(r#"xmlns="http://www.w3.org/2000/svg""#));
}

#[test]
fn the_root_gets_the_namespace_an_img_needs() {
    let out = clean(r#"<svg viewBox="0 0 1 1"><path d="M0 0h1v1z"/></svg>"#);
    assert!(out.starts_with(r#"<svg xmlns="http://www.w3.org/2000/svg""#));
}

#[test]
fn a_script_is_dropped_with_its_content() {
    let out = clean(
        r#"<svg xmlns="http://www.w3.org/2000/svg"><script>alert(1)</script><path d="M0 0"/></svg>"#,
    );
    assert!(!out.contains("script") && !out.contains("alert"));
    assert!(out.contains("<path"));
}

#[test]
fn event_handlers_are_dropped_whatever_their_case() {
    let out = clean(
        r#"<svg xmlns="http://www.w3.org/2000/svg" onload="x()"><path d="M0 0" onclick="y()" ONMOUSEOVER="z()"/></svg>"#,
    );
    assert!(!out.to_lowercase().contains("on"), "{out}");
    assert!(out.contains("<path"));
}

#[test]
fn foreign_object_anchors_animations_and_frames_are_dropped() {
    for element in [
        "<foreignObject><div>hi</div></foreignObject>",
        r#"<a href="https://example.com"><path d="M0 0"/></a>"#,
        r#"<animate attributeName="x" to="1"/>"#,
        r#"<set attributeName="x" to="1"/>"#,
        r#"<iframe src="https://example.com"/>"#,
        r#"<style>@import url(https://example.com/x.css);</style>"#,
        r#"<text>hello</text>"#,
    ] {
        let svg = format!(
            r#"<svg xmlns="http://www.w3.org/2000/svg">{element}<rect width="1" height="1"/></svg>"#
        );
        let out = clean(&svg);
        for word in [
            "foreignObject",
            "<a",
            "animate",
            "<set",
            "iframe",
            "style",
            "hello",
            "example",
        ] {
            assert!(!out.contains(word), "{word} in {out}");
        }
        assert!(out.contains("<rect"), "the rest stays: {out}");
    }
}

#[test]
fn external_references_are_dropped_and_references_inside_stay() {
    let out = clean(
        r##"<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink"><defs><path id="p" d="M0 0"/></defs><use href="#p"/><use xlink:href="https://example.com/x.svg#p"/><use href="x.svg#p"/></svg>"##,
    );
    assert!(out.contains(r##"href="#p""##));
    assert!(!out.contains("example.com") && !out.contains("x.svg"));
}

#[test]
fn url_values_may_only_point_inside_the_image() {
    let out = clean(
        r##"<svg xmlns="http://www.w3.org/2000/svg"><defs><linearGradient id="g"><stop offset="0"/></linearGradient></defs><rect fill="url(#g)" width="1" height="1"/><rect fill="url(https://example.com/a)" width="1" height="1"/><rect fill="url( 'data:image/svg+xml,x')" width="1" height="1"/></svg>"##,
    );
    assert!(out.contains(r##"fill="url(#g)""##));
    assert!(!out.contains("example.com") && !out.contains("data:"));
}

#[test]
fn a_style_attribute_keeps_only_safe_declarations() {
    let out = clean(
        r##"<svg xmlns="http://www.w3.org/2000/svg"><rect style="fill:#fff;behavior:url(x.htc);stroke:url(https://example.com/a);opacity:.5" width="1" height="1"/></svg>"##,
    );
    assert!(out.contains(r##"style="fill:#fff;opacity:.5""##), "{out}");
    assert!(!out.contains("behavior") && !out.contains("example.com"));
}

#[test]
fn an_embedded_png_stays_only_on_an_image_element() {
    let data = "data:image/png;base64,iVBORw0KGgo=";
    let out = clean(&format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg"><image href="{data}" width="1" height="1"/><image href="data:text/html;base64,PHNjcmlwdD4=" width="1" height="1"/><use href="{data}"/></svg>"#
    ));
    assert_eq!(out.matches(data).count(), 1, "{out}");
    assert!(!out.contains("text/html"));
}

#[test]
fn a_doctype_an_entity_and_a_processing_instruction_are_refused() {
    for svg in [
        r#"<!DOCTYPE svg [<!ENTITY a "aaaa">]><svg xmlns="http://www.w3.org/2000/svg"><title>&a;</title></svg>"#,
        r#"<!DOCTYPE svg PUBLIC "-//W3C//DTD SVG 1.1//EN" "http://www.w3.org/Graphics/SVG/1.1/DTD/svg11.dtd"><svg xmlns="http://www.w3.org/2000/svg"/>"#,
        r#"<?xml-stylesheet href="https://example.com/s.css"?><svg xmlns="http://www.w3.org/2000/svg"/>"#,
        r#"<svg xmlns="http://www.w3.org/2000/svg"><title>&unknown;</title></svg>"#,
    ] {
        assert!(sanitize_svg(svg.as_bytes()).is_err(), "{svg}");
    }
}

#[test]
fn the_xml_declaration_and_comments_are_dropped() {
    let out = clean(
        r#"<?xml version="1.0" encoding="UTF-8"?><!-- made with a tool --><svg xmlns="http://www.w3.org/2000/svg"><!-- x --><path d="M0 0"/></svg>"#,
    );
    assert!(!out.contains("<?xml") && !out.contains("<!--"));
}

#[test]
fn foreign_namespaces_such_as_inkscape_metadata_are_dropped() {
    let out = clean(
        r##"<svg xmlns="http://www.w3.org/2000/svg" xmlns:inkscape="http://www.inkscape.org/namespaces/inkscape" xmlns:sodipodi="x" inkscape:version="1"><sodipodi:namedview id="base"/><metadata><rdf:RDF xmlns:rdf="r"/></metadata><path d="M0 0" inkscape:label="x"/></svg>"##,
    );
    assert!(!out.contains("inkscape") && !out.contains("sodipodi") && !out.contains("metadata"));
    assert!(out.contains("<path"));
}

#[test]
fn the_svg_prefix_is_written_without_a_prefix() {
    let out = clean(
        r#"<svg:svg xmlns:svg="http://www.w3.org/2000/svg" viewBox="0 0 1 1"><svg:path d="M0 0"/></svg:svg>"#,
    );
    assert!(out.starts_with("<svg ") && out.contains("<path") && !out.contains("svg:"));
}

#[test]
fn title_text_stays_and_other_text_goes() {
    let out = clean(
        r#"<svg xmlns="http://www.w3.org/2000/svg"><title>A &amp; B</title><g>stray text<path d="M0 0"/></g></svg>"#,
    );
    assert!(out.contains("<title>A &amp; B</title>"));
    assert!(!out.contains("stray"));
}

#[test]
fn limits_and_malformed_files_are_refused() {
    let big = format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg"><title>{}</title></svg>"#,
        "a".repeat(MAX_BYTES)
    );
    assert_eq!(sanitize_svg(big.as_bytes()), Err(SvgError::TooLarge));

    let deep = format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg">{}{}</svg>"#,
        "<g>".repeat(MAX_DEPTH + 1),
        "</g>".repeat(MAX_DEPTH + 1)
    );
    assert_eq!(sanitize_svg(deep.as_bytes()), Err(SvgError::TooDeep));

    assert_eq!(sanitize_svg(&[0xff, 0xfe, 0x00]), Err(SvgError::NotUtf8));
    assert!(sanitize_svg(b"<svg><g></svg>").is_err(), "mismatched tags");
    assert!(sanitize_svg(b"<svg").is_err(), "cut off");
    assert_eq!(sanitize_svg(b"<html/>"), Err(SvgError::NoRoot));
    assert_eq!(sanitize_svg(b""), Err(SvgError::NoRoot));
    assert!(
        sanitize_svg(br#"<svg xmlns="http://www.w3.org/2000/svg"/><svg/>"#).is_err(),
        "a second root"
    );
}

#[test]
fn sanitizing_twice_gives_the_same_bytes() {
    for svg in [
        ICON,
        r##"<?xml version="1.0"?><svg xmlns="http://www.w3.org/2000/svg" onload="x()"><defs><linearGradient id="g"/></defs><rect fill="url(#g)" style="fill:red;x:y" width="1"/><script/></svg>"##,
    ] {
        let once = sanitize_svg(svg.as_bytes()).expect("once");
        let twice = sanitize_svg(&once).expect("twice");
        assert_eq!(once, twice);
    }
}
