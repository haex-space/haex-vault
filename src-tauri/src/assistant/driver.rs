//! The driver of the voice assistant (spec 066 T044): carries out what
//! [`conversation`](super::conversation) decides and feeds it what happens.
//!
//! [`Core`] is the part with decisions of its own, kept free of audio, models and Tauri so tests
//! drive it with a fake [`Ports`]:
//!
//! - Utterances are classified here (`phrases`: stop word, end phrase, nothing understandable).
//! - The sentences of an answer go to the speaker only while the conversation speaks the answer;
//!   muted, the window shows it instead.
//! - haex listens only once it is quiet: after its own tones and speech. Where the stream cancels
//!   haex's own output out of the microphone (PR G, FR-016), a watcher runs while haex speaks:
//!   when the user talks over it, the output stops and haex listens, starting with what was said
//!   just before. What haex hears right after such a stop is dropped when it is only what haex
//!   itself was saying (a rest of the echo).
//! - Outputs of an aborted round and utterances of a stopped listener are dropped.
//! - The assistant is switched on and off by one toggle (operator 2026-11-02). With the wake word
//!   model installed (PR D), switching on plays the mode tone and haex waits "bereit" for "Hex" /
//!   "Hey Hex"; a watcher runs on the stream while it waits, and after a conversation haex waits
//!   again. The listener that follows the wake word starts with what was said just before, so a
//!   question in the same breath arrives whole; the wake word itself is taken off its text.
//!   Without the model there is nothing to wait for: switching on starts a conversation at once
//!   (its listening tone stands for the mode tone), and once the user sends haex to pause and it
//!   is quiet, the assistant switches itself off again, without a second tone.
//! - Every utterance and every output becomes a transcript [`Note`] (T125, FR-047). Notes wait
//!   until the conversation has a thread; a conversation without one leaves none (FR-019). An
//!   answer is noted once it is over, with the full text and, when it was cut off, how far haex
//!   spoke.
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
use crate::storage::voice_transcript::{EntryKind, Outcome, Speaker};

/// What the driver does in the world.
pub trait Ports {
    /// Plays `tone` and announces it to the window (`assistant-tone`); returns how long it sounds
    /// (zero when tones are switched off).
    fn tone(&mut self, tone: ToneId) -> Duration;
    /// Starts listener `id` for utterances; it reports [`Input::Heard`] with this id.
    fn listen(&mut self, id: u64);
    /// Starts listener `id` like [`Ports::listen`], with the last moments before it first: the
    /// user talked over haex, and what they said then belongs to the utterance.
    fn listen_after_barge_in(&mut self, id: u64) {
        self.listen(id);
    }
    /// Watches for the user talking while haex speaks (FR-016); reports [`Input::BargeIn`] with
    /// `id` once and stops.
    fn watch_barge_in(&mut self, _id: u64) {}
    /// Stops the running watcher.
    fn stop_watching(&mut self) {}
    /// Waits for the wake word (FR-011); reports [`Input::WakeWord`] with `id` once and stops.
    fn watch_wake_word(&mut self, _id: u64) {}
    /// Stops the running wake word watcher.
    fn stop_wake_word(&mut self) {}
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
    /// Queues a sentence of an answer for speaking, like [`Ports::say`]; ports that measure how
    /// fast haex speaks (spec 081 FR-007) count only these, not questions or announcements.
    fn say_answer(&mut self, text: String) -> u64 {
        self.say(text)
    }
    /// Marks the end of a streamed answer after all its sentences were queued.
    fn finish_answer(&mut self) {}
    /// Silences speech at once.
    fn stop_output(&mut self);
    /// Sends the state to the window (`assistant-state`) and keeps it for `assistant_get_state`.
    fn publish(&mut self, state: &StateWire);
    /// Releases microphone and speaker.
    fn close_audio(&mut self);
    /// Writes `note` to the transcript of `thread_id`, in the order of the calls.
    fn record(&mut self, thread_id: Uuid, note: Note);
}

/// A transcript entry (`voice_transcript_entries`) as the driver sees it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Note {
    /// When the utterance or output began.
    pub at: Instant,
    pub speaker: Speaker,
    pub kind: EntryKind,
    pub text: String,
    /// Characters of `text` spoken before the output was cut off.
    pub spoken_until: Option<i64>,
    pub outcome: Option<Outcome>,
    /// The id of the question it asks or answers.
    pub ref_id: Option<String>,
}

