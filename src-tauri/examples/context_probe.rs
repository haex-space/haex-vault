//! The measurement of spec 086 (research R9): what a long context takes on this machine and what a smaller
//! KV cache or YaRN costs in answers. One JSON line per run on stdout, the rest on stderr.
//!
//! ```text
//! cargo run --release --manifest-path src-tauri/Cargo.toml --features llm-llamacpp-cuda \
//!   --example context_probe -- <model.gguf> [--cache standard|q8|q4] [--context auto|<tokens>] \
//!   [--fill <tokens>] [--needle-runs <n>] [--no-needle]
//! ```
//!
//! - `load`: free device memory before and after the weights, and what the planner decided.
//! - `speed`: a prompt of about 4k tokens, then one that fills the context (`--fill`, the context less 1024
//!   by default): seconds to the first word, tokens per second afterwards, and the least free device memory
//!   seen while the run was going (a sampler thread looks every 200 ms).
//! - `needle`: a code word hidden at 10 %, 50 % and 90 % of the prompt and asked for at its end, greedy,
//!   `--needle-runs` words at each depth (default 1).
//!
//! Every run builds a full prompt, so `--fill 131072` takes minutes per run on a card.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use holzi_lib::llm::llamacpp::choice::{CacheType, ContextChoice};
use holzi_lib::llm::llamacpp::generate::{generate, Job};
use holzi_lib::llm::llamacpp::model::{free_memory, LoadOptions, LoadedModel};
use serde_json::json;
use tokio_util::sync::CancellationToken;

/// Characters one token is taken to be when a prompt of a size is built; the real count is reported.
const CHARS_PER_TOKEN: usize = 4;
const SHORT_PROMPT_TOKENS: usize = 4096;
const WORDS: [&str; 6] = [
    "Kolibri", "Basalt", "Marzipan", "Lorbeer", "Quarz", "Sextant",
];

struct Args {
    model: PathBuf,
    cache: CacheType,
    choice: ContextChoice,
    fill: Option<usize>,
    needle_runs: usize,
    needle: bool,
}

fn parse() -> Args {
    let mut args = std::env::args().skip(1);
    let model = PathBuf::from(args.next().expect("the path of a .gguf file"));
    let mut parsed = Args {
        model,
        cache: CacheType::Standard,
        choice: ContextChoice::Auto,
        fill: None,
        needle_runs: 1,
        needle: true,
    };
    while let Some(flag) = args.next() {
        let mut value = || {
            args.next()
                .unwrap_or_else(|| panic!("{flag} needs a value"))
        };
        match flag.as_str() {
            "--cache" => {
                parsed.cache = match value().as_str() {
                    "standard" => CacheType::Standard,
                    "q8" => CacheType::Q8,
                    "q4" => CacheType::Q4,
                    other => panic!("unknown cache type {other}"),
                }
            }
            "--context" => {
                parsed.choice = match value().as_str() {
                    "auto" => ContextChoice::Auto,
                    tokens => ContextChoice::Fixed(tokens.parse().expect("a number of tokens")),
                }
            }
            "--fill" => parsed.fill = Some(value().parse().expect("a number of tokens")),
            "--needle-runs" => parsed.needle_runs = value().parse().expect("a number"),
            "--no-needle" => parsed.needle = false,
            other => panic!("unknown argument {other}"),
        }
    }
    parsed
}

fn mib(bytes: u64) -> u64 {
    bytes >> 20
}

/// Free device memory now, in bytes; 0 without a GPU.
fn device_free() -> u64 {
    free_memory().device.unwrap_or(0)
}

/// Filler of about `chars` characters: numbered sentences, so the text never repeats and a model cannot
/// skim it.
fn filler(chars: usize, from: usize) -> String {
    let mut text = String::with_capacity(chars + 80);
    let mut n = from;
    while text.len() < chars {
        text.push_str(&format!(
            "Note {n}: the shelf in room {} holds {} boxes of paper, and nobody has moved them since week {}.\n",
            n % 97,
            n % 13 + 2,
            n % 52 + 1
        ));
        n += 1;
    }
    text
}

struct Run {
    prompt_tokens: usize,
    new_tokens: usize,
    seconds_to_first: f64,
    tokens_per_second: f64,
    least_free_mib: u64,
    answer: String,
}

