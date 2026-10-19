//! The jobs of extension frames that are running (spec 055 R1, R9, FR-028): in memory only, one
//! record per running job, removed the moment the job ends. A record holds what is needed to
//! cancel the job and to enforce its limits, never what the job is about: no provenance, no
//! result, no input.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use super::limits::MAX_ACTIVE_JOBS_PER_FRAME;

pub(crate) type JobId = Uuid;

/// Who started a job: a frame of an extension. A job belongs to its frame alone.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct JobOwner {
    pub extension_id: Uuid,
    pub frame: String,
}

/// A running job. It exists in the registry only while it runs.
#[derive(Debug, Clone)]
pub(crate) struct JobRecord {
    pub id: JobId,
    pub owner: JobOwner,
    pub cancel: CancellationToken,
    /// After this moment the job is over time: its run ends it and reports `timeout`.
    pub deadline: Instant,
}

/// The frame already runs [`MAX_ACTIVE_JOBS_PER_FRAME`] jobs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct TooManyJobs;

#[derive(Clone, Default)]
pub(crate) struct JobRegistry {
    records: Arc<Mutex<HashMap<JobId, JobRecord>>>,
}

fn lock(records: &Mutex<HashMap<JobId, JobRecord>>) -> MutexGuard<'_, HashMap<JobId, JobRecord>> {
    records.lock().unwrap_or_else(PoisonError::into_inner)
}

impl JobRegistry {
    /// Registers a job of `owner` that may run for `timeout`.
    pub(crate) fn start(
        &self,
        owner: JobOwner,
        timeout: Duration,
    ) -> Result<JobRecord, TooManyJobs> {
        let mut records = lock(&self.records);
        let running = records
            .values()
            .filter(|record| record.owner == owner)
            .count();
        if running >= MAX_ACTIVE_JOBS_PER_FRAME {
            return Err(TooManyJobs);
        }
        let record = JobRecord {
            id: Uuid::new_v4(),
            owner,
            cancel: CancellationToken::new(),
            deadline: Instant::now() + timeout,
        };
        records.insert(record.id, record.clone());
        Ok(record)
    }

    pub(crate) fn get(&self, id: JobId) -> Option<JobRecord> {
        lock(&self.records).get(&id).cloned()
    }

    /// Removes a job that ended on its own, after its completion event went out. `false` when it
    /// is gone already (cancelled or over time first): then somebody else reports its end.
    pub(crate) fn finish(&self, id: JobId) -> bool {
        lock(&self.records).remove(&id).is_some()
    }

    /// Cancels a job of `owner`: its token fires and the record is gone. `false` for a job that
    /// does not exist or belongs to another owner; a second cancel is `false`.
    pub(crate) fn cancel(&self, id: JobId, owner: &JobOwner) -> bool {
        let mut records = lock(&self.records);
        if records.get(&id).is_none_or(|record| &record.owner != owner) {
            return false;
        }
        records
            .remove(&id)
            .map(|record| record.cancel.cancel())
            .is_some()
    }

    /// Cancels every job of `owner` (its frame closed or loaded another page). Returns how many.
    pub(crate) fn cancel_owned_by(&self, owner: &JobOwner) -> usize {
        let mut records = lock(&self.records);
        let ids: Vec<JobId> = records
            .values()
            .filter(|record| &record.owner == owner)
            .map(|record| record.id)
            .collect();
        for id in &ids {
            if let Some(record) = records.remove(id) {
                record.cancel.cancel();
            }
        }
        ids.len()
    }
}
