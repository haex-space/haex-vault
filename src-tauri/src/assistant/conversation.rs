//! The conversation state machine of the voice assistant (spec 066 T040, data-model.md "Phasen",
//! research R12).
//!
//! Pure logic: every input is an [`Event`] with the time it happened, every output is a list of
//! [`Effect`]s the caller carries out (tones, listening, turns, output). Nothing here touches audio,
//! the model or the database, and the clock is whatever the caller passes in, so the tests drive
//! every transition deterministically.
//!
//! What the caller decides before an event reaches this machine: whether an utterance was empty,
//! an end phrase or a stop word (`assistant::phrases`), and whether speech during output counts as
//! barging in (echo mode, FR-016). The machine decides what follows: phase, timers, whether the
//! conversation goes on, ends, or continues an earlier thread.
//!
//! Not here yet: the secret that was read aloud (`spoken_secret`, PR F) and the echo mode
//! (`echo_mode`, PR G) of data-model.md's `AssistantState`.

use std::collections::VecDeque;
use std::time::{Duration, Instant};

use serde::Serialize;
use uuid::Uuid;

use super::tones::ToneId;

/// A round that has not started speaking after this long counts as working and gets a short
/// announcement (FR-023).
pub const WORKING_AFTER: Duration = Duration::from_secs(4);

/// Device preference: continuation window in minutes (FR-015, data-model.md "int 0–60", default 5).
pub const PREF_CONTINUE_MINS: &str = "assistant.continueMins";

const CONTINUE_MINS: (i64, i64, i64) = (0, 60, 5);

/// The phase shown by the indicator and sent as `phase` of `assistant-state`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Phase {
    Off,
    Ready,
    Listening,
    Thinking,
    Working,
    Speaking,
    FollowUp,
    AwaitingAnswer,
}

/// The continuation window of the conversation, read from the device preferences. A conversation
/// itself has no time limit: haex listens until the user sends it to pause (operator 2026-11-02).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Timing {
    /// How long after its end a wake word continues the same conversation (FR-015); zero never.
    pub continue_for: Duration,
}

impl Default for Timing {
    fn default() -> Self {
        Self::from_prefs(None)
    }
}

impl Timing {
    /// Builds the window from the stored value; a missing value takes the default, a value out of
    /// range is clamped to it.
    pub fn from_prefs(continue_mins: Option<i64>) -> Self {
        let (min, max, default) = CONTINUE_MINS;
        let mins = continue_mins.map_or(default, |v| v.clamp(min, max)) as u64;
        Self {
            continue_for: Duration::from_secs(mins * 60),
        }
    }
}

/// What kind of open question the agent asked (`openQuestion.kind` of `AssistantStateWire`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum QuestionKind {
    Approval,
    Choice,
    Screen,
}

/// A question of this conversation that waits for the user: a tool approval or a choice.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenQuestion {
    /// `requestId` of the approval or the id of the choice.
    pub id: String,
    pub kind: QuestionKind,
    /// The question as it is spoken and shown.
    pub text: String,
}

/// How the caller classified a finished utterance (`assistant::phrases`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Utterance {
    /// Something to answer.
    Request(String),
    /// Nothing understandable (FR-019).
    Empty,
    /// An end phrase such as "danke, das war's" (FR-014).
    End,
    /// A stop word (FR-017).
    Stop,
}

/// Everything that moves the machine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    /// The mode is switched on.
    SwitchOn,
    /// The mode is switched off (FR-014).
    SwitchOff,
    /// The vault locks or closes (FR-004).
    VaultClosed,
    /// Wake word (FR-011); until the wake word (PR D), switching the assistant on.
    Wake,
    /// The end of an utterance was detected (FR-012).
    Utterance(Utterance),
    /// The user spoke while haex was speaking and it counts as barging in (FR-016).
    BargeIn,
    /// A stop word while a round runs or speaks (FR-017).
    Stop,
    /// "Gespräch beenden" in the indicator or by action (FR-014, FR-039).
    EndConversation,
    /// The turn started by [`Effect::StartTurn`] created or reused this thread.
    ThreadStarted(Uuid),
    /// Words of the answer are ready to speak: the first ones, or more after the output ran dry.
    FirstWords,
    /// The turn ended; the answer may still be speaking.
    TurnEnded,
    /// Everything queued for output was spoken.
    OutputDone,
    /// The agent asks a question of this conversation.
    Question(OpenQuestion),
    /// A question was answered elsewhere (window) or went away.
    QuestionResolved(String),
    /// Mute on or off (US9, FR-040).
    Mute(bool),
    /// Paused by the platform (another app holds the microphone) or resumed (US9).
    Pause(bool),
    /// Time passes; check the timers.
    Tick,
}

