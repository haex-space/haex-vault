//! Typed writes and reads for `voice_transcript_entries` (spec 066 T124, FR-047, FR-048,
//! data-model.md): everything said in a spoken conversation, in both directions.
//!
//! `chat_messages` stays what the model sees; the transcript also keeps stops, closings,
//! announcements and how far an interrupted answer was spoken. Each local append numbers its entry
//! `MAX(seq) + 1` for the thread inside the same transaction. Two devices may write the same `seq`;
//! readers order by `(seq, id)` so equal values come out the same everywhere. Deleting a thread
//! deletes its entries first in the same write ([`delete_for_thread`]).

use ::uuid::Uuid;
use haex_crdt::rusqlite::{params, Result};
use haex_crdt::CrdtTransaction;
use serde::{Serialize, Serializer};

use crate::storage::query::Query;
use crate::storage::uuid;

/// Text-valued columns of the transcript. A value written by a newer build stays readable as
/// `Other` and is written back unchanged.
macro_rules! text_enum {
    ($(#[$doc:meta])* $name:ident { $($(#[$vdoc:meta])* $variant:ident => $text:literal,)+ }) => {
        $(#[$doc])*
        #[derive(Debug, Clone, PartialEq, Eq)]
        pub enum $name {
            $($(#[$vdoc])* $variant,)+
            /// A value this build does not know.
            Other(String),
        }

        impl $name {
            /// The value stored in the column and sent to the window.
            pub fn as_str(&self) -> &str {
                match self {
                    $(Self::$variant => $text,)+
                    Self::Other(text) => text,
                }
            }

            /// Reads the column.
            pub fn from_column(text: String) -> Self {
                match text.as_str() {
                    $($text => Self::$variant,)+
                    _ => Self::Other(text),
                }
            }
        }

        impl Serialize for $name {
            fn serialize<S: Serializer>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error> {
                serializer.serialize_str(self.as_str())
            }
        }
    };
}

text_enum! {
    /// Who spoke. Spec 067 adds participant ids.
    Speaker {
        User => "user",
        Haex => "haex",
    }
}

text_enum! {
    /// What an entry is (data-model.md).
    EntryKind {
        Utterance => "utterance",
        Answer => "answer",
        Announcement => "announcement",
        Progress => "progress",
        Report => "report",
        ApprovalQuestion => "approvalQuestion",
        ApprovalAnswer => "approvalAnswer",
        ChoiceQuestion => "choiceQuestion",
        ChoiceAnswer => "choiceAnswer",
        Stop => "stop",
        Closing => "closing",
        NotUnderstood => "notUnderstood",
        /// "Passwort von ‚<Titel>‘ vorgelesen"; never the secret itself.
        SecretNote => "secretNote",
    }
}

text_enum! {
    /// How an approval or a choice ended. `Open` is updated once it is answered.
    Outcome {
        Approved => "approved",
        Declined => "declined",
        Open => "open",
        Cancelled => "cancelled",
    }
}

/// An entry to append; `seq`, `id` and `created_at` are set by [`append`].
#[derive(Debug, Clone)]
pub struct NewEntry {
    pub thread_id: Uuid,
    /// Unix ms when the utterance or output began.
    pub spoken_at: i64,
    pub speaker: Speaker,
    pub kind: EntryKind,
    /// The full text; never a secret.
    pub text: String,
    /// Character offset up to which an interrupted answer was spoken.
    pub spoken_until: Option<i64>,
    pub outcome: Option<Outcome>,
    /// `requestId` of an approval or a choice, or a `chat_messages.id`.
    pub ref_id: Option<String>,
}

/// One stored entry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TranscriptEntry {
    pub id: Uuid,
    pub thread_id: Uuid,
    pub seq: i64,
    pub spoken_at: i64,
    pub speaker: Speaker,
    pub kind: EntryKind,
    pub text: String,
    pub spoken_until: Option<i64>,
    pub outcome: Option<Outcome>,
    pub ref_id: Option<String>,
    pub created_at: i64,
}

/// Appends an entry as the next of its thread on this device and returns it as stored.
pub fn append(
    tx: &mut CrdtTransaction<'_>,
    entry: NewEntry,
    now: i64,
) -> haex_crdt::Result<TranscriptEntry> {
    let thread = entry.thread_id.to_string();
    let last: Option<Option<i64>> = tx.query_row(
        "SELECT MAX(seq) FROM voice_transcript_entries WHERE thread_id = ?1",
        params![thread],
        |row| row.get(0),
    )?;
    let stored = TranscriptEntry {
        id: Uuid::new_v4(),
        thread_id: entry.thread_id,
        seq: last.flatten().unwrap_or(0) + 1,
        spoken_at: entry.spoken_at,
        speaker: entry.speaker,
        kind: entry.kind,
        text: entry.text,
        spoken_until: entry.spoken_until,
        outcome: entry.outcome,
        ref_id: entry.ref_id,
        created_at: now,
    };
    tx.execute(
        "INSERT INTO voice_transcript_entries \
           (id, thread_id, seq, spoken_at, speaker, kind, text, spoken_until, outcome, ref_id, \
            created_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
        params![
            stored.id.to_string(),
            thread,
            stored.seq,
            stored.spoken_at,
            stored.speaker.as_str(),
            stored.kind.as_str(),
            stored.text,
            stored.spoken_until,
            stored.outcome.as_ref().map(Outcome::as_str),
            stored.ref_id,
            stored.created_at,
        ],
    )?;
    Ok(stored)
}

/// Records how an approval or a choice ended. Returns the number of changed rows (0 or 1).
pub fn set_outcome(
    tx: &mut CrdtTransaction<'_>,
    id: Uuid,
    outcome: &Outcome,
) -> haex_crdt::Result<usize> {
    tx.execute(
        "UPDATE voice_transcript_entries SET outcome = ?1 WHERE id = ?2",
        params![outcome.as_str(), id.to_string()],
    )
}

/// Every entry of a thread in the order it was said; equal `seq` values from two devices are
/// ordered by id.
pub fn list_for_thread(
    q: &mut impl Query,
    thread_id: Uuid,
) -> haex_crdt::Result<Vec<TranscriptEntry>> {
    q.query_map(
        "SELECT id, thread_id, seq, spoken_at, speaker, kind, text, spoken_until, outcome, ref_id, \
                created_at \
         FROM voice_transcript_entries WHERE thread_id = ?1 ORDER BY seq, id",
        params![thread_id.to_string()],
        row_to_entry,
    )
}

/// Deletes every entry of a thread. Run it in the same write that deletes the thread so each
/// entry gets its own delete marker.
pub fn delete_for_thread(
    tx: &mut CrdtTransaction<'_>,
    thread_id: Uuid,
) -> haex_crdt::Result<usize> {
    tx.execute(
        "DELETE FROM voice_transcript_entries WHERE thread_id = ?1",
        params![thread_id.to_string()],
    )
}

fn row_to_entry(row: &haex_crdt::rusqlite::Row<'_>) -> Result<TranscriptEntry> {
    Ok(TranscriptEntry {
        id: uuid::from_row(row, 0)?,
        thread_id: uuid::from_row(row, 1)?,
        seq: row.get(2)?,
        spoken_at: row.get(3)?,
        speaker: Speaker::from_column(row.get(4)?),
        kind: EntryKind::from_column(row.get(5)?),
        text: row.get(6)?,
        spoken_until: row.get(7)?,
        outcome: row.get::<_, Option<String>>(8)?.map(Outcome::from_column),
        ref_id: row.get(9)?,
        created_at: row.get(10)?,
    })
}
