//! The driver of the voice assistant (spec 066 T044): carries out what
//! [`conversation`](super::conversation) decides and feeds it what happens.
//!
//! [`Core`] is the part with decisions of its own, kept free of audio, models and Tauri so tests
//! drive it with a fake [`Ports`]:
//!
//! - Utterances are classified here (`phrases`: stop word, end phrase, nothing understandable).
//! - The sentences of an answer go to the speaker only while the conversation speaks the answer;
//!   muted, the window shows it instead.
//! - haex listens only once it is quiet: after its own tones and speech. Until echo suppression
//!   (PR G) the microphone would otherwise hear haex itself.
//! - Outputs of an aborted round and utterances of a stopped listener are dropped.
//! - Until the wake word (PR D), the microphone is open only during a conversation: "Sprechen"
//!   opens it, and once the conversation is over and haex is quiet, it closes again. The mode
//!   tones stay silent for that, since the user never switched a mode.
//!
//! The runtime ([`super::commands`]) owns a `Core`, implements `Ports` with the audio stream,
//! the speaker, the listener and `assistant::turn`, and sends [`Input::Tick`] at
//! [`Core::next_wakeup`].

use std::time::{Duration, Instant};

use serde::Serialize;
use uuid::Uuid;

use super::conversation::{
    AssistantState, Effect, Event, Line, Mode, Phase, QuestionKind, Timing, Utterance,
};
use super::phrases;
use super::tones::ToneId;
use super::turn::Output;

/// What the driver does in the world.
pub trait Ports {
    /// Plays `tone` and announces it to the window (`assistant-tone`); returns how long it sounds
    /// (zero when tones are switched off).
    fn tone(&mut self, tone: ToneId) -> Duration;
    /// Starts listener `id` for utterances; it reports [`Input::Heard`] with this id.
    fn listen(&mut self, id: u64);
    /// Stops the running listener.
    fn stop_listening(&mut self);
    /// Starts round `round` through `assistant::turn`; it reports [`Input::Turn`] and, when it
    /// cannot start, [`Input::TurnFailed`] with this round.
    fn start_turn(
        &mut self,
        round: u64,
        thread_id: Option<Uuid>,
        text: String,
        delegate_notice_due: bool,
    );
    /// Aborts the running round with its tool calls.
    fn abort_turn(&mut self);
    /// Queues `text` for speaking; returns its ticket. [`Input::Played`] reports it once it has
    /// sounded or was stopped, tickets in order.
    fn say(&mut self, text: String) -> u64;
    /// Silences speech at once.
    fn stop_output(&mut self);
    /// Sends the state to the window (`assistant-state`) and keeps it for `assistant_get_state`.
    fn publish(&mut self, state: &StateWire);
    /// Releases microphone and speaker.
    fn close_audio(&mut self);
}

/// What moves the driver.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Input {
    /// "Sprechen" in the indicator or the shortcut; the audio is open and the models are ready.
    Talk,
    /// "Gespräch beenden".
    End,
    Mute(bool),
    /// Listener `listener` heard `text` (empty when nothing was understood).
    Heard {
        listener: u64,
        text: String,
    },
    /// Round `round` handed on `output`.
    Turn {
        round: u64,
        output: Output,
    },
    /// Round `round` could not start (no model, an error before the turn ran).
    TurnFailed {
        round: u64,
    },
    /// The sentence with this ticket is over.
    Played(u64),
    /// Time passed; check timers and whether haex is quiet.
    Tick,
    /// The vault locks or closes.
    VaultClosed,
}

/// `AssistantStateWire` of contracts/tauri-commands.md.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StateWire {
    pub enabled: bool,
    pub muted: bool,
    pub paused: bool,
    pub phase: Phase,
    pub thread_id: Option<Uuid>,
    pub external_tts: bool,
    pub external_stt: bool,
    pub echo_mode: EchoMode,
    pub open_question: Option<QuestionWire>,
}

/// How speech during output is taken (FR-016). Until PR G only "Sprechen" interrupts haex.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum EchoMode {
    Full,
    KeywordOnly,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct QuestionWire {
    pub kind: QuestionKind,
    pub text: String,
}