/// A line haex speaks that does not come from the model.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Line {
    /// "Wie bitte?" (FR-019).
    Repeat,
    /// The closing after an end phrase (FR-014).
    Goodbye,
    /// The short announcement while a round works (FR-023).
    Progress,
    /// What was done until an active end aborted the round (FR-039).
    DoneSoFar,
}

/// What the caller carries out, in order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Effect {
    Tone(ToneId),
    /// Listen for an utterance without wake word.
    Listen,
    /// Stop listening for utterances (the wake word stays on unless muted or paused).
    StopListening,
    /// Start a turn through `chat::send::start_turn` with `origin = 'voice'`.
    StartTurn {
        thread_id: Option<Uuid>,
        text: String,
    },
    /// Hand a spoken answer to the open question (PR E resolves yes, no and choices).
    AnswerQuestion {
        id: String,
        text: String,
    },
    /// Speak an open question (again).
    AskQuestion(String),
    /// Abort the running turn with its tool calls (`abort_turn`).
    AbortTurn,
    /// Stop speaking now; the transcript keeps how far it got.
    StopOutput,
    /// Speak the answer of the turn.
    SpeakAnswer,
    /// Show the answer without speaking it (muted, FR-040).
    ShowAnswer,
    Say(Line),
    /// The conversation ended; `thread_id` is `None` when nothing understandable was said and no
    /// thread exists (FR-019).
    Ended {
        thread_id: Option<Uuid>,
    },
}

/// The running conversation (data-model.md `Conversation`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Conversation {
    /// Set once the first turn created the thread, or from the start when it continues one.
    pub thread_id: Option<Uuid>,
    pub phase: Phase,
    pub running_turn: bool,
    /// Questions of this conversation in the order they were asked.
    pub open_questions: VecDeque<OpenQuestion>,
    pub spoke_delegate_notice: bool,
    pub spoke_tts_fallback_notice: bool,
    /// Output is queued or sounding.
    speaking: bool,
    /// When the running round started, until its first words are ready.
    round_started: Option<Instant>,
    /// Requests said while a round runs, in order; they become the next round once it is over
    /// (operator 2026-11-06, edge case "Neues Anliegen, während eine Aufgabe läuft").
    queued: Vec<String>,
}

impl Conversation {
    fn new(thread_id: Option<Uuid>, phase: Phase) -> Self {
        Self {
            thread_id,
            phase,
            running_turn: false,
            open_questions: VecDeque::new(),
            spoke_delegate_notice: false,
            spoke_tts_fallback_notice: false,
            speaking: false,
            round_started: None,
            queued: Vec::new(),
        }
    }
}

/// Whether the mode is on (data-model.md `mode`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Off,
    On { muted: bool, paused: bool },
}

const LISTENING: Mode = Mode::On {
    muted: false,
    paused: false,
};

/// The assistant of this process (data-model.md `AssistantState`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssistantState {
    pub mode: Mode,
    pub conversation: Option<Conversation>,
    /// The thread of the last conversation and when it ended, for the continuation window.
    pub last_ended: Option<(Uuid, Instant)>,
    pub timing: Timing,
}

impl AssistantState {
    pub fn new(timing: Timing) -> Self {
        Self {
            mode: Mode::Off,
            conversation: None,
            last_ended: None,
            timing,
        }
    }

    /// The phase the indicator shows.
    pub fn phase(&self) -> Phase {
        match (self.mode, &self.conversation) {
            (Mode::Off, _) => Phase::Off,
            (Mode::On { .. }, None) => Phase::Ready,
            (Mode::On { .. }, Some(c)) => c.phase,
        }
    }

