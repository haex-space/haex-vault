//! Integration tests of the llama.cpp engine against a real model (spec 070 T022).
//!
//! Skipped by default (`#[ignore]`) because they need a GGUF file, named by `HOLZI_TEST_GGUF`, and its
//! memory. Any Qwen3, Qwen3.5 or Gemma 4 file does; `HOLZI_LLAMACPP_GPU_LAYERS` forces the layers on the GPU
//! (otherwise the engine's memory plan decides).
//!
//! ```text
//! HOLZI_TEST_GGUF=<file> cargo test --manifest-path src-tauri/Cargo.toml \
//!   --no-default-features --features llm-llamacpp --test llamacpp_inference -- --ignored --nocapture
//! ```
//!
//! For a CUDA build use `--features llm-llamacpp-cuda`. The two-engines test also needs
//! `llm-cpu` (the mistralrs path) and a model both engines load, such as Qwen3.

#![cfg(feature = "llm-llamacpp")]

use std::env;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use holzi_lib::adapters::types::{
    AdapterStream, ChatMessage, ChatRequest, ChatRole, Sampling, StreamChunk, ToolCall, ToolSpec,
};
use holzi_lib::adapters::ProviderAdapter;
use holzi_lib::llm::llamacpp::{LlamaCppAdapter, LoadOptions};

/// US3 scenario 4: a cancel ends the generation within one second.
const CANCEL_BOUND: Duration = Duration::from_secs(1);

fn model_path() -> PathBuf {
    PathBuf::from(env::var("HOLZI_TEST_GGUF").expect("HOLZI_TEST_GGUF is not set"))
}

async fn load() -> LlamaCppAdapter {
    let gpu_layers = env::var("HOLZI_LLAMACPP_GPU_LAYERS")
        .ok()
        .and_then(|v| v.parse().ok());
    let adapter = LlamaCppAdapter::load(
        &model_path(),
        LoadOptions {
            context_window: None,
            gpu_layers,
        },
    )
    .await
    .expect("model load");
    println!("placement: {:?}", adapter.model().placement());
    adapter
}

fn request(user: &str, tools: Vec<ToolSpec>, max_new_tokens: usize) -> ChatRequest {
    ChatRequest {
        model_id: String::new(),
        thread_id: None,
        system_prompt: None,
        messages: vec![ChatMessage {
            role: ChatRole::User,
            content: user.to_owned(),
            attachments: Vec::new(),
        }],
        reasoning_requested: false,
        max_new_tokens: Some(max_new_tokens),
        tools,
        autonomy_mode: Default::default(),
        reasoning_option: None,
        capabilities: None,
        sampling: Sampling::Deterministic,
    }
}

fn open_settings() -> ToolSpec {
    ToolSpec {
        name: "open_settings".to_owned(),
        description: "Opens a page of the settings".to_owned(),
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {"page": {"type": "string"}},
            "required": ["page"],
        }),
    }
}

/// What a stream delivered up to its end.
#[derive(Default)]
struct Drained {
    content: String,
    reasoning: String,
    calls: Vec<ToolCall>,
    finish_reason: Option<String>,
    completion_tokens: Option<usize>,
    chunks: usize,
}

async fn drain(stream: &mut AdapterStream) -> Drained {
    let mut out = Drained::default();
    while let Some(chunk) = stream.next().await {
        out.chunks += 1;
        match chunk.expect("stream error") {
            StreamChunk::Delta { content, reasoning } => {
                out.content.push_str(&content);
                out.reasoning.push_str(&reasoning.unwrap_or_default());
            }
            StreamChunk::ToolCalls(calls) => out.calls.extend(calls),
            StreamChunk::Done {
                finish_reason,
                completion_tokens,
                ..
            } => {
                out.finish_reason = finish_reason;
                out.completion_tokens = completion_tokens;
            }
            StreamChunk::AgentActivity { .. } => {}
        }
    }
    out
}

#[tokio::test]
#[ignore = "requires HOLZI_TEST_GGUF pointing at a real GGUF file"]
async fn loads_and_answers_in_pieces() {
    let adapter = load().await;
    let mut stream = adapter
        .stream_chat(request("Name three uses of a paper clip.", vec![], 64))
        .await
        .expect("stream");
    let out = drain(&mut stream).await;
    println!("{}", out.content);
    assert!(!out.content.trim().is_empty(), "no answer");
    assert!(out.chunks > 3, "the answer came in {} chunks", out.chunks);
    assert_eq!(out.finish_reason.as_deref(), Some("length"));
    assert_eq!(out.completion_tokens, Some(64));
}

#[tokio::test]
#[ignore = "requires HOLZI_TEST_GGUF pointing at a real GGUF file"]
async fn a_tool_call_arrives_as_a_call_and_not_as_text() {
    let adapter = load().await;
    let mut stream = adapter
        .stream_chat(request(
            "Please open the settings page about sync.",
            vec![open_settings()],
            400,
        ))
        .await
        .expect("stream");
    let out = drain(&mut stream).await;
    println!("content: {:?}\ncalls: {:?}", out.content, out.calls);
    assert_eq!(out.calls.len(), 1, "expected one tool call");
    assert_eq!(out.calls[0].name, "open_settings");
    assert!(
        !out.content.contains("tool_call"),
        "call markup leaked into the text: {}",
        out.content
    );
    assert_eq!(out.finish_reason.as_deref(), Some("tool_calls"));
}

