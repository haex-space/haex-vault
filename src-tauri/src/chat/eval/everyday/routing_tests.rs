// Spec 082 US2 in the everyday measurement: the routed offer of every request, and a run that routes.
use std::time::Duration;

use serde_json::json;
use tokio_util::sync::CancellationToken;

use super::mail_names::MailNames;
use super::runner::{app_registry, run_everyday, Profile};
use super::test_support::request_with;
use super::{embedded_mail_tools, embedded_set, embedded_world, Expectation};
use crate::adapters::types::StreamChunk;
use crate::chat::eval::embedded_tools;
use crate::chat::eval::testing::Scripted;
use crate::chat::tools::availability::ToolAvailability;
use crate::chat::tools::domains::{domain_of, domains};
use crate::chat::tools::offer::routed_offer;

#[test]
fn the_offer_routed_to_the_expected_domain_holds_each_chain_s_first_tool() {
    // Spec 082 US2's independent test, without a model.
    let mail = embedded_mail_tools();
    let names = MailNames::of(&mail);
    let registry = app_registry(&embedded_tools(), &mail, Profile::Desktop);
    let registry = registry.lock().unwrap();
    let installed = domains(&registry);
    let mut checked = 0;
    for request in embedded_set().requests {
        let Expectation::Calls(chains) = &request.expect else {
            continue;
        };
        let first = registry
            .get(names.in_app(&chains[0][0].tool))
            .expect("a registered tool");
        let domain = domain_of(first.as_ref()).expect("a tool of a domain");
        let offer: Vec<String> = routed_offer(
            ToolAvailability::Offered,
            &registry,
            &installed,
            std::slice::from_ref(&domain),
            &request.text,
        )
        .into_iter()
        .map(|tool| tool.name)
        .collect();
        for chain in chains {
            let tool = names.in_app(&chain[0].tool).to_owned();
            assert!(
                offer.contains(&tool),
                "{} routed to {domain}: {} not in {offer:?}",
                request.id,
                tool
            );
        }
        checked += 1;
    }
    assert!(checked > 50, "{checked} requests checked");
}

/// A model that routes to contacts and then answers.
fn routing_model() -> Scripted {
    Scripted::new(|req| {
        let reply = if req.tools.is_empty() {
            "contacts"
        } else {
            "Anna wohnt in Bonn."
        };
        vec![StreamChunk::Delta {
            content: reply.to_owned(),
            reasoning: None,
        }]
    })
}

#[tokio::test]
async fn a_routed_run_offers_the_domain_s_tools_and_records_the_routing_call() {
    let model = routing_model();
    let expect = json!({ "calls": [{ "tool": "contacts_search" }] });
    let runs = run_everyday(
        &model,
        "m",
        true,
        &request_with("Wo wohnt Anna?", expect),
        &embedded_tools(),
        &embedded_mail_tools(),
        &embedded_world(),
        Profile::Desktop,
        Some(Duration::from_secs(3)),
        &CancellationToken::new(),
    )
    .await
    .unwrap();
    let route = runs[0].route.clone().expect("a routing call");
    assert_eq!(route.routed, ["contacts"]);
    assert_eq!(route.hit, Some(true));
    let seen = model.seen.lock().unwrap();
    assert!(seen[0].tools.is_empty(), "the routing call comes first");
    let offered: Vec<&str> = seen[1].tools.iter().map(|t| t.name.as_str()).collect();
    assert!(offered.contains(&"contacts_search"), "{offered:?}");
    // Of the window tools only opening the contacts app comes along.
    let window: Vec<&str> = offered
        .iter()
        .copied()
        .filter(|name| name.starts_with("wm_"))
        .collect();
    assert_eq!(window, ["wm_app_open"], "{offered:?}");
}

#[tokio::test]
async fn a_run_without_routing_makes_no_routing_call() {
    let model = routing_model();
    let runs = run_everyday(
        &model,
        "m",
        true,
        &request_with("Wo wohnt Anna?", json!("question")),
        &embedded_tools(),
        &embedded_mail_tools(),
        &embedded_world(),
        Profile::Desktop,
        None,
        &CancellationToken::new(),
    )
    .await
    .unwrap();
    assert!(runs[0].route.is_none());
    assert!(!model.seen.lock().unwrap()[0].tools.is_empty());
}

#[tokio::test]
async fn a_routed_run_records_the_routing_and_the_first_offer_in_the_set_s_names() {
    let model = Scripted::new(|req| {
        let reply = if req.tools.is_empty() {
            "mail"
        } else {
            "Die Hausverwaltung schreibt wegen der Wartung."
        };
        vec![StreamChunk::Delta {
            content: reply.to_owned(),
            reasoning: None,
        }]
    });
    let expect = json!({ "calls": [{ "tool": "x_mail_find_messages" }] });
    let runs = run_everyday(
        &model,
        "m",
        true,
        &request_with("Was schreibt die Hausverwaltung?", expect),
        &embedded_tools(),
        &embedded_mail_tools(),
        &embedded_world(),
        Profile::Desktop,
        Some(Duration::from_secs(3)),
        &CancellationToken::new(),
    )
    .await
    .unwrap();
    let route = runs[0].route.clone().expect("a routing call");
    // The app's domain key `x_haexmail` is written as the set names it.
    assert_eq!(route.routed, ["x_mail"]);
    assert_eq!(route.hit, Some(true));
    let offer = &runs[0].first_offer;
    assert!(
        offer.contains(&"x_mail_find_messages".to_owned()),
        "{offer:?}"
    );
    assert!(
        offer.iter().all(|name| !name.starts_with("x_haexmail_")),
        "{offer:?}"
    );
    let mut sorted = offer.clone();
    sorted.sort();
    assert_eq!(*offer, sorted);
}
