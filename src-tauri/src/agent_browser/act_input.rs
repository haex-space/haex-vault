//! Keys, hover and the mouse wheel (spec 065 US7, research R19): pages that need more than clicks
//! and text. Every input is real and holzi's own ([`Session::own`]); a key goes under the rules of
//! a click or of typing (FR-062): Enter or Space on an element that submits asks as a click on it
//! would (research R5 step 1), the held requests of step 2 stay, and the website must be one the
//! agent may operate. Hovering and the wheel only read, as scrolling always did.

use std::time::Duration;

use serde::Deserialize;
use serde_json::{json, Value};

use super::{shown_fields, Acting, Target};
use crate::agent_browser::calls::{browser_error, Amount, PressKey, Scroll};
use crate::agent_browser::cdp::input;
use crate::agent_browser::error::BrowserError;
use crate::agent_browser::keys::{self, KeyError, KeyPress, Modifier};
use crate::agent_browser::personal::{form_carries_personal, personal_hint};
use crate::agent_browser::prompts::Effect;
use crate::agent_browser::submit::{element_asks, named_submitting, BrowserField};
use crate::agent_browser::tab::Tab;
use crate::chat::tools::native_action::{NativeError, NativeReply};

/// The most characters `browser.type` presses as keys in one call (FR-050).
const MAX_KEY_TEXT: usize = 500;
/// The most pixels one turn of the wheel scrolls (FR-052).
const MAX_PIXELS: f64 = 5000.0;
/// How much of the view `page` scrolls.
const PAGE_FRACTION: f64 = 0.8;
/// How long the page gets to finish a scroll that animates, and how often it is looked at.
const SETTLE_STEP: Duration = Duration::from_millis(80);
const SETTLE_STEPS: usize = 10;

/// What the page script reports about the point a wheel turns over.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct Probe {
    gone: bool,
    x: f64,
    y: f64,
    /// Width and height of the view.
    size: Vec<f64>,
    /// Scroll position of the document, then of the scroller under the point when there is one.
    pos: Vec<f64>,
}

fn key_error(error: KeyError) -> NativeError {
    match error {
        KeyError::NotAllowed(key) => NativeError::new(
            "key_not_allowed",
            format!(
                "The key {key:?} is not one an agent may press. Allowed: Enter, Escape, Tab, \
                 Backspace, Delete, Space, the arrows, Home, End, PageUp, PageDown and single \
                 characters; Shift with any; Ctrl or Meta only with A, Z, Y."
            ),
        ),
        KeyError::Count => NativeError::invalid_input(Some("count"), "1 to 20"),
    }
}

/// The key presses that type `text`, one per character (FR-050).
pub(super) fn key_presses(text: &str) -> Result<Vec<KeyPress>, NativeError> {
    if text.chars().count() > MAX_KEY_TEXT {
        return Err(NativeError::invalid_input(
            Some("text"),
            "at most 500 characters in keys mode",
        ));
    }
    text.chars()
        .map(|character| {
            let key = character.to_string();
            let shifted: &[Modifier] = if character.is_ascii_uppercase() {
                &[Modifier::Shift]
            } else {
                &[]
            };
            keys::resolve(&key, shifted).map_err(|_| {
                NativeError::invalid_input(
                    Some("text"),
                    "keys mode types printable characters; use browser.press_key for Enter and Tab",
                )
            })
        })
        .collect()
}

/// What a key press is, as a question: a change, or a submit with the form it sends.
fn key_effect(press: &KeyPress, target: &Target) -> (Effect, Vec<BrowserField>) {
    let activates = matches!(press.key.as_str(), "Enter" | " ");
    if target.none || !activates || !press.commands.is_empty() {
        return (Effect::Change, Vec::new());
    }
    if target.kind == "field" {
        // Enter in a text field of a form submits it; a text area and a rich editor take a line
        // break, and Space is only a space.
        let submits = press.key == "Enter" && target.facts.tag == "input" && target.facts.form;
        if submits
            && (form_carries_personal(&target.fields)
                || personal_hint(&target.form_autocomplete)
                || named_submitting(&target.default_submit))
        {
            return (Effect::Submit, shown_fields(&target.fields));
        }
        return (Effect::Change, Vec::new());
    }
    // Enter or Space on a button, a link or a checkbox acts as a click on it.
    let asks = element_asks(&target.kind, &target.facts, &target.name, &target.fields)
        || (target.facts.form && personal_hint(&target.form_autocomplete));
    if asks {
        (Effect::Submit, shown_fields(&target.fields))
    } else {
        (Effect::Change, Vec::new())
    }
}

