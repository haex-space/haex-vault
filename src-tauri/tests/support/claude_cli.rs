//! Claude through the local `claude -p` CLI as a reference model (spec 078 US5, research R8), so a
//! measurement runs on the user's subscription instead of a metered key. Each step is one `claude -p`
//! in an empty directory with Claude Code's own tools (`--tools ""`), MCP servers
//! (`--strict-mcp-config`), hooks (`disableAllHooks`) and the user's CLAUDE.md
//! (`--setting-sources local`) switched off. `--bare` is no option: it never reads the subscription
//! login. The model sees holzi's instruction, the offered tools and the conversation, and answers with
//! one JSON object naming its tool calls; the report says it is not deterministic.

use std::path::PathBuf;
use std::process::Stdio;
use std::time::Instant;

use async_trait::async_trait;
use holzi_lib::adapters::types::ToolCall;
use holzi_lib::adapters::{
    AdapterError, AdapterStream, ChatRequest, ChatRole, ProviderAdapter, ProviderModel,
    StreamChunk, StreamError,
};
use serde_json::{json, Value};
use tokio::process::Command;
use tokio::sync::mpsc;

const PROTOCOL: &str = "\n\n## How to answer\n\
The app passes your tool calls on and runs them. Answer with exactly one JSON object and nothing \
else: {\"text\": \"what you say to the user (may be empty)\", \"calls\": [{\"name\": \"tool name\", \
\"arguments\": {}}]}. Use \"calls\": [] when you call no tool. Call only tools from the list below, \
with arguments that match their schema.\n\n## Tools\n";

#[cfg(test)]
#[path = "claude_cli_tests.rs"]
mod claude_cli_tests;

pub struct ClaudeCli {
    pub workdir: PathBuf,
}

fn transcript(req: &ChatRequest) -> String {
    let mut out = String::new();
    for m in &req.messages {
        match &m.role {
            ChatRole::User => out.push_str(&format!("User: {}\n\n", m.content)),
            ChatRole::Assistant => out.push_str(&format!("You: {}\n\n", m.content)),
            ChatRole::ToolCall { name, input, .. } => {
                out.push_str(&format!("You called: {name} {input}\n\n"))
            }
            ChatRole::ToolResult {
                content, is_error, ..
            } => out.push_str(&format!(
                "Tool {}: {content}\n\n",
                if *is_error { "error" } else { "result" }
            )),
        }
    }
    out.push_str("Your answer (one JSON object):");
    out
}

/// The first complete JSON object in `result`, tolerating a code fence or text around it: Claude
/// once wrote a second, corrected object after the first. A call without a name stays, with an
/// empty one, so the runner scores it as an unknown tool instead of losing it.
fn parse_answer(result: &str) -> (String, Vec<ToolCall>) {
    let object = result.find('{').and_then(|start| {
        serde_json::Deserializer::from_str(&result[start..])
            .into_iter::<Value>()
            .next()
            .and_then(Result::ok)
    });
    let Some(object) = object else {
        return (result.to_owned(), Vec::new());
    };
    let text = object
        .get("text")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned();
    let calls = object
        .get("calls")
        .and_then(Value::as_array)
        .map(|calls| {
            calls
                .iter()
                .enumerate()
                .map(|(i, c)| ToolCall {
                    id: format!("call_{i}"),
                    name: c
                        .get("name")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_owned(),
                    input: c.get("arguments").cloned().unwrap_or_else(|| json!({})),
                })
                .collect()
        })
        .unwrap_or_default();
    (text, calls)
}

#[async_trait]
impl ProviderAdapter for ClaudeCli {
    async fn list_models(&self) -> Result<Vec<ProviderModel>, AdapterError> {
        Ok(Vec::new())
    }

    async fn stream_chat(&self, req: ChatRequest) -> Result<AdapterStream, AdapterError> {
        let tools: Vec<Value> = req
            .tools
            .iter()
            .map(|t| json!({"name": t.name, "description": t.description, "input_schema": t.input_schema}))
            .collect();
        let system = format!(
            "{}{PROTOCOL}{}",
            req.system_prompt.clone().unwrap_or_default(),
            serde_json::to_string_pretty(&tools).unwrap_or_default()
        );
        let prompt = transcript(&req);
        let model = req.model_id.clone();
        let workdir = self.workdir.clone();
        let (tx, rx) = mpsc::unbounded_channel();
        // Dropping the stream aborts this task; `kill_on_drop` then ends the `claude` process too.
        let task = tokio::spawn(async move {
            let start = Instant::now();
            let system_file = workdir.join(format!("system-{}.txt", uuid::Uuid::new_v4()));
            let out = match tokio::fs::write(&system_file, &system).await {
                Ok(()) => {
                    Command::new("claude")
                        .args([
                            "-p",
                            "--model",
                            &model,
                            "--tools",
                            "",
                            "--strict-mcp-config",
                        ])
                        .args(["--no-session-persistence", "--setting-sources", "local"])
                        .args([
                            "--settings",
                            "{\"disableAllHooks\": true}",
                            "--output-format",
                            "json",
                        ])
                        .arg("--system-prompt-file")
                        .arg(&system_file)
                        .arg(&prompt)
                        .current_dir(&workdir)
                        .stdin(Stdio::null())
                        .kill_on_drop(true)
                        .output()
                        .await
                }
                Err(e) => Err(e),
            };
            let _ = tokio::fs::remove_file(&system_file).await;
            let result = match out {
                Ok(o) if o.status.success() => serde_json::from_slice::<Value>(&o.stdout)
                    .ok()
                    .and_then(|v| v.get("result").and_then(Value::as_str).map(str::to_owned))
                    .ok_or_else(|| String::from_utf8_lossy(&o.stdout).into_owned()),
                Ok(o) => Err(format!(
                    "claude exited {}: {}{}",
                    o.status,
                    String::from_utf8_lossy(&o.stdout),
                    String::from_utf8_lossy(&o.stderr)
                )),
                Err(e) => Err(e.to_string()),
            };
            match result {
                Ok(result) => {
                    let (text, calls) = parse_answer(&result);
                    if !text.is_empty() {
                        let _ = tx.send(Ok(StreamChunk::Delta {
                            content: text,
                            reasoning: None,
                        }));
                    }
                    if !calls.is_empty() {
                        let _ = tx.send(Ok(StreamChunk::ToolCalls(calls)));
                    }
                    let _ = tx.send(Ok(StreamChunk::Done {
                        finish_reason: None,
                        prompt_tokens: None,
                        completion_tokens: None,
                        ttft_ms: None,
                        total_ms: start.elapsed().as_millis() as u64,
                    }));
                }
                Err(reason) => {
                    let _ = tx.send(Err(StreamError::Model(reason)));
                }
            }
        });
        Ok(AdapterStream::new(rx, task.abort_handle()))
    }
}
