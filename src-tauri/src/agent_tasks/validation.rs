use super::{
    errors::{TaskError, TaskErrorCode},
    model::{PaperNoteRegionKind, PaperNoteResult},
    TASK_SCHEMA_VERSION,
};

const MAX_REGIONS: usize = 512;

pub(crate) fn validate_paper_note_result(result: &PaperNoteResult) -> Result<(), TaskError> {
    if result.schema_version != TASK_SCHEMA_VERSION {
        return Err(TaskError::new(
            TaskErrorCode::InvalidResult,
            "unsupported paper-note result schema",
        ));
    }
    if result.regions.len() > MAX_REGIONS {
        return Err(TaskError::new(
            TaskErrorCode::InvalidResult,
            "paper-note result contains too many regions",
        ));
    }

    for region in &result.regions {
        let bounds = region.source_box;
        let coordinates = [bounds.x, bounds.y, bounds.width, bounds.height];
        if coordinates
            .iter()
            .any(|value| !value.is_finite() || *value < 0.0 || *value > 1.0)
            || bounds.x + bounds.width > 1.0
            || bounds.y + bounds.height > 1.0
            || bounds.width == 0.0
            || bounds.height == 0.0
        {
            return Err(TaskError::new(
                TaskErrorCode::InvalidResult,
                "paper-note region has invalid normalized bounds",
            ));
        }

        if matches!(region.kind, PaperNoteRegionKind::Text)
            && region.text.as_deref().is_none_or(str::is_empty)
        {
            return Err(TaskError::new(
                TaskErrorCode::InvalidResult,
                "text region is missing text",
            ));
        }
        if region
            .confidence
            .is_some_and(|value| !value.is_finite() || !(0.0..=1.0).contains(&value))
        {
            return Err(TaskError::new(
                TaskErrorCode::InvalidResult,
                "paper-note region has invalid confidence",
            ));
        }
        for suggestion in &region.suggestions {
            if suggestion.original.is_empty() || suggestion.replacement.is_empty() {
                return Err(TaskError::new(
                    TaskErrorCode::InvalidResult,
                    "spelling suggestion is incomplete",
                ));
            }
            if suggestion
                .confidence
                .is_some_and(|value| !value.is_finite() || !(0.0..=1.0).contains(&value))
            {
                return Err(TaskError::new(
                    TaskErrorCode::InvalidResult,
                    "spelling suggestion has invalid confidence",
                ));
            }
        }
    }
    Ok(())
}
