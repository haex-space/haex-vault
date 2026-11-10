//! The voice assistant (spec 066): wake word, spoken conversation and speech output over the
//! built-in agent. Gated by the `voice` feature like `audio`.

pub mod assets;
#[cfg(test)]
mod assets_tests;
pub mod calendar_words;
pub mod commands;
#[cfg(test)]
mod commands_tests;
pub mod conversation;
#[cfg(test)]
mod conversation_tests;
pub mod custom_tones;
#[cfg(test)]
mod custom_tones_tests;
pub mod driver;
#[cfg(test)]
mod driver_tests;
pub mod lexicon;
#[cfg(test)]
mod lexicon_tests;
pub mod listen;
#[cfg(test)]
mod listen_tests;
pub mod number_words;
pub mod passage;
#[cfg(test)]
mod passage_tests;
pub mod phrases;
#[cfg(test)]
mod phrases_tests;
pub mod pinned;
pub mod sentences;
#[cfg(test)]
mod sentences_tests;
pub mod speakable;
#[cfg(test)]
mod speakable_tests;
pub mod speakers;
#[cfg(test)]
mod speakers_tests;
pub mod tones;
#[cfg(test)]
mod tones_tests;
pub mod transcript;
pub mod turn;
#[cfg(test)]
mod turn_tests;
pub mod vad;
#[cfg(test)]
mod vad_tests;
pub mod wakeword;

pub use conversation::AssistantState;
