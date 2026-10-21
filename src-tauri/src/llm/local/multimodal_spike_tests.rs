//! Spike for spec 055 research R8 (T057): does Qwen3-VL-4B run in holzi's process through
//! mistral.rs with 4-bit in-situ quantisation (8-bit with `HOLZI_SPIKE_ISQ_BITS=8`) on the development
//! card (8 GB), and does it read a page? It records the time to load, the time for the page and the
//! peak VRAM, for research.md R8.
//!
//! Ignored: it needs the model in the HuggingFace cache, a CUDA card with nothing else on it (close
//! the holzi dev app) and a photo of a page. Run memory-limited:
//!
//! ```text
//! HOLZI_SPIKE_PAGE=<photo> [HOLZI_SPIKE_ISQ_BITS=8] systemd-run --user --scope -p MemoryMax=12G \
//!   cargo test --release --features llm-cuda --lib multimodal_spike -- --ignored --nocapture
//! ```

use std::process::Command;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use mistralrs::{IsqBits, MultimodalModelBuilder, RequestBuilder, TextMessageRole};

const MODEL: &str = "Qwen/Qwen3-VL-4B-Instruct";

/// The prompt of the Python measurement (transformers, NF4), so both runs read the same task. That run
/// decoded greedily with at most 3000 new tokens; this one does the same.
const PROMPT: &str = r#"This is a photo of a student's handwritten notes or a blackboard picture, in German.
Split the page into regions and return ONLY a JSON object:
{"regions": [{"type": "text" | "table" | "image", "bbox": [x1, y1, x2, y2], "text": "...", "rows": [["cell", ...], ...]}]}
Rules:
- bbox uses coordinates 0-1000 relative to the image width and height.
- "text": transcribe the handwriting exactly as written, keep line breaks, do not correct anything.
- "table": give "rows" as a list of rows of cell strings, no "text".
- "image": drawings, diagrams, sketches and photos, no "text".
- List regions in reading order. Do not invent content that is not on the page."#;

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

/// Samples the VRAM in use every 200 ms until stopped and keeps the highest value.
fn watch_peak() -> (Arc<AtomicBool>, Arc<AtomicU64>, std::thread::JoinHandle<()>) {
    let stop = Arc::new(AtomicBool::new(false));
    let peak = Arc::new(AtomicU64::new(0));
    let (s, p) = (Arc::clone(&stop), Arc::clone(&peak));
    let handle = std::thread::spawn(move || {
        while !s.load(Ordering::Relaxed) {
            if let Some(used) = vram_used_mib() {
                p.fetch_max(used, Ordering::Relaxed);
            }
            std::thread::sleep(Duration::from_millis(200));
        }
    });
    (stop, peak, handle)
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs Qwen3-VL-4B, a CUDA card and HOLZI_SPIKE_PAGE"]
async fn multimodal_spike_qwen3_vl_4b_reads_a_page_with_4bit_isq() {
    let page = std::env::var("HOLZI_SPIKE_PAGE").expect("HOLZI_SPIKE_PAGE names a photo");
    let bits = match std::env::var("HOLZI_SPIKE_ISQ_BITS").as_deref() {
        Ok("8") => IsqBits::Eight,
        _ => IsqBits::Four,
    };
    let before = vram_used_mib();
    let (stop, peak, watcher) = watch_peak();

    let started = Instant::now();
    let model = MultimodalModelBuilder::new(MODEL)
        .with_auto_isq(bits)
        .with_max_num_seqs(1)
        .with_logging()
        .build()
        .await
        .expect("the model loads");
    let loaded = started.elapsed();
    let after_load = vram_used_mib();

    let image = image::open(&page).expect("the photo opens");
    let request = RequestBuilder::new()
        .add_image_message(TextMessageRole::User, PROMPT, vec![image])
        .set_deterministic_sampler()
        .set_sampler_max_len(3000);
    let asked = Instant::now();
    let outcome = model.send_chat_request(request).await;
    let answered = asked.elapsed();

    stop.store(true, Ordering::Relaxed);
    watcher.join().expect("watcher");
    println!(
        "vram before {before:?} MiB, after load {after_load:?} MiB, peak {} MiB",
        peak.load(Ordering::Relaxed)
    );
    let response = outcome.unwrap_or_else(|error| {
        let error: String = error.to_string().chars().take(400).collect();
        panic!(
            "the model failed after {:.1} s: {error}",
            answered.as_secs_f64()
        )
    });
    let text = response.choices[0]
        .message
        .content
        .clone()
        .unwrap_or_default();
    println!("--- answer ---\n{text}\n--- end ---");
    println!(
        "load {:.1} s, page {:.1} s, prompt {} tok ({:.1} tok/s), answer {} tok ({:.1} tok/s)",
        loaded.as_secs_f64(),
        answered.as_secs_f64(),
        response.usage.prompt_tokens,
        response.usage.avg_prompt_tok_per_sec,
        response.usage.completion_tokens,
        response.usage.avg_compl_tok_per_sec,
    );
    assert!(!text.trim().is_empty(), "the model read nothing");
}
