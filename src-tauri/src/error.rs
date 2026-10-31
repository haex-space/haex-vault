//! Top-level error type surfaced across the Tauri boundary. Contract:
//! [`specs/001-frontend-onboarding/contracts/tauri-commands.md`](../../../specs/001-frontend-onboarding/contracts/tauri-commands.md).

use serde::Serialize;
use ts_rs::TS;

use haex_crdt::db::error::DatabaseError;
use haex_crdt::{Error as CrdtError, MigrationJournal};

#[derive(Debug, thiserror::Error, Serialize, TS)]
#[ts(export, export_to = "../../src/types/bindings/")]
#[serde(tag = "kind")]
pub enum HolziError {
    #[error("Instance '{name}' already exists")]
    NameConflict { name: String },

    #[error("Invalid instance name: {reason}")]
    InvalidName { reason: String },

    #[error("Passphrase does not meet policy: {reason}")]
    WeakPassphrase { reason: String },

    #[error("Wrong passphrase")]
    WrongPassphrase,

    #[error("No instance named '{name}'")]
    NotFound { name: String },

    #[error("An instance is already active in this process")]
    InstanceAlreadyActive,

    #[error("The vault is closed")]
    VaultClosed,

    #[error("A vault is already open in this app process")]
    VaultAlreadyActive,

    #[error("Vault file is locked by another process on this host")]
    VaultAlreadyOpenElsewhere,

    #[error("Instance '{name}' is active; close it before this operation")]
    InstanceActive { name: String },

    #[error("File is not a valid holzi instance: {reason}")]
    NotAValidInstance { reason: String },

    #[error("SQLCipher error from haex-crdt: {reason}")]
    CrdtSqlite { reason: String },

    #[error("I/O error from haex-crdt: {reason}")]
    CrdtIo { reason: String },

    #[error("HLC error from haex-crdt: {reason}")]
    CrdtHlc { reason: String },

    #[error("Device UUID mismatch: expected {expected}, supplied {supplied}")]
    DeviceIdMismatch { expected: String, supplied: String },

    #[error("Migration {name} from {journal} is missing from MigrationSource")]
    MigrationMissingFromSource { journal: String, name: String },

    #[error("Migration {name} content drift between database and MigrationSource")]
    MigrationContentDrift {
        name: String,
        expected: String,
        found: String,
    },

    #[error("Legacy schema is incompatible: {reason}")]
    MigrationCompatibility { reason: String },

    #[error("Remote CRDT signature verification failed at change #{first_failed_change}")]
    CrdtSignatureVerificationFailed { first_failed_change: usize },

    #[error("Unexpected non-empty signature under NoopSignatureProvider")]
    CrdtUnexpectedSignatureUnderNoop,

    #[error("CRDT already installed for table {table}")]
    CrdtAlreadyInstalled { table: String },

    #[error("haex-crdt init failed: {reason}")]
    CrdtInit { reason: String },

    #[error("Path resolution failed: {reason}")]
    PathResolution { reason: String },

    #[error("I/O error: {reason}")]
    Io { reason: String },

    #[error("No active instance for this operation")]
    NoActiveInstance,

    /// An action only a main device may take (spec 024, FR-024); the backend checks, not
    /// only the interface.
    #[error("This device is not a main device")]
    NotMainDevice,

    #[error("Catalog entry not found: {id}")]
    CatalogEntryNotFound { id: String },

    #[error("Model download failed: {reason}")]
    ModelDownload { reason: String },

    #[error("Model import failed: {reason}")]
    ModelImport { reason: String },

    #[error("Model not found: {id}")]
    ModelNotFound { id: String },

    #[error("Idempotency key must not be empty")]
    InvalidIdempotencyKey,

    #[error("Idempotency key conflicts with an existing request")]
    IdempotencyKeyConflict,

    #[error("Invalid input: {reason}")]
    InvalidInput { reason: String },

