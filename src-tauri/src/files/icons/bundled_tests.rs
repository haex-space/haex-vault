//! What the bundled packs cover (SC-002): common names and extensions get an image of their own,
//! and an entry in a state of encryption never takes a name or extension image.

use super::builtin;
use super::resolve::resolve;
use super::*;
use crate::files::{EncryptedInfo, EncryptedRole, EncryptedState};

pub(super) fn file(name: &str) -> IconQuery {
    IconQuery {
        name: name.to_owned(),
        is_dir: false,
        no_access: false,
        encrypted: None,
    }
}

pub(super) fn dir(name: &str) -> IconQuery {
    IconQuery {
        is_dir: true,
        ..file(name)
    }
}

pub(super) fn id_of(set: BuiltinSet, scheme: Scheme, query: &IconQuery) -> String {
    let pack = builtin::pack(set).expect("a bundled pack");
    match resolve(Some(pack.theme()), scheme, query) {
        IconAnswer::Image { id, .. } => id,
        IconAnswer::Builtin { name } => format!("builtin:{name}"),
    }
}

fn dark(set: BuiltinSet, query: &IconQuery) -> String {
    id_of(set, Scheme::Dark, query)
}

const MATERIAL_FOLDERS: &[&str] = &[
    "src",
    "docs",
    "doc",
    "images",
    "image",
    "img",
    "config",
    "test",
    "tests",
    "lib",
    "dist",
    "build",
    "node_modules",
    "public",
    "assets",
    "scripts",
    "components",
    "utils",
    "helpers",
    "hooks",
    "pages",
    "views",
    "styles",
    "css",
    "fonts",
    "icons",
    "videos",
    "audio",
    "music",
    "data",
    "database",
    "api",
    "server",
    "client",
    "app",
    "core",
    "types",
    "models",
    "services",
    "middleware",
    "plugins",
    "templates",
    "temp",
    "tmp",
    "logs",
    "vendor",
    "target",
    "bin",
    "docker",
    "android",
    "ios",
    "git",
    ".github",
    ".vscode",
    "e2e",
    "cypress",
    "i18n",
    "locales",
    "layouts",
];

const MATERIAL_FILES: &[&str] = &[
    "readme.md",
    "license",
    "package.json",
    "tsconfig.json",
    "dockerfile",
    "makefile",
    ".gitignore",
    "cargo.toml",
    "cargo.lock",
    "a.json",
    "a.yaml",
    "a.yml",
    "a.toml",
    "a.xml",
    "a.zip",
    "a.rs",
    "a.ts",
    "a.tsx",
    "a.js",
    "a.jsx",
    "a.vue",
    "a.py",
    "a.go",
    "a.java",
    "a.kt",
    "a.c",
    "a.cpp",
    "a.h",
    "a.cs",
    "a.php",
    "a.rb",
    "a.swift",
    "a.sh",
    "a.css",
    "a.scss",
    "a.html",
    "a.md",
    "a.txt",
    "a.pdf",
    "a.csv",
    "a.sql",
    "a.svg",
    "a.png",
    "a.jpg",
    "a.gif",
    "a.mp3",
    "a.mp4",
    "a.lock",
    "a.log",
    "a.env",
    "a.ini",
    "a.lua",
    "a.dart",
    "a.r",
    "a.pl",
    "a.scala",
    "a.hs",
    "a.ex",
    "a.erl",
    "a.clj",
    "a.elm",
    "a.zig",
    "a.nim",
    "a.tf",
    "a.proto",
    "a.graphql",
    "a.svelte",
    "a.astro",
    "a.less",
    "a.sass",
    "a.styl",
    "a.pug",
    "a.docx",
    "a.xlsx",
    "a.pptx",
    "a.7z",
    "a.tar",
    "a.gz",
    "a.wasm",
    "a.jar",
    "a.exe",
    "a.dll",
    "a.so",
    "a.bat",
    "a.ps1",
    "a.vim",
    "a.tex",
    "a.bib",
    "a.diff",
    "a.patch",
    "a.key",
    "a.pem",
    "a.crt",
    "a.ttf",
    "a.woff",
    "a.woff2",
    "a.otf",
    "a.ico",
    "a.webp",
    "a.wav",
    "a.flac",
    "a.mkv",
    "a.mov",
    "a.avi",
    "a.webm",
    "a.ipynb",
    "a.rst",
    "a.adoc",
];

