//! Renders a model's own Jinja chat template (spec 070, ADR 0023).
//!
//! The binding offers only llama.cpp's built-in `llama_chat_apply_template`, which knows a fixed list of
//! template families and, for Qwen3.5, quietly fell back to plain ChatML without tools. So the template text
//! stored in the GGUF is rendered here with `minijinja`, with the context variables mistralrs passes for the
//! same inputs. A template that fails to render is an error, never a fallback.

use minijinja::value::Kwargs;
use minijinja::{context, Environment, Error, ErrorKind};
use serde_json::Value;

/// What a render needs: messages and tools already in the shape the template reads (see
/// `formats::ChatFormat::shape_messages` and `formats::template_tools`).
pub struct RenderInput<'a> {
    pub messages: Vec<Value>,
    pub tools: Vec<Value>,
    pub enable_thinking: bool,
    pub bos_token: &'a str,
    pub eos_token: &'a str,
}

#[derive(Debug, thiserror::Error)]
pub enum TemplateError {
    #[error("the chat template does not parse: {0}")]
    Parse(String),
    #[error("the chat template does not render: {0}")]
    Render(String),
}

/// Rewrites a template needs before `minijinja` takes it. Only what a supported template uses is ported
/// from mistralrs: Python's reversing slice `[::-1]`, which `minijinja` has no syntax for (Qwen3 and Qwen3.5
/// use it on `messages`). mistralrs also rewrites descending `range(…, -1, -1)` calls and some model-specific
/// markup (GLM4, SmolLM3); no supported template contains them, so they are not carried over.
pub(super) fn prepare(template: &str) -> String {
    template.replace("[::-1]", "|reverse")
}

/// `tojson` as the Hugging Face templates use it: compact by default, `indent=N` for pretty output, other
/// keyword arguments (`ensure_ascii`, `sort_keys`) accepted and ignored.
fn tojson(value: minijinja::Value, kwargs: Kwargs) -> Result<String, Error> {
    let indent: Option<usize> = kwargs.get("indent")?;
    let _ = kwargs.get::<Option<bool>>("ensure_ascii")?;
    let _ = kwargs.get::<Option<bool>>("sort_keys")?;
    kwargs.assert_all_used()?;
    let json_error = |e: serde_json::Error| {
        Error::new(ErrorKind::InvalidOperation, "cannot serialize to JSON").with_source(e)
    };
    match indent {
        None => serde_json::to_string(&value).map_err(json_error),
        Some(width) => {
            let pad = " ".repeat(width);
            let mut buf = Vec::new();
            let formatter = serde_json::ser::PrettyFormatter::with_indent(pad.as_bytes());
            let mut serializer = serde_json::Serializer::with_formatter(&mut buf, formatter);
            serde::Serialize::serialize(&value, &mut serializer).map_err(json_error)?;
            String::from_utf8(buf).map_err(|e| {
                Error::new(ErrorKind::InvalidOperation, "JSON is not utf-8").with_source(e)
            })
        }
    }
}

fn raise_exception(message: String) -> Result<String, Error> {
    Err(Error::new(ErrorKind::InvalidOperation, message))
}

/// Renders `template` for `input`.
pub fn render(template: &str, input: &RenderInput<'_>) -> Result<String, TemplateError> {
    let mut env = Environment::new();
    // Python string methods (`.strip()`, `.startswith()`, `.split()`) the templates call.
    env.set_unknown_method_callback(minijinja_contrib::pycompat::unknown_method_callback);
    // Hugging Face renders with these two options on; templates rely on the whitespace they give.
    env.set_lstrip_blocks(true);
    env.set_trim_blocks(true);
    env.add_function("raise_exception", raise_exception);
    env.add_filter("tojson", tojson);
    env.add_template_owned("chat", prepare(template))
        .map_err(|e| TemplateError::Parse(format!("{e:#}")))?;
    let template = env
        .get_template("chat")
        .map_err(|e| TemplateError::Parse(format!("{e:#}")))?;
    let render_error = |e: Error| TemplateError::Render(format!("{e:#}"));
    // `tools` is left undefined without tools: templates test `tools` itself, and an empty list would
    // be a different (if usually harmless) answer than none.
    if input.tools.is_empty() {
        template
            .render(context! {
                messages => &input.messages,
                add_generation_prompt => true,
                bos_token => input.bos_token,
                eos_token => input.eos_token,
                enable_thinking => input.enable_thinking,
            })
            .map_err(render_error)
    } else {
        template
            .render(context! {
                messages => &input.messages,
                add_generation_prompt => true,
                bos_token => input.bos_token,
                eos_token => input.eos_token,
                tools => &input.tools,
                enable_thinking => input.enable_thinking,
            })
            .map_err(render_error)
    }
}

#[cfg(test)]
#[path = "template_tests.rs"]
mod tests;
