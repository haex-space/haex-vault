//! A locally installed model as the model of a job (spec 055 FR-033, R15).
//!
//! The chat keeps one local model in memory and a second does not fit next to it on a small
//! graphics card, so a job does not get a place of its own: it borrows the chat's. The slot lends
//! the adapter of the model the job asked for ([`LocalSlot::lend`]), loading it when the chat holds
//! another one, and the [`Loan`] gives the place back when the job is done with it. Lending waits
//! for a running chat turn instead of cutting it short; the wait counts towards the job's time,
//! because the runner races the whole request against the deadline.

use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::mpsc;

use crate::adapters::{AdapterError, AdapterStream, ChatRequest, ProviderAdapter, ProviderModel};

/// Where the local models of jobs come from; a test hands in a scripted one.
#[async_trait]
pub(crate) trait LocalSlot: Send + Sync {
    /// The adapter of the installed model `model_id`, ready to answer. An error is a message for
    /// the provenance; the caller sanitises it.
    async fn lend(&self, model_id: &str) -> Result<Loan, String>;
}

/// An adapter on loan from the slot. Dropping it gives the place back.
pub(crate) struct Loan {
    adapter: Option<Arc<dyn ProviderAdapter>>,
    release: Option<Box<dyn FnOnce() + Send>>,
}

// Only the chat's place (`chat_slot`, with `llm-cpu`) makes loans outside of tests.
#[cfg_attr(not(feature = "llm-cpu"), allow(dead_code))]
impl Loan {
    /// A loan that runs `release` when it ends, for example to give the chat its model back.
    pub(crate) fn until(
        adapter: Arc<dyn ProviderAdapter>,
        release: impl FnOnce() + Send + 'static,
    ) -> Self {
        Self {
            adapter: Some(adapter),
            release: Some(Box::new(release)),
        }
    }

    /// A loan whose adapter will be installed after a model load. Dropping it before then still
    /// runs `release`, which makes a cancelled load restore the place it displaced.
    pub(crate) fn pending(release: impl FnOnce() + Send + 'static) -> Self {
        Self {
            adapter: None,
            release: Some(Box::new(release)),
        }
    }

    /// Completes a pending loan once its adapter has been loaded.
    pub(crate) fn install(&mut self, adapter: Arc<dyn ProviderAdapter>) {
        assert!(
            self.adapter.replace(adapter).is_none(),
            "loan adapter installed twice"
        );
    }
}

impl Drop for Loan {
    fn drop(&mut self) {
        if let Some(release) = self.release.take() {
            release();
        }
    }
}

/// The adapter of a local model for a job: the model is borrowed when the request starts and given
/// back when its stream ends or is dropped.
pub(crate) struct LocalJobAdapter {
    slot: Arc<dyn LocalSlot>,
    model_id: String,
}

impl LocalJobAdapter {
    pub(crate) fn new(slot: Arc<dyn LocalSlot>, model_id: String) -> Self {
        Self { slot, model_id }
    }
}

#[async_trait]
impl ProviderAdapter for LocalJobAdapter {
    async fn list_models(&self) -> Result<Vec<ProviderModel>, AdapterError> {
        Ok(Vec::new())
    }

    async fn stream_chat(&self, req: ChatRequest) -> Result<AdapterStream, AdapterError> {
        let loan = self
            .slot
            .lend(&self.model_id)
            .await
            .map_err(|reason| AdapterError::Unavailable { reason })?;
        let adapter = Arc::clone(
            loan.adapter
                .as_ref()
                .expect("a local slot must return a completed loan"),
        );
        let mut inner = adapter.stream_chat(req).await?;
        // The loan lives as long as the pump: the task ends with the stream, or is aborted when
        // the job drops it, and either way the loan goes with it.
        let (tx, rx) = mpsc::unbounded_channel();
        let pump = tokio::spawn(async move {
            let _loan = loan;
            while let Some(chunk) = inner.next().await {
                if tx.send(chunk).is_err() {
                    break;
                }
            }
        });
        Ok(AdapterStream::new(rx, pump.abort_handle()))
    }
}