    /// A window manager session above `wm_session::MAX_SESSION_BYTES` (spec
    /// 022-session-restore); the frontend retries once without tab histories.
    #[error("The saved session is too large ({bytes} bytes)")]
    SessionTooLarge {
        #[ts(type = "number")]
        bytes: usize,
    },

    /// The parameters of one vault write transaction exceed
    /// `DatabaseConfig::max_transaction_bytes` (spec 024, research R20); nothing was written.
    #[error("The change is too large to save ({bytes} bytes, limit {limit})")]
    TransactionTooLarge {
        #[ts(type = "number")]
        bytes: usize,
        #[ts(type = "number")]
        limit: usize,
    },

    // --- HuggingFace discovery / install (spec 005) ---------------------
    #[error("Network error contacting Hugging Face: {reason}")]
    Network { reason: String },

    #[error("Hugging Face request timed out: {reason}")]
    Timeout { reason: String },

    #[error("Hugging Face returned HTTP {status}: {reason}")]
    HttpStatus { status: u16, reason: String },

    #[error("Hugging Face rate-limited this request")]
    RateLimited {
        #[ts(type = "number | null")]
        retry_after_seconds: Option<u64>,
    },

    #[error("File is not an installable GGUF: {filename}")]
    UnsupportedFormat { filename: String },

    #[error("A tokenizer repository is required for {repo_id}")]
    TokenizerRequired { repo_id: String },

    #[error("Model size requires explicit hardware confirmation: {fit}")]
    HardwareConfirmationRequired { fit: String },

    #[error("Failed to register downloaded model: {reason}")]
    ModelRegistrationFailed { reason: String },

    // --- Local file integrity (spec 005 Entscheidung 6) ------------------
    #[error("Model {model_id} local file does not match its expected hash")]
    ModelIntegrityMismatch {
        model_id: String,
        expected_sha256: String,
        actual_sha256: String,
    },

    #[error("Model {model_id} has no verifiable expected hash")]
    ModelIntegrityUnknown {
        model_id: String,
        expected_sha256: Option<String>,
    },

    #[error("Model {model_id} could not be hashed: {reason}")]
    ModelIntegrityError { model_id: String, reason: String },

    // --- Voice control (spec 008) -----------------------------------------
    /// The person refused the microphone (spec 043 FR-029); the composer explains how to allow it.
    #[error("Microphone permission is required")]
    MicrophoneDenied,

    #[error("A voice recording is already in progress")]
    AlreadyRecording,

    #[error("No microphone input device is available")]
    DeviceUnavailable,

    #[error("No voice recording is in progress")]
    NotRecording,

    #[error("Transcription failed: {reason}")]
    TranscriptionFailed { reason: String },

    // --- Voice assistant (spec 066) -----------------------------------------
    /// Speech output or one of its models failed; `reason` says what.
    #[error("Speech output failed: {reason}")]
    SpeechFailed { reason: String },

    // --- Address book (spec 051, contracts/tauri-commands.md) ---
    /// The contact or group does not exist, or the caller may not know that it does (FR-031).
    #[error("Contact not found")]
    ContactsNotFound,

    /// The caller has no grant for this operation.
    #[error("Not permitted")]
    ContactsForbidden,

    /// A value or name the address book does not take; `reason` names the problem for the window.
    #[error("Invalid contact: {reason}")]
    ContactsInvalid { reason: String },

    /// A value of a kind this version does not know is read-only (FR-007).
    #[error("This value is read-only")]
    ContactsUnknownKindReadOnly,

    /// The person refused the permission for the phone's address book (FR-017); the settings
    /// explain how to allow it.
    #[error("Contacts permission is required")]
    ContactsDenied,

    /// This device has no phone address book (FR-015).
    #[error("This device has no phone address book")]
    PhoneContactsUnavailable,

    /// This platform offers no External Bridge for browser extensions (spec 049 FR-043).
    #[error("The browser bridge is not available on this platform")]
    BrowserBridgeUnavailable,

