//! Spec 065 T112 (frames): which frame a number belongs to, how a call is rewritten for it and how
//! the place of a frame moves what it reports.

use serde_json::json;

use super::*;

fn slice(first: u32, count: u32, frame: &str) -> FrameSlice {
    FrameSlice {
        first,
        count,
        frame: frame.to_owned(),
        host: None,
        chain: Vec::new(),
    }
}

#[test]
fn a_slice_owns_its_numbers_and_counts_inside_from_one() {
    let slice = slice(11, 5, "f1");
    assert_eq!(slice.local(11), Some(1));
    assert_eq!(slice.local(15), Some(5));
    assert_eq!(slice.local(10), None);
    assert_eq!(slice.local(16), None);
}

#[test]
fn a_number_of_the_main_document_is_not_routed() {
    let slices = [slice(11, 5, "f1")];
    for key in ["target", "focus", "focusElement", "choose", "scrollTo"] {
        let mut args = json!({ key: 4 });
        assert!(route(&slices, &mut args).is_none(), "{key}");
        assert_eq!(args[key], 4, "{key} is left alone");
    }
}

#[test]
fn a_number_of_a_frame_is_rewritten_to_its_own() {
    let slices = [slice(11, 5, "f1"), slice(16, 3, "f2")];
    let mut args = json!({ "target": 12 });
    assert_eq!(route(&slices, &mut args).unwrap().frame, "f1");
    assert_eq!(args["target"], 2);

    let mut args = json!({ "choose": 18, "value": "2" });
    assert_eq!(route(&slices, &mut args).unwrap().frame, "f2");
    assert_eq!(args, json!({ "choose": 3, "value": "2" }));

    let mut args = json!({ "media": { "element": 16, "action": "play", "to": 0.0 } });
    assert_eq!(route(&slices, &mut args).unwrap().frame, "f2");
    assert_eq!(args["media"]["element"], 1);
    assert_eq!(args["media"]["action"], "play");
}

#[test]
fn a_number_past_every_frame_is_not_routed() {
    let slices = [slice(11, 5, "f1")];
    let mut args = json!({ "target": 99 });
    assert!(route(&slices, &mut args).is_none());
    assert!(route(&[], &mut json!({ "target": 12 })).is_none());
    // Without a number nothing is routed: the page's own questions stay on the main document.
    assert!(route(&slices, &mut json!({ "active": true })).is_none());
    assert!(route(&slices, &mut json!({ "probe": { "element": 12 } })).is_none());
}

#[test]
fn the_box_of_a_frame_element_moves_by_the_place_of_the_frame() {
    let mut element = json!({ "n": 12, "box": { "x": 10, "y": 20, "w": 100, "h": 50 } });
    shift_box(&mut element, Some((300.4, 120.0)));
    assert_eq!(
        element["box"],
        json!({ "x": 310, "y": 140, "w": 100, "h": 50 })
    );
    // Where the frame lies is not known: no box, since a wrong one would send a click astray.
    shift_box(&mut element, None);
    assert!(element.get("box").is_none());
    // An element without a box stays without one.
    let mut plain = json!({ "n": 1 });
    shift_box(&mut plain, Some((1.0, 1.0)));
    assert!(plain.get("box").is_none());
}

fn node(id: &str, parent: Option<usize>) -> FrameNode {
    FrameNode {
        id: id.to_owned(),
        url: format!("https://{id}.example"),
        parent,
    }
}

#[test]
fn the_way_up_counts_the_frame_and_each_process_above_it_and_nothing_between() {
    let hop = |host: Option<&str>, frame: &str| Hop {
        host: host.map(str::to_owned),
        frame: frame.to_owned(),
    };
    // A frame of the page's own process: one step, measured in that process.
    let page = Pending {
        session: "s".to_owned(),
        host: None,
        nodes: vec![node("main", None), node("a", Some(0)), node("b", Some(1))],
        base: Vec::new(),
    };
    assert_eq!(page.chain("a"), vec![hop(None, "a")]);
    // The browser reports a box in the root frame of the process, so `b` inside `a` is one step too.
    assert_eq!(page.chain("b"), vec![hop(None, "b")]);

    // A frame of another process, held by the page, and a frame inside it: the second step
    // crosses into the process of the first.
    let inner = Pending {
        session: "x".to_owned(),
        host: Some("p".to_owned()),
        nodes: vec![node("p", None), node("c", Some(0))],
        base: page.chain("p"),
    };
    assert_eq!(inner.chain("c"), vec![hop(Some("p"), "c"), hop(None, "p")]);
}

#[test]
fn a_screenshot_keeps_only_the_elements_in_the_view_of_their_frame() {
    let inside = json!({ "n": 1, "visible": true });
    let outside = json!({ "n": 2, "visible": false });
    let unknown = json!({ "n": 3 });
    assert!(shown_in_frame(true, &inside));
    assert!(!shown_in_frame(true, &outside));
    assert!(!shown_in_frame(true, &unknown));
    // A reading for the agent lists every element, in view or not.
    assert!(shown_in_frame(false, &outside));
}
