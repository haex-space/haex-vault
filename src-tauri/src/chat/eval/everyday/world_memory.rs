//! The memory and the web of the everyday measurement (spec 097 FR-026, FR-027, research R11): pages
//! seeded for a run (`HAEX_EVAL_MEMORY`) or a request (its `memory` field), answered by the app's
//! own search, page view and checks; and web pages a browser tool opens, in the shape of the
//! browser's page result, so a page that tells the agent to remember something is foreign content as
//! in the app. Nothing is stored: a write is checked and answered, and the next call sees the seed.

use std::collections::HashMap;

use serde::Deserialize;
use serde_json::{json, Value};

use super::world::{ok, str_arg, Answer, FixtureWorld};
use crate::agent_browser::read::NOTE;
use crate::agent_memory::check::Field;
use crate::agent_memory::executor::{opened, refused, search_result};
use crate::agent_memory::limits::MAX_PAGES;
use crate::agent_memory::store::{checked, MemoryError, Page};
use crate::agent_memory::toc::render;
use crate::agent_memory::turn::block_of;
use crate::agent_memory::{title_key, AuthorKind};
use crate::chat::tools::native_action::NativeError;
use crate::local_time::Civil;
use crate::mcp_server::grants::MemoryLevel;

/// The device every seeded page was written on.
const DEVICE: &str = "fixture-device";

/// One page of a seeded memory (contracts/everyday-format.md, spec 097 research R11).
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct SeedPage {
    pub title: String,
    pub description: String,
    pub text: String,
    /// `user`, or the name of the model that wrote it (`Claude Code`, `qwen3.5-4b`).
    pub by: String,
    /// `YYYY-MM-DD`, the day it was written.
    pub at: String,
}

impl SeedPage {
    /// Noon of `at` in UTC, in milliseconds; an error for a date that is not `YYYY-MM-DD`.
    pub fn millis(&self) -> Result<i64, String> {
        let parts: Vec<&str> = self.at.split('-').collect();
        let bad = || format!("{}: at must be YYYY-MM-DD, not {}", self.title, self.at);
        let [year, month, day] = parts.as_slice() else {
            return Err(bad());
        };
        let (Ok(year), Ok(month), Ok(day)) = (year.parse(), month.parse(), day.parse()) else {
            return Err(bad());
        };
        if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
            return Err(bad());
        }
        Ok(Civil::new(year, month, day, 12, 0).days() * 86_400_000 + 43_200_000)
    }

    fn page(&self, index: usize) -> Page {
        let user = self.by == "user";
        Page {
            id: format!("page-{index}"),
            title: self.title.clone(),
            description: self.description.clone(),
            body: self.text.clone(),
            author_kind: if user {
                AuthorKind::User
            } else {
                AuthorKind::Model
            }
            .as_str()
            .to_owned(),
            author_key: None,
            author_name: if user { String::new() } else { self.by.clone() },
            author_device: DEVICE.to_owned(),
            created_at: self.millis().unwrap_or_default(),
            changed_kind: None,
            changed_key: None,
            changed_name: None,
            changed_device: None,
            changed_at: None,
        }
    }
}

/// Checks a seeded memory: dates that parse, titles that differ.
pub fn check_seed(pages: &[SeedPage]) -> Result<(), String> {
    let mut titles = std::collections::HashSet::new();
    for page in pages {
        page.millis()?;
        if !titles.insert(title_key(&page.title)) {
            return Err(format!("the title {} is used twice", page.title));
        }
    }
    Ok(())
}

/// Reads a seeded memory (`HAEX_EVAL_MEMORY`): a JSON list of pages.
pub fn load_memory_file(path: &std::path::Path) -> Result<Vec<SeedPage>, String> {
    let json = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let pages: Vec<SeedPage> =
        serde_json::from_str(&json).map_err(|e| format!("{}: {e}", path.display()))?;
    check_seed(&pages).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(pages)
}

