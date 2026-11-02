//! Spec 066 T044: the driver carries out the conversation's decisions, listens only once haex is
//! quiet, drops what belongs to an aborted round or a stopped listener, and closes the microphone
//! after the conversation until the wake word arrives.

use std::time::{Duration, Instant};

use uuid::Uuid;

use super::conversation::{Event, OpenQuestion, Phase, QuestionKind, Timing};
use super::driver::{Core, Input, Ports, StateWire};
use super::phrases;
use super::tones::ToneId;
use super::turn::Output;

const TONE: Duration = Duration::from_millis(300);

#[derive(Debug, Clone, PartialEq, Eq)]
enum Call {
    Tone(ToneId),
    Listen(u64),
    StopListening,
    StartTurn {
        round: u64,
        thread_id: Option<Uuid>,
        text: String,
        delegate_notice_due: bool,
    },
    AbortTurn,
    Say(String),
    StopOutput,
    CloseAudio,
}

#[derive(Default)]
struct Fake {
    calls: Vec<Call>,
    tickets: u64,
    published: Vec<StateWire>,
    tones_off: bool,
}

impl Ports for Fake {
    fn tone(&mut self, tone: ToneId) -> Duration {
        self.calls.push(Call::Tone(tone));
        if self.tones_off {
            Duration::ZERO
        } else {
            TONE
        }
    }
    fn listen(&mut self, id: u64) {
        self.calls.push(Call::Listen(id));
    }
    fn stop_listening(&mut self) {
        self.calls.push(Call::StopListening);
    }
    fn start_turn(
        &mut self,
        round: u64,
        thread_id: Option<Uuid>,
        text: String,
        delegate_notice_due: bool,
    ) {
        self.calls.push(Call::StartTurn {
            round,
            thread_id,
            text,
            delegate_notice_due,
        });
    }
    fn abort_turn(&mut self) {
        self.calls.push(Call::AbortTurn);
    }
    fn say(&mut self, text: String) -> u64 {
        self.calls.push(Call::Say(text));
        self.tickets += 1;
        self.tickets
    }
    fn stop_output(&mut self) {
        self.calls.push(Call::StopOutput);
    }
    fn publish(&mut self, state: &StateWire) {
        self.published.push(state.clone());
    }
    fn close_audio(&mut self) {
        self.calls.push(Call::CloseAudio);
    }
}

struct Rig {
    core: Core,
    ports: Fake,
    now: Instant,
}

impl Rig {
    fn new() -> Self {
        let now = Instant::now();
        Self {
            core: Core::new("de", Timing::default(), now),
            ports: Fake::default(),
            now,
        }
    }

    fn on(&mut self, input: Input) -> Vec<Call> {
        self.core.on(input, self.now, &mut self.ports);
        std::mem::take(&mut self.ports.calls)
    }

    /// Lets time pass up to the driver's next wakeup and ticks.
    fn wake_up(&mut self) -> Vec<Call> {
        let at = self.core.next_wakeup().expect("a wakeup is due");
        self.now = self.now.max(at);
        self.on(Input::Tick)
    }

    fn heard(&mut self, listener: u64, text: &str) -> Vec<Call> {
        self.on(Input::Heard {
            listener,
            text: text.to_owned(),
        })
    }

    fn turn(&mut self, round: u64, output: Output) -> Vec<Call> {
        self.on(Input::Turn { round, output })
    }

    fn phase(&self) -> Phase {
        self.core.state().phase()
    }

    /// "Sprechen", the tone, and a listener once the tone has sounded; returns the listener.
    fn talk(&mut self) -> u64 {
        assert_eq!(self.on(Input::Talk), [Call::Tone(ToneId::Listening)]);
        match self.wake_up().as_slice() {
            [Call::Listen(id)] => *id,
            other => panic!("expected one listener, got {other:?}"),
        }
    }
}

fn say(text: &str) -> Output {
    Output::Say(text.to_owned())
}

const FIRST: Output = Output::Event(Event::FirstWords);
const ENDED: Output = Output::Event(Event::TurnEnded);

#[test]
fn talking_opens_without_mode_tone_and_listens_after_the_tone() {
    let mut rig = Rig::new();
    assert_eq!(rig.on(Input::Talk), [Call::Tone(ToneId::Listening)]);
    assert!(rig.core.is_open());
    assert_eq!(rig.phase(), Phase::Listening);
    assert_eq!(
        rig.core.next_wakeup(),
        Some(rig.now + TONE),
        "listening waits for the tone"
    );
    assert_eq!(rig.wake_up(), [Call::Listen(1)]);
    let last = rig.ports.published.last().expect("published");
    assert!(last.enabled);
    assert_eq!(last.phase, Phase::Listening);
}

