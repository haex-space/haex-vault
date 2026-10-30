//! Spike for spec 070 (T-spike): does llama.cpp through `llama-cpp-2` load a GGUF that mistralrs
//! refuses (Qwen3.5, `qwen35`), answer coherently and emit a tool call as text?
//!
//! It prints load time, prompt and generation speed, peak RAM and VRAM so the same file can be
//! compared with Qwen3 4B. Ignored: it needs a GGUF and a lot of memory. Run memory-limited:
//!
//! ```text
//! HOLZI_SPIKE_GGUF=<file> [HOLZI_SPIKE_GPU_LAYERS=99] [HOLZI_SPIKE_TOOLS=1] \
//!   systemd-run --user --scope -p MemoryMax=12G \
//!   cargo test --release --features llm-llamacpp --lib llamacpp_spike -- --ignored --nocapture
//! ```

use std::num::NonZeroU32;
use std::process::Command;
use std::time::Instant;

use llama_cpp_2::context::params::LlamaContextParams;
use llama_cpp_2::llama_backend::LlamaBackend;
use llama_cpp_2::llama_batch::LlamaBatch;
use llama_cpp_2::model::params::LlamaModelParams;
use llama_cpp_2::model::{LlamaChatMessage, LlamaModel};
use llama_cpp_2::sampling::LlamaSampler;

const MAX_NEW_TOKENS: usize = 200;

const PLAIN_PROMPT: &str = "Name three uses of a paper clip. Answer in one short sentence each.";

/// The Hermes-style tool prompt Qwen models are trained on, so a call shows up as `<tool_call>` text.
const TOOL_SYSTEM: &str = "You are a helpful assistant.\n\n# Tools\n\nYou may call one or more functions to assist with the user query.\n\nYou are provided with function signatures within <tools></tools> XML tags:\n<tools>\n{\"type\":\"function\",\"function\":{\"name\":\"open_settings\",\"description\":\"Opens the settings page\",\"parameters\":{\"type\":\"object\",\"properties\":{\"page\":{\"type\":\"string\"}},\"required\":[\"page\"]}}}\n</tools>\n\nFor each function call, return a json object with function name and arguments within <tool_call></tool_call> XML tags:\n<tool_call>\n{\"name\": <function-name>, \"arguments\": <args-json-object>}\n</tool_call>";
const TOOL_PROMPT: &str = "Please open the settings page about sync.";

/// Peak resident memory of this process in MiB (`VmHWM`).
fn peak_rss_mib() -> Option<u64> {
    let status = std::fs::read_to_string("/proc/self/status").ok()?;
    let line = status.lines().find(|l| l.starts_with("VmHWM:"))?;
    let kib: u64 = line.split_whitespace().nth(1)?.parse().ok()?;
    Some(kib / 1024)
}

/// The VRAM in use on the first card, in MiB, as `nvidia-smi` reports it.
fn vram_used_mib() -> Option<u64> {
    let out = Command::new("nvidia-smi")
        .args(["--query-gpu=memory.used", "--format=csv,noheader,nounits"])
        .output()
        .ok()?;
    String::from_utf8(out.stdout)
        .ok()?
        .lines()
        .next()?
        .trim()
        .parse()
        .ok()
}

/// ChatML as Qwen models use it; written by hand because the binding offers no Jinja.
fn chatml(system: Option<&str>, user: &str) -> String {
    let mut out = String::new();
    if let Some(system) = system {
        out.push_str(&format!("<|im_start|>system\n{system}<|im_end|>\n"));
    }
    out.push_str(&format!(
        "<|im_start|>user\n{user}<|im_end|>\n<|im_start|>assistant\n"
    ));
    out
}

struct Run {
    text: String,
    prompt_tokens: usize,
    prompt_secs: f64,
    new_tokens: usize,
    gen_secs: f64,
}

fn generate(model: &LlamaModel, backend: &LlamaBackend, prompt: &str) -> Run {
    let ctx_params = LlamaContextParams::default()
        .with_n_ctx(NonZeroU32::new(4096))
        .with_n_batch(2048);
    let mut ctx = model.new_context(backend, ctx_params).expect("context");
    let vocab = model.vocab();
    let tokens = vocab.tokenize(prompt.as_bytes(), true, true);

    let mut batch = LlamaBatch::new(2048, 1);
    let last = tokens.len() as i32 - 1;
    for (i, token) in tokens.iter().enumerate() {
        batch.add(*token, i as i32, &[0], i as i32 == last).unwrap();
    }
    let started = Instant::now();
    ctx.decode(&mut batch).expect("prompt decode");
    let prompt_secs = started.elapsed().as_secs_f64();

    let mut sampler = LlamaSampler::greedy();
    let mut bytes: Vec<u8> = Vec::new();
    let mut pos = tokens.len() as i32;
    let mut new_tokens = 0;
    let started = Instant::now();
    while new_tokens < MAX_NEW_TOKENS {
        let token = sampler.sample(&ctx, batch.n_tokens() - 1);
        if vocab.is_eog(token) {
            break;
        }
        bytes.extend(vocab.token_to_piece(token, true, None));
        new_tokens += 1;
        batch.clear();
        batch.add(token, pos, &[0], true).unwrap();
        ctx.decode(&mut batch).expect("decode");
        pos += 1;
    }
    Run {
        text: String::from_utf8_lossy(&bytes).into_owned(),
        prompt_tokens: tokens.len(),
        prompt_secs,
        new_tokens,
        gen_secs: started.elapsed().as_secs_f64(),
    }
}