impl Acting<'_> {
    /// `browser.press_key`: presses a key `count` times, after giving `element` the focus.
    pub(super) async fn press_key(
        &self,
        tab: &Tab,
        call: PressKey,
    ) -> Result<NativeReply, NativeError> {
        let press = keys::resolve(&call.key, &call.modifiers).map_err(key_error)?;
        let count = keys::count(call.count).map_err(key_error)?;
        // Before the focus moves: a website the agent may only read is not touched at all.
        self.operable(&tab.url().await.map_err(browser_error)?)?;
        let mut target = match call.element {
            Some(element) => self.target(tab, json!({ "focusElement": element })).await?,
            None => self.target(tab, json!({ "active": true })).await?,
        };
        if target.none {
            target.name = "page".to_owned();
        }
        self.operable(&target.url)?;
        let (effect, fields) = key_effect(&press, &target);
        self.approve(tab, "browser.press_key", effect, &target, fields)
            .await?;
        tab.take_refusal();
        let events = tab.events();
        for _ in 0..count {
            let pressed = self
                .session
                .own(input::press(self.connection, &tab.session, &press))
                .await;
            if let Err(error) = pressed {
                // The approval armed this tab; a failed key must not leave that allowance for a
                // later page request.
                self.session.disarm();
                return Err(browser_error(error));
            }
        }
        let mut reply = self.finish(tab, events).await?;
        reply.result["focus"] = if target.none {
            json!({ "none": true })
        } else {
            json!({ "kind": target.kind, "name": target.name })
        };
        Ok(reply)
    }

    /// `browser.hover`: the pointer rests on the middle of an element.
    pub(super) async fn hover(&self, tab: &Tab, element: u32) -> Result<NativeReply, NativeError> {
        let target = self.target(tab, json!({ "target": element })).await?;
        tab.take_refusal();
        let events = tab.events();
        self.session
            .own(input::move_to(
                self.connection,
                &tab.session,
                target.x,
                target.y,
            ))
            .await
            .map_err(browser_error)?;
        self.finish(tab, events).await
    }

    /// `browser.scroll`: turns the mouse wheel over the view or an element; an element without an
    /// amount is brought into view instead.
    pub(super) async fn scroll(&self, tab: &Tab, call: Scroll) -> Result<NativeReply, NativeError> {
        let direction = match call.direction.as_deref() {
            Some("down") => Some((0.0, 1.0)),
            Some("up") => Some((0.0, -1.0)),
            Some("right") => Some((1.0, 0.0)),
            Some("left") => Some((-1.0, 0.0)),
            None => None,
            Some(_) => {
                return Err(NativeError::invalid_input(
                    Some("direction"),
                    "up, down, left or right",
                ))
            }
        };
        let events = tab.events();
        let wheel = match (direction, call.element, &call.amount) {
            (None, None, _) => {
                return Err(NativeError::invalid_input(
                    Some("direction"),
                    "up, down, left or right",
                ))
            }
            (None, Some(element), _) | (Some(_), Some(element), None) => {
                tab.scroll(true, Some(element))
                    .await
                    .map_err(browser_error)?;
                None
            }
            (Some(direction), element, amount) => Some((direction, element, amount)),
        };
        let Some(((x, y), element, amount)) = wheel else {
            return self.finish(tab, events).await;
        };
        let pixels = match amount {
            None => None,
            Some(Amount::Page(word)) if word == "page" => None,
            Some(Amount::Pixels(pixels)) if (1.0..=MAX_PIXELS).contains(pixels) => Some(*pixels),
            Some(_) => {
                return Err(NativeError::invalid_input(
                    Some("amount"),
                    "page, or 1 to 5000 pixels",
                ))
            }
        };

        let before = self.probe(tab, json!({ "element": element })).await?;
        let view = |index: usize| before.size.get(index).copied().unwrap_or(800.0);
        let (delta_x, delta_y) = match pixels {
            Some(pixels) => (x * pixels, y * pixels),
            None => (x * view(0) * PAGE_FRACTION, y * view(1) * PAGE_FRACTION),
        };
        tab.take_refusal();
        self.session
            .own(input::wheel(
                self.connection,
                &tab.session,
                before.x,
                before.y,
                delta_x.round(),
                delta_y.round(),
            ))
            .await
            .map_err(browser_error)?;

        // A scroll may animate: look until it stands still, then compare.
        let mut last = before.pos.clone();
        for _ in 0..SETTLE_STEPS {
            tokio::time::sleep(SETTLE_STEP).await;
            let now = self
                .probe(tab, json!({ "x": before.x, "y": before.y }))
                .await?
                .pos;
            let stood = now == last;
            last = now;
            if stood {
                break;
            }
        }
        let mut reply = self.finish(tab, events).await?;
        reply.result["moved"] = Value::Bool(last != before.pos);
        Ok(reply)
    }

    async fn probe(&self, tab: &Tab, probe: Value) -> Result<Probe, NativeError> {
        let reply = crate::agent_browser::read::script(
            self.connection,
            &tab.session,
            &json!({ "probe": probe }),
        )
        .await
        .map_err(browser_error)?;
        let probe: Probe = serde_json::from_value(reply)
            .map_err(|error| browser_error(BrowserError::Protocol(error.to_string())))?;
        if probe.gone {
            return Err(super::element_gone());
        }
        Ok(probe)
    }
}
