//! The steps of [`BrowserTools`] around a tool: asking the user before it, and settling after it
//! (the new tabs it opened, the page it operated read again).

use std::sync::Arc;

use tokio_util::sync::CancellationToken;

use super::BrowserTools;
use crate::agent_browser::calls::{read_tab, unknown_tab};
use crate::agent_browser::prompts::{self, BrowserDecision, BrowserPromptKind, BrowserQuestion};
use crate::agent_browser::session::Session;
use crate::chat::tools::native_action::{NativeError, NativeReply};
use crate::prompt_park::PromptEnd;

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
        };
        self.ask(session, question, cancel).await
    }

    /// Asks `question` for the agent of `session`; anything but "Erlauben" refuses the tool.
    pub(in crate::agent_browser) async fn ask(
        &self,
        session: &Session,
        mut question: BrowserQuestion,
        cancel: &CancellationToken,
    ) -> Result<(), NativeError> {
        question.agent = self.env.agent_name(&session.caller).await;
        match prompts::ask(&self.prompts, &self.closed, &question, cancel).await {
            Ok(answer) if answer.decision == BrowserDecision::Allow => Ok(()),
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

    /// After an action: waits for the new tabs it opened to join the session or close, then reads
    /// `tab` again when the action operated its page, so the reply shows what it changed.
    pub(super) async fn settle(
        &self,
        session: &Arc<Session>,
        action_id: &str,
        tab: Option<String>,
        ended: &CancellationToken,
        outcome: Result<NativeReply, NativeError>,
    ) -> Result<NativeReply, NativeError> {
        session.wait_for_opened_tabs(ended).await;
        let (Some(tab_id), Ok(_)) = (tab, &outcome) else {
            return outcome;
        };
        let connection = self.connection().await?;
        let tabs = session.tabs_of(&connection).await;
        let tab = tabs.get(&tab_id).ok_or_else(|| unknown_tab(&tab_id))?;
        let reply = read_tab(&connection, tab, 0, None).await;
        if let Ok(reply) = &reply {
            session.note(
                &tabs,
                Some((action_id, reply.target.clone().unwrap_or_default())),
            );
        }
        reply
    }
}