    /// No approval or block for this browser extension on this device (spec 049 FR-038).
    #[error("No such browser extension")]
    BrowserBridgeClientNotFound,

    /// A Visitenkarte that is not one, or whose signature does not verify (FR-011).
    #[error("Invalid business card: {reason}")]
    CardInvalid { reason: String },

    /// The Visitenkarte of this vault itself (FR-011).
    #[error("This is your own business card")]
    CardOwn,

    /// A Visitenkarte older than the one already stored for its vault (FR-012).
    #[error("An older business card")]
    CardOlder,

    /// The own Visitenkarte does not fit 2,048 characters even without values.
    #[error("The business card is too large")]
    CardTooLarge,

    // --- Sharing contacts (spec 060, contracts/share-format.md) ---
    /// The own contact, "Ich", cannot be deleted, renamed in its display name or given phone
    /// targets (FR-001 to FR-003, FR-006).
    #[error("This is your own contact")]
    ContactsSelf,

    /// A share that is no share of this version: wrong form, unknown version, a limit broken, or a
    /// choice the preview does not offer; `reason` names the problem for the window.
    #[error("Invalid share: {reason}")]
    ShareInvalid { reason: String },

    /// The share is larger than the limit for link and clipboard (FR-019).
    #[error("The share is too large")]
    ShareTooLarge,

    /// The contact a share was previewed for changed before it was accepted; the window loads the
    /// preview again and nothing was written (FR-024d).
    #[error("The contact changed since the preview")]
    SharePreviewStale,

    /// The phone's address book refused a call; `reason` names the kind of failure, never a
    /// contact's data.
    #[error("The phone's address book failed: {reason}")]
    PhoneContactsFailed { reason: String },

    // --- Extensions as openers for links (spec 063, contracts/tauri-commands.md) ---
    /// A link that is no readable `mailto:`: broken escapes, a line break in a header, a limit
    /// broken; `reason` names the problem for the window, never the link's content.
    #[error("Invalid link: {reason}")]
    OpenerLinkInvalid { reason: String },

    /// The link is larger than the limit for a mail link (FR-008).
    #[error("The link is too large")]
    OpenerLinkTooLarge,

    /// The extension is not installed, not enabled or has not registered for this kind of link.
    #[error("The extension cannot open this link")]
    OpenerNotAvailable,

    // --- Password manager (spec 034, contracts/tauri-commands.md §Fehlerarten) ---
    // None of these carries the value of a secret in a field.
    /// The entry, attachment or passkey does not exist, or the caller may not know that it does
    /// (rule Z5 of `contracts/access.md`).
    #[error("Not found")]
    PasswordsNotFound,

    /// The caller has no grant for this operation (rules Z3 and Z11).
    #[error("Not permitted")]
    PasswordsForbidden,

    /// A save found the entry changed or deleted since it was read (`reason`: `changed` or
    /// `deleted`).
    #[error("Conflict: {reason}")]
    PasswordsConflict { reason: String },

    #[error("The attachment is too large ({bytes} bytes, limit {limit})")]
    PasswordsAttachmentTooLarge {
        #[ts(type = "number")]
        bytes: u64,
        #[ts(type = "number")]
        limit: u64,
    },

    /// An import stopped as a whole (`unreadable`, `wrong_credentials`, `corrupt`,
    /// `unsupported_format`, `encrypted_export`).
    #[error("Import failed: {reason}")]
    PasswordsImportFailed { reason: String },

    /// A placeholder of a value does not resolve (spec 036, `contracts/references.md`): `reason` is
    /// `missing`, `cycle` or `tooDeep`. Never carries a value or the placeholder.
    #[error("Reference does not resolve: {reason}")]
    PasswordsReference { reason: String },

    /// A save would make a value lead back to itself through references (spec 036, FR-046);
    /// names the source of the first step.
    #[error("Reference cycle")]
    PasswordsReferenceCycle {
        #[serde(rename = "sourceItemId")]
        source_item_id: String,
    },

