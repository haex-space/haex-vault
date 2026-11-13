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
use holzi_lib::chat::tools::native_action::NativeReply;
use holzi_lib::mcp_server::grants::SiteLevel;

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
