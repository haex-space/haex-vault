//! `files.read` of the built-in agent (FR-035, FR-035a): documents give their text, images come
//! as a JPEG for the model, and what is too large or unknown gets a note.

use std::io::Cursor;

use image::{DynamicImage, ImageFormat, RgbImage};
use serde_json::json;

use super::test_env::Scene;
use crate::files::access::StorageGrant;
use crate::files::extract::NOTE_TOO_LARGE;

fn fixture(name: &str) -> Vec<u8> {
    std::fs::read(format!(
        "{}/tests/fixtures/files/{name}",
        env!("CARGO_MANIFEST_DIR")
    ))
    .expect("fixture")
}

#[tokio::test]
async fn pdfs_and_office_documents_give_their_text() {
    let scene = Scene::new(None);
    for (name, expected) in [
        ("steuer.pdf", "312 Euro"),
        ("mietvertrag.docx", "850 Euro im Monat"),
        ("mietvertrag.odt", "850 Euro im Monat"),
        ("tabelle.xlsx", "## Ausgaben"),
    ] {
        std::fs::write(scene.path(name), fixture(name)).expect("write");
        let read = scene
            .run(
                "files.read",
                json!({ "source": "device", "path": scene.at(name) }),
            )
            .await
            .expect("read");
        let text = read["text"]
            .as_str()
            .unwrap_or_else(|| panic!("{name}: {read}"));
        assert!(text.contains(expected), "{name}: {text}");
        assert_eq!(read["truncated"], false, "{name}");
    }
    std::fs::write(scene.path("scan.pdf"), fixture("scan.pdf")).expect("write");
    let scan = scene
        .run(
            "files.read",
            json!({ "source": "device", "path": scene.at("scan.pdf") }),
        )
        .await
        .expect("read");
    assert!(
        scan["note"].as_str().unwrap().contains("no text layer"),
        "{scan}"
    );
}

#[tokio::test]
async fn a_document_over_50_mb_is_not_read() {
    let scene = Scene::new(None);
    let file = std::fs::File::create(scene.path("riesig.pdf")).expect("create");
    file.set_len(50 * 1024 * 1024 + 1).expect("sparse");
    let read = scene
        .run(
            "files.read",
            json!({ "source": "device", "path": scene.at("riesig.pdf") }),
        )
        .await
        .expect("read");
    assert_eq!(read["note"], NOTE_TOO_LARGE);
}

#[tokio::test]
async fn an_image_comes_as_a_jpeg_for_the_model() {
    let scene = Scene::new(None);
    let mut png = Vec::new();
    DynamicImage::ImageRgb8(RgbImage::from_pixel(3000, 2000, image::Rgb([0, 90, 0])))
        .write_to(&mut Cursor::new(&mut png), ImageFormat::Png)
        .expect("png");
    std::fs::write(scene.path("wiese.png"), png).expect("write");
    let reply = scene
        .reply(
            "files.read",
            json!({ "source": "device", "path": scene.at("wiese.png") }),
        )
        .await
        .expect("read");
    assert_eq!(reply.result["entry"]["name"], "wiese.png");
    assert!(reply.result.get("text").is_none());
    assert_eq!(reply.images.len(), 1);
    assert_eq!(reply.images[0].media_type, "image/jpeg");
    let shown = image::load_from_memory(&reply.images[0].bytes).expect("a JPEG");
    assert_eq!((shown.width(), shown.height()), (1568, 1045));
}

#[tokio::test]
async fn a_document_in_a_storage_gives_its_text() {
    let scene = Scene::new(None);
    scene
        .agent
        .env
        .grants
        .lock()
        .unwrap()
        .storages
        .insert("s1".into(), StorageGrant::Read);
    scene
        .agent
        .env
        .store
        .insert("s1", "Unterlagen/steuer.pdf", &fixture("steuer.pdf"));
    let read = scene
        .run(
            "files.read",
            json!({ "source": "storage:s1", "path": "/Unterlagen/steuer.pdf" }),
        )
        .await
        .expect("read");
    assert!(
        read["text"]
            .as_str()
            .unwrap()
            .contains("Steuerbescheid 2025"),
        "{read}"
    );
}
