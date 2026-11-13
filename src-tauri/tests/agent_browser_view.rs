//! Spec 065 US8 (FR-056 to FR-058) against a real Chromium and the fixture site over HTTPS: a
//! screenshot states its size and scale, `marks` draws the element numbers on it (the same numbers
//! as `browser.page.read`), a region or an element comes larger with its region and zoom stated,
//! and invalid asks are refused. Without a Chromium-based browser the test says so and stops.

#[path = "common/agent_browser_fixture.rs"]
#[allow(dead_code)]
mod agent_browser_fixture;
#[path = "common/agent_browser_scene.rs"]
mod agent_browser_scene;

use serde_json::{json, Value};

use agent_browser_scene::{content, scene_with, Scene};
use holzi_lib::agent_browser::sites::Site;
use holzi_lib::chat::tools::native_action::NativeReply;
use holzi_lib::mcp_server::grants::{SiteGrant, SiteLevel};

async fn open(scene: &Scene, path: &str) -> NativeReply {
    scene
        .auto("browser.open", json!({ "url": scene.site.url(path) }))
        .await
        .unwrap_or_else(|error| panic!("open {path}: {error:?}"))
}

fn element(page: &NativeReply, name: &str) -> Value {
    content(page)["elements"]
        .as_array()
        .unwrap()
        .iter()
        .find(|element| element["name"] == name)
        .unwrap_or_else(|| panic!("no element {name}: {}", page.result))
        .clone()
}

async fn shot(scene: &Scene, input: Value) -> NativeReply {
    scene
        .auto("browser.page.screenshot", input)
        .await
        .expect("a screenshot")
}

/// The picture of a reply, decoded.
fn decoded(reply: &NativeReply) -> image::RgbImage {
    assert_eq!(reply.images.len(), 1);
    assert_eq!(reply.images[0].media_type, "image/jpeg");
    image::load_from_memory(&reply.images[0].bytes)
        .expect("a JPEG")
        .to_rgb8()
}

