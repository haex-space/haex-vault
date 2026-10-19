//! The fixture of thirty invented extensions and the sentences that use it (spec 018 R14, FR-019).

use std::collections::HashSet;

use super::scoring::{Expect, Kind, Lang};
use super::{embedded_extension_tools, embedded_set};
use crate::chat::tools::action_tool::is_valid_tool_name;

#[test]
fn thirty_extensions_with_two_to_four_tools_each_all_of_them_offerable() {
    let defs = embedded_extension_tools();
    let extensions: HashSet<_> = defs.iter().map(|d| d.extension_id).collect();
    assert_eq!(extensions.len(), 30);
    for id in &extensions {
        let count = defs.iter().filter(|d| d.extension_id == *id).count();
        assert!((2..=4).contains(&count), "{count} tools");
    }
    // Every tool of the file is offerable: nothing is lost to the schema subset or a bad name.
    assert_eq!(defs.len(), 89);
    let names: HashSet<&str> = defs.iter().map(|d| d.model_name.as_str()).collect();
    assert_eq!(names.len(), defs.len(), "names are distinct");
    assert!(defs.iter().all(|d| is_valid_tool_name(&d.model_name)));
    assert!(defs.iter().all(|d| !d.always_ask && !d.dev));
    for def in &defs {
        assert!(!def.tool.examples_de.is_empty() && !def.tool.examples_en.is_empty());
    }
}

#[test]
fn a_mail_extension_counts_unread_messages() {
    let defs = embedded_extension_tools();
    let tool = defs
        .iter()
        .find(|d| d.model_name == "x_mailbox_count_unread")
        .expect("the mail extension of the spec's example");
    assert_eq!(tool.tool.name, "count_unread");
    assert_eq!(tool.extension_title, "Postfach");
}

#[test]
fn the_sentences_for_extensions_name_tools_of_the_fixture_and_rarely_the_extension() {
    let set = embedded_set();
    assert_eq!(set.version, 4);
    let defs = embedded_extension_tools();
    let sentences: Vec<_> = set
        .sentences
        .iter()
        .filter(|s| s.kind == Kind::Extension)
        .collect();
    assert!(sentences.len() >= 10, "{}", sentences.len());
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
