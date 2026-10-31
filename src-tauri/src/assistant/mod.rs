//! The voice assistant (spec 066): wake word, spoken conversation and speech output over the
//! built-in agent. Gated by the `voice` feature like `audio`.

pub mod assets;
#[cfg(test)]
mod assets_tests;
pub mod listen;
#[cfg(test)]
mod listen_tests;
pub mod tones;
#[cfg(test)]
mod tones_tests;
pub mod vad;
#[cfg(test)]
mod vad_tests;
