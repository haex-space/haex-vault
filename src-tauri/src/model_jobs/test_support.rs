//! Scripted models for the tests of model jobs: a [`ModelSource`] whose adapters answer what the
//! test says, per model id, and record every request.

use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex};

use async_trait::async_trait;
use serde_json::json;
use uuid::Uuid;

use super::model_source::{JobModel, ModelSource};
use crate::adapters::types::{AdapterStream, ChatRequest, StreamChunk, StreamError, ToolCall};
use crate::adapters::{AdapterError, ProviderAdapter, ProviderModel};
use crate::extensions::permissions::store::{self, NewPermission};
use crate::extensions::permissions::VAULT_WIDE;
use crate::model_capabilities::ModelCapabilities;
use crate::storage::models::upsert_model;
use crate::storage::models::{IntegrityStatus, ModelRow, SourceKind};
use crate::storage::providers::insert_provider;
use crate::storage::providers::{Provider, ProviderCapability, ProviderKind};
use crate::vault_gate::VaultDb;

/// What one model does when it is asked.
#[derive(Debug, Clone)]
pub(crate) enum Script {
    /// Answers this text, in two pieces, then ends.
    Answer(String),
    /// Calls a tool although none was offered.
    ToolCall,
    /// The stream fails with this message.
    StreamFails(String),
    /// The request fails before the stream with this status and body.
    Status(u16, String),
    /// The adapter cannot be built; the message is the source's.
    Unbuildable(String),
    /// Never answers.
    Hang,
}

/// Counts streams that were dropped while still running.
struct Dropped {
    count: Arc<AtomicUsize>,
    changed: Arc<Condvar>,
    lock: Arc<Mutex<Vec<(String, ChatRequest)>>>,
}

impl Drop for Dropped {
    fn drop(&mut self) {
        let _held = self.lock.lock().unwrap();
        self.count.fetch_add(1, Ordering::SeqCst);
        self.changed.notify_all();
    }
}

#[derive(Default)]
pub(crate) struct FakeSource {
    scripts: Mutex<HashMap<String, Script>>,
    /// The model id and the request of every stream that was started.
    pub seen: Arc<Mutex<Vec<(String, ChatRequest)>>>,
    /// Streams of [`Script::Hang`] that ended because they were dropped.
    pub dropped: Arc<AtomicUsize>,
    asked_changed: Arc<Condvar>,
    dropped_changed: Arc<Condvar>,
}

impl FakeSource {
    pub fn with(scripts: &[(&str, Script)]) -> Arc<Self> {
        let source = Self::default();
        *source.scripts.lock().unwrap() = scripts
            .iter()
            .map(|(id, script)| ((*id).to_owned(), script.clone()))
            .collect();
        Arc::new(source)
    }

    /// The model ids asked, in order.
    pub fn asked(&self) -> Vec<String> {
        self.seen
            .lock()
            .unwrap()
            .iter()
            .map(|(id, _)| id.clone())
            .collect()
    }

    pub fn wait_until_asked(&self, expected: usize) {
        let mut seen = self.seen.lock().unwrap();
        while seen.len() < expected {
            seen = self.asked_changed.wait(seen).unwrap();
        }
    }

    pub fn wait_until_dropped(&self, expected: usize) {
        let guard = self.seen.lock().unwrap();
        let _guard = self
            .dropped_changed
            .wait_while(guard, |_| self.dropped.load(Ordering::SeqCst) < expected)
            .unwrap();
    }
}

impl ModelSource for FakeSource {
    fn adapter(&self, model: &JobModel) -> Result<Arc<dyn ProviderAdapter>, String> {
        let script = self
            .scripts
            .lock()
            .unwrap()
            .get(&model.id)
            .cloned()
            .unwrap_or_else(|| Script::Unbuildable("no script".into()));
        if let Script::Unbuildable(message) = script {
            return Err(message);
        }
        Ok(Arc::new(ScriptedAdapter {
            model_id: model.id.clone(),
            script,
            seen: Arc::clone(&self.seen),
            dropped: Arc::clone(&self.dropped),
            asked_changed: Arc::clone(&self.asked_changed),
            dropped_changed: Arc::clone(&self.dropped_changed),
        }))
    }
}

struct ScriptedAdapter {
    model_id: String,
    script: Script,
    seen: Arc<Mutex<Vec<(String, ChatRequest)>>>,
    dropped: Arc<AtomicUsize>,
    asked_changed: Arc<Condvar>,
    dropped_changed: Arc<Condvar>,
}

fn done() -> StreamChunk {
    StreamChunk::Done {
        finish_reason: Some("stop".into()),
        prompt_tokens: None,
        completion_tokens: None,
        ttft_ms: None,
        total_ms: 0,
    }
}