impl Note {
    fn new(at: Instant, speaker: Speaker, kind: EntryKind, text: String) -> Self {
        Self {
            at,
            speaker,
            kind,
            text,
            spoken_until: None,
            outcome: None,
            ref_id: None,
        }
    }
}

/// The answer of a round while it is spoken: its sentences, each with its speaker ticket (0 when
/// not spoken) and where it ends in `text`.
struct Answer {
    at: Instant,
    text: String,
    sentences: Vec<(u64, usize)>,
    turn_ended: bool,
}

/// What moves the driver.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Input {
    /// The toggle is switched on; the audio is open and the models are ready.
    SwitchOn,
    /// The toggle is switched off: the conversation ends, a round is aborted, the audio closes
    /// once the mode tone has sounded.
    SwitchOff,
    /// "Gespräch beenden" (FR-039).
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
    /// Watcher `watcher` heard the user talk while haex spoke (FR-016).
    BargeIn {
        watcher: u64,
    },
    /// Wake word watcher `watcher` heard "Hex" / "Hey Hex" (FR-011).
    WakeWord {
        watcher: u64,
    },
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

/// How speech during output is taken (FR-016): `Full` where the stream cancels haex's own output
/// (any speech interrupts), `KeywordOnly` elsewhere (only the wake word, PR D, and switching off).
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
    /// The answer being spoken, noted once it is over.
    answer: Option<Answer>,
    /// Notes waiting for the conversation's thread.
    pending_notes: Vec<Note>,
    /// The thread of the conversation that just ended, for what haex says after the end.
    ended_thread: Option<Uuid>,
    /// The stream cancels haex's own output, so the user may talk over it (FR-016).
    barge_in: bool,
    /// The running barge-in watcher, 0 for none; reports of others are dropped.
    watcher: u64,
    watchers: u64,
    /// The next listener starts with what was said just before it (after a barge-in).
    preroll_next: bool,
    /// The next utterance follows a barge-in and is checked against what haex was saying.
    after_barge_in: bool,
    /// The last sentences haex queued, to tell a rest of its echo from the user.
    recent: Vec<String>,
    /// The wake word model is there: haex waits for it instead of starting a conversation at
    /// once, and stays on after a conversation.
    wake_word: bool,
    /// The running wake word watcher, 0 for none; reports of others are dropped.
    wake_watcher: u64,
    wake_watchers: u64,
    /// The next utterance follows the wake word: its text starts with it.
    after_wake: bool,
}

/// How many of haex's last sentences a barge-in is compared with.
const RECENT_SENTENCES: usize = 4;

