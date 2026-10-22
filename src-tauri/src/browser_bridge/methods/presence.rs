//! The presence confirmation of one passkey ceremony (spec 049, FR-030, FR-031, FR-033, ADR-0015):
//! asked once per request and consumed by that request only, never remembered. Confirming gives UP;
//! with `requireUv` the vault password is checked too and gives UV. Declining, a wrong password, an
//! invalid answer, the deadline and a closed connection all end as `USER_CANCELLED`.

use sha2::{Digest, Sha256};

use super::Call;
use crate::browser_bridge::prompts::{
    BridgeAnswer, BridgeClientRef, BridgeQuestion, PresenceAnswer, PresenceCeremony,
    PresenceChoice, PresenceQuestion,
};
use crate::browser_bridge::protocol::ErrorCode;
use crate::browser_bridge::runtime::Notice;
use crate::passwords::model_passkeys::UserPresence;
use crate::prompt_park::PromptEnd;

/// What the page asks the user to confirm.
pub(super) struct Ceremony {
    pub kind: PresenceCeremony,
    pub rp_id: String,
    pub rp_name: Option<String>,
    pub origin: String,
    pub user_name: Option<String>,
    pub require_uv: bool,
    /// The challenge as the client sent it; only its hash goes to the window.
    pub challenge: Vec<u8>,
    pub choices: Vec<PresenceChoice>,
}

/// The user's pick and the presence it proves.
pub(super) struct Confirmed {
    pub choice: PresenceChoice,
    pub presence: UserPresence,
}

/// Asks the user and checks the answer.
pub(super) async fn confirm(call: &Call<'_>, ceremony: Ceremony) -> Result<Confirmed, ErrorCode> {
    let question = PresenceQuestion {
        client: BridgeClientRef {
            client_id: call.client.client_id.clone(),
            client_name: call.client.client_name.clone(),
            origin: call.client.origin.clone(),
        },
        ceremony: ceremony.kind,
        rp_id: ceremony.rp_id,
        rp_name: ceremony.rp_name,
        origin: ceremony.origin,
        user_name: ceremony.user_name,
        require_uv: ceremony.require_uv,
        choices: ceremony.choices,
        challenge_hash: Sha256::digest(&ceremony.challenge)
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect(),
    };
    let mut prompt_id = String::new();
    let answer = call
        .core
        .prompts
        .ask(
            &BridgeQuestion::Presence(question.clone()),
            call.cancel,
            |id| prompt_id = id.to_owned(),
        )
        .await;
    match answer {
        Ok(BridgeAnswer::Presence(answer)) => accept(call, &question, answer).await,
        Ok(_) => Err(ErrorCode::UserCancelled),
        Err(end) => {
            if end != PromptEnd::NoWindow {
                call.core.notify(Notice::PromptCancelled(prompt_id));
            }
            Err(ErrorCode::UserCancelled)
        }
    }
}

async fn accept(
    call: &Call<'_>,
    question: &PresenceQuestion,
    answer: PresenceAnswer,
) -> Result<Confirmed, ErrorCode> {
    let PresenceAnswer {
        confirm,
        choice,
        passphrase,
    } = answer;
    if !confirm {
        return Err(ErrorCode::UserCancelled);
    }
    // The choice must be one the question offered.
    let Some(choice) = choice.and_then(|picked| {
        question
            .choices
            .iter()
            .find(|offered| offered.same_as(&picked))
            .cloned()
    }) else {
        return Err(ErrorCode::UserCancelled);
    };
    let presence = match (question.require_uv, passphrase) {
        (false, None) => UserPresence::Present,
        (true, Some(passphrase)) => {
            if !call.core.verify_passphrase(passphrase).await {
                call.core.notify(Notice::WrongPassphrase);
                return Err(ErrorCode::UserCancelled);
            }
            UserPresence::Verified
        }
        // A password where none was asked, or none where one was.
        _ => return Err(ErrorCode::UserCancelled),
    };
    Ok(Confirmed { choice, presence })
}