    pub fn muted(&self) -> bool {
        matches!(self.mode, Mode::On { muted: true, .. })
    }

    pub fn paused(&self) -> bool {
        matches!(self.mode, Mode::On { paused: true, .. })
    }

    /// The question the indicator shows: the first one still open.
    pub fn open_question(&self) -> Option<&OpenQuestion> {
        self.conversation.as_ref()?.open_questions.front()
    }

    /// The next moment [`Event::Tick`] can change something, for the caller's timer.
    pub fn next_deadline(&self) -> Option<Instant> {
        let c = self.conversation.as_ref()?;
        match c.phase {
            Phase::Thinking => c.round_started.map(|at| at + WORKING_AFTER),
            _ => None,
        }
    }

    /// Applies one event at `now` and returns what the caller has to do.
    pub fn handle(&mut self, event: Event, now: Instant) -> Vec<Effect> {
        let mut out = Vec::new();
        match event {
            Event::SwitchOn => {
                if self.mode == Mode::Off {
                    self.mode = LISTENING;
                    out.push(Effect::Tone(ToneId::ModeOn));
                }
            }
            Event::SwitchOff => {
                if self.mode != Mode::Off {
                    self.end(now, &mut out);
                    self.mode = Mode::Off;
                    out.push(Effect::Tone(ToneId::ModeOff));
                }
            }
            Event::VaultClosed => {
                self.end(now, &mut out);
                self.mode = Mode::Off;
            }
            Event::Wake => self.wake(now, &mut out),
            Event::Utterance(utterance) => self.utterance(utterance, now, &mut out),
            Event::BargeIn => self.barge_in(&mut out),
            Event::Stop => self.stop(now, &mut out),
            Event::EndConversation => {
                if let Some(c) = &self.conversation {
                    let summarize = c.running_turn;
                    self.end(now, &mut out);
                    if summarize {
                        out.push(Effect::Say(Line::DoneSoFar));
                    }
                    out.push(Effect::Tone(ToneId::ConversationEnd));
                }
            }
            Event::ThreadStarted(id) => {
                if let Some(c) = &mut self.conversation {
                    c.thread_id = Some(id);
                }
            }
            Event::FirstWords => self.first_words(&mut out),
            Event::TurnEnded => self.turn_ended(now, &mut out),
            Event::OutputDone => self.output_done(now, &mut out),
            Event::Question(question) => self.question(question, &mut out),
            Event::QuestionResolved(id) => self.question_resolved(&id, now, &mut out),
            Event::Mute(on) => self.mute(on, now, &mut out),
            Event::Pause(on) => self.pause(on, &mut out),
            Event::Tick => self.tick(now, &mut out),
        }
        out
    }

    fn listens(&self) -> bool {
        self.mode == LISTENING
    }

    fn wake(&mut self, now: Instant, out: &mut Vec<Effect>) {
        if !self.listens() {
            return;
        }
        let Some(phase) = self.conversation.as_ref().map(|c| c.phase) else {
            let continue_for = self.timing.continue_for;
            let thread_id = self
                .last_ended
                .filter(|(_, at)| {
                    !continue_for.is_zero() && now.saturating_duration_since(*at) <= continue_for
                })
                .map(|(id, _)| id);
            self.conversation = Some(Conversation::new(thread_id, Phase::Listening));
            out.extend([Effect::Tone(ToneId::Listening), Effect::Listen]);
            return;
        };
        match phase {
            // An open question is asked again first (FR-040).
            Phase::AwaitingAnswer => self.ask_first_question(out),
            Phase::FollowUp => {
                if let Some(c) = &mut self.conversation {
                    c.phase = Phase::Listening;
                }
                out.extend([Effect::Tone(ToneId::Listening), Effect::Listen]);
            }
            // In keyword-only echo mode the wake word is how the user barges in (FR-016).
            Phase::Speaking => self.barge_in(out),
            _ => {}
        }
    }

