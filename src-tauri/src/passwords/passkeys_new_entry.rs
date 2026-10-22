//! A passkey with a new entry (spec 049, research R11, FR-034): the entry created when the user picks
//! "Neuer Eintrag" in the presence confirmation of the External Bridge. It is created through the
//! items layer in the transaction of the passkey, so a passkey that fails leaves no entry behind.

use haex_crdt::CrdtTransaction;

use super::access::authorize_create;
use super::items;
use super::model::ItemInput;
use super::model_passkeys::NewPasskeyEntry;
use super::references_db::{self, Reader};
use crate::error::Result;

/// Whether the caller may create the entry; nothing is written yet.
pub fn authorize(reader: Reader<'_>, entry: &NewPasskeyEntry) -> Result<()> {
    authorize_create(reader.caller, reader.grants, &entry.tags)?;
    Ok(())
}

/// Creates the entry and returns its id. A placeholder in the page's user name or address must
/// point into an entry the caller sees, as for any entry it creates (FR-047).
pub fn create(
    tx: &mut CrdtTransaction<'_>,
    reader: Reader<'_>,
    entry: &NewPasskeyEntry,
) -> Result<String> {
    let texts = [&entry.username, &entry.url]
        .into_iter()
        .filter_map(|text| text.as_deref());
    references_db::check_sources_visible(tx, reader, texts)?;
    let input = ItemInput {
        title: Some(entry.title.clone()),
        url: entry.url.clone(),
        username: entry.username.clone(),
        tags: entry.tags.clone(),
        ..ItemInput::default()
    };
    items::create_item(tx, &input, None)
}
