//! A model job as the extension sends it (spec 055, `contracts/job-format.md`): parsed, decoded and
//! checked before any model is asked. Everything wrong with the request is found here, so a bad job
//! never costs a model call and never fails half way.

use base64::Engine;
use serde::Deserialize;
use serde_json::Value;

use super::limits::{
    DEFAULT_OUTPUT_TOKENS, MAX_IMAGES, MAX_IMAGE_BYTES, MAX_INPUT_TEXT_BYTES,
    MAX_INSTRUCTION_BYTES, MAX_OUTPUT_TOKENS, MAX_STEPS,
};
use super::schema;

/// Why a job was refused before it started.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JobError {
    /// The form is wrong; the text says where.
    Invalid(String),
    /// A limit is exceeded; the name is one of `steps`, `image_count`, `image_bytes`,
    /// `instruction_bytes`, `input_bytes`.
    Limit(&'static str),
}

fn invalid(message: impl Into<String>) -> JobError {
    JobError::Invalid(message.into())
}

/// What a step needs from a model.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Need {
    Text,
    Image,
}

impl Need {
    /// The spelling of the job format.
    pub fn as_str(self) -> &'static str {
        match self {
            Need::Text => "text",
            Need::Image => "image",
        }
    }

    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "text" => Some(Need::Text),
            "image" => Some(Need::Image),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Image {
    pub mime_type: String,
    /// Decoded.
    pub bytes: Vec<u8>,
}

