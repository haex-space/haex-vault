//! The full measurement of how well one model calls holzi's tools (spec 032 US5, FR-019 to FR-022).
//! Skipped by default (`#[ignore]`): it asks a real model every sentence of the example set.
//! It needs no vault and runs no action, so it cannot change any data.
//!
//! Local model (build with the default `llm-cpu` feature):
//!   HOLZI_TEST_GGUF=~/models/Qwen_Qwen3-4B-Q4_K_M.gguf \
//!   HOLZI_TEST_GGUF_TOKENIZER=Qwen/Qwen3-4B \
//!     cargo test --manifest-path src-tauri/Cargo.toml --test model_tool_eval -- --ignored --nocapture
//!
//! Anthropic (the key comes from the environment, never from a file):
//!   HOLZI_EVAL_PROVIDER=anthropic HOLZI_EVAL_MODEL=claude-sonnet-5-5 ANTHROPIC_API_KEY=... \
//!     cargo test --manifest-path src-tauri/Cargo.toml --test model_tool_eval -- --ignored --nocapture
//!
//! The report is written to `target/eval/<model>.json` and printed. Run it twice to see how far
//! two runs differ (a local run is deterministic; a cloud run is not and says so).

use std::env;
use std::path::PathBuf;

use holzi_lib::adapters::anthropic::AnthropicAdapter;
use holzi_lib::adapters::ProviderAdapter;
use holzi_lib::chat::eval::runner::{run_eval, Which};
use holzi_lib::chat::eval::{embedded_extension_tools, embedded_set, embedded_tools};
use tokio_util::sync::CancellationToken;

/// The adapter for the model named in the environment, its request id, and whether the sampler
/// can be made deterministic.
async fn adapter_from_env() -> (Box<dyn ProviderAdapter>, String, bool) {
    match env::var("HOLZI_EVAL_PROVIDER").as_deref() {
        Ok("anthropic") => {
            let model = env::var("HOLZI_EVAL_MODEL").expect("HOLZI_EVAL_MODEL is not set");
            let key = env::var("ANTHROPIC_API_KEY").expect("ANTHROPIC_API_KEY is not set");
            let adapter = AnthropicAdapter::new("https://api.anthropic.com".to_owned(), key)
                .expect("Anthropic adapter");
            (Box::new(adapter), model, false)
        }
        Ok(other) => panic!("HOLZI_EVAL_PROVIDER={other} is not supported (use anthropic)"),
        Err(_) => local_from_env().await,
    }
}

#[cfg(feature = "llm-cpu")]
async fn local_from_env() -> (Box<dyn ProviderAdapter>, String, bool) {
    use holzi_lib::adapters::local::LocalAdapter;
    use holzi_lib::llm::local::LocalModel;

    let path =
        PathBuf::from(env::var("HOLZI_TEST_GGUF").expect(
            "set HOLZI_TEST_GGUF to a GGUF file, or HOLZI_EVAL_PROVIDER and HOLZI_EVAL_MODEL",
        ));
    let tokenizer =
        env::var("HOLZI_TEST_GGUF_TOKENIZER").unwrap_or_else(|_| "Qwen/Qwen3-4B".to_owned());
    let name = path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "local".to_owned());
    let model = LocalModel::load(&path, Some(&tokenizer))
        .await
        .expect("model load");
    (Box::new(LocalAdapter::new(model)), name, true)
}

#[cfg(not(feature = "llm-cpu"))]
async fn local_from_env() -> (Box<dyn ProviderAdapter>, String, bool) {
    panic!("this build has no local inference; set HOLZI_EVAL_PROVIDER and HOLZI_EVAL_MODEL")
}

#[tokio::test]
#[ignore = "asks a real model: needs HOLZI_TEST_GGUF or HOLZI_EVAL_PROVIDER, HOLZI_EVAL_MODEL and a key"]
async fn measure_tool_calling_of_one_model() {
    let (adapter, model, deterministic) = adapter_from_env().await;
    let report = run_eval(
        adapter.as_ref(),
        &model,
        deterministic,
        &embedded_set(),
        &embedded_tools(),
        &embedded_extension_tools(),
        Which::All,
        &CancellationToken::new(),
    )
    .await
    .expect("evaluation");

    let json = serde_json::to_string_pretty(&report).expect("report as JSON");
    println!("{json}");
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/eval");
    std::fs::create_dir_all(&dir).expect("create target/eval");
    let file = model.replace(['/', ':', ' '], "_");
    std::fs::write(dir.join(format!("{file}.json")), json).expect("write report");
}