#[tokio::test]
#[ignore = "requires HOLZI_TEST_GGUF pointing at a real GGUF file"]
async fn a_cancel_ends_the_generation_within_a_second() {
    let adapter = load().await;
    let mut stream = adapter
        .stream_chat(request(
            "Write a very long story about a squirrel.",
            vec![],
            4000,
        ))
        .await
        .expect("stream");
    // Wait for the first text, so the cancel lands in the token loop.
    loop {
        match stream.next().await.expect("stream ended early") {
            Ok(StreamChunk::Delta { content, .. }) if !content.is_empty() => break,
            Ok(_) => {}
            Err(error) => panic!("stream error: {error}"),
        }
    }
    let cancelled_at = Instant::now();
    stream.abort_handle().abort();
    // The task ends on its own once it has seen the cancel; the stream then closes.
    while stream.next().await.is_some() {}
    let took = cancelled_at.elapsed();
    println!("ended {took:?} after the cancel");
    assert!(took < CANCEL_BOUND, "took {took:?}");
}

#[tokio::test]
#[ignore = "requires HOLZI_TEST_GGUF pointing at a real GGUF file"]
async fn a_second_request_waits_for_the_first_and_both_finish() {
    let adapter = load().await;
    let mut first = adapter
        .stream_chat(request("Say hello.", vec![], 16))
        .await
        .expect("first");
    let mut second = adapter
        .stream_chat(request("Say goodbye.", vec![], 16))
        .await
        .expect("second");
    let (a, b) = tokio::join!(drain(&mut first), drain(&mut second));
    assert!(a.finish_reason.is_some() && b.finish_reason.is_some());
    assert!(!a.content.is_empty() && !b.content.is_empty());
}

#[tokio::test]
#[ignore = "requires HOLZI_TEST_GGUF pointing at a real GGUF file"]
async fn greedy_decoding_gives_the_same_answer_twice() {
    let adapter = load().await;
    let mut answers = Vec::new();
    for _ in 0..2 {
        let mut stream = adapter
            .stream_chat(request(
                "What is 7 times 8? Answer with the number.",
                vec![],
                24,
            ))
            .await
            .expect("stream");
        answers.push(drain(&mut stream).await.content);
    }
    assert_eq!(answers[0], answers[1]);
}

/// Both engines in one process and one GPU: the backend of llama.cpp and mistralrs' own CUDA state must
/// not trip over each other, and each must answer.
#[cfg(feature = "llm-cpu")]
#[tokio::test]
#[ignore = "requires HOLZI_TEST_GGUF (a Qwen3 file mistralrs also loads) and memory for both"]
async fn both_engines_answer_in_one_process() {
    use holzi_lib::adapters::local::LocalAdapter;
    use holzi_lib::llm::local::LocalModel;

    let tokenizer =
        env::var("HOLZI_TEST_GGUF_TOKENIZER").unwrap_or_else(|_| "Qwen/Qwen3-4B".to_owned());
    let mistral = LocalAdapter::new(
        LocalModel::load(&model_path(), Some(&tokenizer))
            .await
            .expect("mistralrs load"),
    );
    let llama = load().await;
    for (name, adapter) in [
        ("mistralrs", &mistral as &dyn ProviderAdapter),
        ("llama.cpp", &llama as &dyn ProviderAdapter),
    ] {
        let mut stream = adapter
            .stream_chat(request("Say hello in one word.", vec![], 16))
            .await
            .expect("stream");
        let out = drain(&mut stream).await;
        println!("{name}: {:?}", out.content);
        assert!(!out.content.trim().is_empty(), "{name} gave no answer");
    }
}

/// Spec 070 PR D: the engine follows from the file. Qwen3 loads through mistralrs, Qwen3.5 through
/// llama.cpp, and the adapter answers either way.
#[cfg(feature = "llm-cpu")]
#[tokio::test]
#[ignore = "requires HOLZI_TEST_GGUF pointing at a real GGUF file"]
async fn load_local_picks_the_engine_from_the_file_and_answers() {
    use holzi_lib::llm::engine::{load_local, Engine, LoadRequest};

    let tokenizer =
        env::var("HOLZI_TEST_GGUF_TOKENIZER").unwrap_or_else(|_| "Qwen/Qwen3-4B".to_owned());
    let (adapter, engine) = load_local(
        &model_path(),
        LoadRequest {
            tokenizer_repo: Some(&tokenizer),
            context_window: Some(8192),
        },
    )
    .await
    .expect("load");
    println!("engine: {}", engine.name());
    let expected = env::var("HOLZI_TEST_EXPECT_ENGINE").unwrap_or_default();
    match expected.as_str() {
        "mistralrs" => assert_eq!(engine, Engine::Mistralrs),
        "llama.cpp" => assert_eq!(engine.name(), "llama.cpp"),
        _ => {}
    }
    let mut stream = adapter
        .stream_chat(request("Say hello in one word.", vec![], 16))
        .await
        .expect("stream");
    let out = drain(&mut stream).await;
    assert!(!out.content.trim().is_empty(), "no answer");
}
