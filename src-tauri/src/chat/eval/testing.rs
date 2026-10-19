//! A model that answers by a script, for the tests of the runner and of the self-test.

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use serde_json::json;

use super::embedded_set;
use super::scoring::{Expect, ExpectedCall, Sentence};
use crate::adapters::types::{
    AdapterStream, ChatRequest, StreamChunk, ToolCall, ToolTemplateProbe,
};
use crate::adapters::{AdapterError, ProviderAdapter, ProviderModel};

type Script = Box<dyn Fn(&ChatRequest) -> Vec<StreamChunk> + Send + Sync>;

/// A model that answers by a script and remembers what it was asked.
pub(crate) struct Scripted {
    script: Script,
    pub(crate) seen: Arc<Mutex<Vec<ChatRequest>>>,
    /// What the template probe answers.
    pub(crate) probe: ToolTemplateProbe,
}

impl Scripted {
    /// Creates a scripted adapter whose responses are determined by `script`.
    pub(crate) fn new(
        script: impl Fn(&ChatRequest) -> Vec<StreamChunk> + Send + Sync + 'static,
    ) -> Self {
        Self {
            script: Box::new(script),
            seen: Arc::default(),
            probe: ToolTemplateProbe::Inconclusive,
        }
    }
}

#[async_trait]
impl ProviderAdapter for Scripted {
    async fn list_models(&self) -> Result<Vec<ProviderModel>, AdapterError> {
        Ok(Vec::new())
    }

    async fn stream_chat(&self, req: ChatRequest) -> Result<AdapterStream, AdapterError> {
        let chunks = (self.script)(&req);
        self.seen.lock().unwrap().push(req);
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        let task = tokio::spawn(async move {
            for chunk in chunks {
                let _ = tx.send(Ok(chunk));
            }
            let _ = tx.send(Ok(StreamChunk::Done {
                finish_reason: Some("end_turn".to_owned()),
                prompt_tokens: None,
                completion_tokens: None,
                ttft_ms: None,
                total_ms: 0,
            }));
        });
        Ok(AdapterStream::new(rx, task.abort_handle()))
    }

    async fn probe_tool_template(&self) -> ToolTemplateProbe {
        self.probe
    }
}

/// Builds one scripted batch of provider-neutral tool calls.
pub(crate) fn calls(batch: Vec<(&str, serde_json::Value)>) -> StreamChunk {
    StreamChunk::ToolCalls(
        batch
            .into_iter()
            .enumerate()
            .map(|(i, (name, input))| ToolCall {
                id: format!("c{i}"),
                name: name.to_owned(),
                input,
            })
            .collect(),
    )
}

/// Returns the first user message in an evaluation request.
fn user_text(req: &ChatRequest) -> &str {
    &req.messages[0].content
}

/// Looks up the versioned evaluation sentence matching the request text.
fn sentence<'a>(text: &str, set: &'a super::scoring::EvalSet) -> &'a Sentence {
    set.sentences.iter().find(|s| s.text == text).unwrap()
}

/// The arguments the oracle sends: what the sentence expects. Where a sentence leaves them open
/// (an app named with a typo, the agent's own question), a value the tool's schema accepts.
fn oracle_args(call: &ExpectedCall) -> serde_json::Value {
    if call.tool == crate::chat::tools::ask_user::ASK_USER_TOOL_NAME {
        return json!({ "question": "Which one?", "options": ["One", "Other"] });
    }
    let mut args = call.args.clone();
    let schema = super::embedded_tools()
        .into_iter()
        .find(|def| def.tool_name == call.tool)
        .map(|def| def.input_schema)
        .or_else(|| {
            super::embedded_extension_tools()
                .into_iter()
                .find(|def| def.model_name == call.tool)
                .map(|def| def.tool.input_schema)
        });
    let required = schema
        .as_ref()
        .and_then(|schema| schema.get("required"))
        .and_then(serde_json::Value::as_array)
        .cloned()
        .unwrap_or_default();
    if let Some(object) = args.as_object_mut() {
        for field in required.iter().filter_map(serde_json::Value::as_str) {
            let kind = schema
                .as_ref()
                .and_then(|schema| schema.pointer(&format!("/properties/{field}/type")))
                .and_then(serde_json::Value::as_str);
            object.entry(field).or_insert_with(|| match kind {
                Some("integer" | "number") => json!(1),
                _ => json!("x"),
            });
        }
    }
    args
}

/// The model the set was written for: calls what a sentence expects, searches first when the tool
/// is not offered yet, and only talks for smalltalk.
pub(crate) fn oracle() -> Scripted {
    let set = embedded_set();
    Scripted::new(move |req| {
        let sentence = sentence(user_text(req), &set);
        let Expect::Calls(expected) = &sentence.expect else {
            return vec![StreamChunk::Delta {
                content: "Gern.".to_owned(),
                reasoning: None,
            }];
        };
        let offered = |name: &str| req.tools.iter().any(|t| t.name == name);
        if expected.iter().all(|c| offered(&c.tool)) {
            return vec![calls(
                expected
                    .iter()
                    .map(|c| (c.tool.as_str(), oracle_args(c)))
                    .collect(),
            )];
        }
        // Not offered: search by the words of the tool's name, the way a model would.
        let query = expected[0].tool.replace('_', " ");
        vec![calls(vec![("find_actions", json!({ "query": query }))])]
    })
}
