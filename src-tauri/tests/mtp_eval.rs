//! Spec 079: the MTP drafter with the real files. Skipped by default (`#[ignore]`):
//!
//! ```text
//! HOLZI_TEST_GGUF=<models>/gemma-4-E2B-it-Q4_K_M.gguf \
//! HOLZI_LLAMACPP_DRAFTER=<models>/mtp-gemma-4-E2B-it.gguf \
//!   cargo test --manifest-path src-tauri/Cargo.toml --features llm-llamacpp-cuda \
//!   --test mtp_eval -- --ignored --nocapture
//! ```
//!
//! - `mtp_spike`: the binding's `MtpSpeculative` on its own (task T003, research R6).
//! - `mtp_speed`: holzi's own load and token loop with and without the drafter (T015, SC-001, SC-003).
#![cfg(feature = "llm-llamacpp")]

use std::env;
use std::num::NonZeroU32;
use std::path::PathBuf;
use std::sync::OnceLock;
use std::time::Instant;

use holzi_lib::llm::llamacpp::generate::{generate, Job};
use holzi_lib::llm::llamacpp::model::{LoadOptions, LoadedModel};
use llama_cpp_2::context::params::{LlamaContextParams, LlamaContextType};
use llama_cpp_2::context::LlamaContext;
use llama_cpp_2::llama_backend::LlamaBackend;
use llama_cpp_2::llama_batch::LlamaBatch;
use llama_cpp_2::model::params::LlamaModelParams;
use llama_cpp_2::model::LlamaModel;
use llama_cpp_2::sampling::LlamaSampler;
use llama_cpp_2::speculative::{MtpSpeculative, MtpSpeculativeParams};
use llama_cpp_2::token::LlamaToken;
use llama_cpp_sys_2::LLAMA_FLASH_ATTN_TYPE_DISABLED;
use tokio_util::sync::CancellationToken;

const BATCH: usize = 512;
const CONTEXT: u32 = 4096;
const NEW_TOKENS: usize = 200;
/// Proposals per round, as in the measurement of research R1 (`--spec-draft-n-max 4`).
const DRAFT_MAX: i32 = 4;

/// The four requests of research R1: a short spoken answer, a mail summary, a tool call, a longer reply.
const PROMPTS: [&str; 4] = [
    "Antworte kurz und gesprochen: Wie spät ist es, wenn es 9:41 Uhr ist, und welcher Wochentag ist morgen, wenn heute Mittwoch ist?",
    "Fasse diese Mail in zwei Sätzen zusammen: Hallo, ich wollte fragen, ob es am Donnerstag um sieben im Café am Markt klappt. Ich bringe die Fotos vom Urlaub mit. Viele Grüße, Anna",
    "Gib nur JSON zurück: einen Aufruf {\"name\": \"x_mail_find_messages\", \"arguments\": {...}} für die letzte ungelesene Mail von Anna.",
    "Write a friendly reply of about 120 words to a colleague who asked whether the project plan for Q2 is ready, explaining that two sections are still open.",
];

fn backend() -> &'static LlamaBackend {
    static BACKEND: OnceLock<LlamaBackend> = OnceLock::new();
    BACKEND.get_or_init(|| LlamaBackend::init().expect("llama.cpp backend"))
}

fn path_from_env(name: &str) -> PathBuf {
    PathBuf::from(env::var(name).unwrap_or_else(|_| panic!("{name} is not set")))
}

fn load(path: &PathBuf) -> LlamaModel {
    let params = LlamaModelParams::default().with_n_gpu_layers(999);
    LlamaModel::load_from_file(backend(), path, &params).expect("load gguf")
}

/// Gemma 4's own turn markers; the spike does not need the full chat template.
fn prompt_tokens(model: &LlamaModel, prompt: &str) -> Vec<LlamaToken> {
    let text = format!("<bos><|turn>user\n{prompt}<turn|>\n<|turn>model\n");
    model.vocab().tokenize(text.as_bytes(), false, true)
}

fn text_of(model: &LlamaModel, tokens: &[LlamaToken]) -> String {
    let bytes: Vec<u8> = tokens
        .iter()
        .flat_map(|token| model.vocab().token_to_piece(*token, true, None))
        .collect();
    String::from_utf8_lossy(&bytes).into_owned()
}

fn context_params(flash_attention_off: bool, swa_full: bool) -> LlamaContextParams {
    let params = LlamaContextParams::default()
        .with_n_ctx(NonZeroU32::new(CONTEXT))
        .with_n_batch(BATCH as u32)
        .with_swa_full(swa_full);
    if flash_attention_off {
        params.with_flash_attention_policy(LLAMA_FLASH_ATTN_TYPE_DISABLED)
    } else {
        params
    }
}