impl StateWire {
    pub fn of(state: &AssistantState) -> Self {
        Self {
            enabled: state.mode != Mode::Off,
            muted: state.muted(),
            paused: state.paused(),
            phase: state.phase(),
            thread_id: state.conversation.as_ref().and_then(|c| c.thread_id),
            external_tts: false,
            external_stt: false,
            echo_mode: EchoMode::KeywordOnly,
            open_question: state.open_question().map(|q| QuestionWire {
                kind: q.kind,
                text: q.text.clone(),
            }),
        }
    }

    /// The state before the assistant ran at all.
    pub fn off() -> Self {
        Self::of(&AssistantState::new(Timing::default()))
    }
}

/// The driver's own state around the conversation machine.
pub struct Core {
    state: AssistantState,
    language: String,
    /// Microphone and speaker are open.
    open: bool,
    /// The current round; outputs of others are dropped.
    round: u64,
    /// The running listener, 0 for none; utterances of others are dropped.
    listener: u64,
    listeners: u64,
    /// The conversation wants to listen; starts once haex is quiet.
    listen_wanted: bool,
    /// Sentences of the answer go to the speaker.
    speak_answer: bool,
    /// The last ticket handed out and the last one reported over.
    said: u64,
    played: u64,
    /// When the last tone has sounded.
    tones_until: Instant,
    /// The conversation ended; close the audio once haex is quiet.
    close_when_quiet: bool,
    published: Option<StateWire>,
}

impl Core {
    pub fn new(language: &str, timing: Timing, now: Instant) -> Self {
        Self {
            state: AssistantState::new(timing),
            language: language.to_owned(),
            open: false,
            round: 0,
            listener: 0,
            listeners: 0,
            listen_wanted: false,
            speak_answer: false,
            said: 0,
            played: 0,
            tones_until: now,
            close_when_quiet: false,
            published: None,
        }
    }

    /// The conversation machine, for the state the window asks for.
    pub fn state(&self) -> &AssistantState {
        &self.state
    }

    /// Whether microphone and speaker are open; the runtime opens them before [`Input::Talk`].
    pub fn is_open(&self) -> bool {
        self.open
    }

    /// Before audio opens for a new conversation: the language of the window and the windows of
    /// the device settings. A fresh speaker starts its tickets at 1 again.
    pub fn prepare(&mut self, language: &str, timing: Timing) {
        self.language = language.to_owned();
        self.state.timing = timing;
        self.said = 0;
        self.played = 0;
    }

