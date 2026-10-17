//! The upper limits of a model job (spec 055 FR-029, research R9). An extension may stay below
//! them, never above; the duration is the one the user can change per extension.

/// Steps in one job.
pub const MAX_STEPS: usize = 4;
/// Images in one job, over all steps.
pub const MAX_IMAGES: usize = 8;
/// One image, raw (before Base64).
pub const MAX_IMAGE_BYTES: usize = 25 * 1024 * 1024;
/// The instruction of one step.
pub const MAX_INSTRUCTION_BYTES: usize = 16 * 1024;
/// The input text of one step.
pub const MAX_INPUT_TEXT_BYTES: usize = 256 * 1024;
/// What a step may ask for as an answer's length, and the default.
pub const MAX_OUTPUT_TOKENS: usize = 16_384;
pub const DEFAULT_OUTPUT_TOKENS: usize = 4_096;
/// The result of one step, as text.
pub const MAX_RESULT_BYTES: usize = 256 * 1024;
/// Running jobs of one frame.
pub const MAX_ACTIVE_JOBS_PER_FRAME: usize = 2;
/// The default duration of a job, which the user can change per extension.
pub const DEFAULT_TIMEOUT_MS: u32 = 300_000;
