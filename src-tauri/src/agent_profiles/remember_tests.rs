//! Tests for what "Für diese Website merken" writes (spec 083 T061, FR-015): the profile of the
//! agent, so it holds for every agent in it and for no agent in another profile.

use uuid::Uuid;

use super::remember::{change_browser, change_browser_of};
use super::resolve::rights_of;
use super::store;
use super::test_support::{registered, vault, Vault};
use super::{Subject, STANDARD_PROFILE_ID};
use crate::agent_browser::sites::{remember_fill, Site};
use crate::mcp_server::grants::{BrowserRights, RequestRule, Rights, SiteGrant, SiteLevel};
use crate::mcp_server::registry::load_rights;

fn browsing() -> Rights {
    Rights {
        browser: Some(BrowserRights::default()),
        ..Rights::default()
    }
}

async fn profile_of_browsers(v: &Vault, agents: &[Uuid]) -> Uuid {
    let (profile, members) = (Uuid::new_v4(), agents.to_vec());
    v.db.write(move |tx| {
        store::create_profile(tx, profile, "Browsing")?;
        store::set_rights(tx, profile, &browsing())?;
        for agent in &members {
            store::add_member(tx, &Subject::Agent(*agent), profile)?;
        }
        store::set_active(tx, profile)?;
        Ok(())
    })
    .await
    .expect("profile");
    profile
}

async fn browser_of(v: &Vault, agent: Uuid) -> Option<BrowserRights> {
    let device = v.device;
    v.db.read(move |q| load_rights(q, device, agent))
        .await
        .expect("read")
        .browser
}

fn shop() -> Site {
    Site::parse("https://shop.example.de").expect("site")
}

#[tokio::test]
async fn a_remembered_website_holds_for_every_agent_of_the_profile_and_no_other() {
    let v = vault();
    let (a, b, c) = (
        registered(&v, "a").await,
        registered(&v, "b").await,
        registered(&v, "c").await,
    );
    let shared = profile_of_browsers(&v, &[a, b]).await;
    profile_of_browsers(&v, &[c]).await;
    // Only one profile is active: the one of a and b; c is in a profile that is not.
    v.db.write(move |tx| store::set_active(tx, shared))
        .await
        .expect("activate");
    let (device, site) = (v.device, shop());
    let remembered = site.clone();
    v.db.write(move |tx| {
        change_browser(tx, device, a, |browser| {
            browser.sites.insert(
                remembered,
                SiteGrant {
                    level: SiteLevel::Operate,
                    fill_remembered: false,
                },
            );
            true
        })
    })
    .await
    .expect("remember");

    assert!(browser_of(&v, a)
        .await
        .expect("a")
        .sites
        .contains_key(&site));
    assert!(
        browser_of(&v, b)
            .await
            .expect("b")
            .sites
            .contains_key(&site),
        "an agent of the same profile opens it without a question"
    );
    assert!(
        !browser_of(&v, c)
            .await
            .is_some_and(|browser| browser.sites.contains_key(&site)),
        "an agent that is not in the profile still asks"
    );
}

#[tokio::test]
async fn a_remembered_fill_and_request_rule_follow_the_profile_and_go_with_its_edit() {
    let v = vault();
    let (a, b) = (registered(&v, "a").await, registered(&v, "b").await);
    let profile = profile_of_browsers(&v, &[a, b]).await;
    let (device, site) = (v.device, shop());
    let (opened, filled, requested) = (site.clone(), site.clone(), site.clone());
    v.db.write(move |tx| {
        change_browser(tx, device, a, |browser| {
            browser.sites.insert(
                opened,
                SiteGrant {
                    level: SiteLevel::Operate,
                    fill_remembered: false,
                },
            );
            true
        })?;
        change_browser(tx, device, a, |browser| remember_fill(browser, &filled))?;
        change_browser(tx, device, a, |browser| {
            browser.requests.insert(requested, RequestRule::Allow);
            true
        })
    })
    .await
    .expect("remember");

    let seen = browser_of(&v, b).await.expect("b");
    assert!(seen.sites[&site].fill_remembered, "the fill holds for b");
    assert_eq!(seen.requests.get(&site), Some(&RequestRule::Allow));

    // Removing it on the profile page removes it for both.
    v.db.write(move |tx| store::set_rights(tx, profile, &browsing()))
        .await
        .expect("edit the profile");
    for agent in [a, b] {
        let browser = browser_of(&v, agent).await.expect("browser");
        assert!(browser.sites.is_empty() && browser.requests.is_empty());
    }
}

#[tokio::test]
async fn standard_gets_its_row_with_the_first_remembered_answer() {
    let v = vault();
    let agent = registered(&v, "a").await;
    let device = v.device;
    // Standard gives the factory rights, which have no browser: nothing is remembered.
    v.db.write(move |tx| change_browser(tx, device, agent, |_| true))
        .await
        .expect("no browser, no change");
    assert!(v
        .db
        .read(|q| store::profile(q, STANDARD_PROFILE_ID))
        .await
        .expect("read")
        .is_none());

    v.db.write(|tx| store::set_rights(tx, STANDARD_PROFILE_ID, &browsing()))
        .await
        .expect("Standard may browse");
    let site = shop();
    v.db.write(move |tx| {
        change_browser(tx, device, agent, |browser| {
            browser.sites.insert(
                site,
                SiteGrant {
                    level: SiteLevel::Read,
                    fill_remembered: false,
                },
            );
            true
        })
    })
    .await
    .expect("remember");
    let subject = Subject::Agent(agent);
    let after =
        v.db.read(move |q| rights_of(q, &subject))
            .await
            .expect("read");
    assert!(after.browser.expect("browser").sites.contains_key(&shop()));
}

#[tokio::test]
async fn an_unreadable_profile_cannot_redirect_a_remembered_fill_to_standard() {
    let v = vault();
    let (profile, subject) = (Uuid::new_v4(), Subject::Model("provider:model".to_owned()));
    let setup_subject = subject.clone();
    v.db.write(move |tx| {
        store::create_profile(tx, profile, "Aktiv")?;
        store::set_rights(tx, profile, &browsing())?;
        store::add_member(tx, &setup_subject, profile)?;
        store::set_active(tx, profile)?;
        Ok(())
    })
    .await
    .expect("profile");
    v.db.write(|tx| {
        crate::storage::preferences::insert_or_update(
            tx,
            crate::storage::preferences::PrefScope::Vault,
            store::ACTIVE_KEY,
            "not-a-profile-id",
        )?;
        Ok(())
    })
    .await
    .expect("unreadable active profile");
    let failed =
        v.db.write(move |tx| change_browser_of(tx, Some(&subject), |_| true))
            .await;
    assert!(failed.is_err(), "the fill must not be written to Standard");
    assert!(v
        .db
        .read(|q| store::profile(q, STANDARD_PROFILE_ID))
        .await
        .expect("read")
        .is_none());
}