    /// A copy into the trash or a folder inside it (spec 036, `passwords_copy`).
    #[error("Cannot copy into the trash")]
    PasswordsIntoTrash,

    /// The origin of a passkey request does not belong to its relying party (spec 036, FR-025,
    /// research R9); nothing was created or signed.
    #[error("Origin does not match the relying party")]
    PasswordsPasskeyOriginMismatch,

    /// None of the requested algorithms is ES256 or EdDSA, or a stored passkey uses another
    /// (imported RS256, FR-034) or holds a key that does not decode for its algorithm.
    #[error("Unsupported passkey algorithm")]
    PasswordsPasskeyUnsupportedAlgorithm,

    /// A passkey of `excludeCredentials` already exists in a visible entry; nothing was created.
    #[error("A passkey for this account already exists")]
    PasswordsPasskeyExcluded,

    /// More than one passkey fits a confirmation; the caller repeats with one credential id in
    /// `allowCredentials`. The headers carry no key.
    #[error("Several passkeys fit")]
    PasswordsPasskeyChoiceRequired {
        candidates: Vec<crate::passwords::model_passkeys::PasskeyHeader>,
    },

    /// An extension bundle was refused or could not be installed (spec 017,
    /// contracts/bundle-format.md §Prüfung: `reason` is the error kind, e.g. `signature_invalid`,
    /// `file_mismatch`, `legacy_signature_format`). Never carries content of the bundle.
    #[error("Extension not installed: {reason}")]
    ExtensionInstall { reason: String },

    /// The marketplace for extensions refused something or could not do it (spec 069,
    /// contracts/commands.md "Error kinds"): `reason` is one of `marketplace::kinds`, never text
    /// of a marketplace. The field is not called `kind`: that is the tag of this enum.
    #[error("Marketplace: {reason}")]
    Marketplace { reason: String },

    /// No installed extension with this id.
    #[error("Extension not found")]
    ExtensionNotFound,

    /// The extension cannot start on this device yet (`status`: `transferring`,
    /// `signature_failed`, `migration_failed`).
    #[error("Extension not ready: {status}")]
    ExtensionNotReady { status: String },

    /// The extension is disabled (FR-007).
    #[error("Extension disabled")]
    ExtensionDisabled,

    // --- Storage connections (spec 038, contracts/tauri-commands.md) ---
    // None of these carries an endpoint, a credential or the text of a provider.
    /// A field of a connection or a storage breaks its rule (`field`: `providerName`, `region`,
    /// `endpoint`, `credentials`, `name`, `bucket`); for `endpoint` also an address holzi does not
    /// reach (research R8).
    #[error("Invalid storage field: {field}")]
    StorageInvalid { field: String },

    /// No connection or storage with this id.
    #[error("Storage not found")]
    StorageNotFound,

    /// The connection test failed, nothing was saved (FR-003). `leftoverKey` names the test object
    /// when holzi could not delete it (SC-004).
    #[error("Storage test failed: {outcome:?}")]
    StorageTestFailed {
        outcome: crate::remote_storage::TestOutcome,
        #[serde(rename = "leftoverKey")]
        leftover_key: Option<String>,
        /// The key an SFTP server showed when it was not the confirmed one (spec 050).
        #[serde(rename = "hostKey", skip_serializing_if = "Option::is_none")]
        #[ts(optional)]
        host_key: Option<crate::remote_storage::model::ShownHostKey>,
    },

    /// The credentials entry of the connection is not on this device (`syncing`) or was deleted
    /// (`missing`), data-model.md.
    #[error("Storage credentials unavailable")]
    StorageCredentialsUnavailable {
        state: crate::remote_storage::CredentialsState,
    },

