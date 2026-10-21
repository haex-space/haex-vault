//! The fixture of thirty extensions (twenty-nine invented ones and haex-mail with its fifteen real
//! tools) and the sentences that use it (spec 018 R14, FR-019; spec 019 FR-035).

use std::collections::HashSet;

use super::scoring::{Expect, Kind, Lang};
use super::{embedded_extension_tools, embedded_set};
use crate::chat::tools::action_tool::is_valid_tool_name;
use crate::extensions::bundle::ToolEffect;

/// The extension whose tools are the real ones of haex-mail (spec 019); the others are invented.
const MAIL: &str = "mail";

#[test]
fn thirty_extensions_all_of_their_tools_offerable() {
    let defs = embedded_extension_tools();
    let extensions: HashSet<_> = defs.iter().map(|d| d.extension_id).collect();
    assert_eq!(extensions.len(), 30);
    for id in &extensions {
        let tools: Vec<_> = defs.iter().filter(|d| d.extension_id == *id).collect();
        if tools[0].model_name.starts_with(&format!("x_{MAIL}_")) {
            assert_eq!(tools.len(), 15, "haex-mail declares fifteen tools");
        } else {
            assert!((2..=4).contains(&tools.len()), "{} tools", tools.len());
        }
    }
    // Every tool of the file is offerable: nothing is lost to the schema subset or a bad name.
    assert_eq!(defs.len(), 29 * 3 + 15 - 1);
    let names: HashSet<&str> = defs.iter().map(|d| d.model_name.as_str()).collect();
    assert_eq!(names.len(), defs.len(), "names are distinct");
    assert!(defs.iter().all(|d| is_valid_tool_name(&d.model_name)));
    assert!(defs.iter().all(|d| !d.always_ask && !d.dev));
    for def in &defs {
        assert!(!def.tool.examples_de.is_empty() && !def.tool.examples_en.is_empty());
    }
}

#[test]
fn haex_mail_offers_its_fifteen_tools_three_of_them_destructive() {
    let defs = embedded_extension_tools();
    let mail: Vec<_> = defs
        .iter()
        .filter(|d| d.model_name.starts_with(&format!("x_{MAIL}_")))
        .collect();
    assert_eq!(mail.len(), 15);
    assert!(mail.iter().all(|d| d.extension_title == "haex-mail"));
    let count_unread = mail
        .iter()
        .find(|d| d.model_name == "x_mail_list_mailboxes")
        .expect("the tool that answers how many mails are unread");
    assert_eq!(count_unread.tool.name, "list_mailboxes");
    let mut destructive: Vec<_> = mail
        .iter()
        .filter(|d| matches!(d.tool.effect, ToolEffect::Destructive))
        .map(|d| d.tool.name.as_str())
        .collect();
    destructive.sort_unstable();
    assert_eq!(destructive, ["add_rule", "delete_messages", "send_mail"]);
}

#[test]
fn the_sentences_for_extensions_name_tools_of_the_fixture_and_rarely_the_extension() {
    let set = embedded_set();
    assert_eq!(set.version, 5);
    let defs = embedded_extension_tools();
    let sentences: Vec<_> = set
        .sentences
        .iter()
        .filter(|s| s.kind == Kind::Extension)
        .collect();
    assert!(sentences.len() >= 24 + 10, "{}", sentences.len());
    let mail = sentences
        .iter()
        .filter(|s| matches!(&s.expect, Expect::Calls(c) if c.iter().all(|c| c.tool.starts_with(&format!("x_{MAIL}_")))))
        .count();
    assert!(mail >= 24, "{mail} sentences for haex-mail");
    assert!(sentences.iter().any(|s| s.lang == Lang::De));
    assert!(sentences.iter().any(|s| s.lang == Lang::En));
    assert!(sentences.iter().all(|s| !s.self_test));

    let mut naming = 0;
    for sentence in &sentences {
        let Expect::Calls(calls) = &sentence.expect else {
            panic!("{} expects a call", sentence.id);
        };
        for call in calls {
            let def = defs
                .iter()
                .find(|d| d.model_name == call.tool)
                .unwrap_or_else(|| panic!("{}: {} is not in the fixture", sentence.id, call.tool));
            if sentence
                .text
                .to_lowercase()
                .contains(&def.extension_title.to_lowercase())
            {
                naming += 1;
            }
        }
    }
    assert!(naming <= 2, "{naming} sentences name their extension");
}