    /// Speaks the first open question and listens for the answer.
    fn ask_first_question(&mut self, out: &mut Vec<Effect>) {
        let Some(c) = &self.conversation else {
            return;
        };
        let Some(q) = c.open_questions.front() else {
            return;
        };
        out.extend([
            Effect::Tone(ToneId::Question),
            Effect::AskQuestion(q.id.clone()),
            Effect::Listen,
        ]);
    }

    fn utterance(&mut self, utterance: Utterance, now: Instant, out: &mut Vec<Effect>) {
        let Some(c) = &mut self.conversation else {
            return;
        };
        let busy = matches!(c.phase, Phase::Thinking | Phase::Working);
        let accepts = busy
            || matches!(
                c.phase,
                Phase::Listening | Phase::FollowUp | Phase::AwaitingAnswer
            );
        if !accepts {
            return;
        }
        // haex keeps listening while a round runs (operator 2026-11-06): a stop word aborts it
        // (FR-017), an end phrase ends the conversation, and a request waits for the round to be
        // over; nothing is said, so "Wie bitte?" never talks over the work.
        if busy {
            match utterance {
                Utterance::Request(text) => {
                    c.queued.push(text);
                    out.extend([Effect::Tone(ToneId::Understood), Effect::Listen]);
                    return;
                }
                Utterance::Empty => {
                    out.push(Effect::Listen);
                    return;
                }
                Utterance::End | Utterance::Stop => {}
            }
        }
        match utterance {
            Utterance::Request(text) if c.phase == Phase::AwaitingAnswer => {
                let Some(q) = c.open_questions.pop_front() else {
                    return;
                };
                out.extend([
                    Effect::Tone(ToneId::Understood),
                    Effect::StopListening,
                    Effect::AnswerQuestion { id: q.id, text },
                ]);
                if !c.open_questions.is_empty() {
                    self.ask_first_question(out);
                } else if c.running_turn {
                    c.phase = Phase::Thinking;
                    c.round_started = Some(now);
                    out.push(Effect::Listen);
                } else {
                    self.after_round(now, out);
                }
            }
            Utterance::Request(text) => {
                c.phase = Phase::Thinking;
                c.running_turn = true;
                c.round_started = Some(now);
                // Listening goes on while the round runs, once the tone has sounded.
                out.extend([
                    Effect::Tone(ToneId::Understood),
                    Effect::Listen,
                    Effect::StartTurn {
                        thread_id: c.thread_id,
                        text,
                    },
                ]);
            }
            Utterance::Empty => {
                if c.phase == Phase::Listening {
                    c.phase = Phase::FollowUp;
                }
                out.extend([
                    Effect::Tone(ToneId::NotUnderstood),
                    Effect::Say(Line::Repeat),
                    Effect::Listen,
                ]);
            }
            Utterance::End => {
                self.end(now, out);
                out.extend([
                    Effect::Say(Line::Goodbye),
                    Effect::Tone(ToneId::ConversationEnd),
                ]);
            }
            Utterance::Stop => self.stop(now, out),
        }
    }

    fn barge_in(&mut self, out: &mut Vec<Effect>) {
        if !self.listens() {
            return;
        }
        let Some(c) = &mut self.conversation else {
            return;
        };
        if c.phase != Phase::Speaking {
            return;
        }
        out.push(Effect::StopOutput);
        c.speaking = false;
        if c.running_turn {
            out.push(Effect::AbortTurn);
            c.running_turn = false;
            c.open_questions.clear();
        }
        c.phase = Phase::Listening;
        c.round_started = None;
        out.extend([Effect::Tone(ToneId::Listening), Effect::Listen]);
    }

    /// A stop word: aborts the round with its output and tool calls, the conversation stays open
    /// (FR-017). Questions of the aborted round go with it.
    fn stop(&mut self, now: Instant, out: &mut Vec<Effect>) {
        let Some(c) = &mut self.conversation else {
            return;
        };
        // A stop word aborts the requests said meanwhile too.
        c.queued.clear();
        if !c.running_turn && !c.speaking {
            if matches!(c.phase, Phase::Listening | Phase::FollowUp) {
                c.phase = Phase::FollowUp;
                out.push(Effect::Listen);
            }
            return;
        }
        if c.speaking {
            out.push(Effect::StopOutput);
            c.speaking = false;
        }
        if c.running_turn {
            out.push(Effect::AbortTurn);
            c.running_turn = false;
            c.open_questions.clear();
        }
        out.push(Effect::Tone(ToneId::Aborted));
        c.round_started = None;
        self.after_round(now, out);
    }

