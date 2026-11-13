//! Positions (spec 065 US8, research R22): clicking, hovering and dragging at points of the
//! screenshot. A position is turned into the element under it first, and that element's rules
//! apply, exactly as for a number: the website must be one the agent may operate, a click on a
//! button that submits asks as a click on it would, and the held requests of step 2 stay. A point
//! with no known element is a change. The view must be the one the screenshot showed (FR-058);
//! otherwise nothing reaches the page.

use std::sync::Arc;
use std::time::{Duration, Instant};

use serde_json::{json, Value};

use super::{click_effect, Acting, Target};
use crate::agent_browser::calls::{browser_error, ClickAt, Drag, Hover};
use crate::agent_browser::cdp::drag::{self, Dragged};
use crate::agent_browser::cdp::input::{self, Button};
use crate::agent_browser::cdp::{page, target};
use crate::agent_browser::frames::{self, Attached};
use crate::agent_browser::prompts::Effect;
use crate::agent_browser::submit::BrowserField;
use crate::agent_browser::tab::Tab;
use crate::agent_browser::view::{self, View, ViewError};
use crate::chat::tools::native_action::{NativeError, NativeReply};

/// How long a question has to take before the page is looked at again: the view may have moved on
/// while the user thought.
const THOUGHT: Duration = Duration::from_millis(250);

/// What is at a point: the facts of the element a click there would operate.
struct Located {
    target: Target,
    /// The element's number in the last reading of the page; `None` when there is none.
    n: Option<u32>,
}

impl Located {
    /// What was hit, as the agent is told (FR-061).
    fn told(&self) -> Value {
        match self.n {
            Some(n) if !self.target.none => {
                json!({ "n": n, "kind": self.target.kind, "name": self.target.name })
            }
            _ => json!({ "none": true }),
        }
    }

    /// Whether `self` and `other` are the same element, as far as a question could tell.
    fn same(&self, other: &Located) -> bool {
        let (this, that) = (&self.target, &other.target);
        // What a question showed counts too: the website and, for a submit, the form's data. A
        // value changed while the question was open must not go out under the approval.
        self.n == other.n
            && this.none == that.none
            && this.kind == that.kind
            && this.name == that.name
            && this.url == that.url
            && this.facts == that.facts
            && this.fields == that.fields
            && this.default_submit == that.default_submit
            && this.form_autocomplete == that.form_autocomplete
    }
}

/// A point in image pixels as it is named in a question.
fn label((x, y): (f64, f64)) -> String {
    format!("Position ({}, {})", x.round(), y.round())
}

fn view_error(error: ViewError) -> NativeError {
    match error {
        ViewError::NoView => NativeError::new(
            "no_view",
            "Take a screenshot first: positions are in the pixels of the last screenshot.",
        ),
        ViewError::Changed => NativeError::new(
            "view_changed",
            "The page no longer looks like the last screenshot (scrolled, loaded, resized or \
             zoomed). Take a new screenshot and use its positions.",
        ),
    }
}

/// The point of the view an image pixel stands for, when it lies inside the screenshot.
fn inside(view: &View, field: &'static str, x: f64, y: f64) -> Result<(f64, f64), NativeError> {
    let (width, height) = view.image();
    if !(x.is_finite() && y.is_finite())
        || x < 0.0
        || y < 0.0
        || x >= f64::from(width)
        || y >= f64::from(height)
    {
        return Err(NativeError::invalid_input(
            Some(field),
            format!("a point inside the screenshot: x 0 to {width}, y 0 to {height}"),
        ));
    }
    Ok(view.css(x, y))
}

/// What a drag asks as, and about which website: the end that submits wins, with its own fields
/// and its own site, so the user approves a submission under the site it goes to.
fn drag_question(grab: &Located, dropped: &Located) -> (Effect, Vec<BrowserField>, String) {
    let (grab_effect, grab_fields) = click_effect(&grab.target);
    let (drop_effect, drop_fields) = click_effect(&dropped.target);
    if drop_effect == Effect::Submit && grab_effect != Effect::Submit {
        (drop_effect, drop_fields, dropped.target.url.clone())
    } else {
        (grab_effect, grab_fields, grab.target.url.clone())
    }
}

