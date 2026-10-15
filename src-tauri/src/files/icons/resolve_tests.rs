use super::*;
use crate::files::{EncryptedInfo, EncryptedRole, EncryptedState};

const THEME: &str = r#"{
  "iconDefinitions": {
    "file": {"iconPath": "i/file.svg"}, "folder": {"iconPath": "i/folder.svg"},
    "rust": {"iconPath": "i/rust.svg"}, "rust-light": {"iconPath": "i/rust_light.svg"},
    "ts": {"iconPath": "i/ts.svg"}, "testts": {"iconPath": "i/testts.svg"},
    "docker": {"iconPath": "i/docker.svg"}, "src": {"iconPath": "i/src.svg"},
    "image": {"iconPath": "i/image.png"}, "pdf": {"iconPath": "i/pdf.webp"},
    "lock": {"iconPath": "i/lock.svg"}, "odd": {"iconPath": "i/odd.gif"}
  },
  "file": "file", "folder": "folder",
  "fileNames": {"dockerfile": "docker", "main.rs": "ts"},
  "fileExtensions": {"rs": "rust", "ts": "ts", "test.ts": "testts", "weird": "odd"},
  "folderNames": {"src": "src"},
  "light": {"fileExtensions": {"rs": "rust-light"}},
  "x-holzi": {"roles": {"image": "image", "pdf": "pdf", "lockedFile": "lock", "folder": "folder"}}
}"#;

fn theme() -> IconTheme {
    IconTheme::parse(THEME.as_bytes()).expect("theme")
}

fn file(name: &str) -> IconQuery {
    IconQuery {
        name: name.to_owned(),
        is_dir: false,
        no_access: false,
        encrypted: None,
    }
}

fn dir(name: &str) -> IconQuery {
    IconQuery {
        is_dir: true,
        ..file(name)
    }
}

fn id(answer: IconAnswer) -> String {
    match answer {
        IconAnswer::Image { id, .. } => id,
        IconAnswer::Builtin { name } => format!("builtin:{name}"),
    }
}

fn answer(query: &IconQuery) -> String {
    id(resolve(Some(&theme()), Scheme::Dark, query))
}

#[test]
fn a_name_wins_over_an_extension() {
    assert_eq!(answer(&file("Dockerfile")), "i/docker.svg");
    assert_eq!(
        answer(&file("main.rs")),
        "i/ts.svg",
        "the name, not the extension rs"
    );
}

#[test]
fn the_longest_extension_is_tried_first_and_case_does_not_matter() {
    assert_eq!(answer(&file("a.test.ts")), "i/testts.svg");
    assert_eq!(answer(&file("a.ts")), "i/ts.svg");
    assert_eq!(answer(&file("LIB.RS")), "i/rust.svg");
}

#[test]
fn a_folder_by_name_then_the_role_then_the_default() {
    assert_eq!(answer(&dir("SRC")), "i/src.svg");
    assert_eq!(answer(&dir("other")), "i/folder.svg");
}

#[test]
fn a_file_without_a_name_or_extension_hit_takes_its_role_then_the_default() {
    assert_eq!(answer(&file("photo.png")), "i/image.png", "the role image");
    assert_eq!(answer(&file("paper.pdf")), "i/pdf.webp");
    assert_eq!(
        answer(&file("notes.unknownext")),
        "i/file.svg",
        "the default of the theme"
    );
}

#[test]
fn what_the_theme_does_not_know_falls_back_to_the_standard_set() {
    let bare = IconTheme::parse(b"{}").expect("an empty theme");
    for (query, symbol) in [
        (file("a.rs"), "lucide:file-text"),
        (file("a.png"), "lucide:image"),
        (file("a.mp4"), "lucide:film"),
        (file("a.mp3"), "lucide:music"),
        (file("a.zip"), "lucide:file"),
        (dir("d"), "lucide:folder"),
    ] {
        assert_eq!(
            resolve(Some(&bare), Scheme::Dark, &query),
            IconAnswer::Builtin {
                name: symbol.to_owned()
            },
            "{}",
            query.name
        );
    }
}

#[test]
fn the_standard_set_answers_for_every_role_without_images() {
    assert_eq!(
        resolve(None, Scheme::Light, &dir("d")),
        IconAnswer::Builtin {
            name: "lucide:folder".to_owned()
        }
    );
    let mut denied = dir("d");
    denied.no_access = true;
    assert_eq!(
        resolve(None, Scheme::Dark, &denied),
        IconAnswer::Builtin {
            name: "lucide:folder-lock".to_owned()
        }
    );
}

#[test]
fn an_image_that_is_not_svg_png_or_webp_falls_back() {
    assert_eq!(
        answer(&file("a.weird")),
        "builtin:lucide:file",
        "a gif cannot be shown: the role answers"
    );
}

#[test]
fn the_light_scheme_reads_the_light_block_first() {
    let theme = theme();
    assert_eq!(
        id(resolve(Some(&theme), Scheme::Light, &file("a.rs"))),
        "i/rust_light.svg"
    );
    assert_eq!(
        id(resolve(Some(&theme), Scheme::Dark, &file("a.rs"))),
        "i/rust.svg"
    );
    assert_eq!(
        id(resolve(Some(&theme), Scheme::Light, &file("a.ts"))),
        "i/ts.svg",
        "not in the block"
    );
}

fn state(role: EncryptedRole, state: EncryptedState) -> Option<EncryptedInfo> {
    Some(EncryptedInfo {
        role,
        state,
        foreign: 0,
    })
}

#[test]
fn a_state_is_never_covered_by_a_name_or_an_extension() {
    // A locked file called main.rs: the name and the extension would match, the state wins.
    let mut locked = file("main.rs");
    locked.encrypted = state(EncryptedRole::Inside, EncryptedState::OtherVault);
    assert_eq!(
        answer(&locked),
        "i/lock.svg",
        "the role lockedFile of the theme"
    );

    // A set without an image for the state: the standard set answers, never an empty answer.
    let mut damaged = file("Dockerfile");
    damaged.encrypted = state(EncryptedRole::Inside, EncryptedState::Damaged);
    assert_eq!(answer(&damaged), "builtin:lucide:file-warning");

    let mut folder = dir("src");
    folder.encrypted = state(EncryptedRole::Folder, EncryptedState::Readable);
    assert_eq!(
        answer(&folder),
        "builtin:lucide:folder-key",
        "not the image of the name src"
    );

    let mut denied = dir("src");
    denied.no_access = true;
    assert_eq!(answer(&denied), "builtin:lucide:folder-lock");
}

#[test]
fn every_entry_gets_an_answer_and_never_an_error() {
    let theme = theme();
    for query in [
        file(""),
        file("."),
        file(".."),
        file("a."),
        dir(""),
        file("ÄÖÜ.ß"),
    ] {
        let _ = resolve(Some(&theme), Scheme::Dark, &query);
    }
}
