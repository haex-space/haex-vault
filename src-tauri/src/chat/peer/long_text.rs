//! Sending a text (spec 072 FR-012, research R2 as amended 2026-11-20): a text goes as a message of
//! its own when it fits one wrap and as a file with a preview when it does not. A note has no wrap,
//! so it always goes as itself. Only the bound of [`envelope::MAX_TEXT_CHARS`] refuses a text.

use uuid::Uuid;

use super::delivery::now_ms;
use super::session::Session;
use super::{envelope, send};
use crate::error::{HaexError, Result};
use crate::vault_gate::VaultDb;

/// Queues `text` to the conversation `thread_id` and answers the message id. A retry with the same
/// `event_id` (from the command's idempotency key) answers the first message, whichever way the text
/// went the first time.
pub async fn send_text(
    db: &VaultDb,
    session: &Session,
    thread_id: Uuid,
    event_id: &str,
    text: &str,
) -> Result<Uuid> {
    let text = envelope::checked_text(text)?.to_owned();
    let me = session.me.clone();
    let now = now_ms();
    let (probe_me, probe_text) = (me.clone(), text.clone());
    let fits = db
        .read(move |r| {
            Ok(send::fits_inline(
                r,
                &probe_me,
                thread_id,
                &probe_text,
                now,
            )?)
        })
        .await?;
    let message_id = if fits {
        let event_id = event_id.to_owned();
        db.write(move |tx| {
            Ok(send::message(
                tx,
                &me,
                thread_id,
                &event_id,
                &text,
                Vec::new(),
                now,
            )?)
        })
        .await?
    } else {
        let files = session
            .files
            .get()
            .ok_or_else(|| HaexError::NotFound {
                name: "chat files".into(),
            })?
            .clone();
        files
            .send_long_text(&me, thread_id, event_id, &text)
            .await?
    };
    session.kick.notify_one();
    Ok(message_id)
}

#[cfg(test)]
#[path = "long_text_tests.rs"]
mod tests;
