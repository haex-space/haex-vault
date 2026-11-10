//! The voices of other languages for passages of an answer (spec 066 FR-022d): the speech model
//! and voice chosen for a language on this device, loaded the first time a passage needs them and
//! kept for the conversation or reading. Nothing is downloaded; a language without a speech model
//! is spoken with the sentence's own voice.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, PoisonError};

use tauri::{AppHandle, Manager};

use crate::error::Result;
use crate::state::AppState;

use super::check::{Level, SpeechShare};
use super::local::{LanguageVoice, LanguageVoices};
use super::{commands, voices};

/// The voices of this device's settings, by language.
pub struct AppVoices {
    app: AppHandle,
    loaded: Mutex<HashMap<String, Option<Arc<LanguageVoice>>>>,
}

impl AppVoices {
    pub fn new(app: AppHandle) -> Arc<Self> {
        Arc::new(Self {
            app,
            loaded: Mutex::new(HashMap::new()),
        })
    }
}

impl LanguageVoices for AppVoices {
    fn voice(&self, language: &str) -> Option<Arc<LanguageVoice>> {
        if let Some(known) = self
            .loaded
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(language)
        {
            return known.clone();
        }
        // Called on the speaker's own thread, never inside the async runtime.
        let voice = match tauri::async_runtime::block_on(load(&self.app, language)) {
            Ok(voice) => Some(Arc::new(voice)),
            Err(error) => {
                log::info!("speech: no voice for a passage in {language}: {error}");
                None
            }
        };
        self.loaded
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(language.to_owned(), voice.clone());
        voice
    }
}

async fn load(app: &AppHandle, language: &str) -> Result<LanguageVoice> {
    let state = app.state::<AppState>();
    let engines = commands::load_installed_speech_engines(app, state.gate(), language).await?;
    let (preference, _) = commands::voice_settings(app, language).await?;
    let own = commands::own_recording(&state, preference.as_deref()).await?;
    let reference = voices::choose(preference.as_deref(), language, own)?.reference()?;
    let speech = engines
        .engine
        .part_words()
        .map(|_| Arc::new(Level) as Arc<dyn SpeechShare>);
    Ok(LanguageVoice {
        engine: engines.engine,
        reserve: engines.reserve,
        reference: Arc::new(reference),
        speech,
    })
}
