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
    let moved = Place(vec![Placement::offset(300.4, 120.0)]);
    shift_box(&mut element, Some(&moved));
    assert_eq!(
        element["box"],
        json!({ "x": 310, "y": 140, "w": 100, "h": 50 })
    );
    // Where the frame lies is not known: no box, since a wrong one would send a click astray.
    shift_box(&mut element, None);
    assert!(element.get("box").is_none());
    // An element without a box stays without one.
    let mut plain = json!({ "n": 1 });
    shift_box(&mut plain, Some(&moved));
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

#[test]
fn a_scaled_frame_moves_a_point_by_its_whole_quad_not_by_its_corner() {
    // An iframe of 200 x 100 with `transform: scale(2)`, its content starting 10 px inside it, and
    // its border box at (50, 60) in the holding viewport.
    let model = json!({
        "content": [70.0, 80.0, 470.0, 80.0, 470.0, 280.0, 70.0, 280.0],
        "border": [50.0, 60.0, 450.0, 60.0, 450.0, 260.0, 50.0, 260.0],
        "width": 200, "height": 100
    });
    let placement = page::placement(&model).unwrap();
    assert_eq!(placement.across, (2.0, 0.0));
    assert_eq!(placement.down, (0.0, 2.0));
    // The point (10, 5) of the frame is 20 px right and 10 px down of its corner.
    assert_eq!(placement.apply(10.0, 5.0), (90.0, 90.0));

    // A box is the one that holds its four moved corners.
    let mut element = json!({ "n": 1, "box": { "x": 10, "y": 5, "w": 30, "h": 20 } });
    shift_box(&mut element, Some(&Place(vec![placement])));
    assert_eq!(
        element["box"],
        json!({ "x": 90, "y": 90, "w": 60, "h": 40 })
    );

    // A turned frame (90 degrees): across goes down the page.
    let turned = json!({
        "content": [100.0, 0.0, 100.0, 50.0, 0.0, 50.0, 0.0, 0.0],
        "border": [100.0, 0.0, 100.0, 50.0, 0.0, 50.0, 0.0, 0.0],
        "width": 50, "height": 100
    });
    let placement = page::placement(&turned).unwrap();
    assert_eq!(placement.across, (0.0, 1.0));
    assert_eq!(placement.apply(10.0, 0.0), (100.0, 10.0));

    // Without a usable size the frame is only moved.
    let flat = json!({ "content": [5.0, 6.0, 9.0, 6.0, 9.0, 9.0, 5.0, 9.0] });
    assert_eq!(page::placement(&flat), Some(Placement::offset(5.0, 6.0)));
    assert!(page::placement(&json!({})).is_none());

    // Several frames: the points go through each in turn.
    let chain = Place(vec![
        Placement::offset(1.0, 2.0),
        Placement::offset(10.0, 20.0),
    ]);
    assert_eq!(chain.apply(0.0, 0.0), (11.0, 22.0));
}

#[test]
fn the_rect_of_a_frame_element_moves_with_its_box_and_goes_when_the_place_is_unknown() {
    let place = Place(vec![Placement::offset(10.0, 20.0)]);
    let mut element = json!({
        "box": { "x": 1, "y": 2, "w": 3, "h": 4 },
        "rect": { "x": 5.0, "y": 6.0, "w": 7.0, "h": 8.0 }
    });
    shift_box(&mut element, Some(&place));
    assert_eq!(element["box"], json!({ "x": 11, "y": 22, "w": 3, "h": 4 }));
    assert_eq!(element["rect"], json!({ "x": 15, "y": 26, "w": 7, "h": 8 }));
    shift_box(&mut element, None);
    assert!(element.get("box").is_none() && element.get("rect").is_none());
}