    /// When the runtime has to send [`Input::Tick`]: a timer of the conversation, or the end of a
    /// tone that listening or closing waits for.
    pub fn next_wakeup(&self) -> Option<Instant> {
        let waits_for_tone = self.listen_wanted || self.close_when_quiet;
        let tone = waits_for_tone.then_some(self.tones_until);
        match (self.state.next_deadline(), tone) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (a, b) => a.or(b),
        }
    }

    pub fn on(&mut self, input: Input, now: Instant, ports: &mut dyn Ports) {
        match input {
            Input::Talk => {
                if !self.open {
                    self.open = true;
                    self.drive(Event::SwitchOn, now, ports);
                }
                self.close_when_quiet = false;
                self.drive(Event::Wake, now, ports);
            }
            Input::End => self.drive(Event::EndConversation, now, ports),
            Input::Mute(on) => self.drive(Event::Mute(on), now, ports),
            Input::Heard { listener, text } => {
                if listener != 0 && listener == self.listener {
                    let utterance = self.classify(text);
                    self.drive(Event::Utterance(utterance), now, ports);
                }
            }
            Input::Turn { round, output } => {
                if round == self.round {
                    self.turn_output(output, now, ports);
                }
            }
            Input::TurnFailed { round } => {
                if round == self.round {
                    let length = ports.tone(ToneId::Error);
                    self.tones_until = self.tones_until.max(now) + length;
                    self.drive(Event::TurnEnded, now, ports);
                }
            }
            Input::Played(ticket) => {
                self.played = self.played.max(ticket);
                if self.played >= self.said {
                    self.drive(Event::OutputDone, now, ports);
                }
            }
            Input::Tick => self.drive(Event::Tick, now, ports),
            Input::VaultClosed => {
                self.drive(Event::VaultClosed, now, ports);
                self.close(ports);
            }
        }
        self.settle(now, ports);
    }

    fn classify(&self, text: String) -> Utterance {
        if phrases::stop(&text).is_some() {
            Utterance::Stop
        } else if phrases::is_end(&self.language, &text) {
            Utterance::End
        } else if phrases::is_empty(&self.language, &text) {
            Utterance::Empty
        } else {
            Utterance::Request(text)
        }
    }

    fn turn_output(&mut self, output: Output, now: Instant, ports: &mut dyn Ports) {
        match output {
            Output::Say(sentence) => {
                if self.speak_answer {
                    self.said = ports.say(sentence);
                }
            }
            Output::Event(event) => self.drive(event, now, ports),
            Output::SpokeDelegateNotice => {
                if let Some(c) = &mut self.state.conversation {
                    c.spoke_delegate_notice = true;
                }
            }
        }
    }

    /// Hands `event` to the conversation and carries out what it decides.
    fn drive(&mut self, event: Event, now: Instant, ports: &mut dyn Ports) {
        for effect in self.state.handle(event, now) {
            self.apply(effect, now, ports);
        }
    }

    fn apply(&mut self, effect: Effect, now: Instant, ports: &mut dyn Ports) {
        match effect {
            // Nobody switched a mode before the wake word arrives (PR D).
            Effect::Tone(ToneId::ModeOn | ToneId::ModeOff) => {}
            Effect::Tone(tone) => {
                let length = ports.tone(tone);
                self.tones_until = self.tones_until.max(now) + length;
            }
            Effect::Listen => {
                // A listener running now would hear what haex says next.
                self.stop_listener(ports);
                self.listen_wanted = true;
            }
            Effect::StopListening => {
                self.listen_wanted = false;
                self.stop_listener(ports);
            }
            Effect::StartTurn { thread_id, text } => {
                self.round += 1;
                self.speak_answer = false;
                let due = self
                    .state
                    .conversation
                    .as_ref()
                    .is_none_or(|c| !c.spoke_delegate_notice);
                ports.start_turn(self.round, thread_id, text, due);
            }
            // Spoken yes, no and choices come with PR E; until then the window answers.
            Effect::AnswerQuestion { id, .. } => {
                log::info!("assistant: spoken answer to question {id} is left to the window");
            }
            Effect::AskQuestion(id) => {
                let text = self
                    .state
                    .conversation
                    .as_ref()
                    .and_then(|c| c.open_questions.iter().find(|q| q.id == id))
                    .map(|q| q.text.clone());
                if let Some(text) = text {
                    self.said = ports.say(text);
                }
            }
            Effect::AbortTurn => {
                ports.abort_turn();
                self.round += 1;
                self.speak_answer = false;
            }
            Effect::StopOutput => {
                ports.stop_output();
                self.speak_answer = false;
            }
            Effect::SpeakAnswer => self.speak_answer = true,
            Effect::ShowAnswer => self.speak_answer = false,
            Effect::Say(line) => {
                // The progress announcement and the summary after an active end come with PR E.
                let text = match line {
                    Line::Repeat => Some(phrases::repeat(&self.language)),
                    Line::Goodbye => Some(phrases::goodbye(&self.language)),
                    Line::Progress | Line::DoneSoFar => None,
                };
                if let Some(text) = text {
                    self.said = ports.say(text);
                }
            }
            Effect::Ended { .. } => {
                self.listen_wanted = false;
                self.close_when_quiet = true;
            }
        }
    }

    fn stop_listener(&mut self, ports: &mut dyn Ports) {
        if self.listener != 0 {
            ports.stop_listening();
            self.listener = 0;
        }
    }

    fn quiet(&self, now: Instant) -> bool {
        self.played >= self.said && now >= self.tones_until
    }

    /// Starts listening or closes the audio once haex is quiet, and publishes a changed state.
    fn settle(&mut self, now: Instant, ports: &mut dyn Ports) {
        if self.open && self.quiet(now) {
            if self.listen_wanted {
                self.listen_wanted = false;
                self.listeners += 1;
                self.listener = self.listeners;
                ports.listen(self.listener);
            } else if self.close_when_quiet && self.state.conversation.is_none() {
                self.close_when_quiet = false;
                self.drive(Event::SwitchOff, now, ports);
                self.close(ports);
            }
        }
        let wire = StateWire::of(&self.state);
        if self.published.as_ref() != Some(&wire) {
            ports.publish(&wire);
            self.published = Some(wire);
        }
    }

    fn close(&mut self, ports: &mut dyn Ports) {
        self.stop_listener(ports);
        self.listen_wanted = false;
        self.close_when_quiet = false;
        self.speak_answer = false;
        if self.open {
            self.open = false;
            ports.close_audio();
        }
    }
}
