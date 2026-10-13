use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use super::{
    errors::{TaskError, TaskErrorCode},
    model::TaskProvenance,
};

pub(crate) type TaskId = Uuid;

pub(crate) const MAX_ACTIVE_TASKS_PER_FRAME: usize = 2;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct TaskOwner {
    pub extension_id: Uuid,
    pub frame: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TaskRunStatus {
    Running,
    Completed,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone)]
pub(crate) struct TaskRecord {
    pub id: TaskId,
    pub owner: TaskOwner,
    pub status: TaskRunStatus,
    pub cancel: CancellationToken,
    pub provenance: Option<TaskProvenance>,
}

#[derive(Clone, Default)]
pub(crate) struct TaskRegistry {
    records: Arc<Mutex<HashMap<TaskId, TaskRecord>>>,
}

impl TaskRegistry {
    pub(crate) fn start(&self, owner: TaskOwner) -> Result<TaskRecord, TaskError> {
        let mut records = self.records.lock().unwrap_or_else(|e| e.into_inner());
        let active = records
            .values()
            .filter(|record| record.owner == owner && record.status.is_active())
            .count();
        if active >= MAX_ACTIVE_TASKS_PER_FRAME {
            return Err(TaskError::new(
                TaskErrorCode::LimitExceeded,
                "too many active tasks for this frame",
            ));
        }

        let record = TaskRecord {
            id: Uuid::new_v4(),
            owner,
            status: TaskRunStatus::Running,
            cancel: CancellationToken::new(),
            provenance: None,
        };
        records.insert(record.id, record.clone());
        Ok(record)
    }

    pub(crate) fn get(&self, id: TaskId) -> Option<TaskRecord> {
        self.records
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(&id)
            .cloned()
    }

    pub(crate) fn transition(
        &self,
        id: TaskId,
        status: TaskRunStatus,
        provenance: Option<TaskProvenance>,
    ) -> bool {
        let mut records = self.records.lock().unwrap_or_else(|e| e.into_inner());
        let Some(record) = records.get_mut(&id) else {
            return false;
        };
        if !record.status.is_active() {
            return false;
        }
        record.status = status;
        if provenance.is_some() {
            record.provenance = provenance;
        }
        true
    }

    pub(crate) fn cancel(&self, id: TaskId, owner: &TaskOwner) -> bool {
        let Some(record) = self.get(id) else {
            return false;
        };
        if &record.owner != owner || !record.status.is_active() {
            return false;
        }
        record.cancel.cancel();
        self.transition(id, TaskRunStatus::Cancelled, None)
    }

    pub(crate) fn cancel_owned_by(&self, owner: &TaskOwner) -> usize {
        let ids: Vec<_> = self
            .records
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .values()
            .filter(|record| &record.owner == owner && record.status.is_active())
            .map(|record| record.id)
            .collect();
        ids.into_iter().filter(|id| self.cancel(*id, owner)).count()
    }
}

impl TaskRunStatus {
    pub(crate) fn is_active(self) -> bool {
        matches!(self, Self::Running)
    }
}
