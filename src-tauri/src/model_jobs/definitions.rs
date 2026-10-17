use super::{model::TaskInput, PAPER_NOTE_RECOGNITION_TASK, TASK_SCHEMA_VERSION};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct TaskDefinition {
    pub name: &'static str,
    pub schema_version: u32,
    pub accepts_image: bool,
    pub allows_general_tools: bool,
}

pub(crate) const PAPER_NOTE_DEFINITION: TaskDefinition = TaskDefinition {
    name: PAPER_NOTE_RECOGNITION_TASK,
    schema_version: TASK_SCHEMA_VERSION,
    accepts_image: true,
    allows_general_tools: false,
};

pub(crate) fn definition_for(input: &TaskInput) -> Option<TaskDefinition> {
    (input.task == PAPER_NOTE_DEFINITION.name
        && input.schema_version == PAPER_NOTE_DEFINITION.schema_version)
        .then_some(PAPER_NOTE_DEFINITION)
}