impl Acting<'_> {
    /// How the tab looks now, when that is how the last screenshot showed it.
    async fn shown(&self, tab: &Tab) -> Result<View, NativeError> {
        let now = view::current(self.connection, &tab.session)
            .await
            .map_err(browser_error)?;
        view::check(tab.view().as_ref(), &now).map_err(view_error)?;
        Ok(now)
    }

    /// The element at `(x, y)` of the view (CSS pixels), in shadow roots and frames.
    async fn locate(
        &self,
        tab: &Tab,
        (x, y): (f64, f64),
        label: String,
    ) -> Result<Located, NativeError> {
        let landed = page::node_at(self.connection, &tab.session, x, y)
            .await
            .map_err(browser_error)?;
        let (main, _) = page::main_frame(self.connection, &tab.session)
            .await
            .map_err(browser_error)?;
        let frame = landed.owned.unwrap_or(landed.frame);
        let mut located = if frame == main {
            let target = self
                .target(tab, json!({ "at": { "x": x, "y": y } }))
                .await?;
            Located {
                n: target.n,
                target,
            }
        } else {
            self.locate_in_frame(tab, &frame, (x, y)).await?
        };
        located.target.x = x;
        located.target.y = y;
        if located.target.none {
            located.target.name = label;
        }
        Ok(located)
    }

    /// The element at a point of the page that lies in `frame`, which is asked in the frame's own
    /// view. A frame with no listed element is no element: its website is still the one the rights
    /// are checked against.
    async fn locate_in_frame(
        &self,
        tab: &Tab,
        frame: &str,
        (x, y): (f64, f64),
    ) -> Result<Located, NativeError> {
        let Some(slice) = tab.frames().into_iter().find(|slice| slice.frame == frame) else {
            let url = self.frame_url(tab, frame).await;
            return Ok(Located {
                target: Target {
                    none: true,
                    url,
                    ..Target::default()
                },
                n: None,
            });
        };
        let mut attached = Attached::new(self.connection);
        let reply = match frames::origin(&mut attached, tab, &slice.chain).await {
            Ok((left, top)) => {
                let args = json!({ "at": { "x": x - left, "y": y - top } });
                frames::script(&mut attached, tab, &slice, &args).await
            }
            Err(error) => Err(error),
        };
        attached.release().await;
        let mut target: Target =
            serde_json::from_value(reply.map_err(browser_error)?).map_err(|error| {
                browser_error(crate::agent_browser::error::BrowserError::Protocol(
                    error.to_string(),
                ))
            })?;
        // The number is the frame's own; the list numbers frames after the main document.
        target.n = target.n.map(|n| n + slice.first - 1);
        Ok(Located {
            n: target.n,
            target,
        })
    }

    /// The address a frame shows, whether it is of this process or another; empty when not known.
    async fn frame_url(&self, tab: &Tab, frame: &str) -> String {
        if let Ok(nodes) = page::frame_nodes(self.connection, &tab.session).await {
            if let Some(node) = nodes.iter().find(|node| node.id == frame) {
                return node.url.clone();
            }
        }
        target::info(self.connection, frame)
            .await
            .map(|(_, url)| url)
            .unwrap_or_default()
    }

    /// `browser.click_at`: a real click at a point of the screenshot.
    pub(super) async fn click_at(
        &self,
        tab: &Tab,
        call: ClickAt,
    ) -> Result<NativeReply, NativeError> {
        let button = match call.button.as_deref() {
            None | Some("left") => Button::Left,
            Some("right") => Button::Right,
            Some(_) => return Err(NativeError::invalid_input(Some("button"), "left or right")),
        };
        let count = match call.count {
            None | Some(1) => 1,
            Some(2) => 2,
            Some(_) => return Err(NativeError::invalid_input(Some("count"), "1 or 2")),
        };
        let view = self.shown(tab).await?;
        let point = inside(&view, "x", call.x, call.y)?;
        let named = label((call.x, call.y));
        // The pointer comes first: a page that shows something when it arrives (a menu, an
        // overlay) is asked about as it is under the pointer, so the press lands on what was
        // approved.
        self.session
            .own(input::move_to(
                self.connection,
                &tab.session,
                point.0,
                point.1,
            ))
            .await
            .map_err(browser_error)?;
        let located = self.locate(tab, point, named.clone()).await?;
        self.operable(&located.target.url)?;
        let (effect, fields) = click_effect(&located.target);
        let asked = Instant::now();
        self.approve(tab, "browser.click_at", effect, &located.target, fields)
            .await?;
        if asked.elapsed() >= THOUGHT {
            self.unchanged(tab, point, named, &located).await?;
        }
        tab.take_refusal();
        let events = tab.events();
        let clicked = self
            .session
            .own(input::press_at(
                self.connection,
                &tab.session,
                point.0,
                point.1,
                button,
                count,
            ))
            .await;
        if clicked.is_err() {
            // The approval armed this tab; a failed input must not leave that allowance for a
            // later page request.
            self.session.disarm();
        }
        clicked.map_err(browser_error)?;
        let mut reply = self.finish(tab, events).await?;
        reply.result["hit"] = json!([located.told()]);
        Ok(reply)
    }

    /// Whether the page still looks as it did when the question was asked: the same view and the
    /// same element at the point. Nothing reaches the page otherwise.
    async fn unchanged(
        &self,
        tab: &Tab,
        point: (f64, f64),
        label: String,
        before: &Located,
    ) -> Result<(), NativeError> {
        let outcome = async {
            self.shown(tab).await?;
            let now = self.locate(tab, point, label).await?;
            if now.same(before) {
                Ok(())
            } else {
                Err(view_error(ViewError::Changed))
            }
        }
        .await;
        if outcome.is_err() {
            self.session.disarm();
        }
        outcome
    }

    /// `browser.hover` with a position: the pointer rests there. It only reads, as hovering does.
    pub(super) async fn hover_at(
        &self,
        tab: &Tab,
        call: &Hover,
    ) -> Result<NativeReply, NativeError> {
        let (Some(x), Some(y)) = (call.x, call.y) else {
            return Err(NativeError::invalid_input(
                Some("x"),
                "x and y together, or an element",
            ));
        };
        let view = self.shown(tab).await?;
        let point = inside(&view, "x", x, y)?;
        let located = self.locate(tab, point, label((x, y))).await?;
        tab.take_refusal();
        let events = tab.events();
        self.session
            .own(input::move_to(
                self.connection,
                &tab.session,
                point.0,
                point.1,
            ))
            .await
            .map_err(browser_error)?;
        let mut reply = self.finish(tab, events).await?;
        reply.result["hit"] = json!([located.told()]);
        Ok(reply)
    }

    /// `browser.drag`: press at one point, move, release at another. Both ends are looked at; the
    /// stricter question wins, so dragging onto a "delete" zone asks.
    pub(super) async fn drag(&self, tab: &Tab, call: Drag) -> Result<NativeReply, NativeError> {
        let view = self.shown(tab).await?;
        let from = inside(&view, "from", call.from.x, call.from.y)?;
        let to = inside(&view, "to", call.to.x, call.to.y)?;
        let (grab_label, drop_label) = (
            label((call.from.x, call.from.y)),
            label((call.to.x, call.to.y)),
        );
        let grab = self.locate(tab, from, grab_label.clone()).await?;
        let dropped = self.locate(tab, to, drop_label.clone()).await?;
        self.operable(&grab.target.url)?;
        self.operable(&dropped.target.url)?;
        let (effect, fields, site) = drag_question(&grab, &dropped);
        // The question names both ends.
        let asked_about = Target {
            name: format!("{} → {}", grab.target.name, dropped.target.name),
            url: site,
            ..Target::default()
        };
        let asked = Instant::now();
        self.approve(tab, "browser.drag", effect, &asked_about, fields)
            .await?;
        if asked.elapsed() >= THOUGHT {
            self.unchanged(tab, from, grab_label, &grab).await?;
            self.unchanged(tab, to, drop_label, &dropped).await?;
        }
        tab.take_refusal();
        let intercepts = tab.events();
        let events = tab.events();
        let result = self
            .session
            .own(drag::drag(
                Arc::clone(self.connection),
                tab.session.clone(),
                intercepts,
                from,
                to,
            ))
            .await;
        if result.is_err() {
            self.session.disarm();
        }
        if result.map_err(browser_error)? == Dragged::TooSlow {
            self.session.disarm();
            return Err(NativeError::new(
                "drag_too_slow",
                "The drag took longer than 3 seconds; the button was released.",
            ));
        }
        let mut reply = self.finish(tab, events).await?;
        reply.result["hit"] = json!([grab.told(), dropped.told()]);
        Ok(reply)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent_browser::submit::Facts;

    fn submit_button(url: &str, value: &str) -> Located {
        Located {
            n: Some(3),
            target: Target {
                kind: "button".to_owned(),
                name: "Send".to_owned(),
                url: url.to_owned(),
                facts: Facts {
                    tag: "button".to_owned(),
                    kind: "submit".to_owned(),
                    form: true,
                },
                fields: vec![(
                    "email".to_owned(),
                    value.to_owned(),
                    false,
                    "email".to_owned(),
                )],
                ..Target::default()
            },
        }
    }

    fn plain(url: &str) -> Located {
        Located {
            n: Some(1),
            target: Target {
                kind: "div".to_owned(),
                name: "Zone".to_owned(),
                url: url.to_owned(),
                ..Target::default()
            },
        }
    }

    #[test]
    fn a_changed_value_or_site_is_not_the_element_that_was_asked_about() {
        let asked = submit_button("https://a.example/", "a@example.org");
        assert!(asked.same(&submit_button("https://a.example/", "a@example.org")));
        assert!(!asked.same(&submit_button("https://a.example/", "b@example.org")));
        assert!(!asked.same(&submit_button("https://b.example/", "a@example.org")));
    }

    #[test]
    fn a_drag_asks_about_the_site_of_the_end_that_submits() {
        let (effect, fields, site) = drag_question(
            &plain("https://a.example/"),
            &submit_button("https://b.example/", "x"),
        );
        assert_eq!(effect, Effect::Submit);
        assert_eq!(fields.len(), 1);
        assert_eq!(site, "https://b.example/");
        let (_, _, site) =
            drag_question(&plain("https://a.example/"), &plain("https://b.example/"));
        assert_eq!(site, "https://a.example/");
    }
}