fn report(label: &str, run: &Run) {
    println!(
        "--- {label}: prompt {} tokens in {:.2}s ({:.0} t/s), generated {} tokens in {:.2}s ({:.1} t/s)\n{}\n",
        run.prompt_tokens,
        run.prompt_secs,
        run.prompt_tokens as f64 / run.prompt_secs.max(1e-9),
        run.new_tokens,
        run.gen_secs,
        run.new_tokens as f64 / run.gen_secs.max(1e-9),
        run.text
    );
}

#[test]
#[ignore = "spec 070 spike: needs HOLZI_SPIKE_GGUF and a lot of memory"]
fn llamacpp_spike_loads_and_answers() {
    let path = std::env::var("HOLZI_SPIKE_GGUF").expect("HOLZI_SPIKE_GGUF");
    let gpu_layers: u32 = std::env::var("HOLZI_SPIKE_GPU_LAYERS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);

    let backend = LlamaBackend::init().expect("backend");
    let params = LlamaModelParams::default().with_n_gpu_layers(gpu_layers);
    let started = Instant::now();
    let model = LlamaModel::load_from_file(&backend, &path, &params).expect("load");
    println!(
        "=== {path}\nloaded in {:.2}s, {} params, {} MiB of weights, gpu layers {gpu_layers}",
        started.elapsed().as_secs_f64(),
        model.n_params(),
        model.size() / (1024 * 1024)
    );

    // Does the binding's template API cope with the template baked into the file?
    match model.chat_template(None) {
        Ok(template) => {
            let msgs = vec![LlamaChatMessage::new("user".into(), PLAIN_PROMPT.into()).unwrap()];
            match model.apply_chat_template(&template, &msgs, true) {
                Ok(rendered) => println!("apply_chat_template ok:\n{rendered}\n"),
                Err(e) => println!("apply_chat_template FAILED: {e:?}\n"),
            }
        }
        Err(e) => println!("no chat template in the file: {e:?}\n"),
    }

    let plain = generate(&model, &backend, &chatml(None, PLAIN_PROMPT));
    report("plain", &plain);
    assert!(plain.new_tokens > 0, "no tokens generated");

    if std::env::var("HOLZI_SPIKE_TOOLS").is_ok() {
        let tool = generate(&model, &backend, &chatml(Some(TOOL_SYSTEM), TOOL_PROMPT));
        report("tool prompt", &tool);
        println!(
            "tool call text present: {}",
            tool.text.contains("<tool_call>")
        );
    }

    println!(
        "peak RAM {:?} MiB, VRAM in use {:?} MiB",
        peak_rss_mib(),
        vram_used_mib()
    );
}

/// The same measurement through the engine holzi uses today, for the comparison table. mistralrs
/// reports its own prompt and generation speed in the response usage.
#[tokio::test]
#[ignore = "spec 070 spike: needs HOLZI_SPIKE_GGUF and a lot of memory"]
async fn mistralrs_baseline_for_llamacpp_spike() {
    use mistralrs::{RequestBuilder, TextMessageRole};

    let path =
        std::path::PathBuf::from(std::env::var("HOLZI_SPIKE_GGUF").expect("HOLZI_SPIKE_GGUF"));
    let tokenizer = std::env::var("HOLZI_SPIKE_TOKENIZER").ok();
    let started = Instant::now();
    let model = super::LocalModel::load(&path, tokenizer.as_deref())
        .await
        .expect("mistralrs load");
    println!(
        "=== mistralrs {path:?}\nloaded in {:.2}s",
        started.elapsed().as_secs_f64()
    );

    let request = RequestBuilder::new()
        .add_message(TextMessageRole::User, PLAIN_PROMPT)
        .set_sampler_max_len(MAX_NEW_TOKENS)
        .set_sampler_temperature(0.0);
    // One warm-up so the CUDA kernels are compiled before the timed run.
    let _ = model
        .inner()
        .send_chat_request(request.clone())
        .await
        .expect("warm-up");
    let response = model
        .inner()
        .send_chat_request(request)
        .await
        .expect("chat");
    let usage = &response.usage;
    println!(
        "--- mistralrs plain: prompt {} tokens ({:.0} t/s), generated {} tokens ({:.1} t/s)\n{}\n",
        usage.prompt_tokens,
        usage.avg_prompt_tok_per_sec,
        usage.completion_tokens,
        usage.avg_compl_tok_per_sec,
        response.choices[0]
            .message
            .content
            .clone()
            .unwrap_or_default()
    );
    println!(
        "peak RAM {:?} MiB, VRAM in use {:?} MiB",
        peak_rss_mib(),
        vram_used_mib()
    );
}