/// Whether `heard` is only a rest of what haex was saying (`said`), not the user: at least two
/// words, and nearly all of them (four in five) words haex just said.
pub fn echoes(heard: &str, said: &[String]) -> bool {
    let words = |text: &str| -> Vec<String> {
        text.split(|c: char| !c.is_alphanumeric())
            .filter(|w| !w.is_empty())
            .map(str::to_lowercase)
            .collect()
    };
    let heard = words(heard);
    if heard.len() < 2 {
        return false;
    }
    let said: std::collections::HashSet<String> =
        said.iter().flat_map(|sentence| words(sentence)).collect();
    let known = heard.iter().filter(|w| said.contains(*w)).count();
    known * 5 >= heard.len() * 4
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
            answer: None,
            pending_notes: Vec::new(),
            ended_thread: None,
            barge_in: false,
            watcher: 0,
            watchers: 0,
            preroll_next: false,
            after_barge_in: false,
            recent: Vec::new(),
            wake_word: false,
            wake_watcher: 0,
            wake_watchers: 0,
            after_wake: false,
        }
    }

    /// Whether the stream that opens cancels haex's own output (FR-016); set before
    /// [`Input::SwitchOn`].
    pub fn set_barge_in(&mut self, on: bool) {
        self.barge_in = on;
    }

    /// Whether haex waits for the wake word (its model is installed); set before
    /// [`Input::SwitchOn`].
    pub fn set_wake_word(&mut self, on: bool) {
        self.wake_word = on;
    }

    /// The conversation machine, for the state the window asks for.
    pub fn state(&self) -> &AssistantState {
        &self.state
    }

    /// Whether microphone and speaker are open; the runtime opens them before [`Input::SwitchOn`].
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
        let waits_for_tone = self.listen_wanted
            || self.close_when_quiet
            || (self.wake_watcher == 0 && self.waits_for_wake_word());
        let tone = waits_for_tone.then_some(self.tones_until);
        match (self.state.next_deadline(), tone) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (a, b) => a.or(b),
        }
    }

    pub fn on(&mut self, input: Input, now: Instant, ports: &mut dyn Ports) {
        match input {
            Input::SwitchOn => {
                self.open = true;
                self.close_when_quiet = false;
                if self.state.mode == Mode::Off {
                    self.drive(Event::SwitchOn, now, ports);
                }
                if self.state.conversation.is_none() && !self.wake_word {
                    // A new conversation: notes of the last one are gone.
                    self.pending_notes.clear();
                    self.ended_thread = None;
                    // Without the wake word model, switching on is the wake word.
                    self.drive(Event::Wake, now, ports);
                }
            }
            Input::SwitchOff => {
                if self.open {
                    self.drive(Event::SwitchOff, now, ports);
                    self.close_when_quiet = true;
                }
            }
            Input::End => self.drive(Event::EndConversation, now, ports),
            Input::Mute(on) => self.drive(Event::Mute(on), now, ports),
            Input::Heard { listener, text } => {
                let current = listener != 0 && listener == self.listener;
                let after_barge_in = current && std::mem::take(&mut self.after_barge_in);
                let after_wake = current && std::mem::take(&mut self.after_wake);
                let text = if after_wake {
                    phrases::without_wake_word(&self.language, &text)
                } else {
                    text
                };
                if after_wake && text.is_empty() {
                    // Only the wake word, then a pause: the question comes next.
                    self.listen_wanted = true;
                } else if after_barge_in && echoes(&text, &self.recent) {
                    // A rest of haex's own voice stopped it, not the user: listen on.
                    log::info!("assistant: dropped an utterance that was haex's own echo");
                    self.listen_wanted = true;
                } else if current {
                    let utterance = self.classify(text.clone());
                    self.note_utterance(&utterance, text, now, ports);
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
                self.answer_over_if_done(ports);
                if self.played >= self.said {
                    self.drive(Event::OutputDone, now, ports);
                }
            }
            Input::BargeIn { watcher } => {
                if watcher != 0 && watcher == self.watcher {
                    self.watcher = 0;
                    self.preroll_next = true;
                    self.after_barge_in = true;
                    self.drive(Event::BargeIn, now, ports);
                }
            }
            Input::WakeWord { watcher } => {
                if watcher != 0 && watcher == self.wake_watcher {
                    self.wake_watcher = 0;
                    if self.state.conversation.is_none() {
                        self.pending_notes.clear();
                        self.ended_thread = None;
                    }
                    self.preroll_next = true;
                    self.after_wake = true;
                    self.drive(Event::Wake, now, ports);
                }
            }
            Input::Tick => self.drive(Event::Tick, now, ports),
            Input::VaultClosed => {
                self.drive(Event::VaultClosed, now, ports);
                self.close(ports);
            }
        }
        self.flush_notes(ports);
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
                let ticket = if self.speak_answer {
                    self.remember(&sentence);
                    self.said = ports.say_answer(sentence.clone());
                    self.said
                } else {
                    0
                };
                let answer = self.answer.get_or_insert_with(|| Answer {
                    at: now,
                    text: String::new(),
                    sentences: Vec::new(),
                    turn_ended: false,
                });
                if !answer.text.is_empty() {
                    answer.text.push(' ');
                }
                answer.text.push_str(&sentence);
                answer.sentences.push((ticket, answer.text.chars().count()));
            }
            Output::Event(Event::TurnEnded) => {
                ports.finish_answer();
                if let Some(answer) = &mut self.answer {
                    answer.turn_ended = true;
                }
                self.answer_over_if_done(ports);
                self.drive(Event::TurnEnded, now, ports);
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
            // Without the wake word the listening tone of the conversation that starts at once
            // says that haex is on.
            Effect::Tone(ToneId::ModeOn) if !self.wake_word => {}
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
                self.answer_over(ports);
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
                // What was said before the question is over.
                self.answer_over(ports);
                let question = self
                    .state
                    .conversation
                    .as_ref()
                    .and_then(|c| c.open_questions.iter().find(|q| q.id == id))
                    .map(|q| (q.text.clone(), q.kind));
                if let Some((text, kind)) = question {
                    self.said = ports.say(text.clone());
                    let kind = match kind {
                        QuestionKind::Approval => EntryKind::ApprovalQuestion,
                        QuestionKind::Choice | QuestionKind::Screen => EntryKind::ChoiceQuestion,
                    };
                    let mut note = Note::new(now, Speaker::Haex, kind, text);
                    note.outcome = Some(Outcome::Open);
                    note.ref_id = Some(id);
                    self.note(note, ports);
                }
            }
            Effect::AbortTurn => {
                self.answer_over(ports);
                ports.abort_turn();
                self.round += 1;
                self.speak_answer = false;
            }
            Effect::StopOutput => {
                self.answer_over(ports);
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
                    self.said = ports.say(text.clone());
                    let note = Note::new(now, Speaker::Haex, EntryKind::Announcement, text);
                    self.note(note, ports);
                }
            }
            Effect::Ended { thread_id } => {
                self.answer_over(ports);
                self.ended_thread = thread_id;
                match thread_id {
                    Some(thread_id) => {
                        for note in std::mem::take(&mut self.pending_notes) {
                            ports.record(thread_id, note);
                        }
                    }
                    // Nothing understandable was said: no thread, no transcript (FR-019).
                    None => self.pending_notes.clear(),
                }
                self.listen_wanted = false;
                self.close_when_quiet = true;
            }
        }
    }

    /// The thread notes go to: the conversation's, or the one that just ended.
    fn notes_thread(&self) -> Option<Uuid> {
        match &self.state.conversation {
            Some(c) => c.thread_id,
            None => self.ended_thread,
        }
    }

    fn note(&mut self, note: Note, ports: &mut dyn Ports) {
        match self.notes_thread() {
            Some(thread_id) => ports.record(thread_id, note),
            None => self.pending_notes.push(note),
        }
    }

    fn flush_notes(&mut self, ports: &mut dyn Ports) {
        if self.pending_notes.is_empty() {
            return;
        }
        if let Some(thread_id) = self.notes_thread() {
            for note in std::mem::take(&mut self.pending_notes) {
                ports.record(thread_id, note);
            }
        }
    }

    fn note_utterance(
        &mut self,
        utterance: &Utterance,
        text: String,
        now: Instant,
        ports: &mut dyn Ports,
    ) {
        let question = self
            .state
            .open_question()
            .filter(|_| self.state.phase() == Phase::AwaitingAnswer)
            .map(|q| (q.id.clone(), q.kind));
        let (kind, ref_id) = match (utterance, question) {
            (Utterance::Request(_), Some((id, QuestionKind::Approval))) => {
                (EntryKind::ApprovalAnswer, Some(id))
            }
            (Utterance::Request(_), Some((id, _))) => (EntryKind::ChoiceAnswer, Some(id)),
            (Utterance::Request(_), None) => (EntryKind::Utterance, None),
            (Utterance::Empty, _) => (EntryKind::NotUnderstood, None),
            (Utterance::End, _) => (EntryKind::Closing, None),
            (Utterance::Stop, _) => (EntryKind::Stop, None),
        };
        let mut note = Note::new(now, Speaker::User, kind, text);
        note.ref_id = ref_id;
        self.note(note, ports);
    }

    /// Notes the answer once its turn ended and everything spoken of it has sounded.
    fn answer_over_if_done(&mut self, ports: &mut dyn Ports) {
        let done = self.answer.as_ref().is_some_and(|a| {
            a.turn_ended
                && a.sentences
                    .iter()
                    .all(|(ticket, _)| *ticket == 0 || *ticket <= self.played)
        });
        if done {
            self.answer_over(ports);
        }
    }

    /// Notes the answer so far: its full text and, unless all of it sounded, how far haex spoke.
    fn answer_over(&mut self, ports: &mut dyn Ports) {
        let Some(answer) = self.answer.take() else {
            return;
        };
        if answer.text.is_empty() {
            return;
        }
        let played = self.played;
        let spoken = answer
            .sentences
            .iter()
            .take_while(|(ticket, _)| *ticket != 0 && *ticket <= played)
            .last()
            .map_or(0, |(_, end)| *end);
        let all = answer.sentences.last().map_or(0, |(_, end)| *end);
        let mut note = Note::new(answer.at, Speaker::Haex, EntryKind::Answer, answer.text);
        note.spoken_until = (spoken < all).then_some(spoken as i64);
        self.note(note, ports);
    }

    fn stop_listener(&mut self, ports: &mut dyn Ports) {
        if self.listener != 0 {
            ports.stop_listening();
            self.listener = 0;
        }
    }

    /// Keeps the last [`RECENT_SENTENCES`] sentences haex said (FR-016 echo check).
    fn remember(&mut self, sentence: &str) {
        self.recent.push(sentence.to_owned());
        if self.recent.len() > RECENT_SENTENCES {
            self.recent.remove(0);
        }
    }

    /// Whether haex waits for the wake word now: "bereit", or with an open question and nobody
    /// listening (FR-011, FR-040); not muted or paused.
    fn waits_for_wake_word(&self) -> bool {
        let phase = self.state.phase();
        let waiting = phase == Phase::Ready
            || (phase == Phase::AwaitingAnswer && self.listener == 0 && !self.listen_wanted);
        self.open && self.wake_word && waiting && !self.state.muted() && !self.state.paused()
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
                if std::mem::take(&mut self.preroll_next) {
                    ports.listen_after_barge_in(self.listener);
                } else {
                    ports.listen(self.listener);
                }
            } else if self.close_when_quiet
                && self.state.conversation.is_none()
                && self.wake_word
                && self.state.mode != Mode::Off
            {
                // With the wake word, haex waits for the next conversation.
                self.close_when_quiet = false;
            } else if self.close_when_quiet && self.state.conversation.is_none() {
                self.close_when_quiet = false;
                // After a pause the conversation-end tone has said it all: no mode tone too.
                for effect in self.state.handle(Event::SwitchOff, now) {
                    if effect != Effect::Tone(ToneId::ModeOff) {
                        self.apply(effect, now, ports);
                    }
                }
                self.close(ports);
            }
        }
        // While haex speaks over a stream that cancels its echo, a watcher waits for the user.
        let watch = self.open && self.barge_in && self.state.phase() == Phase::Speaking;
        if watch && self.watcher == 0 {
            self.watchers += 1;
            self.watcher = self.watchers;
            ports.watch_barge_in(self.watcher);
        } else if !watch && self.watcher != 0 {
            self.watcher = 0;
            ports.stop_watching();
        }
        // While haex is "bereit", or waits for the answer to an open question, the wake word
        // watcher runs (FR-011, FR-040).
        // It starts once haex is quiet, so it never hears haex's own tones and goodbye.
        let wake = self.waits_for_wake_word() && self.quiet(now) && !self.close_when_quiet;
        if wake && self.wake_watcher == 0 {
            self.wake_watchers += 1;
            self.wake_watcher = self.wake_watchers;
            ports.watch_wake_word(self.wake_watcher);
        } else if !self.waits_for_wake_word() && self.wake_watcher != 0 {
            self.wake_watcher = 0;
            ports.stop_wake_word();
        }
        let mut wire = StateWire::of(&self.state);
        if self.barge_in {
            wire.echo_mode = EchoMode::Full;
        }
        if self.published.as_ref() != Some(&wire) {
            ports.publish(&wire);
            self.published = Some(wire);
        }
    }

    fn close(&mut self, ports: &mut dyn Ports) {
        self.stop_listener(ports);
        if self.watcher != 0 {
            self.watcher = 0;
            ports.stop_watching();
        }
        if self.wake_watcher != 0 {
            self.wake_watcher = 0;
            ports.stop_wake_word();
        }
        self.listen_wanted = false;
        self.close_when_quiet = false;
        self.speak_answer = false;
        if self.open {
            self.open = false;
            ports.close_audio();
        }
    }
}