#[test]
fn a_request_starts_a_round_and_its_answer_is_spoken() {
    let mut rig = Rig::new();
    rig.talk();
    assert_eq!(
        rig.heard(1, "Wie wird das Wetter morgen?"),
        [
            Call::Tone(ToneId::Understood),
            Call::StopListening,
            Call::StartTurn {
                round: 1,
                thread_id: None,
                text: "Wie wird das Wetter morgen?".to_owned(),
                delegate_notice_due: true,
            },
        ]
    );
    let thread = Uuid::new_v4();
    rig.turn(1, Output::Event(Event::ThreadStarted(thread)));
    assert!(rig.turn(1, FIRST).is_empty());
    assert_eq!(rig.phase(), Phase::Speaking);
    assert_eq!(
        rig.turn(1, say("Morgen regnet es.")),
        [Call::Say("Morgen regnet es.".to_owned())]
    );
    assert!(rig.turn(1, ENDED).is_empty(), "still speaking");
    assert_eq!(rig.phase(), Phase::Speaking);

    // Once the sentence has sounded and the tone is over, haex listens for the follow-up.
    rig.on(Input::Played(1));
    assert_eq!(rig.phase(), Phase::FollowUp);
    let calls = rig.wake_up();
    assert_eq!(calls, [Call::Listen(2)]);
    assert_eq!(
        rig.core
            .state()
            .conversation
            .as_ref()
            .and_then(|c| c.thread_id),
        Some(thread)
    );

    // The follow-up continues the thread.
    let calls = rig.heard(2, "Und übermorgen?");
    assert!(calls.contains(&Call::StartTurn {
        round: 2,
        thread_id: Some(thread),
        text: "Und übermorgen?".to_owned(),
        delegate_notice_due: true,
    }));
}

#[test]
fn listening_waits_until_haex_has_finished_speaking() {
    let mut rig = Rig::new();
    rig.talk();
    // Nothing understandable: "Wie bitte?", and listening only after it.
    assert_eq!(
        rig.heard(1, ""),
        [
            Call::Tone(ToneId::NotUnderstood),
            Call::Say(phrases::repeat("de")),
            Call::StopListening,
        ]
    );
    rig.now += TONE;
    assert!(rig.on(Input::Tick).is_empty(), "the sentence still sounds");
    assert_eq!(rig.on(Input::Played(1)), [Call::Listen(2)]);
}

#[test]
fn utterances_of_a_stopped_listener_and_outputs_of_an_aborted_round_are_dropped() {
    let mut rig = Rig::new();
    rig.talk();
    rig.heard(1, "Schreib eine Mail an Jonas");
    assert!(
        rig.heard(1, "noch etwas").is_empty(),
        "listener 1 was stopped"
    );
    rig.turn(1, FIRST);
    rig.turn(1, say("Ich schreibe die Mail."));

    // "Sprechen" while haex speaks interrupts it (keyword-only echo mode until PR G).
    let calls = rig.on(Input::Talk);
    assert_eq!(
        calls,
        [
            Call::StopOutput,
            Call::AbortTurn,
            Call::Tone(ToneId::Listening),
        ]
    );
    assert!(rig.turn(1, say("An Jonas.")).is_empty());
    assert!(rig.turn(1, ENDED).is_empty());
    assert_eq!(rig.phase(), Phase::Listening);
    rig.on(Input::Played(1));
    assert_eq!(rig.wake_up(), [Call::Listen(2)]);
}

#[test]
fn an_end_phrase_says_goodbye_and_closes_once_quiet() {
    let mut rig = Rig::new();
    rig.talk();
    assert_eq!(
        rig.heard(1, "Danke, das war's"),
        [
            Call::StopListening,
            Call::Say(phrases::goodbye("de")),
            Call::Tone(ToneId::ConversationEnd),
        ]
    );
    assert!(rig.on(Input::Played(1)).is_empty(), "the tone still sounds");
    assert_eq!(rig.wake_up(), [Call::CloseAudio]);
    assert!(!rig.core.is_open());
    let last = rig.ports.published.last().expect("published");
    assert!(!last.enabled);
    assert_eq!(last.phase, Phase::Off);
    assert_eq!(rig.core.next_wakeup(), None);
}

#[test]
fn the_follow_up_window_ends_the_conversation_and_talking_again_continues_it() {
    let mut rig = Rig::new();
    rig.talk();
    rig.heard(1, "Wie spät ist es?");
    let thread = Uuid::new_v4();
    rig.turn(1, Output::Event(Event::ThreadStarted(thread)));
    rig.turn(1, FIRST);
    rig.turn(1, say("Zehn Uhr."));
    rig.turn(1, ENDED);
    rig.on(Input::Played(1));
    assert_eq!(rig.wake_up(), [Call::Listen(2)]);

    // Eight seconds of silence.
    let calls = rig.wake_up();
    assert_eq!(
        calls,
        [Call::StopListening, Call::Tone(ToneId::ConversationEnd)]
    );
    assert_eq!(rig.wake_up(), [Call::CloseAudio]);

    // Within five minutes "Sprechen" continues the same thread.
    rig.now += Duration::from_secs(60);
    let listener = rig.talk();
    assert_eq!(listener, 3);
    let calls = rig.heard(listener, "Und morgen?");
    assert!(calls.iter().any(|c| matches!(
        c,
        Call::StartTurn { thread_id: Some(t), .. } if *t == thread
    )));
}