    /// Words are ready to speak: the first of the answer, or more after the output ran dry.
    fn first_words(&mut self, out: &mut Vec<Effect>) {
        let listens = self.listens();
        let Some(c) = &mut self.conversation else {
            return;
        };
        if !c.running_turn {
            return;
        }
        c.round_started = None;
        if !listens {
            out.push(Effect::ShowAnswer);
            return;
        }
        c.speaking = true;
        match c.phase {
            Phase::Speaking => return,
            // Without echo cancellation (PR G) haex would hear itself: no listening while it
            // speaks.
            Phase::Thinking | Phase::Working => out.push(Effect::StopListening),
            // The answer arrives while haex waits for something else (FR-040).
            _ => {
                out.extend([Effect::StopListening, Effect::Tone(ToneId::Attention)]);
            }
        }
        c.phase = Phase::Speaking;
        out.push(Effect::SpeakAnswer);
    }

    fn turn_ended(&mut self, now: Instant, out: &mut Vec<Effect>) {
        let Some(c) = &mut self.conversation else {
            return;
        };
        if !c.running_turn {
            return;
        }
        c.running_turn = false;
        c.round_started = None;
        let over = match c.phase {
            // Nothing (more) to say.
            Phase::Thinking | Phase::Working => true,
            Phase::Speaking => !c.speaking,
            _ => false,
        };
        if over {
            self.after_round(now, out);
        }
    }

    fn output_done(&mut self, now: Instant, out: &mut Vec<Effect>) {
        let Some(c) = &mut self.conversation else {
            return;
        };
        c.speaking = false;
        // Over once the turn ended, or when the turn waits for a question asked while speaking.
        if c.phase == Phase::Speaking && (!c.running_turn || !c.open_questions.is_empty()) {
            self.after_round(now, out);
        }
    }

    /// The round is over: ask the next open question, or listen for the next utterance until the
    /// user sends haex to pause.
    fn after_round(&mut self, now: Instant, out: &mut Vec<Effect>) {
        let listens = self.listens();
        let Some(c) = &mut self.conversation else {
            return;
        };
        if !c.open_questions.is_empty() {
            c.phase = Phase::AwaitingAnswer;
            if listens {
                self.ask_first_question(out);
            }
            return;
        }
        // What the user asked while the round ran is the next round, at once and in one message
        // (operator 2026-11-06).
        if !c.queued.is_empty() {
            let text = std::mem::take(&mut c.queued).join(" ");
            c.phase = Phase::Thinking;
            c.running_turn = true;
            c.round_started = Some(now);
            out.push(Effect::StartTurn {
                thread_id: c.thread_id,
                text,
            });
            if listens {
                out.push(Effect::Listen);
            }
            return;
        }
        c.phase = Phase::FollowUp;
        // Muted or paused, haex listens again once that ends (FR-040).
        if listens {
            out.push(Effect::Listen);
        }
    }

    fn question(&mut self, question: OpenQuestion, out: &mut Vec<Effect>) {
        let listens = self.listens();
        let Some(c) = &mut self.conversation else {
            return;
        };
        if !c.running_turn {
            return;
        }
        let first = c.open_questions.is_empty();
        c.open_questions.push_back(question);
        // Further questions wait for their turn; one asked while haex speaks comes after the
        // answer so far (`after_round`).
        if !first || c.phase == Phase::Speaking {
            return;
        }
        c.phase = Phase::AwaitingAnswer;
        c.round_started = None;
        if listens {
            out.push(Effect::StopListening);
            self.ask_first_question(out);
        }
    }

