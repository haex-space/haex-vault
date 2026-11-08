//! What the user interface sees of a link (contracts/tauri-commands.md).
//! Payloads are structured data, never localized text.

use serde::Serialize;
use ts_rs::TS;

/// Where a link stands on the main device.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[ts(export, export_to = "../../src/types/bindings/")]
#[serde(rename_all = "snake_case")]
pub enum LinkingStage {
    /// The code is shown and nobody has entered it yet.
    CodeShown,
    /// A new device proved the code and waits for the user's answer.
    AwaitingConfirmation,
}

/// The link in progress on a main device, `null` when there is none.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export, export_to = "../../src/types/bindings/")]
#[serde(rename_all = "camelCase")]
pub struct LinkingStatus {
    pub stage: LinkingStage,
    #[ts(optional)]
    pub new_device_name: Option<String>,
}

/// The code a main device shows.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export, export_to = "../../src/types/bindings/")]
#[serde(rename_all = "camelCase")]
pub struct LinkCodeInfo {
    pub code: String,
    /// The text of the QR code and of the link to copy: the code, the vault's name and the servers
    /// (spec 085). Complete, never shortened.
    pub text: String,
    /// The QR code of `text`; `None` when the text is too long for a QR code that scans reliably
    /// (spec 085, research R7). The link to copy is complete either way.
    pub qr_svg: Option<String>,
    /// Milliseconds since the epoch.
    #[ts(type = "number")]
    pub expires_at: u64,
}

/// Why a join failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[ts(export, export_to = "../../src/types/bindings/")]
#[serde(rename_all = "snake_case")]
pub enum LinkFailure {
    /// Nobody answered: the code ran out, was used already, or was never
    /// shown by a main device.
    Expired,
    Used,
    /// The main device did not accept the proof of the code.
    WrongCode,
    /// The user on the main device declined.
    Rejected,
    /// The two devices run versions that cannot sync (FR-029).
    Incompatible,
    ConnectionLost,
}

/// The join on the new installation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export, export_to = "../../src/types/bindings/")]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum LinkJoinState {
    Searching,
    WaitingForConfirmation,
    /// Pages of the snapshot applied so far.
    Transferring {
        progress: u32,
    },
    #[serde(rename_all = "camelCase")]
    Done {
        vault_name: String,
    },
    Failed {
        reason: LinkFailure,
    },
}
