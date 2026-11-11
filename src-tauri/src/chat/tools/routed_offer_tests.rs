// The routed offer of spec 082 research R5 (T036).
use std::sync::Arc;

use super::ask_user::AskUserTool;
use super::availability::ToolAvailability;
use super::cli::CliTool;
use super::domains::domains;
use super::domains_tests::registry_with;
use super::find_actions::FindActionsTool;
use super::offer::{first_offer, routed_offer, with_search, MAX_ACTION_OFFER, MAX_DOMAIN_TOOLS};
use super::{RiskClass, ToolRegistry};
use crate::chat::eval::runner::register_extensions;
use crate::chat::eval::{embedded_extension_tools, embedded_tools};

/// The chat's registry on a desktop: holzi's actions, haex-mail's fifteen tools and the command tool.
fn desktop() -> ToolRegistry {
    let mut registry = registry_with(Vec::new());
    let mail: Vec<_> = embedded_extension_tools()
        .into_iter()
        .filter(|def| def.model_name.starts_with("x_mail_"))
        .collect();
    assert_eq!(mail.len(), 15);
    register_extensions(&mut registry, &mail);
    registry.register(Arc::new(FindActionsTool::new(embedded_tools())));
    registry.register(Arc::new(AskUserTool));
    registry.register(Arc::new(CliTool::default()));
    registry
}

fn offer(registry: &ToolRegistry, routed: &[&str], message: &str) -> Vec<String> {
    let routed: Vec<String> = routed.iter().map(|key| key.to_string()).collect();
    routed_offer(
        ToolAvailability::Offered,
        registry,
        &domains(registry),
        &routed,
        message,
    )
    .into_iter()
    .map(|tool| tool.name)
    .collect()
}

fn of_domain<'a>(offer: &'a [String], prefix: &str) -> Vec<&'a String> {
    offer
        .iter()
        .filter(|name| name.starts_with(prefix))
        .collect()
}

#[test]
fn a_small_domain_gets_all_its_tools_with_the_search_ask_user_and_the_command_tool() {
    let registry = desktop();
    let offer = offer(&registry, &["contacts"], "Wie ist die Nummer von Anna?");
    let contacts: Vec<String> = registry
        .iter()
        .filter(|tool| tool.name().starts_with("contacts_"))
        .map(|tool| tool.name().to_owned())
        .collect();
    assert!(!contacts.is_empty());
    for name in &contacts {
        assert!(offer.contains(name), "{name}");
    }
    for name in ["ask_user", "run_command"] {
        assert!(offer.contains(&name.to_owned()), "{name}");
    }
}

/// The tool search is always offered (operator 2026-11-11, amending spec 087 research R6.3): a
/// routed first step has it beside the area's tools, so a misrouted request can still find its
/// tools, and the next step does not add it twice.
#[test]
fn a_routed_first_step_has_the_tool_search_once() {
    let registry = desktop();
    let mut tools = routed_offer(
        ToolAvailability::Offered,
        &registry,
        &domains(&registry),
        &["x_mail".to_owned()],
        "Lösch die Mail von Bernd",
    );
    assert!(tools.iter().any(|tool| tool.name == "find_actions"));
    assert!(!with_search(&registry, &mut tools));
    with_search(&registry, &mut tools);
    assert_eq!(
        tools
            .iter()
            .filter(|tool| tool.name == "find_actions")
            .count(),
        1
    );
    // An unrouted first step keeps the search.
    let first = first_offer(
        ToolAvailability::Offered,
        &registry,
        "Lösch die Mail von Bernd",
    );
    assert!(first.iter().any(|tool| tool.name == "find_actions"));
}

#[test]
fn a_large_domain_gets_its_reading_tools_first_then_the_best_matches_up_to_twelve() {
    let registry = desktop();
    let offer = offer(&registry, &["x_mail"], "Lösch die Mail von Bernd");
    let mail = of_domain(&offer, "x_mail_");
    assert_eq!(mail.len(), MAX_DOMAIN_TOOLS);
    for (position, name) in mail.iter().enumerate() {
        let risk = registry.get(name).unwrap().risk_class();
        assert_eq!(
            risk == RiskClass::Safe,
            position < 5,
            "{name} at {position}"
        );
    }
    // The deletion the message names comes right after the lookups.
    assert_eq!(mail[5], "x_mail_delete_messages");
}

#[test]
fn a_domain_brings_what_it_also_needs() {
    let registry = desktop();
    let offer = offer(&registry, &["passwords"], "Wie ist mein Netflix-Passwort?");
    assert!(offer.contains(&"wm_app_open".to_owned()));
}

#[test]
fn window_tools_come_only_with_their_domain_or_as_needed() {
    let registry = desktop();
    // Contacts have an app: opening it is the one window tool they bring.
    let contacts = offer(&registry, &["contacts"], "Öffne die Kontakte-App");
    assert_eq!(of_domain(&contacts, "wm_"), ["wm_app_open"], "{contacts:?}");
    let web = offer(&registry, &["browser"], "Suche nach dem Wetter");
    assert!(of_domain(&web, "wm_").is_empty(), "{web:?}");
    let apps = offer(&registry, &["wm"], "Schließ den Tab");
    assert!(apps.contains(&"wm_tab_close".to_owned()));
}

#[test]
fn two_domains_stay_within_the_limit() {
    let registry = desktop();
    let offer = offer(
        &registry,
        &["x_mail", "files"],
        "Schick Anna die Steuererklärung",
    );
    let routed = offer.iter().filter(|name| *name != "run_command").count();
    assert!(routed <= MAX_ACTION_OFFER, "{routed}: {offer:?}");
    assert!(!of_domain(&offer, "x_mail_").is_empty() && !of_domain(&offer, "files_").is_empty());
}

#[test]
fn no_domain_gives_today_s_first_offer() {
    let registry = desktop();
    let text = "Was ist die Hauptstadt von Frankreich?";
    let today: Vec<String> = first_offer(ToolAvailability::Offered, &registry, text)
        .into_iter()
        .map(|tool| tool.name)
        .collect();
    assert_eq!(offer(&registry, &[], text), today);
    assert_eq!(offer(&registry, &["calendar"], text), today);
}

#[test]
fn a_spoken_message_gives_the_same_offer() {
    let registry = desktop();
    assert_eq!(
        offer(&registry, &["x_mail"], "Lösch die Mail von Bernd!"),
        offer(&registry, &["x_mail"], "lösch die mail von bernd")
    );
}