/// A web page of the world, found by its address.
#[derive(Debug, Clone, Deserialize)]
pub struct WebPage {
    pub url: String,
    pub title: String,
    pub text: String,
}

/// An address without scheme, `www.` and the slash at its end, in lower case.
fn address_key(url: &str) -> String {
    let lower = url.trim().to_lowercase();
    let rest = lower
        .strip_prefix("https://")
        .or_else(|| lower.strip_prefix("http://"))
        .unwrap_or(&lower);
    let rest = rest.strip_prefix("www.").unwrap_or(rest);
    rest.trim_end_matches('/').to_owned()
}

fn page_result(tab: &str, url: &str, title: &str, text: &str) -> Answer {
    let site = address_key(url)
        .split('/')
        .next()
        .unwrap_or_default()
        .to_owned();
    ok(json!({
        "tab": tab,
        "untrusted_page_content": {
            "site": site, "url": url, "title": title, "text": text,
            "truncated": false, "elements": [],
        },
        "note": NOTE,
    }))
}

fn tab_of(index: usize) -> String {
    format!("t{}", index + 1)
}

/// A refusal of a memory action, as the model reads it (`{"error": {code, message, field?}}`).
fn refusal(error: NativeError) -> Answer {
    let mut body = json!({ "code": error.code, "message": error.message });
    if let Some(field) = error.field {
        body["field"] = json!(field);
    }
    Answer {
        content: json!({ "error": body }).to_string(),
        is_error: true,
    }
}

fn invalid(field: &str) -> Answer {
    refusal(NativeError::invalid_input(
        Some(field),
        format!("{field} must be a string"),
    ))
}

impl FixtureWorld {
    /// The world with `extra` pages added to its memory (a request's own).
    pub fn with_memory(&self, extra: &[SeedPage]) -> FixtureWorld {
        let mut world = self.clone();
        world.memory.extend(extra.iter().cloned());
        world
    }

    /// The seeded pages as the store lists them: changed last first.
    pub fn memory_pages(&self) -> Vec<Page> {
        let mut pages: Vec<Page> = self
            .memory
            .iter()
            .enumerate()
            .map(|(index, seed)| seed.page(index))
            .collect();
        pages.sort_by(|a, b| {
            b.last_change()
                .cmp(&a.last_change())
                .then_with(|| a.id.cmp(&b.id))
        });
        pages
    }

    /// The memory block of a turn of the model with the subject key `model`, as the app gives it to
    /// a model whose profile lets it read and write (the factory "Standard").
    pub fn memory_block(&self, context_tokens: Option<usize>, model: &str) -> Option<String> {
        let snapshot = render(&self.memory_pages());
        block_of(
            &snapshot,
            MemoryLevel::ReadWrite,
            context_tokens,
            Some(model),
        )
    }

    fn memory_page(&self, title: &str) -> Option<Page> {
        let key = title_key(title);
        self.memory_pages()
            .into_iter()
            .find(|page| title_key(&page.title) == key)
    }

    /// `None` for a tool that is not one of the memory's.
    pub(super) fn answer_memory(&self, tool: &str, input: &Value) -> Option<Answer> {
        let answer = match tool {
            "memory_search" => match str_arg(input, "query") {
                Some(query) => ok(search_result(&self.memory_pages(), query)),
                None => invalid("query"),
            },
            "memory_open" => match str_arg(input, "title") {
                Some(title) => match self.memory_page(title) {
                    Some(page) => {
                        let devices = HashMap::from([(
                            DEVICE.to_owned(),
                            self.settings["deviceName"]
                                .as_str()
                                .unwrap_or_default()
                                .to_owned(),
                        )]);
                        ok(opened(&page, None, &self.memory_pages(), &devices))
                    }
                    None => refusal(refused(MemoryError::NotFound)),
                },
                None => invalid("title"),
            },
            "memory_add" | "memory_write" | "memory_remove" => self.memory_write(tool, input),
            _ => return None,
        };
        Some(answer)
    }

