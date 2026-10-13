//! The connection test (spec 038 FR-003, FR-004, SC-004, research R9): write, read, list and
//! delete a test object `holzi-test/<UUID>`. Reading alone would call a read-only key suitable,
//! and an extension could then not upload. The object is deleted also after a failed step; when
//! that delete fails too, the result names its key, which is all holzi can do.
//!
//! On WebDAV and SFTP (spec 050 research R10) the test first asks for the folder of the storage, so
//! a missing folder is `folderMissing` without any write, and afterwards removes the folder
//! `holzi-test/` again when it is empty.

use std::time::Duration;

use tokio::time::Instant;
use uuid::Uuid;

use super::model::ShownHostKey;
use super::{Access, RemoteStore, StorageError, TestOutcome};

/// The time the four steps get together.
pub const TEST_TIME: Duration = Duration::from_secs(15);

/// The time the cleanup gets after a failed step, on top of [`TEST_TIME`].
pub const CLEANUP_TIME: Duration = Duration::from_secs(5);

/// The prefix of test objects; a test never writes elsewhere.
pub const TEST_PREFIX: &str = "holzi-test/";

const CONTENT: &[u8] = b"holzi storage test";

/// The result of a test: the outcome, and the key of a test object holzi could not delete.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProbeResult {
    pub outcome: TestOutcome,
    pub leftover_key: Option<String>,
    /// The key an SFTP server showed when it was not the confirmed one (spec 050 research R5).
    pub host_key: Option<ShownHostKey>,
}

/// The key the server of `access` showed, when the outcome is about host keys.
fn shown(store: &dyn RemoteStore, access: &Access, outcome: TestOutcome) -> Option<ShownHostKey> {
    if !matches!(
        outcome,
        TestOutcome::HostKeyUnknown | TestOutcome::HostKeyChanged
    ) {
        return None;
    }
    let key = store.shown_host_key(&access.location)?;
    Some(ShownHostKey {
        fingerprint: super::sftp::keys::fingerprint(&key)?,
        key,
    })
}

/// The outcome for an error in a step after the object was written: the credentials work, so a
/// refusal now is a missing right.
fn after_write(error: StorageError) -> TestOutcome {
    match error {
        StorageError::Network | StorageError::TimedOut | StorageError::Unsupported => {
            TestOutcome::Unreachable
        }
        StorageError::Protocol => TestOutcome::NotWebDav,
        StorageError::HostKeyUnknown => TestOutcome::HostKeyUnknown,
        StorageError::HostKeyChanged => TestOutcome::HostKeyChanged,
        StorageError::AccessDenied
        | StorageError::MissingRight
        | StorageError::NotFound
        | StorageError::TooLarge
        | StorageError::QuotaExceeded => TestOutcome::MissingRight,
    }
}

/// The outcome for an error of the first write; a missing target is a missing folder on WebDAV
/// and SFTP (spec 050 research R10).
pub(super) fn outcome_of_write(error: StorageError, tree: bool) -> TestOutcome {
    on_write(error, tree)
}

/// See [`outcome_of_write`].
fn on_write(error: StorageError, tree: bool) -> TestOutcome {
    match error {
        StorageError::AccessDenied => TestOutcome::AccessDenied,
        StorageError::MissingRight | StorageError::QuotaExceeded => TestOutcome::MissingRight,
        StorageError::NotFound if tree => TestOutcome::FolderMissing,
        StorageError::NotFound => TestOutcome::BucketMissing,
        StorageError::HostKeyUnknown => TestOutcome::HostKeyUnknown,
        StorageError::HostKeyChanged => TestOutcome::HostKeyChanged,
        StorageError::Protocol => TestOutcome::NotWebDav,
        StorageError::Network
        | StorageError::TimedOut
        | StorageError::TooLarge
        | StorageError::Unsupported => TestOutcome::Unreachable,
    }
}

/// Write, read and list the test object; the caller deletes it. An error says whether the object
/// may have been written (also when the write itself ran out of time).
async fn steps(
    store: &dyn RemoteStore,
    access: &Access,
    key: &str,
    deadline: Instant,
) -> Result<(), (TestOutcome, bool)> {
    store
        .put(access, key, CONTENT.to_vec(), deadline)
        .await
        .map_err(|error| {
            let maybe_written = matches!(error, StorageError::Network | StorageError::TimedOut);
            (on_write(error, access.location.is_tree()), maybe_written)
        })?;
    let written = |error| (after_write(error), true);
    let read = store
        .get(access, key, CONTENT.len(), deadline)
        .await
        .map_err(written)?;
    if read != CONTENT {
        return Err((TestOutcome::MissingRight, true));
    }
    let listed = store
        .list(access, key, 1, deadline)
        .await
        .map_err(written)?;
    if !listed.iter().any(|object| object.key == key) {
        return Err((TestOutcome::MissingRight, true));
    }
    Ok(())
}

/// Whether the folder of a WebDAV or SFTP storage is there; an error is the outcome of the test.
async fn folder_there(
    store: &dyn RemoteStore,
    access: &Access,
    deadline: Instant,
) -> Result<(), TestOutcome> {
    match store.head(access, "", deadline).await {
        Ok(_) => Ok(()),
        Err(StorageError::NotFound) => Err(TestOutcome::FolderMissing),
        Err(error) => Err(on_write(error, true)),
    }
}

/// Tests `access` within [`TEST_TIME`] (see the module).
pub async fn probe(store: &dyn RemoteStore, access: &Access) -> ProbeResult {
    let key = format!("{TEST_PREFIX}{}", Uuid::new_v4());
    let deadline = Instant::now() + TEST_TIME;
    let tree = access.location.is_tree();
    if tree {
        if let Err(outcome) = folder_there(store, access, deadline).await {
            return ProbeResult {
                host_key: shown(store, access, outcome),
                outcome,
                leftover_key: None,
            };
        }
    }
    let steps = steps(store, access, &key, deadline).await;
    let written = steps.as_ref().err().is_none_or(|(_, written)| *written);
    let deleted = if written {
        let cleanup = deadline.max(Instant::now()) + CLEANUP_TIME;
        let deleted = store.delete(access, &key, cleanup).await;
        if tree && deleted.is_ok() {
            // Only an empty folder goes; a failure here leaves nothing but an empty folder.
            let _ = store.delete(access, TEST_PREFIX, cleanup).await;
        }
        deleted
    } else {
        Ok(())
    };
    let outcome = match (steps, &deleted) {
        (Err((outcome, _)), _) => outcome,
        (Ok(()), Ok(())) => TestOutcome::Passed,
        (Ok(()), Err(error)) => after_write(*error),
    };
    ProbeResult {
        host_key: shown(store, access, outcome),
        outcome,
        leftover_key: deleted.is_err().then_some(key),
    }
}