#[async_trait]
impl ProviderAdapter for ScriptedAdapter {
    async fn list_models(&self) -> Result<Vec<ProviderModel>, AdapterError> {
        Ok(Vec::new())
    }

    async fn stream_chat(&self, req: ChatRequest) -> Result<AdapterStream, AdapterError> {
        if let Script::Status(status, body) = &self.script {
            return Err(AdapterError::Status {
                status: *status,
                body: body.clone(),
            });
        }
        self.seen.lock().unwrap().push((self.model_id.clone(), req));
        self.asked_changed.notify_all();
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        let script = self.script.clone();
        let dropped = Arc::clone(&self.dropped);
        let dropped_changed = Arc::clone(&self.dropped_changed);
        let seen = Arc::clone(&self.seen);
        let task = tokio::spawn(async move {
            match script {
                Script::Answer(text) => {
                    let middle = text.len() / 2;
                    let middle = (middle..=text.len())
                        .find(|i| text.is_char_boundary(*i))
                        .unwrap_or(text.len());
                    for piece in [&text[..middle], &text[middle..]] {
                        let _ = tx.send(Ok(StreamChunk::Delta {
                            content: piece.to_owned(),
                            reasoning: None,
                        }));
                    }
                    let _ = tx.send(Ok(done()));
                }
                Script::ToolCall => {
                    let _ = tx.send(Ok(StreamChunk::ToolCalls(vec![ToolCall {
                        id: "call-1".into(),
                        name: "run_command".into(),
                        input: json!({ "command": "rm -rf ~" }),
                    }])));
                    let _ = tx.send(Ok(done()));
                }
                Script::StreamFails(message) => {
                    let _ = tx.send(Err(StreamError::Model(message)));
                }
                Script::Hang => {
                    let _guard = Dropped {
                        count: dropped,
                        changed: dropped_changed,
                        lock: seen,
                    };
                    let _tx = tx;
                    std::future::pending::<()>().await;
                }
                Script::Status(..) | Script::Unbuildable(_) => {}
            }
        });
        Ok(AdapterStream::new(rx, task.abort_handle()))
    }
}

/// A provider at `base_url`; `kind` decides where its models run.
pub(crate) fn provider(kind: ProviderKind, base_url: Option<&str>, key: Option<&str>) -> Provider {
    Provider {
        id: Uuid::new_v4(),
        kind,
        adapter: (kind == ProviderKind::ApiKey).then(|| "openai_compatible".to_owned()),
        name: "Test provider".into(),
        base_url: base_url.map(str::to_owned),
        credentials: key.map(|k| k.as_bytes().to_vec()),
        created_at: 0,
        capability: ProviderCapability::Chat,
    }
}

/// A model row of `provider` named `remote`.
pub(crate) fn model_row(
    provider: &Provider,
    remote: &str,
    capabilities: Option<ModelCapabilities>,
) -> ModelRow {
    ModelRow {
        id: format!("{}:{remote}", provider.id),
        provider_id: provider.id,
        name: remote.to_owned(),
        context_window: None,
        fetched_at: None,
        tokenizer_repo: None,
        hf_repo: None,
        hf_filename: None,
        hf_revision: None,
        hf_revision_ref: None,
        file_sha256: None,
        integrity_status: IntegrityStatus::Unknown,
        source_kind: SourceKind::Provider,
        capabilities,
    }
}

/// Stores `provider` and its `models`.
pub(crate) fn store_models(db: &VaultDb, provider: &Provider, models: &[ModelRow]) {
    let (provider, models) = (provider.clone(), models.to_vec());
    db.write_blocking(move |tx| {
        insert_provider(tx, &provider)?;
        for model in &models {
            upsert_model(tx, model)?;
        }
        Ok(())
    })
    .unwrap();
}

/// Remembers a vault-wide permission of `extension_id`, granted or denied at `at_ms`.
pub(crate) fn remember(
    db: &VaultDb,
    extension_id: Uuid,
    (kind, action, target): (&str, &str, &str),
    status: &str,
    at_ms: i64,
) {
    let (kind, action, target, status) = (
        kind.to_owned(),
        action.to_owned(),
        target.to_owned(),
        status.to_owned(),
    );
    db.write_blocking(move |tx| {
        let permission = NewPermission {
            kind: &kind,
            action: &action,
            target: &target,
            status: &status,
            declared: false,
            vault_device_uuid: VAULT_WIDE,
        };
        store::put(tx, extension_id, &permission, at_ms)
            .map(drop)
            .map_err(Into::into)
    })
    .unwrap();
}