/// A part of an earlier step's result, handed to this step as text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Source {
    pub step: String,
    /// A JSON pointer (RFC 6901); empty means the whole result.
    pub pointer: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StepInput {
    pub text: Option<String>,
    pub images: Vec<Image>,
    pub from: Option<Source>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Format {
    pub name: String,
    pub version: u32,
    pub schema: Value,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Step {
    pub id: String,
    pub needs: Vec<Need>,
    pub instruction: String,
    pub input: StepInput,
    pub format: Format,
    pub max_output_tokens: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Job {
    pub steps: Vec<Step>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawJob {
    steps: Vec<RawStep>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RawStep {
    id: String,
    needs: Vec<String>,
    instruction: String,
    input: RawInput,
    format: RawFormat,
    #[serde(default)]
    max_output_tokens: Option<usize>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RawInput {
    #[serde(default)]
    text: Option<String>,
    #[serde(default)]
    images: Vec<RawImage>,
    #[serde(default)]
    from: Option<RawFrom>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RawImage {
    mime_type: String,
    data: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawFrom {
    step: String,
    pointer: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawFormat {
    name: String,
    version: u32,
    schema: Value,
}

const MIME_TYPES: [&str; 3] = ["image/jpeg", "image/png", "image/webp"];

/// Parses and checks the `job` of `extension_model_job_start`.
pub fn parse(job: &Value) -> Result<Job, JobError> {
    let raw: RawJob =
        serde_json::from_value(job.clone()).map_err(|error| invalid(shortened(&error)))?;
    if raw.steps.is_empty() {
        return Err(invalid("a job has at least one step"));
    }
    if raw.steps.len() > MAX_STEPS {
        return Err(JobError::Limit("steps"));
    }
    let mut steps: Vec<Step> = Vec::with_capacity(raw.steps.len());
    let mut images_in_all = 0;
    for raw_step in raw.steps {
        let step = parse_step(raw_step, &steps, &mut images_in_all)?;
        steps.push(step);
    }
    Ok(Job { steps })
}

fn parse_step(raw: RawStep, earlier: &[Step], images_in_all: &mut usize) -> Result<Step, JobError> {
    let id = raw.id;
    if !valid_id(&id) {
        return Err(invalid(format!(
            "the step id {id:?} must match [a-z][a-z0-9_-]{{0,31}}"
        )));
    }
    if earlier.iter().any(|step| step.id == id) {
        return Err(invalid(format!("the step id {id:?} is used twice")));
    }
    let needs = parse_needs(&id, &raw.needs)?;
    let instruction = raw.instruction;
    if instruction.trim().is_empty() {
        return Err(invalid(format!("step {id}: the instruction is empty")));
    }
    if instruction.len() > MAX_INSTRUCTION_BYTES {
        return Err(JobError::Limit("instruction_bytes"));
    }

    let mut images = Vec::with_capacity(raw.input.images.len());
    for image in raw.input.images {
        *images_in_all += 1;
        if *images_in_all > MAX_IMAGES {
            return Err(JobError::Limit("image_count"));
        }
        images.push(decode_image(&id, image)?);
    }
    if images.is_empty() == needs.contains(&Need::Image) {
        return Err(invalid(format!(
            "step {id}: `needs` has \"image\" exactly when the step has images"
        )));
    }
    if let Some(text) = &raw.input.text {
        if text.len() > MAX_INPUT_TEXT_BYTES {
            return Err(JobError::Limit("input_bytes"));
        }
    }
    let from = raw
        .input
        .from
        .map(|from| check_from(&id, from, earlier))
        .transpose()?;
    let no_text = raw
        .input
        .text
        .as_deref()
        .is_none_or(|text| text.trim().is_empty());
    if no_text && images.is_empty() && from.is_none() {
        return Err(invalid(format!("step {id}: the input is empty")));
    }

    let format = parse_format(&id, raw.format)?;
    let max_output_tokens = raw.max_output_tokens.unwrap_or(DEFAULT_OUTPUT_TOKENS);
    if !(1..=MAX_OUTPUT_TOKENS).contains(&max_output_tokens) {
        return Err(invalid(format!(
            "step {id}: maxOutputTokens is between 1 and {MAX_OUTPUT_TOKENS}"
        )));
    }
    Ok(Step {
        id,
        needs,
        instruction,
        input: StepInput {
            text: raw.input.text,
            images,
            from,
        },
        format,
        max_output_tokens,
    })
}

/// A serde message quotes the offending value, which can be megabytes of Base64.
fn shortened(error: &serde_json::Error) -> String {
    const MAX_CHARS: usize = 300;
    let text = error.to_string();
    if text.chars().count() <= MAX_CHARS {
        return text;
    }
    let cut: String = text.chars().take(MAX_CHARS).collect();
    format!("{cut}…")
}

fn valid_id(id: &str) -> bool {
    let mut chars = id.chars();
    chars.next().is_some_and(|c| c.is_ascii_lowercase())
        && id.len() <= 32
        && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-')
}

fn parse_needs(id: &str, raw: &[String]) -> Result<Vec<Need>, JobError> {
    let mut needs = Vec::new();
    for text in raw {
        let need = Need::parse(text)
            .ok_or_else(|| invalid(format!("step {id}: unknown need {text:?}")))?;
        if needs.contains(&need) {
            return Err(invalid(format!("step {id}: `needs` names {text:?} twice")));
        }
        needs.push(need);
    }
    if !needs.contains(&Need::Text) {
        return Err(invalid(format!("step {id}: `needs` includes \"text\"")));
    }
    Ok(needs)
}

fn decode_image(step: &str, image: RawImage) -> Result<Image, JobError> {
    if !MIME_TYPES.contains(&image.mime_type.as_str()) {
        return Err(invalid(format!(
            "step {step}: the image type {:?} is not supported",
            image.mime_type
        )));
    }
    // Refuse by the length of the text before decoding a giant string.
    if image.data.len() > MAX_IMAGE_BYTES.div_ceil(3) * 4 + 4 {
        return Err(JobError::Limit("image_bytes"));
    }
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(image.data.as_bytes())
        .map_err(|_| invalid(format!("step {step}: an image is not valid Base64")))?;
    if bytes.is_empty() {
        return Err(invalid(format!("step {step}: an image is empty")));
    }
    if bytes.len() > MAX_IMAGE_BYTES {
        return Err(JobError::Limit("image_bytes"));
    }
    Ok(Image {
        mime_type: image.mime_type,
        bytes,
    })
}

fn check_from(step: &str, from: RawFrom, earlier: &[Step]) -> Result<Source, JobError> {
    let Some(source) = earlier.iter().find(|candidate| candidate.id == from.step) else {
        return Err(invalid(format!(
            "step {step}: `from` names {:?}, which is not an earlier step",
            from.step
        )));
    };
    let tokens = pointer_tokens(&from.pointer).ok_or_else(|| {
        invalid(format!(
            "step {step}: {:?} is not a JSON pointer",
            from.pointer
        ))
    })?;
    if !pointer_fits(&source.format.schema, &tokens) {
        return Err(invalid(format!(
            "step {step}: the pointer {:?} cannot occur in the result of step {}",
            from.pointer, source.id
        )));
    }
    Ok(Source {
        step: from.step,
        pointer: from.pointer,
    })
}

/// The reference tokens of a JSON pointer (RFC 6901), unescaped; `None` when it is not one.
fn pointer_tokens(pointer: &str) -> Option<Vec<String>> {
    if pointer.is_empty() {
        return Some(Vec::new());
    }
    let rest = pointer.strip_prefix('/')?;
    let mut tokens = Vec::new();
    for raw in rest.split('/') {
        let mut token = String::with_capacity(raw.len());
        let mut chars = raw.chars();
        while let Some(character) = chars.next() {
            if character != '~' {
                token.push(character);
                continue;
            }
            match chars.next() {
                Some('0') => token.push('~'),
                Some('1') => token.push('/'),
                _ => return None,
            }
        }
        tokens.push(token);
    }
    Some(tokens)
}

/// Whether a pointer can lead somewhere in a value that has this schema, judged as
/// [`schema::validate`] judges the value: a name must be a declared property or the object must be
/// open, an index needs an array (whose `items`, if given, take the rest).
fn pointer_fits(schema: &Value, tokens: &[String]) -> bool {
    let Some((token, rest)) = tokens.split_first() else {
        return true;
    };
    if let Some(child) = schema
        .get("properties")
        .and_then(|properties| properties.get(token))
    {
        return pointer_fits(child, rest);
    }
    if schema::allows_type(schema, "object") && schema::is_open_object(schema) {
        return true;
    }
    let index = !token.is_empty()
        && token.bytes().all(|byte| byte.is_ascii_digit())
        && (token == "0" || !token.starts_with('0'));
    if !index || !schema::allows_type(schema, "array") {
        return false;
    }
    schema
        .get("items")
        .is_none_or(|items| pointer_fits(items, rest))
}

fn parse_format(step: &str, raw: RawFormat) -> Result<Format, JobError> {
    if raw.name.trim().is_empty() || raw.name.chars().count() > 64 {
        return Err(invalid(format!(
            "step {step}: the format name is 1 to 64 characters"
        )));
    }
    if raw.version == 0 {
        return Err(invalid(format!(
            "step {step}: the format version is at least 1"
        )));
    }
    schema::check_subset(&raw.schema)
        .map_err(|why| invalid(format!("step {step}: the format schema: {why}")))?;
    Ok(Format {
        name: raw.name,
        version: raw.version,
        schema: raw.schema,
    })
}
