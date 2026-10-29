//! Deletions waiting for the user (delete mode "ask", FR-021): the list, and what the user chose.
//! A choice runs at once against the places, through the steps a run uses; a file that changed or
//! came back since the run found it is left to the next run, which looks at it again.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use tokio_util::sync::CancellationToken;
use ts_rs::TS;

use super::execute::{execute, Executed, Target};
use super::model::{SyncRule, SyncRuleSide};
use super::place::{split, Listing, Place};
use super::plan::Step;
use super::progress::Reporter;
use super::store::{self, StateRow};
use crate::files::{FilesError, FilesErrorCode};
use crate::vault_gate::VaultDb;

/// A deletion waiting for the user: `side` is where the file would go.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub struct SyncRulePending {
    pub path: String,
    pub side: SyncRuleSide,
}

/// What the user chose for waiting deletions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub enum SyncRulePendingChoice {
    /// Deletes the file, into the trash as with "mirror".
    Delete,
    /// The file stays where it is and is not copied back.
    Keep,
    /// The file is copied back to where it was deleted.
    Restore,
}

fn unavailable(error: impl std::fmt::Display) -> FilesError {
    log::warn!("file_sync: the waiting deletions could not be used: {error}");
    FilesError::new(FilesErrorCode::Unsupported, "the vault cannot be used now")
}

/// The deletions of `rule_id` waiting for the user.
pub async fn list(
    db: &VaultDb,
    device: &str,
    rule_id: &str,
) -> Result<Vec<SyncRulePending>, FilesError> {
    let (device, rule_id) = (device.to_owned(), rule_id.to_owned());
    let rows = db
        .read(move |r| Ok(store::pending(r, &device, &rule_id)))
        .await
        .map_err(unavailable)?
        .map_err(unavailable)?;
    Ok(rows
        .into_iter()
        .map(|row| SyncRulePending {
            path: row.path,
            side: row.side,
        })
        .collect())
}

/// Applies `choice` to the waiting deletions of `rule` at `paths`. A row goes once its file is
/// handled or the next run has to look at it again; a row whose step failed stays.
#[allow(clippy::too_many_arguments)]
pub async fn resolve(
    db: &VaultDb,
    device: &str,
    rule: &SyncRule,
    run_id: &str,
    a: &dyn Place,
    b: &dyn Place,
    paths: &[String],
    choice: SyncRulePendingChoice,
) -> Result<Executed, FilesError> {
    let (owned, id) = (device.to_owned(), rule.id.clone());
    let (pending, state) = db
        .read(move |r| {
            Ok((
                store::pending(r, &owned, &id)?,
                store::state(r, &owned, &id)?,
            ))
        })
        .await
        .map_err(unavailable)?;
    let known: BTreeMap<&str, &StateRow> =
        state.iter().map(|row| (row.path.as_str(), row)).collect();
    let place = |side: SyncRuleSide| match side {
        SyncRuleSide::A => a,
        SyncRuleSide::B => b,
    };
    let (mut listing_a, mut listing_b) = (Listing::default(), Listing::default());
    let mut steps = Vec::new();
    let mut resolved = Vec::new();
    for row in pending.iter().filter(|row| paths.contains(&row.path)) {
        let (side, path) = (row.side, row.path.as_str());
        let here = place(side).stat(path).await?;
        let there = place(side.other()).stat(path).await?;
        // The file is still as the rule knew it, and still missing on the other side.
        let unchanged = match (&here, known.get(path).and_then(|row| row.seen(side))) {
            (Some(facts), Some(seen)) => facts.size == seen.size && facts.mtime_ms == seen.mtime_ms,
            _ => false,
        };
        let (exists, missing) = (here.is_some(), there.is_none());
        if let Some(facts) = here {
            match side {
                SyncRuleSide::A => listing_a.files.insert(path.to_owned(), facts),
                SyncRuleSide::B => listing_b.files.insert(path.to_owned(), facts),
            };
        }
        let path = path.to_owned();
        match choice {
            SyncRulePendingChoice::Delete if unchanged && missing => {
                steps.push(Step::Delete {
                    side,
                    path: path.clone(),
                });
            }
            SyncRulePendingChoice::Keep if unchanged && missing => {
                steps.push(Step::Keep { path: path.clone() });
            }
            SyncRulePendingChoice::Restore if exists && missing => {
                let (folder, _) = split(&path);
                if !folder.is_empty() {
                    steps.push(Step::Folder {
                        side: side.other(),
                        path: folder.to_owned(),
                    });
                }
                steps.push(Step::Copy {
                    from: side,
                    path: path.clone(),
                    replaces: false,
                });
            }
            _ => {}
        }
        resolved.push(path);
    }
    let executed = execute(
        &Target {
            db,
            device,
            rule,
            run_id,
            a,
            b,
            listing_a: &listing_a,
            listing_b: &listing_b,
            state: &state,
            cancel: &CancellationToken::new(),
            progress: &Reporter::silent(),
        },
        &steps,
    )
    .await;
    resolved.retain(|path| !executed.failures.iter().any(|(failed, _)| failed == path));
    let (owned, id) = (device.to_owned(), rule.id.clone());
    db.write(move |tx| {
        for path in &resolved {
            store::remove_pending(tx, &owned, &id, path)?;
        }
        Ok(())
    })
    .await
    .map_err(unavailable)?;
    Ok(executed)
}

#[cfg(test)]
#[path = "pending_tests.rs"]
mod tests;
