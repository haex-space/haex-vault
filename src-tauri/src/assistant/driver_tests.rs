//! Spec 066 T044: the driver carries out the conversation's decisions, listens only once haex is
//! quiet, drops what belongs to an aborted round or a stopped listener, is switched on and off by
//! one toggle, and until the wake word switches itself off after a pause (operator 2026-11-02).

use std::time::{Duration, Instant};

use uuid::Uuid;

use super::conversation::{Event, OpenQuestion, Phase, QuestionKind, Timing};
use super::driver::{echoes, Core, EchoMode, Input, Note, Ports, StateWire};
use super::phrases;
use super::tones::ToneId;
use super::turn::Output;
use crate::storage::voice_transcript::{EntryKind, Outcome, Speaker};

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
    ListenAfterBargeIn(u64),
    Watch(u64),
    StopWatching,
    WatchWake(u64),
    StopWake,
}

#[derive(Default)]
struct Fake {
    calls: Vec<Call>,
    tickets: u64,
    published: Vec<StateWire>,
    tones_off: bool,
    notes: Vec<(Uuid, Note)>,
    /// The texts queued as sentences of an answer (`say_answer`).
    answer_sentences: Vec<String>,
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
    fn listen_after_barge_in(&mut self, id: u64) {
        self.calls.push(Call::ListenAfterBargeIn(id));
    }
    fn watch_barge_in(&mut self, id: u64) {
        self.calls.push(Call::Watch(id));
    }
    fn stop_watching(&mut self) {
        self.calls.push(Call::StopWatching);
    }
    fn watch_wake_word(&mut self, id: u64) {
        self.calls.push(Call::WatchWake(id));
    }
    fn stop_wake_word(&mut self) {
        self.calls.push(Call::StopWake);
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
    fn say_answer(&mut self, text: String) -> u64 {
        self.answer_sentences.push(text.clone());
        self.say(text)
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
    fn record(&mut self, thread_id: Uuid, note: Note) {
        self.notes.push((thread_id, note));
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

    /// The toggle on, the listening tone, and a listener once the tone has sounded; returns the
    /// listener.
    fn talk(&mut self) -> u64 {
        assert_eq!(self.on(Input::SwitchOn), [Call::Tone(ToneId::Listening)]);
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
fn switching_on_starts_a_conversation_and_listens_after_the_tone() {
    let mut rig = Rig::new();
    // Until the wake word, the listening tone stands for the mode tone.
    assert_eq!(rig.on(Input::SwitchOn), [Call::Tone(ToneId::Listening)]);
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
    assert!(
        rig.on(Input::SwitchOn).is_empty(),
        "on again changes nothing"
    );
}

#[test]
fn switching_off_aborts_the_round_and_closes_after_the_mode_tone() {
    let mut rig = Rig::new();
    rig.talk();
    rig.heard(1, "Erzähl mir was");
    rig.turn(1, FIRST);
    rig.turn(1, say("Es war einmal."));
    assert_eq!(
        rig.on(Input::SwitchOff),
        [
            Call::StopOutput,
            Call::AbortTurn,
            Call::Tone(ToneId::ModeOff),
        ]
    );
    assert!(rig.core.is_open(), "the mode tone still sounds");
    assert!(rig.turn(1, say("Ende.")).is_empty(), "the round is gone");
    rig.on(Input::Played(1));
    assert_eq!(rig.wake_up(), [Call::CloseAudio]);
    let last = rig.ports.published.last().expect("published");
    assert!(!last.enabled);
    assert_eq!(last.phase, Phase::Off);
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

    // "Gespräch beenden" aborts the round; what it still sends is dropped.
    let calls = rig.on(Input::End);
    assert!(calls.contains(&Call::AbortTurn));
    assert!(calls.contains(&Call::StopOutput));
    assert!(rig.turn(1, say("An Jonas.")).is_empty());
    assert!(rig.turn(1, ENDED).is_empty());
}

#[test]
fn a_pause_says_goodbye_and_switches_off_once_quiet() {
    let mut rig = Rig::new();
    rig.talk();
    assert_eq!(
        rig.heard(1, "Mach Pause"),
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
fn haex_listens_without_a_time_limit_and_switching_on_after_a_pause_continues_the_thread() {
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

    // An hour of silence ends nothing.
    assert_eq!(rig.core.next_wakeup(), None);
    rig.now += Duration::from_secs(3_600);
    assert!(rig.on(Input::Tick).is_empty());
    assert!(rig.core.is_open());

    let calls = rig.heard(2, "Danke, das war's");
    assert!(calls.contains(&Call::Tone(ToneId::ConversationEnd)));
    rig.on(Input::Played(2));
    assert_eq!(rig.wake_up(), [Call::CloseAudio]);

    // Switched on again within five minutes, the same thread goes on.
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
    assert_eq!(
        calls,
        [Call::StopListening, Call::Listen(3)],
        "haex listens on while the round runs (operator 2026-11-06)"
    );
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
        rig.on(Input::SwitchOn),
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

/// The notes as (thread, speaker, kind, text, spoken until).
fn notes(rig: &Rig) -> Vec<(Uuid, Speaker, EntryKind, String, Option<i64>)> {
    rig.ports
        .notes
        .iter()
        .map(|(thread, n)| {
            (
                *thread,
                n.speaker.clone(),
                n.kind.clone(),
                n.text.clone(),
                n.spoken_until,
            )
        })
        .collect()
}

#[test]
fn notes_wait_for_the_thread_and_keep_their_order() {
    let mut rig = Rig::new();
    rig.talk();
    rig.heard(1, "äh");
    rig.on(Input::Played(1));
    rig.wake_up();
    rig.heard(2, "Wie wird das Wetter?");
    assert!(rig.ports.notes.is_empty(), "no thread yet");
    let thread = Uuid::new_v4();
    rig.turn(1, Output::Event(Event::ThreadStarted(thread)));
    rig.turn(1, FIRST);
    rig.turn(1, say("Morgen regnet es."));
    rig.turn(1, say("Nimm einen Schirm mit."));
    rig.turn(1, ENDED);
    assert_eq!(
        rig.ports.notes.len(),
        3,
        "the answer is noted once it has sounded"
    );
    rig.on(Input::Played(3));
    assert_eq!(
        notes(&rig),
        [
            (
                thread,
                Speaker::User,
                EntryKind::NotUnderstood,
                "äh".to_owned(),
                None
            ),
            (
                thread,
                Speaker::Haex,
                EntryKind::Announcement,
                phrases::repeat("de"),
                None
            ),
            (
                thread,
                Speaker::User,
                EntryKind::Utterance,
                "Wie wird das Wetter?".to_owned(),
                None
            ),
            (
                thread,
                Speaker::Haex,
                EntryKind::Answer,
                "Morgen regnet es. Nimm einen Schirm mit.".to_owned(),
                None
            ),
        ]
    );
}

#[test]
fn a_conversation_without_a_request_leaves_no_transcript() {
    let mut rig = Rig::new();
    rig.talk();
    rig.heard(1, "");
    rig.on(Input::Played(1));
    rig.wake_up();
    rig.heard(2, "Pause");
    rig.on(Input::Played(2));
    rig.wake_up();
    assert!(!rig.core.is_open());
    assert!(rig.ports.notes.is_empty());

    // The next conversation starts without the notes of the last one.
    rig.talk();
    rig.heard(3, "Hallo");
    rig.turn(1, Output::Event(Event::ThreadStarted(Uuid::new_v4())));
    assert_eq!(rig.ports.notes.len(), 1);
}

#[test]
fn an_interrupted_answer_keeps_its_full_text_and_how_far_it_was_spoken() {
    let mut rig = Rig::new();
    rig.talk();
    rig.heard(1, "Lies mir die Mail vor");
    let thread = Uuid::new_v4();
    rig.turn(1, Output::Event(Event::ThreadStarted(thread)));
    rig.turn(1, FIRST);
    rig.turn(1, say("Hallo Jonas."));
    rig.turn(1, say("Das Treffen fällt aus."));
    rig.on(Input::Played(1));
    rig.on(Input::SwitchOff);
    let (_, answer) = rig.ports.notes.last().expect("answer noted");
    assert_eq!(answer.kind, EntryKind::Answer);
    assert_eq!(answer.text, "Hallo Jonas. Das Treffen fällt aus.");
    assert_eq!(
        answer.spoken_until,
        Some("Hallo Jonas.".chars().count() as i64)
    );
}

#[test]
fn a_muted_answer_is_noted_as_not_spoken() {
    let mut rig = Rig::new();
    rig.talk();
    rig.heard(1, "Was steht an?");
    rig.turn(1, Output::Event(Event::ThreadStarted(Uuid::new_v4())));
    rig.on(Input::Mute(true));
    rig.turn(1, FIRST);
    rig.turn(1, say("Zwei Termine."));
    rig.turn(1, ENDED);
    let (_, answer) = rig.ports.notes.last().expect("answer noted");
    assert_eq!(answer.text, "Zwei Termine.");
    assert_eq!(answer.spoken_until, Some(0));
}

#[test]
fn the_closing_and_the_goodbye_go_to_the_thread_that_ended() {
    let mut rig = Rig::new();
    rig.talk();
    rig.heard(1, "Wie spät ist es?");
    let thread = Uuid::new_v4();
    rig.turn(1, Output::Event(Event::ThreadStarted(thread)));
    rig.turn(1, ENDED);
    rig.wake_up();
    rig.heard(2, "Danke, das war's");
    let kinds: Vec<_> = notes(&rig)
        .into_iter()
        .map(|(t, speaker, kind, _, _)| (t, speaker, kind))
        .collect();
    assert_eq!(
        kinds,
        [
            (thread, Speaker::User, EntryKind::Utterance),
            (thread, Speaker::User, EntryKind::Closing),
            (thread, Speaker::Haex, EntryKind::Announcement),
        ]
    );
}

#[test]
fn a_spoken_question_and_its_answer_are_noted_with_their_id() {
    let mut rig = Rig::new();
    rig.talk();
    rig.heard(1, "Schick die Mail");
    rig.turn(1, Output::Event(Event::ThreadStarted(Uuid::new_v4())));
    rig.turn(
        1,
        Output::Event(Event::Question(OpenQuestion {
            id: "q1".to_owned(),
            kind: QuestionKind::Approval,
            text: "mail.send".to_owned(),
        })),
    );
    rig.on(Input::Played(1));
    rig.wake_up();
    rig.heard(2, "Ja");
    let asked = &rig.ports.notes[1].1;
    assert_eq!(asked.kind, EntryKind::ApprovalQuestion);
    assert_eq!(asked.outcome, Some(Outcome::Open));
    assert_eq!(asked.ref_id.as_deref(), Some("q1"));
    let answered = &rig.ports.notes[2].1;
    assert_eq!(answered.kind, EntryKind::ApprovalAnswer);
    assert_eq!(answered.text, "Ja");
    assert_eq!(answered.ref_id.as_deref(), Some("q1"));
}

/// Spec 081 FR-007: only the sentences of answers are measured, not questions or announcements.
#[test]
fn only_the_sentences_of_an_answer_go_to_the_speed_measurement() {
    let mut rig = Rig::new();
    rig.talk();
    rig.heard(1, "Schick die Mail");
    rig.turn(1, Output::Event(Event::ThreadStarted(Uuid::new_v4())));
    rig.turn(
        1,
        Output::Event(Event::Question(OpenQuestion {
            id: "q1".to_owned(),
            kind: QuestionKind::Approval,
            text: "mail.send".to_owned(),
        })),
    );
    assert!(
        rig.ports.answer_sentences.is_empty(),
        "a question is not an answer"
    );

    let mut rig = Rig::new();
    rig.talk();
    rig.heard(1, "Was steht an?");
    rig.turn(1, Output::Event(Event::ThreadStarted(Uuid::new_v4())));
    rig.turn(1, FIRST);
    rig.turn(1, say("Zwei Termine."));
    assert_eq!(rig.ports.answer_sentences, ["Zwei Termine."]);
}

/// Operator 2026-11-06: "Was ist die Hauptstadt von Australien?" starts a search; "Und wie viele
/// Einwohner hat das Land?" said meanwhile is heard and becomes the next round once the first ends.
#[test]
fn a_request_heard_while_the_round_runs_is_the_next_round_after_it() {
    let mut rig = Rig::new();
    rig.talk();
    rig.heard(1, "Was ist die Hauptstadt von Australien?");
    assert_eq!(
        rig.wake_up(),
        [Call::Listen(2)],
        "listening goes on after the tone"
    );

    let calls = rig.heard(2, "Und wie viele Einwohner hat das Land?");
    assert!(calls.contains(&Call::Tone(ToneId::Understood)));
    assert!(
        !calls.iter().any(|c| matches!(c, Call::StartTurn { .. })),
        "the first round goes on alone: {calls:?}"
    );
    assert_eq!(rig.wake_up(), [Call::Listen(3)]);

    let calls = rig.turn(1, ENDED);
    assert!(
        calls.iter().any(|c| matches!(
            c,
            Call::StartTurn { round: 2, text, .. } if text == "Und wie viele Einwohner hat das Land?"
        )),
        "{calls:?}"
    );
    assert_eq!(rig.phase(), Phase::Thinking);
}

#[test]
fn a_stop_word_heard_while_the_round_runs_aborts_it() {
    let mut rig = Rig::new();
    rig.talk();
    rig.heard(1, "Such mir alle Flüge nach Sydney");
    rig.wake_up();
    let calls = rig.heard(2, "Stopp");
    assert!(calls.contains(&Call::AbortTurn), "{calls:?}");
    assert_eq!(rig.phase(), Phase::FollowUp);
}

/// A conversation over a stream that cancels haex's echo, haex speaking its first sentence.
fn speaking_with_barge_in() -> Rig {
    let mut rig = Rig::new();
    rig.core.set_barge_in(true);
    rig.talk();
    rig.heard(1, "Lies mir Hamlet vor");
    rig.turn(1, FIRST);
    let calls = rig.turn(1, say("Sein oder Nichtsein, das ist hier die Frage."));
    assert_eq!(
        calls,
        [Call::Say(
            "Sein oder Nichtsein, das ist hier die Frage.".to_owned()
        )]
    );
    rig
}

/// FR-016: while haex speaks over a stream that cancels its echo, a watcher waits for the user;
/// once they talk, the output stops and haex listens with what was said just before.
#[test]
fn talking_over_haex_stops_it_and_listens_with_what_was_said() {
    let mut rig = speaking_with_barge_in();
    assert_eq!(rig.phase(), Phase::Speaking);
    // The watcher started when haex began to speak (in the call above the turn output, so look
    // at what was published).
    assert_eq!(
        rig.ports.published.last().map(|s| s.echo_mode),
        Some(EchoMode::Full)
    );
    rig.turn(1, ENDED);
    let calls = rig.on(Input::BargeIn { watcher: 1 });
    assert_eq!(calls, [Call::StopOutput, Call::Tone(ToneId::Listening)]);
    rig.on(Input::Played(1));
    assert_eq!(rig.wake_up(), [Call::ListenAfterBargeIn(2)]);
    // What the user said is the next round.
    let calls = rig.heard(2, "Stopp, lies lieber Faust");
    assert!(
        calls.iter().any(
            |c| matches!(c, Call::StartTurn { text, .. } if text == "Stopp, lies lieber Faust")
        ),
        "{calls:?}"
    );
}

#[test]
fn the_watcher_runs_only_while_haex_speaks() {
    let mut rig = Rig::new();
    rig.core.set_barge_in(true);
    rig.talk();
    rig.heard(1, "Lies mir Hamlet vor");
    let calls = rig.turn(1, FIRST);
    assert_eq!(calls, [Call::Watch(1)], "speaking starts the watcher");
    rig.turn(1, say("Sein oder Nichtsein."));
    rig.turn(1, ENDED);
    let calls = rig.on(Input::Played(1));
    assert!(calls.contains(&Call::StopWatching), "{calls:?}");
    // A report of the stopped watcher changes nothing.
    assert!(rig.on(Input::BargeIn { watcher: 1 }).is_empty());
}

#[test]
fn without_echo_cancellation_nothing_watches() {
    let mut rig = Rig::new();
    rig.talk();
    rig.heard(1, "Lies mir Hamlet vor");
    assert!(rig.turn(1, FIRST).is_empty());
    assert_eq!(
        rig.ports.published.last().map(|s| s.echo_mode),
        Some(EchoMode::KeywordOnly)
    );
}

/// A rest of haex's own voice that stopped it is not taken as a request: haex listens on.
#[test]
fn haex_hearing_its_own_echo_after_a_barge_in_listens_on() {
    let mut rig = speaking_with_barge_in();
    rig.turn(1, ENDED);
    rig.on(Input::BargeIn { watcher: 1 });
    rig.on(Input::Played(1));
    assert_eq!(rig.wake_up(), [Call::ListenAfterBargeIn(2)]);
    assert_eq!(rig.heard(2, "das ist hier die Frage"), [Call::Listen(3)]);
}

#[test]
fn an_echo_is_told_from_the_user_by_its_words() {
    let said = ["Sein oder Nichtsein, das ist hier die Frage.".to_owned()];
    assert!(echoes("das ist hier die Frage", &said));
    assert!(echoes("Nichtsein das ist", &said));
    assert!(!echoes("Stopp", &said), "one word is never an echo");
    assert!(!echoes("Stopp, lies lieber Faust", &said));
    assert!(!echoes("das ist mir zu lang", &said));
}

/// The toggle on with the wake word model installed: the mode tone, and haex waits for "Hex".
fn waiting_for_the_wake_word() -> Rig {
    let mut rig = Rig::new();
    rig.core.set_wake_word(true);
    assert_eq!(rig.on(Input::SwitchOn), [Call::Tone(ToneId::ModeOn)]);
    // The watcher starts once the mode tone has sounded.
    assert_eq!(rig.wake_up(), [Call::WatchWake(1)]);
    rig
}

#[test]
fn with_the_wake_word_switching_on_waits_ready() {
    let rig = waiting_for_the_wake_word();
    assert!(rig.core.is_open());
    assert_eq!(rig.phase(), Phase::Ready);
    assert_eq!(
        rig.core.next_wakeup(),
        None,
        "nothing to do until the wake word"
    );
}

#[test]
fn the_wake_word_starts_a_conversation_that_hears_what_came_just_before() {
    let mut rig = waiting_for_the_wake_word();
    // The watcher ended by itself when it fired.
    assert_eq!(
        rig.on(Input::WakeWord { watcher: 1 }),
        [Call::Tone(ToneId::Listening)]
    );
    assert_eq!(rig.phase(), Phase::Listening);
    // The question said in the same breath is in the ring: the listener starts with it.
    assert_eq!(rig.wake_up(), [Call::ListenAfterBargeIn(1)]);
    let calls = rig.heard(1, "Hey Hex, wie spät ist es?");
    assert!(
        calls.contains(&Call::StartTurn {
            round: 1,
            thread_id: None,
            text: "Wie spät ist es?".to_owned(),
            delegate_notice_due: true,
        }),
        "the wake word is taken off the request: {calls:?}"
    );
}

#[test]
fn only_the_wake_word_then_a_pause_listens_for_the_question() {
    let mut rig = waiting_for_the_wake_word();
    rig.on(Input::WakeWord { watcher: 1 });
    rig.wake_up();
    assert_eq!(rig.heard(1, "Hey Hex."), [Call::Listen(2)]);
    let calls = rig.heard(2, "Wie wird das Wetter?");
    assert!(calls
        .iter()
        .any(|c| matches!(c, Call::StartTurn { text, .. } if text == "Wie wird das Wetter?")));
}

#[test]
fn after_a_pause_haex_waits_for_the_wake_word_again() {
    let mut rig = waiting_for_the_wake_word();
    rig.on(Input::WakeWord { watcher: 1 });
    rig.wake_up();
    let heard = rig.heard(1, "Hey Hex, mach Pause");
    assert!(
        !heard.contains(&Call::WatchWake(2)),
        "not while haex says goodbye: {heard:?}"
    );
    rig.on(Input::Played(1));
    let calls = rig.wake_up();
    assert!(!calls.contains(&Call::CloseAudio), "{calls:?}");
    assert!(calls.contains(&Call::WatchWake(2)), "{calls:?}");
    assert!(rig.core.is_open());
    assert_eq!(rig.phase(), Phase::Ready);
    let last = rig.ports.published.last().expect("published");
    assert!(last.enabled);
}

#[test]
fn a_report_of_an_old_wake_word_watcher_is_dropped() {
    let mut rig = waiting_for_the_wake_word();
    assert!(rig.on(Input::WakeWord { watcher: 7 }).is_empty());
    assert_eq!(rig.phase(), Phase::Ready);
}

#[test]
fn switching_off_stops_waiting_for_the_wake_word() {
    let mut rig = waiting_for_the_wake_word();
    let calls = rig.on(Input::SwitchOff);
    assert!(calls.contains(&Call::StopWake), "{calls:?}");
    assert_eq!(rig.wake_up(), [Call::CloseAudio]);
}

#[test]
fn muted_haex_does_not_wait_for_the_wake_word() {
    let mut rig = waiting_for_the_wake_word();
    let calls = rig.on(Input::Mute(true));
    assert!(calls.contains(&Call::StopWake), "{calls:?}");
}