    fn question_resolved(&mut self, id: &str, now: Instant, out: &mut Vec<Effect>) {
        let listens = self.listens();
        let Some(c) = &mut self.conversation else {
            return;
        };
        let before = c.open_questions.len();
        let was_first = c.open_questions.front().is_some_and(|q| q.id == id);
        c.open_questions.retain(|q| q.id != id);
        if c.open_questions.len() == before || c.phase != Phase::AwaitingAnswer {
            return;
        }
        if !c.open_questions.is_empty() {
            if listens && was_first {
                self.ask_first_question(out);
            }
        } else if c.running_turn {
            c.phase = Phase::Thinking;
            c.round_started = Some(now);
            out.push(if listens {
                Effect::Listen
            } else {
                Effect::StopListening
            });
        } else {
            self.after_round(now, out);
        }
    }

    fn mute(&mut self, on: bool, now: Instant, out: &mut Vec<Effect>) {
        let Mode::On { muted, paused } = self.mode else {
            return;
        };
        if muted == on {
            return;
        }
        self.mode = Mode::On { muted: on, paused };
        if !on {
            out.push(Effect::Tone(ToneId::Unmuted));
            self.resume_listening(out);
            return;
        }
        out.push(Effect::Tone(ToneId::Muted));
        // Muting ends a conversation only when no round runs (FR-014).
        let ends = self
            .conversation
            .as_ref()
            .is_some_and(|c| !c.running_turn && c.open_questions.is_empty());
        if ends {
            self.end(now, out);
        } else {
            out.push(Effect::StopListening);
            self.hold(out);
        }
    }

    fn pause(&mut self, on: bool, out: &mut Vec<Effect>) {
        let Mode::On { muted, paused } = self.mode else {
            return;
        };
        if paused == on {
            return;
        }
        self.mode = Mode::On { muted, paused: on };
        if on {
            out.push(Effect::StopListening);
            self.hold(out);
        } else {
            self.resume_listening(out);
        }
    }

    /// Muted or paused with the conversation kept: nothing speaks or listens, no window runs.
    fn hold(&mut self, out: &mut Vec<Effect>) {
        let Some(c) = &mut self.conversation else {
            return;
        };
        if c.speaking {
            out.push(Effect::StopOutput);
            c.speaking = false;
        }
        c.phase = match c.phase {
            Phase::Speaking if c.running_turn => Phase::Working,
            Phase::Speaking | Phase::Listening => {
                if c.open_questions.is_empty() {
                    Phase::FollowUp
                } else {
                    Phase::AwaitingAnswer
                }
            }
            other => other,
        };
    }

    /// After mute or pause ends: listen again (FR-040). An open question is not repeated on its
    /// own; the next wake word asks it.
    fn resume_listening(&mut self, out: &mut Vec<Effect>) {
        if !self.listens() {
            return;
        }
        let Some(c) = &self.conversation else {
            return;
        };
        if matches!(
            c.phase,
            Phase::FollowUp | Phase::AwaitingAnswer | Phase::Thinking | Phase::Working
        ) {
            out.push(Effect::Listen);
        }
    }

    fn tick(&mut self, now: Instant, out: &mut Vec<Effect>) {
        let listens = self.listens();
        let Some(c) = &mut self.conversation else {
            return;
        };
        if c.phase != Phase::Thinking {
            return;
        }
        let late = c
            .round_started
            .is_some_and(|at| now.saturating_duration_since(at) >= WORKING_AFTER);
        if late {
            c.phase = Phase::Working;
            out.push(Effect::Tone(ToneId::Working));
            if listens {
                // Listening pauses for the tone and the line, so haex does not hear itself.
                out.extend([Effect::Say(Line::Progress), Effect::Listen]);
            }
        }
    }

    /// Ends the conversation: aborts a running round (FR-039), stops output and listening, and
    /// remembers the thread for the continuation window.
    fn end(&mut self, now: Instant, out: &mut Vec<Effect>) {
        let Some(c) = self.conversation.take() else {
            return;
        };
        if c.speaking {
            out.push(Effect::StopOutput);
        }
        if c.running_turn {
            out.push(Effect::AbortTurn);
        }
        out.push(Effect::StopListening);
        if let Some(id) = c.thread_id {
            self.last_ended = Some((id, now));
        }
        out.push(Effect::Ended {
            thread_id: c.thread_id,
        });
    }
}