    /// A write checked as the store checks it; nothing is kept.
    fn memory_write(&self, tool: &str, input: &Value) -> Answer {
        let Some(title) = str_arg(input, "title") else {
            return invalid("title");
        };
        let found = self.memory_page(title);
        let count = self.memory.len();
        let fields: &[(Field, &str)] = match tool {
            "memory_add" => &[(Field::Body, "text")],
            "memory_write" => &[(Field::Description, "description"), (Field::Body, "text")],
            _ => &[],
        };
        let mut checks = vec![(Field::Title, title)];
        for (field, key) in fields {
            match str_arg(input, key) {
                Some(text) => checks.push((*field, text)),
                None => return invalid(key),
            }
        }
        if let Some(description) = str_arg(input, "description").filter(|_| tool == "memory_add") {
            checks.push((Field::Description, description));
        }
        if let Some(new_title) = str_arg(input, "newTitle") {
            checks.push((Field::Title, new_title));
        }
        for (field, text) in checks {
            if let Err(error) = checked(field, text, None, &[]) {
                return refusal(refused(error));
            }
        }
        let result = match (tool, found) {
            ("memory_add", Some(page)) => json!({ "title": page.title, "created": false }),
            ("memory_add", None) if str_arg(input, "description").is_none() => {
                return refusal(refused(MemoryError::DescriptionNeeded));
            }
            ("memory_add", None) => {
                json!({ "title": title.trim(), "created": true, "pages": count + 1 })
            }
            (_, None) => return refusal(refused(MemoryError::NotFound)),
            ("memory_write", Some(page)) => json!({
                "title": str_arg(input, "newTitle").unwrap_or(&page.title),
            }),
            (_, Some(_)) => json!({ "pages": count - 1 }),
        };
        let mut result = result;
        if result.get("pages").is_none() {
            result["pages"] = json!(count);
        }
        result["limit"] = json!(MAX_PAGES);
        ok(result)
    }

    /// `None` for a tool that is not one of the browser's the world answers.
    pub(super) fn answer_web(&self, tool: &str, input: &Value) -> Option<Answer> {
        // Each page of the world has its own agent tab, `t1`, `t2`, …; an unknown address opens one
        // after them.
        let open = |url: &str| {
            let key = address_key(url);
            match self
                .web_pages
                .iter()
                .position(|page| address_key(&page.url) == key)
            {
                Some(index) => {
                    let page = &self.web_pages[index];
                    page_result(&tab_of(index), &page.url, &page.title, &page.text)
                }
                None => page_result(
                    &tab_of(self.web_pages.len()),
                    url,
                    "404 Not Found",
                    "This page does not exist.",
                ),
            }
        };
        let answer = match tool {
            "browser_open" | "browser_tabs_open" => open(str_arg(input, "url")?),
            "browser_navigate" => {
                let to = str_arg(input, "to")?;
                if matches!(to, "back" | "forward" | "reload") {
                    return None;
                }
                open(to)
            }
            "browser_search" => {
                let query = str_arg(input, "query")?.to_lowercase();
                let lines: Vec<String> = self
                    .web_pages
                    .iter()
                    .filter(|page| {
                        query.split_whitespace().any(|word| {
                            page.title.to_lowercase().contains(word)
                                || page.text.to_lowercase().contains(word)
                        })
                    })
                    .map(|page| format!("{} - {}", page.title, page.url))
                    .collect();
                page_result(
                    &tab_of(self.web_pages.len()),
                    "https://search.example/",
                    "Search",
                    &lines.join("\n"),
                )
            }
            "browser_page_read" => {
                let tab = str_arg(input, "tab")?;
                let (index, page) = self
                    .web_pages
                    .iter()
                    .enumerate()
                    .find(|(index, _)| tab_of(*index) == tab)?;
                page_result(&tab_of(index), &page.url, &page.title, &page.text)
            }
            _ => return None,
        };
        Some(answer)
    }
}
