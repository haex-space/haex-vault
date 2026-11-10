//! Asking to switch the browser on (spec 065 revision 2026-11-02, FR-040): when the built-in agent
//! reaches for the browser while its rights group `browser` is off, holzi asks the user once instead
//! of refusing silently. "Erlauben" switches the group on without any website; every website then
//! asks on its first visit as before (FR-007). External agents keep their rights from the MCP
//! settings and are refused as before.

use tokio_util::sync::CancellationToken;

use super::calls::not_allowed;
use super::executor::BrowserTools;
use super::prompts::{self, BrowserDecision, BrowserPromptKind, BrowserQuestion};
use crate::caller::Caller;
use crate::chat::tools::native_action::NativeError;

impl BrowserTools {
    /// Asks whether `caller` may use the browser and switches it on when the user allows.
    pub(super) async fn ask_to_switch_on(
        &self,
        caller: &Caller,
        cancel: &CancellationToken,
    ) -> Result<(), NativeError> {
        if !matches!(caller, Caller::BuiltinAgent) {
            return Err(not_allowed());
        }
        let env = self.env();
        let profile = env.profile_note(caller).await;
        let question = BrowserQuestion {
            kind: BrowserPromptKind::SwitchOn,
            agent: env.agent_name(caller).await,
            site: None,
            tool: None,
            label: None,
            fields: Vec::new(),
            reason: None,
            bytes: None,
            entries: Vec::new(),
            profile: profile.clone(),
            mapping: Vec::new(),
        };
        let (prompts, closed) = self.prompt_channels();
        match prompts::ask(prompts, closed, &question, cancel).await {
            Ok(answer) if answer.decision == BrowserDecision::Allow => env
                .switch_on(caller, profile.as_ref().map(|profile| profile.id.as_str()))
                .await
                .map_err(|error| {
                    NativeError::new(
                        "unavailable",
                        format!("The browser was not switched on: {error}"),
                    )
                }),
            Ok(_) => Err(NativeError::new(
                "user_declined",
                "The user did not allow the browser. Answer without it, and do not ask again in \
                 this conversation unless the user wants it.",
            )),
            Err(_) => Err(not_allowed()),
        }
    }
}
