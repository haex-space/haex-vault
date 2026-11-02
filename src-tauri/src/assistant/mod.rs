//! The voice assistant (spec 066): wake word, spoken conversation and speech output over the
//! built-in agent. Gated by the `voice` feature like `audio`.

pub mod assets;
#[cfg(test)]
mod assets_tests;
pub mod commands;
pub mod conversation;
#[cfg(test)]
mod conversation_tests;
pub mod driver;
#[cfg(test)]
mod driver_tests;
pub mod lexicon;
#[cfg(test)]
mod lexicon_tests;
pub mod listen;
#[cfg(test)]
mod listen_tests;
pub mod phrases;
#[cfg(test)]
mod phrases_tests;
pub mod sentences;
#[cfg(test)]
mod sentences_tests;
pub mod speakable;
#[cfg(test)]
mod speakable_tests;
pub mod tones;
#[cfg(test)]
mod tones_tests;
pub mod turn;
#[cfg(test)]
mod turn_tests;
pub mod vad;
#[cfg(test)]
mod vad_tests;

pub use conversation::AssistantState;