#[test]
fn muted_the_answer_is_shown_instead_of_spoken() {
    let mut rig = Rig::new();
    rig.talk();
    rig.heard(1, "Was steht heute an?");
    let calls = rig.on(Input::Mute(true));
    assert_eq!(calls, [Call::Tone(ToneId::Muted)]);
    assert!(rig.ports.published.last().expect("published").muted);
    assert!(rig.turn(1, FIRST).is_empty());
    assert!(
        rig.turn(1, say("Zwei Termine.")).is_empty(),
        "the window shows the answer"
    );
}

#[test]
fn a_round_that_cannot_start_plays_the_error_tone_and_listens_again() {
    let mut rig = Rig::new();
    rig.talk();
    rig.heard(1, "Hallo");
    assert_eq!(
        rig.on(Input::TurnFailed { round: 1 }),
        [Call::Tone(ToneId::Error)]
    );
    assert_eq!(rig.phase(), Phase::FollowUp);
    assert_eq!(rig.wake_up(), [Call::Listen(2)]);
}

#[test]
fn an_open_question_is_spoken_and_answered_in_the_window() {
    let mut rig = Rig::new();
    rig.talk();
    rig.heard(1, "Schick die Mail");
    let question = OpenQuestion {
        id: "q1".to_owned(),
        kind: QuestionKind::Approval,
        text: "mail.send".to_owned(),
    };
    let calls = rig.turn(1, Output::Event(Event::Question(question)));
    assert_eq!(
        calls,
        [
            Call::Tone(ToneId::Question),
            Call::Say("mail.send".to_owned()),
        ]
    );
    let shown = rig.ports.published.last().expect("published");
    assert_eq!(shown.phase, Phase::AwaitingAnswer);
    assert_eq!(
        shown.open_question.as_ref().map(|q| q.text.as_str()),
        Some("mail.send")
    );
    rig.on(Input::Played(1));
    assert_eq!(rig.wake_up(), [Call::Listen(2)]);

    // Approved in the window: the turn goes on.
    let calls = rig.turn(1, Output::Event(Event::QuestionResolved("q1".to_owned())));
    assert_eq!(calls, [Call::StopListening]);
    assert_eq!(rig.phase(), Phase::Thinking);
}

#[test]
fn the_delegate_notice_is_due_until_said() {
    let mut rig = Rig::new();
    rig.talk();
    rig.heard(1, "Hallo");
    rig.turn(1, Output::SpokeDelegateNotice);
    rig.turn(1, ENDED);
    rig.wake_up();
    let calls = rig.heard(2, "Noch eine Frage");
    assert!(calls.iter().any(|c| matches!(
        c,
        Call::StartTurn {
            delegate_notice_due: false,
            ..
        }
    )));
}

#[test]
fn closing_the_vault_releases_everything() {
    let mut rig = Rig::new();
    rig.talk();
    rig.heard(1, "Was gibt es Neues?");
    rig.turn(1, FIRST);
    rig.turn(1, say("Nichts."));
    let calls = rig.on(Input::VaultClosed);
    assert_eq!(
        calls,
        [Call::StopOutput, Call::AbortTurn, Call::CloseAudio,]
    );
    assert_eq!(
        rig.ports.published.last().map(|s| s.phase),
        Some(Phase::Off)
    );
}

#[test]
fn a_stop_word_with_nothing_running_keeps_listening() {
    let mut rig = Rig::new();
    rig.ports.tones_off = true;
    assert_eq!(
        rig.on(Input::Talk),
        [Call::Tone(ToneId::Listening), Call::Listen(1)]
    );
    assert_eq!(
        rig.heard(1, "Stopp"),
        [Call::StopListening, Call::Listen(2)]
    );
    assert_eq!(rig.phase(), Phase::FollowUp);
}

#[test]
fn ending_a_running_round_aborts_it() {
    let mut rig = Rig::new();
    rig.talk();
    rig.heard(1, "Erzähl mir was");
    assert_eq!(
        rig.on(Input::End),
        [Call::AbortTurn, Call::Tone(ToneId::ConversationEnd)]
    );
    assert!(rig.core.state().conversation.is_none());
    assert_eq!(rig.wake_up(), [Call::CloseAudio]);
}
