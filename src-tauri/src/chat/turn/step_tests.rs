use super::*;

use std::sync::Arc;

use crate::chat::commands::abort_turn;
use crate::storage::providers::ProviderKind;

struct StartAdapter {
    attempts: std::sync::atomic::AtomicUsize,
    entered: tokio::sync::Notify,
    error: Option<u16>,
    /// Refuse the provider's certificate instead (spec 043 FR-024).
    untrusted: bool,
}

#[async_trait::async_trait]
impl crate::adapters::ProviderAdapter for StartAdapter {
    async fn list_models(
        &self,
    ) -> std::result::Result<Vec<crate::adapters::ProviderModel>, crate::adapters::AdapterError>
    {
        Ok(Vec::new())
    }

    async fn stream_chat(
        &self,
        _request: ChatRequest,
    ) -> std::result::Result<crate::adapters::types::AdapterStream, crate::adapters::AdapterError>
    {
        self.attempts
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        self.entered.notify_one();
        if self.untrusted {
            return Err(crate::adapters::AdapterError::UntrustedCertificate {
                reason: "POST https://127.0.0.1/v1/messages: invalid peer certificate".into(),
            });
        }
        match self.error {
            Some(status) => Err(crate::adapters::AdapterError::Status {
                status,
                body: "test failure".into(),
            }),
            None => std::future::pending().await,
        }
    }
}

#[tokio::test(start_paused = true)]
async fn initial_http_failures_use_the_same_bounded_retry_budget() {
    for status in [429, 503, 401] {
        let adapter = Arc::new(StartAdapter {
            attempts: Default::default(),
            entered: Default::default(),
            error: Some(status),
            untrusted: false,
        });
        let session = ActiveSession {
            model_id: "test".into(),
            provider_id: None,
            provider_kind: ProviderKind::Local,
            adapter: adapter.clone(),
            tokenizer_repo: String::new(),
            context_window: None,
            engine: None,
        };
        let request = ChatRequest {
            sampling: Default::default(),
            model_id: "test".into(),
            thread_id: None,
            system_prompt: None,
            messages: Vec::new(),
            reasoning_requested: false,
            max_new_tokens: None,
            tools: Vec::new(),
            autonomy_mode: Default::default(),
            reasoning_option: Default::default(),
            capabilities: Default::default(),
        };
        let mut attempts = 0;
        let mut events = Vec::new();
        let result = start_step_stream(
            &session,
            &ChatState::new(),
            &request,
            &CancellationToken::new(),
            &mut attempts,
            Uuid::new_v4(),
            Uuid::new_v4(),
            &mut |name, payload| events.push((name, payload)),
        )
        .await;
        assert!(matches!(result, Err(StreamStartError::Failed(_))));
        let retries = if status == 401 { 0 } else { MAX_RETRY_ATTEMPTS };
        assert_eq!(attempts, retries);
        assert_eq!(
            adapter.attempts.load(std::sync::atomic::Ordering::SeqCst),
            retries + 1
        );
        assert_eq!(events.len(), retries);
    }
}

#[tokio::test]
async fn initial_request_can_be_cancelled_before_response_headers() {
    let adapter = Arc::new(StartAdapter {
        attempts: Default::default(),
        entered: Default::default(),
        error: None,
        untrusted: false,
    });
    let session = ActiveSession {
        model_id: "test".into(),
        provider_id: None,
        provider_kind: ProviderKind::Local,
        adapter: adapter.clone(),
        tokenizer_repo: String::new(),
        context_window: None,
        engine: None,
    };
    let chat = Arc::new(ChatState::new());
    let cancel = CancellationToken::new();
    *chat.turn_cancellation.lock().unwrap() = Some(cancel.clone());
    let task_chat = chat.clone();
    let task = tokio::spawn(async move {
        let request = ChatRequest {
            sampling: Default::default(),
            model_id: "test".into(),
            thread_id: None,
            system_prompt: None,
            messages: Vec::new(),
            reasoning_requested: false,
            max_new_tokens: None,
            tools: Vec::new(),
            autonomy_mode: Default::default(),
            reasoning_option: Default::default(),
            capabilities: Default::default(),
        };
        start_step_stream(
            &session,
            &task_chat,
            &request,
            &cancel,
            &mut 0,
            Uuid::new_v4(),
            Uuid::new_v4(),
            &mut |_, _| {},
        )
        .await
    });
    adapter.entered.notified().await;
    abort_turn(&chat).unwrap();
    assert!(matches!(
        task.await.unwrap(),
        Err(StreamStartError::Cancelled)
    ));
}

#[tokio::test(start_paused = true)]
async fn an_untrusted_certificate_is_reported_as_such_and_never_retried() {
    let adapter = Arc::new(StartAdapter {
        attempts: Default::default(),
        entered: Default::default(),
        error: None,
        untrusted: true,
    });
    let session = ActiveSession {
        model_id: "test".into(),
        provider_id: None,
        provider_kind: ProviderKind::ApiKey,
        adapter: adapter.clone(),
        tokenizer_repo: String::new(),
        context_window: None,
        engine: None,
    };
    let request = ChatRequest {
        sampling: Default::default(),
        model_id: "test".into(),
        thread_id: None,
        system_prompt: None,
        messages: Vec::new(),
        reasoning_requested: false,
        max_new_tokens: None,
        tools: Vec::new(),
        autonomy_mode: Default::default(),
        reasoning_option: Default::default(),
        capabilities: Default::default(),
    };
    let mut attempts = 0;
    let mut events = Vec::new();
    let result = start_step_stream(
        &session,
        &ChatState::new(),
        &request,
        &CancellationToken::new(),
        &mut attempts,
        Uuid::new_v4(),
        Uuid::new_v4(),
        &mut |name, payload| events.push((name, payload)),
    )
    .await;

    assert!(matches!(
        result,
        Err(StreamStartError::UntrustedCertificate(_))
    ));
    assert_eq!(
        adapter.attempts.load(std::sync::atomic::Ordering::SeqCst),
        1
    );
    assert!(events.is_empty(), "no retry was announced");
}
