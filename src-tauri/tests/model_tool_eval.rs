//! The full measurement of how well one model calls holzi's tools (spec 032 US5, FR-019 to FR-022).
//! Skipped by default (`#[ignore]`): it asks a real model every sentence of the example set.
//! It needs no vault and runs no action, so it cannot change any data.
//!
//! Local model (build with the default `llm-llamacpp` feature):
//!   HOLZI_TEST_GGUF=~/models/Qwen_Qwen3-4B-Q4_K_M.gguf \
//! //!     cargo test --manifest-path src-tauri/Cargo.toml --test model_tool_eval -- --ignored --nocapture
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
//! `measure_everyday_voice` runs spec 078's everyday set and writes
//! `target/eval/everyday-<model>-<profile>.json` (`HOLZI_EVAL_MACHINE` names the machine in that
//! report; `HOLZI_EVAL_PROFILE=phone` measures a phone's offer, without the command tool); it needs
//! `--features everyday-eval`.
//! Run one by its name after `--nocapture`.
//! Run it twice to see how far two runs differ (a local run is deterministic; a cloud run is not and
//! says so).

use std::env;
use std::path::PathBuf;

use holzi_lib::adapters::anthropic::AnthropicAdapter;
use holzi_lib::adapters::openai_compatible::OpenAiCompatibleAdapter;
use holzi_lib::adapters::ProviderAdapter;
#[cfg(feature = "everyday-eval")]
use holzi_lib::chat::eval::everyday::runner::Profile;
#[cfg(feature = "everyday-eval")]
use holzi_lib::chat::eval::everyday::{
    embedded_mail_tools, embedded_mail_tools_as, embedded_set as embedded_everyday_set,
    embedded_world, load_set_file,
    report::{build, RunInfo},
    runner::run_everyday,
    scoring::effects,
};
use holzi_lib::chat::eval::runner::{run_eval, Which};
use holzi_lib::chat::eval::{embedded_extension_tools, embedded_set, embedded_tools};
#[cfg(feature = "everyday-eval")]
use holzi_lib::chat::tools::route::{HOSTED_ROUTE_TIMEOUT, LOCAL_ROUTE_TIMEOUT};
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
            ..LoadOptions::default()
        },
    )
    .await
    .expect("llama.cpp load");
    // What the memory plan gave, not what was asked: it halves the context before it moves layers
    // to the host, and how far depends on the memory free at load time (spec 082 T032).
    let placed = adapter.model().placement();
    let asked = context_window
        .filter(|&c| c != placed.context)
        .map_or_else(String::new, |c| format!(" (asked {c})"));
    Measured {
        adapter: Box::new(adapter),
        model: name,
        deterministic: true,
        provider: "llamacpp",
        placement: format!(
            "gpu:{} ctx:{}{asked}{}",
            placed.gpu_layers,
            placed.context,
            if drafter.is_some() { " mtp" } else { "" }
        ),
    }
}

#[cfg(feature = "llm-llamacpp")]
async fn local_from_env() -> Measured {
    llamacpp_from_env().await
}

#[cfg(not(feature = "llm-llamacpp"))]
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
    write_report(&model, "", "", &json);
}

/// Writes a report to `target/eval/<prefix><model>.json`.
fn write_report(model: &str, prefix: &str, suffix: &str, json: &str) {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/eval");
    std::fs::create_dir_all(&dir).expect("create target/eval");
    let file = model.replace(['/', ':', ' '], "_");
    std::fs::write(dir.join(format!("{prefix}{file}{suffix}.json")), json).expect("write report");
}

#[cfg(feature = "everyday-eval")]
#[tokio::test]
#[ignore = "asks a real model: needs HOLZI_TEST_GGUF or HOLZI_EVAL_PROVIDER and HOLZI_EVAL_MODEL"]
async fn measure_everyday_voice() {
    let measured = adapter_from_env().await;
    // Spec 087: `HAEX_EVAL_SET` runs another set file (the chain set) instead of the embedded one.
    let (set, set_name) = match env::var("HAEX_EVAL_SET") {
        Ok(path) => {
            let path = PathBuf::from(path);
            let set = load_set_file(&path).unwrap_or_else(|e| panic!("HAEX_EVAL_SET: {e}"));
            let name = path
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_default();
            (set, name)
        }
        Err(_) => (embedded_everyday_set(), "everyday".to_owned()),
    };
    let tools = embedded_tools();
    // haex-mail named as the app names it; `HOLZI_EVAL_MAIL_NAME` (and `HOLZI_EVAL_MAIL_TITLE`)
    // call it something else, to measure that a request reaches it whatever it is called.
    let (mail, mail_extension) = match env::var("HOLZI_EVAL_MAIL_NAME") {
        Ok(name) => {
            let title = env::var("HOLZI_EVAL_MAIL_TITLE").unwrap_or_else(|_| name.clone());
            (
                embedded_mail_tools_as(&name, &title),
                format!("{name} ({title})"),
            )
        }
        Err(_) => (embedded_mail_tools(), "haex-mail".to_owned()),
    };
    let world = embedded_world();
    let profile = match env::var("HOLZI_EVAL_PROFILE") {
        Ok(name) => Profile::parse(&name).expect("HOLZI_EVAL_PROFILE is desktop or phone"),
        Err(_) => Profile::Desktop,
    };
    // Spec 082 US2/US6: the first offer is routed as in the chat, where only local engines route
    // (`route::routes`); `HOLZI_EVAL_ROUTING=on` or `off` overrides that, the routing call bounded as
    // for the engine it runs on.
    let local = ["llamacpp", "local"].contains(&measured.provider);
    let timeout = if local {
        LOCAL_ROUTE_TIMEOUT
    } else {
        HOSTED_ROUTE_TIMEOUT
    };
    let routing = match env::var("HOLZI_EVAL_ROUTING").as_deref() {
        Ok("off") => None,
        Ok("on") => Some(timeout),
        _ => local.then_some(timeout),
    };
    let runs = run_everyday(
        measured.adapter.as_ref(),
        &measured.model,
        measured.deterministic,
        &set,
        &tools,
        &mail,
        &world,
        profile,
        routing,
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
            profile,
            mail_extension,
            set: set_name.clone(),
        },
    );
    let json = serde_json::to_string_pretty(&report).expect("report as JSON");
    println!("{json}");
    // One file per device profile: a desktop and a phone run of one model must not overwrite each other;
    // and one per set, so the chain set does not overwrite the everyday report of the same model.
    let prefix = match set_name.strip_suffix(".json") {
        Some(stem) => format!("everyday-{stem}-"),
        None => "everyday-".to_owned(),
    };
    write_report(
        &measured.model,
        &prefix,
        &format!("-{}", profile.as_str()),
        &json,
    );
}
