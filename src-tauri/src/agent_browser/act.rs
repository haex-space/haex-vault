//! Operating a page (spec 065 US2, research R4, R5): clicking, typing and choosing. Clicks and
//! typing are real input events at the element the reading script numbered; choosing sets a
//! selection list from holzi's world. A website the agent may only read refuses all of it
//! (`read_only_site`, FR-008).
//!
//! Each action asks once at most (contracts/browser-tools.md "Freigabe"): in Manuell before it
//! changes something, in Manuell and Auto before it submits (research R5 step 1). A submit the user
//! allowed lets exactly its next changing request go ([`Session::arm`]); any other such request
//! asks on its own (step 2). The action reports after the page settled and every question it set
//! off is answered.

use std::sync::Arc;
use std::time::Duration;

use serde::Deserialize;
use serde_json::{json, Value};
use tokio_util::sync::CancellationToken;

use super::calls::{
    browser_error, input as parsed, read_tab, site_of, tab_error, unknown_tab, Click, Select,
    TypeText,
};
use super::cdp::{input, Connection, Event};
use super::error::BrowserError;
use super::executor::BrowserTools;
use super::prompts::{need, BrowserPromptKind, BrowserQuestion, Effect, Need, PROMPT_TIMEOUT};
use super::read;
use super::session::{Session, Tabs};
use super::sites::may_operate;
use super::submit::{element_submits, shown, BrowserField, Facts};
use super::tab::Tab;
use crate::chat::tools::native_action::{NativeError, NativeReply};
use crate::chat::tools::permission::PermissionMode;
use crate::mcp_server::grants::BrowserRights;

/// What the reading script reports about the element of a number.
#[derive(Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct Target {
    gone: bool,
    x: f64,
    y: f64,
    kind: String,
    name: String,
    facts: Facts,
    /// Of the element's form: name, value, secret.
    fields: Vec<(String, String, bool)>,
    url: String,
    not_field: bool,
    not_select: bool,
    no_option: bool,
}

/// One call that operates a page.
pub(super) struct Acting<'a> {
    pub tools: &'a BrowserTools,
    pub connection: &'a Arc<Connection>,
    pub session: &'a Arc<Session>,
    pub rights: &'a BrowserRights,
    pub mode: PermissionMode,
    pub cancel: &'a CancellationToken,
}

fn element_gone() -> NativeError {
    browser_error(BrowserError::Protocol("element_gone".to_owned()))
}

fn shown_fields(fields: &[(String, String, bool)]) -> Vec<BrowserField> {
    fields
        .iter()
        .map(|(name, value, secret)| shown(name, value, *secret))
        .collect()
}