/// Decodes all prompt tokens but the last, in batches.
fn decode_prefix(ctx: &mut LlamaContext<'_>, prefix: &[LlamaToken]) {
    let mut batch = LlamaBatch::new(BATCH, 1);
    for (start, chunk) in prefix.chunks(BATCH).enumerate() {
        batch.clear();
        for (i, token) in chunk.iter().enumerate() {
            let position = (start * BATCH + i) as i32;
            batch.add(*token, position, &[0], false).expect("batch");
        }
        ctx.decode(&mut batch).expect("decode prompt");
    }
}

/// The plain loop of `generate.rs`, greedy.
fn plain(
    model: &LlamaModel,
    tokens: &[LlamaToken],
    flash_attention_off: bool,
) -> (Vec<LlamaToken>, f64) {
    let mut ctx = model
        .new_context(backend(), context_params(flash_attention_off, false))
        .expect("context");
    let (prefix, last) = tokens.split_at(tokens.len() - 1);
    decode_prefix(&mut ctx, prefix);
    let mut sampler = LlamaSampler::greedy();
    let mut batch = LlamaBatch::new(BATCH, 1);
    let mut position = prefix.len() as i32;
    let mut id_last = last[0];
    let mut out = Vec::new();
    let started = Instant::now();
    while out.len() < NEW_TOKENS {
        batch.clear();
        batch.add(id_last, position, &[0], true).expect("batch");
        ctx.decode(&mut batch).expect("decode");
        position += 1;
        id_last = sampler.sample(&ctx, 0);
        if model.vocab().is_eog(id_last) {
            break;
        }
        out.push(id_last);
    }
    let rate = out.len() as f64 / started.elapsed().as_secs_f64();
    (out, rate)
}

struct SpeculativeRun {
    tokens: Vec<LlamaToken>,
    tokens_per_second: f64,
    drafted: usize,
    accepted: usize,
    /// Every partial removal from the target cache succeeded.
    removal_works: bool,
    /// How many rounds proposed 0, 1, … tokens.
    lengths: [usize; DRAFT_MAX as usize + 1],
    /// Seconds in drafting, in the model's decode, and in sampling.
    times: [f64; 3],
}

/// The loop of research R6, in the order of llama.cpp's `examples/speculative-simple`.
fn speculative(
    model: &LlamaModel,
    drafter: &LlamaModel,
    tokens: &[LlamaToken],
    swa_full: bool,
) -> SpeculativeRun {
    let target = model
        .new_context(backend(), context_params(true, swa_full))
        .expect("target context");
    let draft_params = context_params(true, false).with_context_type(LlamaContextType::Mtp);
    let draft = drafter
        .new_context_with_ctx_other(backend(), draft_params, &target)
        .expect("drafter context");
    let params = MtpSpeculativeParams {
        n_max: DRAFT_MAX,
        ..MtpSpeculativeParams::default()
    };
    let mut spec = MtpSpeculative::new(target, draft, params).expect("MtpSpeculative");

    let (prefix, last) = tokens.split_at(tokens.len() - 1);
    let mut batch = LlamaBatch::new(BATCH, 1);
    for (start, chunk) in prefix.chunks(BATCH).enumerate() {
        batch.clear();
        for (i, token) in chunk.iter().enumerate() {
            batch
                .add(*token, (start * BATCH + i) as i32, &[0], false)
                .expect("batch");
        }
        spec.target_context_mut()
            .decode(&mut batch)
            .expect("decode prompt");
        spec.process(&batch).expect("process prompt");
    }
    let mut history: Vec<LlamaToken> = prefix.to_vec();
    spec.begin(&history).expect("begin");

    let mut sampler = LlamaSampler::greedy();
    let mut id_last = last[0];
    let mut n_past = prefix.len() as i32;
    let (mut drafted, mut accepted, mut removal_works) = (0, 0, true);
    let mut lengths = [0; DRAFT_MAX as usize + 1];
    let mut times = [0.0; 3];
    let mut out = Vec::new();
    let started = Instant::now();
    'generation: while out.len() < NEW_TOKENS {
        let clock = Instant::now();
        let proposal = spec.draft(n_past, id_last, &history).expect("draft");
        times[0] += clock.elapsed().as_secs_f64();
        lengths[proposal.len()] += 1;
        spec.draft_context_mut()
            .clear_kv_cache_seq(Some(0), Some(n_past as u32), None)
            .expect("drafter cache");

        batch.clear();
        batch.add(id_last, n_past, &[0], true).expect("batch");
        for (i, token) in proposal.iter().enumerate() {
            batch
                .add(*token, n_past + 1 + i as i32, &[0], true)
                .expect("batch");
        }
        let clock = Instant::now();
        spec.target_context_mut()
            .decode(&mut batch)
            .expect("decode draft");
        spec.process(&batch).expect("process draft");
        times[1] += clock.elapsed().as_secs_f64();
        let clock = Instant::now();

        let mut kept = Vec::with_capacity(proposal.len() + 1);
        for i in 0..=proposal.len() {
            let token = sampler.sample(spec.target_context(), i as i32);
            kept.push(token);
            if i == proposal.len() || proposal[i] != token {
                break;
            }
        }
        times[2] += clock.elapsed().as_secs_f64();
        if !proposal.is_empty() {
            spec.accept((kept.len() - 1) as u16).expect("accept");
        }
        drafted += proposal.len();
        accepted += kept.len() - 1;
        n_past += kept.len() as i32;

        let removed = spec
            .target_context_mut()
            .clear_kv_cache_seq(Some(0), Some(n_past as u32), None)
            .expect("target cache");
        removal_works &= removed;
        spec.draft_context_mut()
            .clear_kv_cache_seq(Some(0), Some(n_past as u32), None)
            .expect("drafter cache");

        for token in kept {
            history.push(id_last);
            id_last = token;
            if model.vocab().is_eog(token) || out.len() >= NEW_TOKENS {
                break 'generation;
            }
            out.push(token);
        }
    }
    SpeculativeRun {
        tokens_per_second: out.len() as f64 / started.elapsed().as_secs_f64(),
        tokens: out,
        drafted,
        accepted,
        removal_works,
        lengths,
        times,
    }
}