/// One generation of `body` through the model's own template, with the memory sampler running.
fn run(model: &LoadedModel, body: &str, max_new: usize) -> Run {
    let prompt = model.render_user_message(body).expect("render the prompt");
    let least = Arc::new(AtomicU64::new(u64::MAX));
    let stop = Arc::new(AtomicBool::new(false));
    let sampler = {
        let (least, stop) = (Arc::clone(&least), Arc::clone(&stop));
        std::thread::spawn(move || {
            while !stop.load(Ordering::Relaxed) {
                least.fetch_min(device_free(), Ordering::Relaxed);
                std::thread::sleep(Duration::from_millis(200));
            }
        })
    };
    let started = Instant::now();
    let mut first = None;
    let mut answer = String::new();
    let outcome = generate(
        model,
        &Job {
            prompt: &prompt,
            max_new_tokens: max_new,
            deterministic: true,
        },
        &CancellationToken::new(),
        |piece| {
            first.get_or_insert_with(|| started.elapsed());
            answer.push_str(piece);
            true
        },
    );
    // The time of the generation is taken before the sampler is joined: it may sleep for 200 ms.
    let total = started.elapsed();
    stop.store(true, Ordering::Relaxed);
    sampler.join().ok();
    let outcome = outcome.unwrap_or_else(|e| panic!("generation failed: {e}"));
    let first = first.unwrap_or(total);
    let after = total.saturating_sub(first).as_secs_f64();
    Run {
        prompt_tokens: outcome.prompt_tokens,
        new_tokens: outcome.new_tokens,
        seconds_to_first: first.as_secs_f64(),
        tokens_per_second: if after > 0.0 {
            outcome.new_tokens.saturating_sub(1) as f64 / after
        } else {
            0.0
        },
        least_free_mib: mib(least.load(Ordering::Relaxed)),
        answer,
    }
}

fn speed_line(kind: &str, requested: usize, run: &Run) -> serde_json::Value {
    json!({
        "kind": kind,
        "requestedTokens": requested,
        "promptTokens": run.prompt_tokens,
        "newTokens": run.new_tokens,
        "secondsToFirstWord": run.seconds_to_first,
        "tokensPerSecond": run.tokens_per_second,
        "leastFreeDeviceMiB": run.least_free_mib,
    })
}

fn main() {
    let args = parse();
    let before = device_free();
    let started = Instant::now();
    let model = LoadedModel::load(
        &args.model,
        LoadOptions {
            choice: args.choice,
            cache: args.cache,
            ..LoadOptions::default()
        },
    )
    .unwrap_or_else(|e| panic!("load failed: {e}"));
    let placement = model.placement();
    println!(
        "{}",
        json!({
            "kind": "load",
            "model": args.model.file_name().map(|n| n.to_string_lossy().into_owned()),
            "cache": placement.cache.name(),
            "context": placement.context,
            "gpuLayers": placement.gpu_layers,
            "yarnFrom": placement.yarn_from,
            "drafter": placement.drafter,
            "freeDeviceMiBBefore": mib(before),
            "freeDeviceMiBAfterWeights": mib(device_free()),
            "loadSeconds": started.elapsed().as_secs_f64(),
        })
    );

    let fill = args
        .fill
        .unwrap_or_else(|| (placement.context as usize).saturating_sub(1024))
        .min((placement.context as usize).saturating_sub(1024));
    for tokens in [SHORT_PROMPT_TOKENS.min(fill), fill] {
        eprintln!("speed: a prompt of about {tokens} tokens");
        let body = format!(
            "{}\nHow many notes are there? Answer briefly.",
            filler(tokens * CHARS_PER_TOKEN, 0)
        );
        println!("{}", speed_line("speed", tokens, &run(&model, &body, 64)));
        if tokens == fill {
            break;
        }
    }

    if !args.needle {
        return;
    }
    for depth in [10usize, 50, 90] {
        for round in 0..args.needle_runs {
            let word = WORDS[(depth / 10 + round) % WORDS.len()];
            let chars = fill * CHARS_PER_TOKEN;
            let at = chars * depth / 100;
            let body = format!(
                "{}\nThe secret code word is {word}.\n{}\nWhat is the secret code word? Answer with the word only.",
                filler(at, 0),
                filler(chars - at, 100_000)
            );
            eprintln!("needle: {word} at {depth} %");
            let found = run(&model, &body, 128);
            println!(
                "{}",
                json!({
                    "kind": "needle",
                    "depthPercent": depth,
                    "word": word,
                    "found": found.answer.contains(word),
                    "promptTokens": found.prompt_tokens,
                    "answer": found.answer.chars().take(120).collect::<String>(),
                })
            );
        }
    }
}
