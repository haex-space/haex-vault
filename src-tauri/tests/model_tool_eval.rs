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
//! A server speaking the OpenAI chat format (the key, if it needs one, from the environment):
//!   HOLZI_EVAL_PROVIDER=openai HOLZI_EVAL_BASE_URL=http://127.0.0.1:11434/v1 HOLZI_EVAL_MODEL=qwen3.5:4b \
//!     cargo test --manifest-path src-tauri/Cargo.toml --test model_tool_eval -- --ignored --nocapture
//!
//! Claude on the user's subscription through `claude -p` (spec 078 US5, `support/claude_cli.rs`):
//!   HOLZI_EVAL_PROVIDER=claude-cli HOLZI_EVAL_MODEL=claude-opus-5-5 HOLZI_EVAL_WORKDIR=<empty dir> \
//!     cargo test --manifest-path src-tauri/Cargo.toml --test model_tool_eval -- --ignored --nocapture
//!
//! `measure_tool_calling_of_one_model` runs spec 032's set and writes `target/eval/<model>.json`;
//! `measure_everyday_voice` runs spec 078's everyday set and writes `target/eval/everyday-<model>.json`
//! (`HOLZI_EVAL_MACHINE` names the machine in that report); it needs `--features everyday-eval`.
//! Run one by its name after `--nocapture`.
//! Run it twice to see how far two runs differ (a local run is deterministic; a cloud run is not and
//! says so).

use std::env;
use std::path::PathBuf;

use holzi_lib::adapters::anthropic::AnthropicAdapter;
use holzi_lib::adapters::openai_compatible::OpenAiCompatibleAdapter;
use holzi_lib::adapters::ProviderAdapter;
#[cfg(feature = "everyday-eval")]
use holzi_lib::chat::eval::everyday::{
    embedded_mail_tools, embedded_set as embedded_everyday_set, embedded_world,
    report::{build, RunInfo},
    runner::run_everyday,
    scoring::effects,
};
use holzi_lib::chat::eval::runner::{run_eval, Which};
use holzi_lib::chat::eval::{embedded_extension_tools, embedded_set, embedded_tools};
use tokio_util::sync::CancellationToken;

#[path = "support/claude_cli.rs"]
mod claude_cli;

/// The model named in the environment: its adapter, its request id, whether the sampler can be made
/// deterministic, and the provider and placement the everyday report names.
#[cfg_attr(not(feature = "everyday-eval"), allow(dead_code))]
struct Measured {
    adapter: Box<dyn ProviderAdapter>,
    model: String,
    deterministic: bool,
    provider: &'static str,
    placement: String,
}

fn model_from_env() -> String {
    env::var("HOLZI_EVAL_MODEL").expect("HOLZI_EVAL_MODEL is not set")
}

async fn adapter_from_env() -> Measured {
    let cloud = |adapter: Box<dyn ProviderAdapter>, provider, placement: &str| Measured {
        adapter,
        model: model_from_env(),
        deterministic: false,
        provider,
        placement: placement.to_owned(),
    };
    match env::var("HOLZI_EVAL_PROVIDER").as_deref() {
        Ok("anthropic") => {
            let key = env::var("ANTHROPIC_API_KEY").expect("ANTHROPIC_API_KEY is not set");
            let adapter = AnthropicAdapter::new("https://api.anthropic.com".to_owned(), key)
                .expect("Anthropic adapter");
            cloud(Box::new(adapter), "anthropic", "cloud")
        }
        Ok("openai") => {
            let base = env::var("HOLZI_EVAL_BASE_URL").expect("HOLZI_EVAL_BASE_URL is not set");
            let key = env::var("HOLZI_EVAL_API_KEY").unwrap_or_default();
            let adapter =
                OpenAiCompatibleAdapter::new(base, key).expect("OpenAI-compatible adapter");
            cloud(Box::new(adapter), "openai", "server")
        }
        Ok("claude-cli") => {
            let workdir = PathBuf::from(
                env::var("HOLZI_EVAL_WORKDIR").expect("HOLZI_EVAL_WORKDIR is not set"),
            );
            cloud(
                Box::new(claude_cli::ClaudeCli { workdir }),
                "claude-cli",
                "cloud",
            )
        }
        Ok(other) => panic!(
            "HOLZI_EVAL_PROVIDER={other} is not supported (use anthropic, openai or claude-cli)"
        ),
        #[cfg(feature = "llm-llamacpp")]
        Err(_) if env::var("HOLZI_EVAL_ENGINE").as_deref() == Ok("llamacpp") => {
            llamacpp_from_env().await
        }
        Err(_) => local_from_env().await,
    }
}

