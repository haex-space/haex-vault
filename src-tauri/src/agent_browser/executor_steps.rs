//! The steps of [`BrowserTools`] around a tool: asking the user before it, and settling after it
//! (the new tabs it opened, the page it operated read again).

use std::sync::Arc;

use tokio_util::sync::CancellationToken;

use super::BrowserTools;
use crate::agent_browser::calls::{read_tab, unknown_tab};
use crate::agent_browser::frames::FrameAccess;
use crate::agent_browser::prompts::{
    self, BrowserAnswer, BrowserDecision, BrowserPromptKind, BrowserQuestion,
};
use crate::agent_browser::redaction::Filled;
use crate::agent_browser::session::Session;
use crate::caller::Caller;
use crate::chat::tools::native_action::{NativeError, NativeReply};
use crate::mcp_server::redact::SECRET_PLACEHOLDER;
use crate::prompt_park::PromptEnd;

/// A field whose value holds something filled from the password manager shows `•••`; so does such
/// a value in the question's label.
pub(in crate::agent_browser) fn hide_filled(filled: &Filled, question: &mut BrowserQuestion) {
    if !filled.any() {
        return;
    }
    for field in &mut question.fields {
        if filled.shows(&field.value) {
            field.value = SECRET_PLACEHOLDER.to_owned();
        }
    }
    question.label = question.label.take().map(|label| filled.scrub(&label));
}

impl BrowserTools {
    /// Asks in Manuell before a tool that changes something.
    pub(in crate::agent_browser) async fn confirm(
        &self,
        session: &Session,
        tool: &str,
        label: String,
        site: Option<String>,
        cancel: &CancellationToken,
    ) -> Result<(), NativeError> {
        let question = BrowserQuestion {
            kind: BrowserPromptKind::Action,
            agent: String::new(),
            site,
            tool: Some(tool.to_owned()),
            label: Some(label),
            fields: Vec::new(),
            reason: None,
            bytes: None,
            entries: Vec::new(),
            profile: None,
            mapping: Vec::new(),
        };
        self.ask(session, question, cancel).await
    }

    /// Asks `question` for the agent of `session`; anything but "Erlauben" refuses the tool.
    pub(in crate::agent_browser) async fn ask(
        &self,
        session: &Session,
        question: BrowserQuestion,
        cancel: &CancellationToken,
    ) -> Result<(), NativeError> {
        self.ask_later(session, question, cancel.clone()).await
    }

    /// [`Self::ask`] as a task of its own, for a question that runs while a download goes on.
    pub(in crate::agent_browser) fn ask_later(
        &self,
        session: &Session,
        question: BrowserQuestion,
        cancel: CancellationToken,
    ) -> impl std::future::Future<Output = Result<(), NativeError>> + Send + 'static {
        let answer = self.ask_for(session, question, cancel);
        async move { answer.await.map(drop) }
    }

    /// Asks `question` and returns the user's answer when it allows; anything else refuses the
    /// tool. What the session filled from the password manager shows as `•••` (research R7).
    pub(in crate::agent_browser) fn ask_for(
        &self,
        session: &Session,
        mut question: BrowserQuestion,
        cancel: CancellationToken,
    ) -> impl std::future::Future<Output = Result<BrowserAnswer, NativeError>> + Send + 'static
    {
        hide_filled(&session.filled(), &mut question);
        let (env, prompts, closed, caller) = (
            Arc::clone(&self.env),
            self.prompts.clone(),
            Arc::clone(&self.closed),
            session.caller.clone(),
        );
        async move {
            question.agent = env.agent_name(&caller).await;
            question.profile = env.profile_note(&caller).await;
            match prompts::ask(&prompts, &closed, &question, &cancel).await {
                Ok(answer) if answer.decision == BrowserDecision::Allow => Ok(answer),
                Ok(_) => Err(NativeError::new(
                    "user_declined",
                    "The user declined this action.",
                )),
                Err(PromptEnd::Expired) => Err(NativeError::new(
                    "no_answer",
                    "The user did not answer in time.",
                )),
                Err(_) => Err(NativeError::new(
                    "unavailable",
                    "Nobody could be asked; try again when holzi is open.",
                )),
            }
        }
    }

    /// Where the running browser puts its downloads (research R11).
    pub(in crate::agent_browser) async fn browser_downloads(&self) -> Option<std::path::PathBuf> {
        self.browser.downloads_dir().await
    }

    /// The app as the browser tools see it.
    pub(in crate::agent_browser) fn env(&self) -> &Arc<dyn super::BrowserEnv> {
        &self.env
    }

    /// After an action: waits for the new tabs it opened to join the session or close, then reads
    /// `tab` again when the action operated its page, so the reply shows what it changed.
    pub(super) async fn settle(
        &self,
        session: &Arc<Session>,
        caller: &Caller,
        action_id: &str,
        tab: Option<String>,
        ended: &CancellationToken,
        outcome: Result<NativeReply, NativeError>,
    ) -> Result<NativeReply, NativeError> {
        session.wait_for_opened_tabs(ended).await;
        let (Some(tab_id), Ok(done)) = (tab, &outcome) else {
            return outcome;
        };
        // What the action reported about itself (the focus of a key, whether the wheel moved the
        // page) rides on the page it shows after. The focus names an element of the page, so it
        // goes under `untrusted_page_content`; nothing from the page stays outside it.
        let extras: Vec<(String, serde_json::Value)> = done
            .result
            .as_object()
            .map(|fields| {
                fields
                    .iter()
                    .filter(|(name, _)| name.as_str() != "tab")
                    .map(|(name, value)| (name.clone(), value.clone()))
                    .collect()
            })
            .unwrap_or_default();
        let connection = self.connection().await?;
        let tabs = session.tabs_of(&connection).await;
        let tab = tabs.get(&tab_id).ok_or_else(|| unknown_tab(&tab_id))?;
        // The rights as they are now decide which frames the page shows (FR-053).
        // The action has been done: without rights to look further, its own reply stands.
        let Some(rights) = self.env.rights(caller).await else {
            return outcome;
        };
        let access = FrameAccess {
            session,
            rights: &rights,
        };
        let mut reply = read_tab(&connection, tab, 0, None, &access).await;
        if let Ok(reply) = &mut reply {
            for (name, value) in extras {
                if name == "focus" {
                    reply.result["untrusted_page_content"][name.as_str()] = value;
                } else {
                    reply.result[name.as_str()] = value;
                }
            }
        }
        if let Ok(reply) = &reply {
            session.note(
                &tabs,
                Some((action_id, reply.target.clone().unwrap_or_default())),
            );
        }
        reply
    }
}
