//! The commands of the chat settings (spec 072 FR-080, contracts/ipc.md). The settings themselves
//! are vault preferences, read and written through the preferences commands like the other
//! settings; what needs the backend is the test of a Blossom-Server address.

use serde::Deserialize;

use crate::chat::peer::files::probe::{self, Check};
use crate::error::Result;
use crate::remote_storage::address::SystemResolver;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BlossomCheckArgs {
    pub url: String,
}

/// Asks the Blossom-Server at `url` whether it takes chat files (research R12).
#[tauri::command]
pub async fn chat_blossom_check(args: BlossomCheckArgs) -> Result<Check> {
    Ok(probe::check(&args.url, &SystemResolver).await)
}