const PHOSPHOR_EXTENSIONS: &[&str] = &[
    "pdf", "doc", "docx", "odt", "xls", "xlsx", "ods", "ppt", "pptx", "odp", "zip", "7z", "tar",
    "gz", "rar", "png", "jpg", "jpeg", "gif", "webp", "svg", "mp3", "flac", "wav", "mp4", "mkv",
    "mov", "rs", "ts", "tsx", "js", "jsx", "vue", "py", "md", "csv", "ini", "txt", "html", "css",
];

#[test]
fn material_gives_common_folder_names_a_folder_image_of_their_own() {
    let default = dark(BuiltinSet::Material, &dir("zz-no-such-folder"));
    let own = MATERIAL_FOLDERS
        .iter()
        .filter(|name| dark(BuiltinSet::Material, &dir(name)) != default)
        .count();
    assert!(
        own >= 50,
        "{own} of {} folder names",
        MATERIAL_FOLDERS.len()
    );
}

#[test]
fn material_gives_common_names_and_extensions_an_image_of_their_own() {
    let default = dark(BuiltinSet::Material, &file("zz-no-such.file-kind"));
    let own = MATERIAL_FILES
        .iter()
        .filter(|name| dark(BuiltinSet::Material, &file(name)) != default)
        .count();
    assert!(own >= 100, "{own} of {} names", MATERIAL_FILES.len());
}

#[test]
fn phosphor_gives_common_extensions_an_image_of_their_own() {
    let default = dark(BuiltinSet::Phosphor, &file("zz-no-such.file-kind"));
    let missing: Vec<_> = PHOSPHOR_EXTENSIONS
        .iter()
        .filter(|ext| dark(BuiltinSet::Phosphor, &file(&format!("a.{ext}"))) == default)
        .collect();
    assert!(
        PHOSPHOR_EXTENSIONS.len() - missing.len() >= 40,
        "no image of their own: {missing:?}"
    );
}

fn encrypted(role: EncryptedRole, state: EncryptedState) -> Option<EncryptedInfo> {
    Some(EncryptedInfo {
        role,
        state,
        foreign: 0,
    })
}

#[test]
fn every_state_resolves_for_both_packs_and_never_takes_a_name_or_extension_image() {
    for set in [BuiltinSet::Phosphor, BuiltinSet::Material] {
        let plain_file = dark(set, &file("main.rs"));
        let plain_dir = dark(set, &dir("src"));
        let cases = [
            (
                "lockedFile",
                IconQuery {
                    encrypted: encrypted(EncryptedRole::Inside, EncryptedState::OtherVault),
                    ..file("main.rs")
                },
                &plain_file,
            ),
            (
                "damaged",
                IconQuery {
                    encrypted: encrypted(EncryptedRole::Inside, EncryptedState::Damaged),
                    ..file("main.rs")
                },
                &plain_file,
            ),
            (
                "lockedFolder",
                IconQuery {
                    encrypted: encrypted(EncryptedRole::Folder, EncryptedState::NewerFormat),
                    ..dir("src")
                },
                &plain_dir,
            ),
            (
                "encryptedFolder",
                IconQuery {
                    encrypted: encrypted(EncryptedRole::Folder, EncryptedState::Readable),
                    ..dir("src")
                },
                &plain_dir,
            ),
            (
                "folderNoAccess",
                IconQuery {
                    no_access: true,
                    ..dir("src")
                },
                &plain_dir,
            ),
        ];
        for (state, query, plain) in cases {
            assert_ne!(
                &dark(set, &query),
                plain,
                "{set:?} {state}: a state never takes the image of the name"
            );
        }
    }
}
