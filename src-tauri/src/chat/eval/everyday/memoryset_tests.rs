// Spec 097's memory set and seeded memory (T078): they load, every fact a request asks for stands in
// one of its pages, a fact of a `body-` request stands only in a page's text and the search finds it
// (SC-009), and every address a request opens is a page of the world.
use std::path::PathBuf;

use serde_json::json;

use super::world_memory::load_memory_file;
use super::{embedded_world, load_set_file, ArgMatch, Category, Expectation};

fn analysis(file: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../specs/097-agent-memory/analysis")
        .join(file)
}

#[test]
fn every_fact_of_the_memory_set_can_be_found() {
    let set = load_set_file(&analysis("memoryset.json")).unwrap();
    assert!(set.requests.len() >= 20, "{} requests", set.requests.len());
    assert!(set.requests.iter().all(|r| r.category == Category::Memory));
    for request in &set.requests {
        let Expectation::Recall(facts) = &request.expect else {
            continue;
        };
        let world = embedded_world().with_memory(&request.memory);
        for alternatives in facts {
            let holder = request.memory.iter().find(|page| {
                alternatives
                    .iter()
                    .any(|fact| page.text.to_lowercase().contains(&fact.to_lowercase()))
            });
            let page =
                holder.unwrap_or_else(|| panic!("{}: no page holds {alternatives:?}", request.id));
            if !request.id.starts_with("body-") {
                continue;
            }
            let fact = &alternatives[0];
            let outside = request.memory.iter().any(|page| {
                format!("{} {}", page.title, page.description)
                    .to_lowercase()
                    .contains(&fact.to_lowercase())
            });
            assert!(
                !outside,
                "{}: {fact} stands in a title or description",
                request.id
            );
            let found = world.answer("memory_search", &json!({ "query": fact }));
            assert!(
                found
                    .content
                    .contains(&format!("\"title\":\"{}\"", page.title)),
                "{}: {}",
                request.id,
                found.content
            );
        }
    }
}

#[test]
fn every_page_a_request_opens_is_in_the_world() {
    let set = load_set_file(&analysis("memoryset.json")).unwrap();
    let world = embedded_world();
    for request in &set.requests {
        let Expectation::Calls(chains) = &request.expect else {
            continue;
        };
        for call in chains.iter().flatten().filter(|c| c.tool == "browser_open") {
            let Some(ArgMatch::Contains { contains }) = call.args.get("url") else {
                panic!("{}: browser_open names its page", request.id);
            };
            assert!(
                world
                    .web_pages
                    .iter()
                    .any(|page| page.url.contains(contains.as_str())),
                "{}: no page {contains}",
                request.id
            );
        }
    }
}

#[test]
fn the_seeded_memory_has_sixty_pages() {
    let pages = load_memory_file(&analysis("seed-memory.json")).unwrap();
    assert_eq!(pages.len(), 60);
    let mut world = embedded_world();
    world.memory = pages;
    let block = world.memory_block(Some(32_768), "model:m").unwrap();
    assert!(!block.contains("more pages are not listed"), "{block}");
}