impl Acting<'_> {
    /// Runs `browser.click`, `browser.type` or `browser.select` on one of `tabs`.
    pub async fn run(
        &self,
        action_id: &str,
        input_value: Value,
        tabs: &Tabs,
    ) -> Result<NativeReply, NativeError> {
        match action_id {
            "browser.click" => {
                let Click { tab, element } = parsed(input_value)?;
                let tab = tabs.get(&tab).ok_or_else(|| unknown_tab(&tab))?;
                self.click(tab, element).await
            }
            "browser.type" => {
                let TypeText {
                    tab,
                    element,
                    text,
                    submit,
                } = parsed(input_value)?;
                let tab = tabs.get(&tab).ok_or_else(|| unknown_tab(&tab))?;
                self.type_text(tab, element, &text, submit).await
            }
            _ => {
                let Select {
                    tab,
                    element,
                    value,
                } = parsed(input_value)?;
                let tab = tabs.get(&tab).ok_or_else(|| unknown_tab(&tab))?;
                self.select(tab, element, &value).await
            }
        }
    }

    async fn target(&self, tab: &Tab, args: Value) -> Result<Target, NativeError> {
        let reply = read::script(self.connection, &tab.session, &args)
            .await
            .map_err(browser_error)?;
        let target: Target = serde_json::from_value(reply)
            .map_err(|error| browser_error(BrowserError::Protocol(error.to_string())))?;
        if target.gone {
            return Err(element_gone());
        }
        Ok(target)
    }

    /// Refuses a page of a website the agent may only read.
    fn operable(&self, url: &str) -> Result<(), NativeError> {
        if may_operate(url, &self.rights.sites, &self.session.once_allowed_sites()) {
            Ok(())
        } else {
            Err(NativeError::new(
                "read_only_site",
                "The agent may only read this website, not operate it.",
            ))
        }
    }

    /// Asks what the approval table asks for `effect`; an allowed submit arms the tab.
    async fn approve(
        &self,
        tab: &Tab,
        tool: &str,
        effect: Effect,
        target: &Target,
        fields: Vec<BrowserField>,
    ) -> Result<(), NativeError> {
        let kind = match need(self.mode, effect) {
            Need::Run => return Ok(()),
            Need::Refuse(code) => {
                return Err(NativeError::new(
                    code,
                    "The approval mode of this agent only lets reading actions run.",
                ))
            }
            Need::Ask(kind) => kind,
        };
        let question = BrowserQuestion {
            kind,
            agent: String::new(),
            site: Some(site_of(&target.url)),
            tool: Some(tool.to_owned()),
            label: Some(target.name.clone()),
            fields,
        };
        self.tools.ask(self.session, question, self.cancel).await?;
        if kind == BrowserPromptKind::Submit {
            self.session.arm(&tab.id);
        }
        Ok(())
    }

    /// Waits until the page settled and the questions the action set off are answered, then
    /// reads the page.
    async fn finish(
        &self,
        tab: &Tab,
        events: tokio::sync::mpsc::UnboundedReceiver<Event>,
    ) -> Result<NativeReply, NativeError> {
        let session = Arc::clone(self.session);
        let settled = tab.settle(events, &move || session.submitting()).await;
        let _ = tokio::time::timeout(PROMPT_TIMEOUT + Duration::from_secs(5), async {
            while self.session.submitting() {
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        })
        .await;
        self.session.disarm();
        settled.map_err(tab_error)?;
        read_tab(self.connection, tab, 0, None).await
    }

    /// `browser.click`: a real click in the middle of the element.
    async fn click(&self, tab: &Tab, element: u32) -> Result<NativeReply, NativeError> {
        let target = self.target(tab, json!({ "target": element })).await?;
        self.operable(&target.url)?;
        let submits = element_submits(&target.kind, &target.facts, &target.name);
        let (effect, fields) = if submits {
            (Effect::Submit, shown_fields(&target.fields))
        } else {
            (Effect::Change, Vec::new())
        };
        self.approve(tab, "browser.click", effect, &target, fields)
            .await?;
        tab.take_refusal();
        let events = tab.events();
        let clicked = input::click(self.connection, &tab.session, target.x, target.y).await;
        if clicked.is_err() {
            // The approval armed this tab; a failed CDP input must not leave that one-request
            // allowance available to a later page request.
            self.session.disarm();
        }
        clicked.map_err(browser_error)?;
        self.finish(tab, events).await
    }

    /// `browser.type`: replaces the field's text; `submit` presses Enter, which submits.
    async fn type_text(
        &self,
        tab: &Tab,
        element: u32,
        text: &str,
        submit: bool,
    ) -> Result<NativeReply, NativeError> {
        let target = self.target(tab, json!({ "focus": element })).await?;
        if target.not_field {
            return Err(NativeError::invalid_input(
                Some("element"),
                "not a field to type into",
            ));
        }
        self.operable(&target.url)?;
        if !submit {
            self.approve(tab, "browser.type", Effect::Change, &target, Vec::new())
                .await?;
        }
        tab.take_refusal();
        let events = tab.events();
        input::insert_text(self.connection, &tab.session, text)
            .await
            .map_err(browser_error)?;
        if submit {
            // The question shows the form as it would be sent, with what was just typed.
            let form = self.target(tab, json!({ "formOf": element })).await?;
            self.approve(
                tab,
                "browser.type",
                Effect::Submit,
                &target,
                shown_fields(&form.fields),
            )
            .await?;
            let entered = input::enter(self.connection, &tab.session).await;
            if entered.is_err() {
                self.session.disarm();
            }
            entered.map_err(browser_error)?;
        }
        self.finish(tab, events).await
    }

    /// `browser.select`: chooses the option with that value or text.
    async fn select(
        &self,
        tab: &Tab,
        element: u32,
        value: &str,
    ) -> Result<NativeReply, NativeError> {
        let target = self.target(tab, json!({ "target": element })).await?;
        if target.kind != "select" {
            return Err(NativeError::invalid_input(
                Some("element"),
                "not a selection list",
            ));
        }
        self.operable(&target.url)?;
        self.approve(tab, "browser.select", Effect::Change, &target, Vec::new())
            .await?;
        tab.take_refusal();
        let events = tab.events();
        let chosen = self
            .target(tab, json!({ "choose": element, "value": value }))
            .await?;
        if chosen.not_select {
            return Err(NativeError::invalid_input(
                Some("element"),
                "not a selection list",
            ));
        }
        if chosen.no_option {
            return Err(NativeError::invalid_input(Some("value"), "no such option"));
        }
        self.finish(tab, events).await
    }
}