/// The first character where two texts part, or `None` when they are equal.
fn first_difference(a: &str, b: &str) -> Option<usize> {
    if a == b {
        return None;
    }
    Some(a.chars().zip(b.chars()).take_while(|(x, y)| x == y).count())
}

#[test]
#[ignore = "needs HOLZI_TEST_GGUF (Gemma 4) and HOLZI_LLAMACPP_DRAFTER (its mtp-*.gguf)"]
fn mtp_spike() {
    let model = load(&path_from_env("HOLZI_TEST_GGUF"));
    let drafter = load(&path_from_env("HOLZI_LLAMACPP_DRAFTER"));
    for swa_full in [false, true] {
        println!("== swa_full = {swa_full}");
        let mut all_removals_work = true;
        for prompt in PROMPTS {
            let tokens = prompt_tokens(&model, prompt);
            let (plain_fa, plain_fa_rate) = plain(&model, &tokens, false);
            let (plain_off, plain_off_rate) = plain(&model, &tokens, true);
            let run = speculative(&model, &drafter, &tokens, swa_full);
            assert!(!run.tokens.is_empty(), "the drafter run wrote nothing");
            let text = text_of(&model, &run.tokens);
            let reference = text_of(&model, &plain_off);
            println!(
                "plain FA auto {plain_fa_rate:.1} tok/s, plain FA off {plain_off_rate:.1}, drafter {:.1} \
                 ({:.2}x over FA auto), accepted {}/{}, partial removal works: {}",
                run.tokens_per_second,
                run.tokens_per_second / plain_fa_rate,
                run.accepted,
                run.drafted,
                run.removal_works,
            );
            println!(
                "  equal to plain FA off: {:?}, to plain FA auto: {:?}",
                first_difference(&text, &reference),
                first_difference(&text, &text_of(&model, &plain_fa)),
            );
            println!(
                "  proposals per round {:?}, seconds draft/decode/sample {:.2?}",
                run.lengths, run.times
            );
            println!(
                "  {}",
                text.chars()
                    .take(120)
                    .collect::<String>()
                    .replace('\n', " ")
            );
            all_removals_work &= run.removal_works;
        }
        if all_removals_work {
            println!("partial removal works with swa_full = {swa_full}");
            break;
        }
    }
}

/// What one pass through holzi's loader and token loop measured.
struct AppRun {
    answers: Vec<String>,
    /// Tokens per second of writing, from the first token on.
    rate: f64,
    /// Mean seconds to the first token over the four requests.
    first: f64,
    /// Seconds to the first token after a prompt as long as a chat with its system prompt and tools.
    first_long: f64,
}

