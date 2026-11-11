//! Integration tests of the llama.cpp engine against a real model (spec 070 T022).
//!
//! Skipped by default (`#[ignore]`) because they need a GGUF file, named by `HOLZI_TEST_GGUF`, and its
//! memory. Any Qwen3, Qwen3.5 or Gemma 4 file does; `HOLZI_LLAMACPP_GPU_LAYERS` forces the layers on the GPU
//! (otherwise the engine's memory plan decides), and `HOLZI_LLAMACPP_THREADS` the CPU threads.
//!
//! ```text
//! HOLZI_TEST_GGUF=<file> cargo test --manifest-path src-tauri/Cargo.toml \
//!   --no-default-features --features llm-llamacpp --test llamacpp_inference measures_the_speed -- --ignored --nocapture
//! ```
//!
//! For CUDA use `--features llm-llamacpp-cuda`.

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
    let number = |name| env::var(name).ok().and_then(|v| v.parse().ok());
    let adapter = LlamaCppAdapter::load(
        &model_path(),
        LoadOptions {
            context_window: None,
            gpu_layers: number("HOLZI_LLAMACPP_GPU_LAYERS"),
            threads: number("HOLZI_LLAMACPP_THREADS"),
            drafter: None,
            ..LoadOptions::default()
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
        autonomy_deny: Default::default(),
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
    // A small model may finish its list before the limit.
    match out.finish_reason.as_deref() {
        Some("length") => assert_eq!(out.completion_tokens, Some(64)),
        Some("stop") => assert!(out.completion_tokens.is_some_and(|n| n > 0 && n < 64)),
        other => panic!("finish reason {other:?}"),
    }
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

/// The engine used by the speed measurement.
async fn speed_adapter() -> LlamaCppAdapter {
    load().await
}

/// The process' peak resident memory in MiB, where the system tells it (Linux).
fn peak_memory_mib() -> Option<u64> {
    let status = std::fs::read_to_string("/proc/self/status").ok()?;
    let line = status.lines().find(|l| l.starts_with("VmHWM:"))?;
    let kib: u64 = line.split_whitespace().nth(1)?.parse().ok()?;
    Some(kib / 1024)
}

/// Spec 070 R3 and T042: load time, prompt and generation speed. It prints and does not judge; the figures go into `research.md`.
#[tokio::test]
#[ignore = "requires HOLZI_TEST_GGUF pointing at a real GGUF file"]
async fn measures_the_speed() {
    let loading = Instant::now();
    let adapter = speed_adapter().await;
    println!("load: {:.2} s", loading.elapsed().as_secs_f64());
    let prompt = "Summarise the following notes in two sentences. ".repeat(40);
    for round in 0..2 {
        let started = Instant::now();
        let mut stream = adapter
            .stream_chat(request(&prompt, vec![], 128))
            .await
            .expect("stream");
        let mut first = None;
        let mut out = Drained::default();
        let mut delta_chunks = 0usize;
        let mut saw_done = false;
        while let Some(chunk) = stream.next().await {
            match chunk.expect("stream error") {
                StreamChunk::Delta { content, .. } => {
                    delta_chunks += 1;
                    if !content.is_empty() {
                        first.get_or_insert_with(|| started.elapsed());
                    }
                }
                StreamChunk::Done {
                    completion_tokens, ..
                } => {
                    assert!(!saw_done, "stream emitted more than one Done frame");
                    saw_done = true;
                    out.completion_tokens = completion_tokens;
                }
                _ => {}
            }
        }
        assert!(saw_done, "stream ended without a Done frame");
        let total = started.elapsed();
        let first = first.expect("no text");
        // Do not infer token counts from text chunks: one chunk can hold several tokens after UTF-8 buffering.
        // Report chunk throughput separately when the count is unavailable.
        // The first round warms the caches up; only the second counts.
        if round == 1 {
            let after_first = (total - first).as_secs_f64();
            match out.completion_tokens.filter(|&tokens| tokens > 0) {
                Some(tokens) => println!(
                    "speed: first text after {:.2} s, {:.1} tokens/s after it",
                    first.as_secs_f64(),
                    (tokens.saturating_sub(1)) as f64 / after_first
                ),
                None => println!(
                    "speed: first text after {:.2} s, completion token count unavailable; {:.1} delta chunks/s after it",
                    first.as_secs_f64(),
                    delta_chunks.saturating_sub(1) as f64 / after_first
                ),
            }
        }
    }
    if let Some(mib) = peak_memory_mib() {
        println!("peak memory: {mib} MiB");
    }
}

/// The selected LLM engine loads the GGUF and answers.
#[cfg(feature = "llm-llamacpp")]
#[tokio::test]
#[ignore = "requires HOLZI_TEST_GGUF pointing at a real GGUF file"]
async fn load_local_picks_the_engine_from_the_file_and_answers() {
    use holzi_lib::llm::engine::{load_local, Engine, LoadRequest};

    let (adapter, engine) = load_local(
        &model_path(),
        LoadRequest {
            context_window: Some(8192),
            drafter: None,
            ..LoadRequest::default()
        },
    )
    .await
    .expect("load");
    println!("engine: {}", engine.name());
    assert_eq!(engine, Engine::LlamaCpp);
    let mut stream = adapter
        .stream_chat(request("Say hello in one word.", vec![], 16))
        .await
        .expect("stream");
    let out = drain(&mut stream).await;
    assert!(!out.content.trim().is_empty(), "no answer");
}
