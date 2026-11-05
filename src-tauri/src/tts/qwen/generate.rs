//! One part of a sentence through Qwen3-TTS (spec 081 research R1, R4, R6): read the text, then
//! generate frame by frame until the model stops, the caller cancels or the frame cap is reached.

use std::time::{Duration, Instant};

use super::super::TtsError;
use super::ffi::{Handles, Input, Sampler, VoiceBitmap};

/// Qwen3-TTS (12 Hz) makes 12.5 frames per second of speech.
pub const FRAMES_PER_SECOND: f64 = 12.5;

/// Prompt tokens read per step.
const PROMPT_BATCH: i32 = 512;

/// The most frames a part of `chars` characters may take before it counts as running on: the
/// longest a part may be by `tts::check` (research R6).
pub fn frame_cap(chars: usize) -> usize {
    (super::super::check::max_secs(chars) * FRAMES_PER_SECOND).ceil() as usize
}

/// How one part ended.
#[derive(Debug, Clone, PartialEq)]
pub struct Generated {
    /// PCM at the model's sample rate; empty when cancelled.
    pub samples: Vec<f32>,
    /// Whether the frame cap stopped the model before it ended by itself.
    pub hit_cap: bool,
    /// Computing time of the part, prompt included.
    pub took: Duration,
}

/// A seed that depends only on `text`, so the same part sounds the same every time (research
/// R4); a retry passes another one.
pub fn seed_for(text: &str) -> u32 {
    // FNV-1a, folded to 32 bits.
    let hash = text.bytes().fold(0xcbf2_9ce4_8422_2325u64, |hash, byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3)
    });
    (hash ^ (hash >> 32)) as u32
}

/// Speaks `text` in `language` with `voice`.
pub fn part(
    handles: &mut Handles,
    text: &str,
    language: &str,
    voice: &VoiceBitmap,
    seed: u32,
    cancelled: &dyn Fn() -> bool,
) -> Result<Generated, TtsError> {
    let started = Instant::now();
    let cap = frame_cap(text.chars().count());
    handles.reset();
    handles.set_input(&Input {
        text,
        language,
        voice,
        seed,
    })?;
    while !handles.step_prompt(PROMPT_BATCH)? {
        if cancelled() {
            return Ok(cancelled_part(started));
        }
    }
    let mut sampler = Sampler::new(seed)?;
    let mut state = handles.first_state()?;
    let mut frames = 0;
    let mut hit_cap = false;
    loop {
        if cancelled() {
            return Ok(cancelled_part(started));
        }
        if frames >= cap {
            hit_cap = true;
            break;
        }
        let token = handles.sample(&mut sampler);
        match handles.step_frame(token, state)? {
            Some(next) => {
                state = next;
                frames += 1;
            }
            None => break,
        }
    }
    Ok(Generated {
        samples: handles.output()?,
        hit_cap,
        took: started.elapsed(),
    })
}

fn cancelled_part(started: Instant) -> Generated {
    Generated {
        samples: Vec::new(),
        hit_cap: false,
        took: started.elapsed(),
    }
}
