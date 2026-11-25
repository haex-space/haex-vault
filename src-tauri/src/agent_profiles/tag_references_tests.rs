//! Tests for the profiles that name a tag (spec 100, FR-010): the tag list, the default tag, the
//! comparison by `fold`, and the profiles that must not be reported.

use uuid::Uuid;

use super::store;
use super::tag_references::profiles_naming_tag;
use super::test_support::{vault, Vault};
use crate::mcp_server::grants::{PasswordLevel, PasswordRights, Rights};

fn passwords(tags: Option<&[&str]>, default_tag: Option<&str>) -> Rights {
    Rights {
        passwords: Some(PasswordRights {
            level: PasswordLevel::ReadWrite,
            tags: tags.map(|tags| tags.iter().map(|tag| (*tag).to_owned()).collect()),
            default_tag: default_tag.map(str::to_owned),
        }),
        ..Rights::default()
    }
}

async fn profile(v: &Vault, name: &str, rights: Rights) {
    let (id, name) = (Uuid::new_v4(), name.to_owned());
    v.db.write(move |tx| {
        store::create_profile(tx, id, &name)?;
        store::set_rights(tx, id, &rights)
    })
    .await
    .expect("profile");
}

async fn naming(v: &Vault, tag: &str) -> Vec<String> {
    let tag = tag.to_owned();
    v.db.read(move |q| profiles_naming_tag(q, &tag))
        .await
        .expect("read")
}

#[tokio::test]
async fn a_profile_naming_the_tag_in_its_list_is_reported() {
    let v = vault();
    profile(&v, "Work agents", passwords(Some(&["Arbeit", "Job"]), None)).await;
    profile(&v, "Home agents", passwords(Some(&["Home"]), None)).await;
    assert_eq!(naming(&v, "Job").await, ["Work agents"]);
    assert_eq!(naming(&v, "Home").await, ["Home agents"]);
}

#[tokio::test]
async fn the_tag_new_entries_get_counts_as_naming_it() {
    let v = vault();
    profile(&v, "Writers", passwords(Some(&["Inbox"]), Some("Inbox"))).await;
    profile(&v, "Defaults only", passwords(Some(&[]), Some("Later"))).await;
    assert_eq!(naming(&v, "Later").await, ["Defaults only"]);
}

#[tokio::test]
async fn the_comparison_ignores_case_and_the_form_of_umlauts() {
    let v = vault();
    profile(&v, "Mueller", passwords(Some(&["M\u{fc}ller"]), None)).await;
    assert_eq!(naming(&v, " m\u{fc}LLER ").await, ["Mueller"]);
    assert_eq!(naming(&v, "Mu\u{308}ller").await, ["Mueller"]);
}

#[tokio::test]
async fn a_profile_for_every_entry_or_without_password_rights_names_nothing() {
    let v = vault();
    profile(&v, "Everything", passwords(None, None)).await;
    profile(&v, "No passwords", Rights::default()).await;
    assert!(naming(&v, "Arbeit").await.is_empty());
}

#[tokio::test]
async fn no_profile_gives_an_empty_list_and_a_name_is_reported_once() {
    let v = vault();
    assert!(naming(&v, "Arbeit").await.is_empty());
    profile(
        &v,
        "Twice",
        passwords(Some(&["Arbeit", "arbeit"]), Some("ARBEIT")),
    )
    .await;
    assert_eq!(naming(&v, "Arbeit").await, ["Twice"]);
}