/// How many pixels are close to one of the colours the marks use.
fn mark_coloured(image: &image::RgbImage) -> usize {
    let colours: Vec<image::Rgb<u8>> = (0..6)
        .map(holzi_lib::agent_browser::marks::colour)
        .collect();
    image
        .pixels()
        .filter(|pixel| {
            colours.iter().any(|colour| {
                (0..3)
                    .map(|i| (i32::from(pixel[i]) - i32::from(colour[i])).pow(2))
                    .sum::<i32>()
                    < 40 * 40
            })
        })
        .count()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_screenshot_states_its_size_and_marks_draw_the_numbers_of_the_page() {
    let Some(scene) = scene_with(true, SiteLevel::Operate).await else {
        return;
    };
    let page = open(&scene, "/canvas.html").await;
    let plain = shot(&scene, json!({ "tab": "t1" })).await;
    let image = decoded(&plain);
    assert_eq!(plain.result["image"]["width"], image.width());
    assert_eq!(plain.result["image"]["height"], image.height());
    assert_eq!(plain.result["view"]["width"], image.width());
    assert!(plain.result["view"]["scale"].as_f64().unwrap() <= 1.0);
    assert!(plain.result.get("region").is_none());
    assert!(content(&plain).get("marks").is_none());

    let marked = shot(&scene, json!({ "tab": "t1", "marks": true })).await;
    let listed = content(&marked)["marks"].as_array().unwrap().clone();
    let number = |name: &str| {
        listed
            .iter()
            .find(|mark| mark["name"] == name)
            .unwrap_or_else(|| panic!("{name} is marked: {listed:?}"))["n"]
            .clone()
    };
    // The numbers are those of browser.page.read.
    assert_eq!(number("Abspielen"), element(&page, "Abspielen")["n"]);
    assert_eq!(number("Jetzt kaufen"), element(&page, "Jetzt kaufen")["n"]);
    let drawn = decoded(&marked);
    assert_eq!(drawn.dimensions(), image.dimensions());
    assert!(
        mark_coloured(&drawn) > mark_coloured(&image) + 100,
        "boxes and numbers are on the picture: {} against {}",
        mark_coloured(&drawn),
        mark_coloured(&image)
    );
    assert_ne!(drawn.as_raw(), image.as_raw());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_region_and_an_element_come_larger_and_say_where_they_are() {
    let Some(scene) = scene_with(true, SiteLevel::Operate).await else {
        return;
    };
    let page = open(&scene, "/canvas.html").await;
    let full = shot(&scene, json!({ "tab": "t1" })).await;
    let full_size = decoded(&full).dimensions();

    let region = json!({ "x": 0, "y": 100, "width": 400, "height": 250 });
    let crop = shot(&scene, json!({ "tab": "t1", "region": region })).await;
    assert_eq!(crop.result["region"], region);
    let zoom = crop.result["zoom"].as_f64().unwrap();
    assert!((1.0..=2.0).contains(&zoom), "zoom {zoom}");
    let image = decoded(&crop);
    let expected = (400.0 * zoom).round() as i64;
    assert!(
        (i64::from(image.width()) - expected).abs() <= 2,
        "{} against {expected}",
        image.width()
    );
    assert!(image.width() <= 1280);
    // The view is that of the full picture, not of the crop.
    assert_eq!(crop.result["view"]["width"], full_size.0);

    // An element in view comes with a little room around it.
    let play = element(&page, "Abspielen")["n"].clone();
    let near = shot(&scene, json!({ "tab": "t1", "element": play })).await;
    assert!(near.result["region"]["width"].as_f64().unwrap() > 20.0);
    assert!(near.result["zoom"].as_f64().unwrap() >= 1.0);
    assert!(decoded(&near).width() < full_size.0);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn invalid_asks_are_refused_before_anything_is_captured() {
    let Some(scene) = scene_with(true, SiteLevel::Operate).await else {
        return;
    };
    let page = open(&scene, "/canvas.html").await;
    let play = element(&page, "Abspielen")["n"].clone();
    for input in [
        json!({ "tab": "t1", "region": { "x": 0, "y": 0, "width": 4, "height": 4 } }),
        json!({ "tab": "t1", "region": { "x": 5000, "y": 5000, "width": 100, "height": 100 } }),
        json!({ "tab": "t1", "element": 9999 }),
        json!({ "tab": "t1", "element": play, "region": { "x": 0, "y": 0, "width": 100, "height": 100 } }),
    ] {
        let refused = scene
            .auto("browser.page.screenshot", input.clone())
            .await
            .expect_err("refused");
        assert_eq!(refused.code, "invalid_input", "{input}");
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn at_most_eighty_elements_are_marked_and_the_result_says_there_are_more() {
    let Some(scene) = scene_with(true, SiteLevel::Operate).await else {
        return;
    };
    open(&scene, "/many-buttons.html").await;
    let marked = shot(&scene, json!({ "tab": "t1", "marks": true })).await;
    assert_eq!(content(&marked)["marks"].as_array().unwrap().len(), 80);
    assert_eq!(content(&marked)["marks_more"], true);
    // In the order of the page: the first one is the first button.
    assert_eq!(content(&marked)["marks"][0]["name"], "K1");
}

/// The place of the canvas in CSS pixels: its box on the page.
fn canvas_box(page: &NativeReply) -> (f64, f64) {
    let canvas = content(page)["elements"]
        .as_array()
        .unwrap()
        .iter()
        .find(|element| element["kind"] == "canvas")
        .unwrap_or_else(|| panic!("no canvas: {}", page.result))
        .clone();
    (
        canvas["box"]["x"].as_f64().unwrap(),
        canvas["box"]["y"].as_f64().unwrap(),
    )
}

fn point(scale: f64, x: f64, y: f64) -> Value {
    json!({ "x": x * scale, "y": y * scale })
}

async fn page_text(scene: &Scene) -> String {
    let page = scene
        .auto("browser.page.read", json!({ "tab": "t1" }))
        .await
        .expect("the page");
    content(&page)["text"].as_str().unwrap().to_owned()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_position_needs_a_screenshot_and_says_what_it_hit() {
    let Some(scene) = scene_with(true, SiteLevel::Operate).await else {
        return;
    };
    let page = open(&scene, "/canvas.html").await;
    let (box_x, box_y) = canvas_box(&page);

    // Without a screenshot nothing is clicked.
    let refused = scene
        .auto(
            "browser.click_at",
            json!({ "tab": "t1", "x": 100, "y": 100 }),
        )
        .await
        .expect_err("no view");
    assert_eq!(refused.code, "no_view");
    assert!(!page_text(&scene).await.contains("geklickt"));

    let scale = shot(&scene, json!({ "tab": "t1" })).await.result["view"]["scale"]
        .as_f64()
        .unwrap();
    // The red square of the canvas has no number of its own; the canvas does.
    let red = point(scale, box_x + 2.0 + 50.0, box_y + 2.0 + 50.0);
    let clicked = scene
        .auto(
            "browser.click_at",
            json!({ "tab": "t1", "x": red["x"], "y": red["y"] }),
        )
        .await
        .expect("a click on the canvas");
    assert_eq!(
        clicked.result["hit"][0]["kind"], "canvas",
        "{}",
        clicked.result
    );
    assert!(page_text(&scene).await.contains("geklickt: rot"));
    // The canvas is no question in Auto.
    assert!(scene.asked.lock().unwrap().is_empty());

    // A point with no element says so, and is a change that asks nothing in Auto either.
    let empty = scene
        .auto(
            "browser.click_at",
            json!({ "tab": "t1", "x": 700.0 * scale, "y": 400.0 * scale }),
        )
        .await
        .expect("a click on nothing");
    assert_eq!(empty.result["hit"][0]["none"], true, "{}", empty.result);

    // A double click and a right click are the page's `dblclick` and `contextmenu`.
    scene
        .auto(
            "browser.click_at",
            json!({ "tab": "t1", "x": red["x"], "y": red["y"], "count": 2 }),
        )
        .await
        .expect("a double click");
    assert!(page_text(&scene).await.contains("doppelt"));
    scene
        .auto(
            "browser.click_at",
            json!({ "tab": "t1", "x": red["x"], "y": red["y"], "button": "right" }),
        )
        .await
        .expect("a right click");
    assert!(page_text(&scene).await.contains("kontext"));

    // Invalid asks.
    for input in [
        json!({ "tab": "t1", "x": -5, "y": 10 }),
        json!({ "tab": "t1", "x": 100000, "y": 10 }),
        json!({ "tab": "t1", "x": 10, "y": 10, "button": "middle" }),
        json!({ "tab": "t1", "x": 10, "y": 10, "count": 3 }),
    ] {
        let refused = scene
            .auto("browser.click_at", input.clone())
            .await
            .expect_err("refused");
        assert_eq!(refused.code, "invalid_input", "{input}");
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_view_that_moved_on_refuses_the_position() {
    let Some(scene) = scene_with(true, SiteLevel::Operate).await else {
        return;
    };
    open(&scene, "/many-buttons.html").await;
    let scale = shot(&scene, json!({ "tab": "t1" })).await.result["view"]["scale"]
        .as_f64()
        .unwrap();
    let click = json!({ "tab": "t1", "x": 20.0 * scale, "y": 20.0 * scale });

    // A scroll changes what the positions mean.
    scene
        .auto(
            "browser.scroll",
            json!({ "tab": "t1", "direction": "down" }),
        )
        .await
        .expect("scrolled");
    let moved = scene
        .auto("browser.click_at", click.clone())
        .await
        .expect_err("the page scrolled");
    assert_eq!(moved.code, "view_changed");
    // A new screenshot makes the positions good again.
    shot(&scene, json!({ "tab": "t1" })).await;
    scene
        .auto("browser.click_at", click.clone())
        .await
        .expect("after a new screenshot");

    // A new document does too.
    shot(&scene, json!({ "tab": "t1" })).await;
    scene
        .auto(
            "browser.navigate",
            json!({ "tab": "t1", "to": scene.site.url("/canvas.html") }),
        )
        .await
        .expect("navigated");
    let moved = scene
        .auto("browser.click_at", click)
        .await
        .expect_err("a new document");
    assert_eq!(moved.code, "view_changed");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_click_at_a_buying_button_asks_as_a_click_on_it_does() {
    let Some(scene) = scene_with(true, SiteLevel::Operate).await else {
        return;
    };
    open(&scene, "/canvas.html").await;
    let scale = shot(&scene, json!({ "tab": "t1" })).await.result["view"]["scale"]
        .as_f64()
        .unwrap();
    // The buttons have places of their own in the fixture: "Jetzt kaufen" is at (195, 63).
    let buy = point(scale, 195.0, 63.0);
    let click = json!({ "tab": "t1", "x": buy["x"], "y": buy["y"] });

    *scene.allow.lock().unwrap() = false;
    let declined = scene
        .auto("browser.click_at", click.clone())
        .await
        .expect_err("the user said no");
    assert_eq!(declined.code, "user_declined");
    let asked = scene.asked.lock().unwrap().clone();
    assert_eq!(asked.len(), 1);
    assert_eq!(asked[0].label.as_deref(), Some("Jetzt kaufen"));
    assert!(page_text(&scene).await.contains("bereit"));

    *scene.allow.lock().unwrap() = true;
    let bought = scene
        .auto("browser.click_at", click)
        .await
        .expect("the user said yes");
    assert_eq!(bought.result["hit"][0]["name"], "Jetzt kaufen");
    assert!(page_text(&scene).await.contains("gekauft"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn the_marker_bar_of_the_agent_tab_is_no_element_of_the_page() {
    let Some(scene) = scene_with(true, SiteLevel::Operate).await else {
        return;
    };
    open(&scene, "/canvas.html").await;
    let shown = shot(&scene, json!({ "tab": "t1" })).await;
    let scale = shown.result["view"]["scale"].as_f64().unwrap();
    let width = shown.result["view"]["width"].as_f64().unwrap();
    // The bar floats at the top, centred: the label in its middle is above the page, so a point
    // there is no element the page listed, whatever lies under the bar in the page's own layout.
    let under_bar = scene
        .auto(
            "browser.hover",
            json!({ "tab": "t1", "x": width / 2.0, "y": 20.0 * scale }),
        )
        .await
        .expect("hovering the bar");
    assert_eq!(
        under_bar.result["hit"][0]["none"], true,
        "{}",
        under_bar.result
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_drag_checks_both_ends_and_the_stricter_one_asks() {
    let Some(scene) = scene_with(true, SiteLevel::Operate).await else {
        return;
    };
    let page = open(&scene, "/canvas.html").await;
    let (box_x, box_y) = canvas_box(&page);
    let scale = shot(&scene, json!({ "tab": "t1" })).await.result["view"]["scale"]
        .as_f64()
        .unwrap();
    let red = point(scale, box_x + 2.0 + 50.0, box_y + 2.0 + 50.0);
    let target = point(scale, box_x + 2.0 + 340.0, box_y + 2.0 + 150.0);

    // The square is dropped on the target: two canvas hits, nothing asked in Auto.
    let dropped = scene
        .auto(
            "browser.drag",
            json!({ "tab": "t1", "from": red, "to": target }),
        )
        .await
        .expect("a drag on the canvas");
    assert_eq!(dropped.result["hit"].as_array().unwrap().len(), 2);
    assert!(page_text(&scene).await.contains("abgelegt: rot"));
    assert!(scene.asked.lock().unwrap().is_empty());

    // Dropped on the buying button, the drag asks, and a no leaves the page as it was.
    let buy = point(scale, 195.0, 63.0);
    *scene.allow.lock().unwrap() = false;
    let declined = scene
        .auto(
            "browser.drag",
            json!({ "tab": "t1", "from": red, "to": buy }),
        )
        .await
        .expect_err("the user said no");
    assert_eq!(declined.code, "user_declined");
    assert_eq!(scene.asked.lock().unwrap().len(), 1);
    assert!(!page_text(&scene).await.contains("gekauft"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn on_a_website_that_may_only_be_read_a_position_hovers_but_does_not_click() {
    let Some(scene) = scene_with(true, SiteLevel::Read).await else {
        return;
    };
    open(&scene, "/canvas.html").await;
    let scale = shot(&scene, json!({ "tab": "t1" })).await.result["view"]["scale"]
        .as_f64()
        .unwrap();
    let spot = point(scale, 195.0, 63.0);
    let hovered = scene
        .auto(
            "browser.hover",
            json!({ "tab": "t1", "x": spot["x"], "y": spot["y"] }),
        )
        .await
        .expect("hovering only reads");
    assert_eq!(hovered.result["hit"][0]["name"], "Jetzt kaufen");
    for (tool, input) in [
        (
            "browser.click_at",
            json!({ "tab": "t1", "x": spot["x"], "y": spot["y"] }),
        ),
        (
            "browser.drag",
            json!({ "tab": "t1", "from": spot, "to": spot }),
        ),
    ] {
        let refused = scene.auto(tool, input).await.expect_err("read only");
        assert_eq!(refused.code, "read_only_site", "{tool}");
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_position_in_a_frame_of_another_website_finds_the_element_of_the_frame() {
    let Some(scene) = scene_with(true, SiteLevel::Operate).await else {
        return;
    };
    let partner = Site::parse(&scene.site.partner("/")).unwrap();
    scene.env.rights.lock().unwrap().sites.insert(
        partner.clone(),
        SiteGrant {
            level: SiteLevel::Operate,
            fill_remembered: false,
        },
    );
    let page = open(&scene, "/frame-host.html").await;
    let player = element(&page, "Ton")["n"].clone();
    // The frame lies far down: bring its player into view and read where it is now.
    let seen = scene
        .auto("browser.scroll", json!({ "tab": "t1", "element": player }))
        .await
        .expect("into view");
    let box_of = element(&seen, "Ton")["box"].clone();
    let centre = (
        box_of["x"].as_f64().unwrap() + box_of["w"].as_f64().unwrap() / 2.0,
        box_of["y"].as_f64().unwrap() + box_of["h"].as_f64().unwrap() / 2.0,
    );
    let scale = shot(&scene, json!({ "tab": "t1" })).await.result["view"]["scale"]
        .as_f64()
        .unwrap();
    let spot = point(scale, centre.0, centre.1);

    let hovered = scene
        .auto(
            "browser.hover",
            json!({ "tab": "t1", "x": spot["x"], "y": spot["y"] }),
        )
        .await
        .expect("hover in the frame");
    let hit = &hovered.result["hit"][0];
    assert_eq!(hit["kind"], "media", "{}", hovered.result);
    assert_eq!(hit["n"], player, "the number is the one of the page's list");

    // Where the agent may only read the frame's website, a click there is refused.
    scene
        .env
        .rights
        .lock()
        .unwrap()
        .sites
        .get_mut(&partner)
        .unwrap()
        .level = SiteLevel::Read;
    let refused = scene
        .auto(
            "browser.click_at",
            json!({ "tab": "t1", "x": spot["x"], "y": spot["y"] }),
        )
        .await
        .expect_err("a frame that may only be read");
    assert_eq!(refused.code, "read_only_site");
}
