//! Spec 065 T009: the entries for a page are those of the caller's grants with the page's site.

use std::sync::Arc;

use url::Url;

use super::*;
use crate::passwords::access::{GrantAction, Scope};
use crate::passwords::items;
use crate::passwords::model::ItemInput;
use crate::passwords::test_support::open_test_vault;
use crate::vault_gate::VaultGate;

fn entry(title: &str, url: &str) -> ItemInput {
    ItemInput {
        title: Some(title.to_owned()),
        url: Some(url.to_owned()),
        ..ItemInput::default()
    }
}

#[tokio::test]
async fn only_entries_of_the_same_site_belong_to_a_page() {
    let (_dir, db) = open_test_vault();
    let vault = VaultGate::new().vault_db(Arc::new(db)).expect("vault");
    vault
        .write_blocking(|tx| {
            for (title, url) in [
                ("Shop", "https://www.shop.example.de/login"),
                ("Shop ohne Schema", "shop.example.de"),
                ("Nachbar", "https://bad-example.de"),
                ("Lookalike", "https://shop.example.de.evil.test"),
                ("Ohne Adresse", ""),
            ] {
                items::create_item(tx, &entry(title, url), None)?;
            }
            Ok(())
        })
        .expect("entries");
    let service = PasswordsService::new(vault);
    // A caller with grants; the built-in agent sees only its own list view (034 FR-027).
    let agent = Caller::ExternalAgent {
        id: "agent".to_owned(),
    };
    let grants = [Grant::new(GrantAction::Read, Scope::All)];
    let page = Url::parse("https://login.shop.example.de/kasse").expect("page");
    let mut titles: Vec<String> = service
        .headers_for_page(&agent, &grants, &page)
        .await
        .expect("headers")
        .into_iter()
        .filter_map(|header| header.title)
        .collect();
    titles.sort();
    assert_eq!(titles, ["Shop", "Shop ohne Schema"]);

    let suffix = Url::parse("https://co.uk/").expect("page");
    assert!(service
        .headers_for_page(&agent, &grants, &suffix)
        .await
        .expect("headers")
        .is_empty());
    assert!(
        service
            .headers_for_page(&agent, &[], &page)
            .await
            .map(|headers| headers.is_empty())
            .unwrap_or(true),
        "no grant, no entry"
    );
}