#[cfg(feature = "llm-llamacpp")]
async fn llamacpp_from_env() -> Measured {
    // The same measurement through llama.cpp. `HOLZI_LLAMACPP_GPU_LAYERS` forces the layers on the GPU;
    // without it the memory plan of the engine decides.
    use holzi_lib::llm::llamacpp::{LlamaCppAdapter, LoadOptions};
    let path = PathBuf::from(env::var("HOLZI_TEST_GGUF").expect("HOLZI_TEST_GGUF"));
    let gpu_layers = env::var("HOLZI_LLAMACPP_GPU_LAYERS")
        .ok()
        .and_then(|v| v.parse().ok());
    // `HOLZI_LLAMACPP_CONTEXT` overrides the context the memory plan picks: a chain with the
    // tool list needs more than the 4096 tokens a 9B model gets on an 8 GB card.
    let context_window = env::var("HOLZI_LLAMACPP_CONTEXT")
        .ok()
        .and_then(|v| v.parse().ok());
    // `HOLZI_LLAMACPP_DRAFTER` loads the model's MTP drafter with it (spec 079 FR-013).
    let drafter = env::var_os("HOLZI_LLAMACPP_DRAFTER").map(PathBuf::from);
    let name = format!(
        "llamacpp-{}{}",
        path.file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default(),
        if drafter.is_some() { "-mtp" } else { "" }
    );
    let adapter = LlamaCppAdapter::load(
        &path,
        LoadOptions {
            context_window,
            gpu_layers,
            threads: None,
            drafter: drafter.clone(),
        },
    )
    .await
    .expect("llama.cpp load");
    Measured {
        adapter: Box::new(adapter),
        model: name,
        deterministic: true,
        provider: "llamacpp",
        placement: format!(
            "{}{}{}",
            gpu_layers.map_or_else(|| "auto".to_owned(), |l| format!("gpu:{l}")),
            context_window.map_or_else(String::new, |c: u32| format!(" ctx:{c}")),
            if drafter.is_some() { " mtp" } else { "" }
        ),
    }
}

#[cfg(feature = "llm-cpu")]
async fn local_from_env() -> Measured {
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
    Measured {
        adapter: Box::new(LocalAdapter::new(model)),
        model: name,
        deterministic: true,
        provider: "local",
        placement: "local".to_owned(),
    }
}

#[cfg(not(feature = "llm-cpu"))]
async fn local_from_env() -> Measured {
    panic!("this build has no local inference; set HOLZI_EVAL_PROVIDER and HOLZI_EVAL_MODEL")
}

#[tokio::test]
#[ignore = "asks a real model: needs HOLZI_TEST_GGUF or HOLZI_EVAL_PROVIDER, HOLZI_EVAL_MODEL and a key"]
async fn measure_tool_calling_of_one_model() {
    let Measured {
        adapter,
        model,
        deterministic,
        ..
    } = adapter_from_env().await;
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
    write_report(&model, "", &json);
}

/// Writes a report to `target/eval/<prefix><model>.json`.
fn write_report(model: &str, prefix: &str, json: &str) {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/eval");
    std::fs::create_dir_all(&dir).expect("create target/eval");
    let file = model.replace(['/', ':', ' '], "_");
    std::fs::write(dir.join(format!("{prefix}{file}.json")), json).expect("write report");
}

#[cfg(feature = "everyday-eval")]
#[tokio::test]
#[ignore = "asks a real model: needs HOLZI_TEST_GGUF or HOLZI_EVAL_PROVIDER and HOLZI_EVAL_MODEL"]
async fn measure_everyday_voice() {
    let measured = adapter_from_env().await;
    let set = embedded_everyday_set();
    let tools = embedded_tools();
    let mail = embedded_mail_tools();
    let world = embedded_world();
    let runs = run_everyday(
        measured.adapter.as_ref(),
        &measured.model,
        measured.deterministic,
        &set,
        &tools,
        &mail,
        &world,
        &CancellationToken::new(),
    )
    .await
    .expect("everyday evaluation");
    let report = build(
        &set,
        &runs,
        &effects(&tools, &mail),
        &world,
        RunInfo {
            model: measured.model.clone(),
            provider: measured.provider.to_owned(),
            deterministic: measured.deterministic,
            machine: env::var("HOLZI_EVAL_MACHINE").unwrap_or_default(),
            placement: measured.placement,
        },
    );
    let json = serde_json::to_string_pretty(&report).expect("report as JSON");
    println!("{json}");
    write_report(&measured.model, "everyday-", &json);
}
