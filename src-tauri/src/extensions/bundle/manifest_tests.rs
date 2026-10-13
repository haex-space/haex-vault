use std::path::Path;

use super::*;
use crate::extensions::bundle::verify_bundle;
use crate::extensions::permissions::PermissionKind;

fn vector(name: &str) -> VerifiedBundle {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/extension_bundles")
        .join(name);
    verify_bundle(&std::fs::read(path).expect("vector")).expect("valid vector")
}

#[test]
fn the_notes_like_vector_gives_every_field() {
    let manifest = Manifest::from_verified(&vector("good-notes-like.xt")).unwrap();
    assert_eq!(manifest.name.as_str(), "notes-like");
    assert_eq!(manifest.version, semver::Version::new(1, 2, 0));
    assert_eq!(
        manifest.public_key.as_str(),
        "3614253f84ba66a8faa168d317a6979992979a3e7ad15ae83ca2823f5ca97d34"
    );
    assert_eq!(manifest.title(), "Notes");
    assert_eq!(manifest.author.as_deref(), Some("Test Author"));
    assert_eq!(manifest.homepage, None, "null counts as absent");
    assert_eq!(manifest.icon.as_deref(), Some("icon.svg"));
    assert_eq!(manifest.entry, "index.html");
    assert!(!manifest.single_instance);
    assert_eq!(
        manifest.migrations_dir.as_deref(),
        Some("database/migrations")
    );
    assert_eq!(
        manifest.i18n.as_ref().unwrap()["en"]["name"],
        serde_json::json!("Notes")
    );
    assert!(manifest.permissions.declared.is_empty());
}

#[test]
fn a_minimal_manifest_falls_back_to_name_and_index_html() {
    let manifest = Manifest::from_verified(&vector("good-minimal.xt")).unwrap();
    assert_eq!(manifest.title(), manifest.name.as_str());
    assert_eq!(manifest.entry, "index.html");
    assert_eq!(manifest.migrations_dir, None);
}

#[test]
fn a_prerelease_version_is_kept() {
    let manifest = Manifest::from_verified(&vector("good-semver-prerelease.xt")).unwrap();
    assert!(!manifest.version.pre.is_empty());
}

#[test]
fn declared_permissions_are_mapped() {
    let mut bundle = vector("good-minimal.xt");
    bundle.manifest.insert(
        "permissions".into(),
        haex_bundle::jcs::parse_restricted(
            r#"{"http":[{"target":"https://example.org/*"}],"spaces":[{"target":"*"}]}"#,
        )
        .unwrap(),
    );
    let manifest = Manifest::from_verified(&bundle).unwrap();
    assert_eq!(manifest.permissions.declared.len(), 1);
    assert_eq!(manifest.permissions.declared[0].kind, PermissionKind::Web);
    assert_eq!(manifest.permissions.unsupported_categories, ["spaces"]);
}

#[test]
fn the_stored_manifest_bytes_give_the_same_view() {
    let bundle = vector("good-notes-like.xt");
    assert_eq!(
        Manifest::from_stored(&bundle.manifest_bytes).unwrap(),
        Manifest::from_verified(&bundle).unwrap()
    );
    assert!(Manifest::from_stored(b"{ }").is_err());
}

/// Checks that verified tools retain their metadata, schemas and effects.
#[test]
fn the_tools_vector_gives_each_tool_with_its_effect() {
    let manifest = Manifest::from_verified(&vector("good-tools.xt")).unwrap();
    let [count, add] = manifest.tools.as_slice() else {
        panic!("two tools, got {:?}", manifest.tools);
    };
    assert_eq!(count.name, "count_unread");
    assert_eq!(count.title_de, "Ungelesene zählen");
    assert_eq!(count.title_en, "Count unread");
    assert_eq!(count.description, "Counts unread items.");
    assert_eq!(count.effect, ToolEffect::Read);
    assert_eq!(count.examples_de, ["Wie viele ungelesene habe ich?"]);
    assert_eq!(count.examples_en, ["How many unread do I have?"]);
    assert_eq!(
        count.input_schema["properties"]["role"]["enum"],
        serde_json::json!(["inbox", "all"])
    );
    assert!(count.supported);
    assert_eq!(add.name, "add_entry");
    assert_eq!(add.effect, ToolEffect::Change);
}

/// Checks that an absent tools block produces an empty tool list.
#[test]
fn a_manifest_without_tools_has_none() {
    let manifest = Manifest::from_verified(&vector("good-minimal.xt")).unwrap();
    assert!(manifest.tools.is_empty());
}

/// Checks that unsupported schemas preserve the tool but mark it unavailable to the agent.
#[test]
fn a_tool_outside_the_schema_subset_is_kept_but_not_supported() {
    let mut bundle = vector("good-tools.xt");
    let mut tools = serde_json::Value::from(&bundle.manifest["tools"]);
    tools[0]["inputSchema"]["properties"]["role"]["pattern"] = serde_json::json!("^[a-z]+$");
    bundle.manifest.insert(
        "tools".into(),
        haex_bundle::jcs::parse_restricted(&tools.to_string()).unwrap(),
    );
    let manifest = Manifest::from_verified(&bundle).unwrap();
    assert!(!manifest.tools[0].supported);
    assert!(manifest.tools[1].supported);
}

/// Checks that stored and freshly verified manifests yield the same tools.
#[test]
fn the_stored_tools_give_the_same_view() {
    let bundle = vector("good-tools.xt");
    assert_eq!(
        Manifest::from_stored(&bundle.manifest_bytes).unwrap().tools,
        Manifest::from_verified(&bundle).unwrap().tools
    );
}

const DEV_TOOL: &str = r#"{"name":"count_unread","title":{"de":"Zählen","en":"Count"},"description":"Counts.","inputSchema":{"type":"object"},"effect":"destructive","examples":{"de":["Zähle"],"en":["Count"]}}"#;

/// Builds a development manifest with the supplied JSON tools block.
fn dev_manifest(tools: &str) -> String {
    format!(
        r#"{{"name":"dev-tools","version":"0.1.0","publicKey":"3614253f84ba66a8faa168d317a6979992979a3e7ad15ae83ca2823f5ca97d34","permissions":{{}},"tools":{tools}}}"#
    )
}

/// Checks that development manifests load valid tools and their effects.
#[test]
fn a_development_version_reads_its_tools() {
    let manifest = Manifest::from_dev_file(&dev_manifest(&format!("[{DEV_TOOL}]")), None).unwrap();
    assert_eq!(manifest.tools.len(), 1);
    assert_eq!(manifest.tools[0].effect, ToolEffect::Destructive);
}

/// Checks that development manifests reject tool names outside the bundle format.
#[test]
fn a_development_version_with_a_malformed_tools_block_is_refused() {
    let bad = DEV_TOOL.replace("count_unread", "Count-Unread");
    let error = Manifest::from_dev_file(&dev_manifest(&format!("[{bad}]")), None).unwrap_err();
    assert_eq!(error.kind, "manifest_invalid");
}