/// About 3000 tokens, the size of a chat prompt with holzi's system prompt and tools.
fn long_prompt() -> String {
    let paragraph = "Die Werkzeuge des Assistenten lesen Mails, Termine, Kontakte und Dateien des Nutzers. \
                     Jedes Werkzeug hat einen Namen, eine Beschreibung und ein JSON-Schema seiner Argumente. ";
    format!("{}\n\n{}", paragraph.repeat(100), PROMPTS[1])
}

/// The four requests of research R1 and the long prompt, through holzi's loader and token loop.
fn app_run(drafter: Option<PathBuf>) -> AppRun {
    let loaded = LoadedModel::load(
        &path_from_env("HOLZI_TEST_GGUF"),
        LoadOptions {
            drafter,
            ..LoadOptions::default()
        },
    )
    .expect("load");
    println!("placement: {:?}", loaded.placement());
    let (mut answers, mut tokens, mut seconds, mut first_total) = (Vec::new(), 0, 0.0, 0.0);
    for prompt in PROMPTS {
        let rendered = loaded.render_user_message(prompt).expect("chat template");
        let job = Job {
            prompt: &rendered,
            max_new_tokens: NEW_TOKENS,
            deterministic: true,
        };
        let mut text = String::new();
        let mut first = None;
        let started = Instant::now();
        let outcome = generate(&loaded, &job, &CancellationToken::new(), |piece| {
            first.get_or_insert_with(|| started.elapsed().as_secs_f64());
            text.push_str(piece);
            true
        })
        .expect("generate");
        let first = first.unwrap_or_default();
        let writing = started.elapsed().as_secs_f64() - first;
        // The first token is counted in the time to it, not in the writing.
        let written = outcome.new_tokens.saturating_sub(1);
        println!(
            "  {} tokens, first after {:.0} ms, then {:.1} tok/s: {}",
            outcome.new_tokens,
            first * 1000.0,
            written as f64 / writing,
            text.chars().take(80).collect::<String>().replace('\n', " ")
        );
        tokens += written;
        seconds += writing;
        first_total += first;
        answers.push(text);
    }
    let rendered = loaded
        .render_user_message(&long_prompt())
        .expect("chat template");
    let job = Job {
        prompt: &rendered,
        max_new_tokens: 1,
        deterministic: true,
    };
    let started = Instant::now();
    let outcome = generate(&loaded, &job, &CancellationToken::new(), |_| true).expect("long");
    let first_long = started.elapsed().as_secs_f64();
    println!(
        "  long prompt of {} tokens: first token after {:.0} ms",
        outcome.prompt_tokens,
        first_long * 1000.0
    );
    AppRun {
        answers,
        rate: tokens as f64 / seconds,
        first: first_total / PROMPTS.len() as f64,
        first_long,
    }
}

/// The JSON object of an answer, so a tool call compares by its content and not by its spacing.
fn json_of(answer: &str) -> Option<serde_json::Value> {
    let start = answer.find('{')?;
    let end = answer.rfind('}')?;
    serde_json::from_str(answer.get(start..=end)?).ok()
}

#[test]
#[ignore = "needs HOLZI_TEST_GGUF (Gemma 4) and HOLZI_LLAMACPP_DRAFTER (its mtp-*.gguf)"]
fn mtp_speed() {
    println!("== without the drafter");
    let plain = app_run(None);
    println!("== with the drafter");
    let drafted = app_run(Some(path_from_env("HOLZI_LLAMACPP_DRAFTER")));
    let gain = drafted.rate / plain.rate;
    println!(
        "{:.1} -> {:.1} tok/s, {gain:.2}x; first token {:.0} -> {:.0} ms, after the long prompt {:.0} -> {:.0} ms",
        plain.rate,
        drafted.rate,
        plain.first * 1000.0,
        drafted.first * 1000.0,
        plain.first_long * 1000.0,
        drafted.first_long * 1000.0
    );
    let (plain, drafted) = (plain.answers, drafted.answers);
    for (a, b) in plain.iter().zip(&drafted) {
        println!("  same text: {:?}", first_difference(a, b));
    }
    // SC-003: the tool call of the JSON request is the same; wording may differ in rare places.
    assert_eq!(
        json_of(&plain[2]),
        json_of(&drafted[2]),
        "the tool call changed"
    );
    assert!(
        json_of(&plain[2]).is_some(),
        "the JSON request gave no JSON"
    );
    // SC-001: 1.3x on the app's own prompts (operator, 2026-11-04; research R11).
    assert!(gain >= 1.3, "only {gain:.2}x");
}