    // --- Chosen files and vault files (spec 043, contracts picked-file.md, import-instance.md) ---
    /// The chosen file gives nothing back: gone, the permission was taken back, or a cloud file
    /// is offline.
    #[error("The chosen file cannot be read")]
    Unreadable,

    /// The device ran out of space while copying.
    #[error("Not enough free space")]
    NotEnoughSpace,

    /// A vault file whose vault identity another vault of this installation already has.
    #[error("This vault is already on this device as '{name}'")]
    AlreadyOnThisDevice { name: String },

    /// The server's certificate is not trusted by the system (spec 043 FR-024): the connection
    /// was refused. `reason` names the request, for the log.
    #[error("The server's certificate is not trusted")]
    UntrustedCertificate { reason: String },
}

pub type Result<T> = std::result::Result<T, HolziError>;

impl From<CrdtError> for HolziError {
    /// Maps a `haex-crdt` error into the stable error contract exposed by holzi.
    fn from(err: CrdtError) -> Self {
        match err {
            CrdtError::Sqlite(e) => HolziError::CrdtSqlite {
                reason: e.to_string(),
            },
            CrdtError::Io(e) => HolziError::CrdtIo {
                reason: e.to_string(),
            },
            CrdtError::Hlc(reason) => HolziError::CrdtHlc { reason },
            CrdtError::SignatureVerificationFailed {
                first_failed_change,
            } => HolziError::CrdtSignatureVerificationFailed {
                first_failed_change,
            },
            CrdtError::UnexpectedSignatureUnderNoop => HolziError::CrdtUnexpectedSignatureUnderNoop,
            CrdtError::MigrationMissingFromSource { journal, name } => {
                HolziError::MigrationMissingFromSource {
                    journal: journal_label(journal).to_string(),
                    name,
                }
            }
            CrdtError::MigrationContentDrift {
                name,
                expected,
                found,
            } => HolziError::MigrationContentDrift {
                name,
                expected,
                found,
            },
            CrdtError::MigrationCompatibility { reason } => {
                HolziError::MigrationCompatibility { reason }
            }
            CrdtError::CrdtAlreadyInstalled { table } => HolziError::CrdtAlreadyInstalled { table },
            CrdtError::Database(DatabaseError::VaultAlreadyOpenElsewhere { .. }) => {
                HolziError::VaultAlreadyOpenElsewhere
            }
            CrdtError::Database(DatabaseError::TransactionTooLarge { bytes, limit }) => {
                HolziError::TransactionTooLarge { bytes, limit }
            }
            // A `HolziError` returned from inside a `Database::write` / `Database::read`
            // closure travels as `Consumer` and comes back out unchanged.
            CrdtError::Consumer(source) => match source.downcast::<HolziError>() {
                Ok(holzi) => *holzi,
                Err(source) => HolziError::CrdtInit {
                    reason: source.to_string(),
                },
            },
            // RemoteHlcDriftTooLarge and the remaining database-layer variants have no
            // dedicated holzi variant yet; sync (spec 024) adds the ones it surfaces.
            other => HolziError::CrdtInit {
                reason: other.to_string(),
            },
        }
    }
}

impl From<HolziError> for CrdtError {
    /// Carries a holzi error out of a `Database::write` / `Database::read` closure; the
    /// reverse mapping above unwraps it again.
    fn from(err: HolziError) -> Self {
        CrdtError::consumer(err)
    }
}

impl From<std::io::Error> for HolziError {
    /// Wraps application-level I/O failures for transport across the Tauri boundary.
    fn from(e: std::io::Error) -> Self {
        HolziError::Io {
            reason: e.to_string(),
        }
    }
}

/// Returns the database table name associated with a migration journal.
fn journal_label(j: MigrationJournal) -> &'static str {
    match j {
        MigrationJournal::CrateOwned => "haex_crdt_migrations_no_sync",
        MigrationJournal::ConsumerOwned => "haex_app_migrations_no_sync",
    }
}

#[cfg(test)]
#[path = "error_tests.rs"]
mod tests;
