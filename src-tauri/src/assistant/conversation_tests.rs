//! Spec 066 T033: the conversation state machine with an injected clock: every transition of the
//! diagram in data-model.md, listening without a time limit until the user sends haex to pause
//! (operator 2026-11-02), the continuation window and mute/pause as cross states.

use std::time::{Duration, Instant};

use uuid::Uuid;

use super::conversation::{
    AssistantState, Effect, Event, Line, OpenQuestion, Phase, QuestionKind, Timing, Utterance,
    WORKING_AFTER,
};
use super::tones::ToneId;

/// A machine with the default continuation window (5 min) and a clock at `t0`.
struct Run {
    state: AssistantState,
    t0: Instant,
}

impl Run {
    fn new() -> Self {
        Self::with(Timing::default())
    }

    fn with(timing: Timing) -> Self {
        Self {
            state: AssistantState::new(timing),
            t0: Instant::now(),
        }
    }

    fn at(&self, ms: u64) -> Instant {
        self.t0 + Duration::from_millis(ms)
    }

    fn send(&mut self, event: Event, ms: u64) -> Vec<Effect> {
        let now = self.at(ms);
        self.state.handle(event, now)
    }

    fn phase(&self) -> Phase {
        self.state.phase()
    }

    /// On, woken, asked something; the round runs in `Thinking`.
    fn asking(ms: u64) -> Self {
        let mut run = Self::new();
        run.send(Event::SwitchOn, 0);
        run.send(Event::Wake, ms);
        run.send(request("Wie wird das Wetter?"), ms);
        run
    }

    /// A full round that answered and left the conversation in `FollowUp`.
    fn answered(ms: u64) -> (Self, Uuid) {
        let mut run = Self::asking(0);
        let thread = Uuid::new_v4();
        run.send(Event::ThreadStarted(thread), 10);
        run.send(Event::FirstWords, 500);
        run.send(Event::TurnEnded, 600);
        run.send(Event::OutputDone, ms);
        (run, thread)
    }
}

fn request(text: &str) -> Event {
    Event::Utterance(Utterance::Request(text.to_owned()))
}

fn question(id: &str) -> OpenQuestion {
    OpenQuestion {
        id: id.to_owned(),
        kind: QuestionKind::Approval,
        text: "Soll ich die Mail senden?".to_owned(),
    }
}

/// An hour: silence this long ends nothing.
const HOUR: u64 = 3_600_000;

fn pause() -> Event {
    Event::Utterance(Utterance::End)
}

#[test]
fn switching_on_and_off() {
    let mut run = Run::new();
    assert_eq!(run.phase(), Phase::Off);
    assert!(
        run.send(Event::Wake, 0).is_empty(),
        "nothing listens while off"
    );
    assert_eq!(run.send(Event::SwitchOn, 0), [Effect::Tone(ToneId::ModeOn)]);
    assert_eq!(run.phase(), Phase::Ready);
    assert!(run.send(Event::SwitchOn, 1).is_empty());
    assert_eq!(
        run.send(Event::SwitchOff, 2),
        [Effect::Tone(ToneId::ModeOff)]
    );
    assert_eq!(run.phase(), Phase::Off);
}

#[test]
fn a_round_goes_through_listening_thinking_speaking_and_follow_up_until_paused() {
    let mut run = Run::new();
    run.send(Event::SwitchOn, 0);
    assert_eq!(
        run.send(Event::Wake, 100),
        [Effect::Tone(ToneId::Listening), Effect::Listen]
    );
    assert_eq!(run.phase(), Phase::Listening);

    assert_eq!(
        run.send(request("Wie wird das Wetter?"), 2_000),
        [
            Effect::Tone(ToneId::Understood),
            Effect::Listen,
            Effect::StartTurn {
                thread_id: None,
                text: "Wie wird das Wetter?".to_owned()
            },
        ]
    );
    assert_eq!(run.phase(), Phase::Thinking);
    let thread = Uuid::new_v4();
    run.send(Event::ThreadStarted(thread), 2_010);

    assert_eq!(
        run.send(Event::FirstWords, 3_000),
        [Effect::StopListening, Effect::SpeakAnswer]
    );
    assert_eq!(run.phase(), Phase::Speaking);
    assert!(
        run.send(Event::FirstWords, 3_200).is_empty(),
        "already speaking"
    );
    assert!(
        run.send(Event::TurnEnded, 3_500).is_empty(),
        "still speaking"
    );
    assert_eq!(run.phase(), Phase::Speaking);

    assert_eq!(run.send(Event::OutputDone, 5_000), [Effect::Listen]);
    assert_eq!(run.phase(), Phase::FollowUp);
    assert_eq!(run.state.next_deadline(), None, "no time limit");
    assert!(
        run.send(Event::Tick, HOUR).is_empty(),
        "silence ends nothing"
    );
    assert_eq!(run.phase(), Phase::FollowUp);

    assert_eq!(
        run.send(pause(), HOUR + 1),
        [
            Effect::StopListening,
            Effect::Ended {
                thread_id: Some(thread)
            },
            Effect::Say(Line::Goodbye),
            Effect::Tone(ToneId::ConversationEnd),
        ]
    );
    assert_eq!(run.phase(), Phase::Ready);
}

#[test]
fn an_utterance_after_any_silence_is_the_next_round_of_the_same_thread() {
    let (mut run, thread) = Run::answered(1_000);
    let effects = run.send(request("Und am Wochenende?"), HOUR);
    assert!(effects.contains(&Effect::StartTurn {
        thread_id: Some(thread),
        text: "Und am Wochenende?".to_owned()
    }));
    assert_eq!(run.phase(), Phase::Thinking);
    assert_eq!(
        run.state.next_deadline(),
        Some(run.at(HOUR) + WORKING_AFTER)
    );
}

#[test]
fn a_wake_word_while_following_up_listens_again() {
    let (mut run, _) = Run::answered(1_000);
    assert_eq!(
        run.send(Event::Wake, 2_000),
        [Effect::Tone(ToneId::Listening), Effect::Listen]
    );
    assert_eq!(run.phase(), Phase::Listening);
    assert_eq!(run.state.next_deadline(), None);
}

#[test]
fn a_round_without_words_after_four_seconds_is_working_and_announces_progress() {
    let mut run = Run::asking(0);
    assert!(run.send(Event::Tick, 3_999).is_empty());
    assert_eq!(
        run.send(Event::Tick, 4_000),
        [
            Effect::Tone(ToneId::Working),
            Effect::Say(Line::Progress),
            Effect::Listen
        ]
    );
    assert_eq!(run.phase(), Phase::Working);
    assert!(run.send(Event::Tick, 9_000).is_empty(), "announced once");
    assert_eq!(
        run.send(Event::FirstWords, 9_500),
        [Effect::StopListening, Effect::SpeakAnswer]
    );
    assert_eq!(run.phase(), Phase::Speaking);
}

#[test]
fn a_round_that_ends_without_words_goes_to_the_follow_up_window() {
    let mut run = Run::asking(0);
    run.send(Event::Tick, 5_000);
    assert_eq!(run.send(Event::TurnEnded, 6_000), [Effect::Listen]);
    assert_eq!(run.phase(), Phase::FollowUp);
}

#[test]
fn output_that_ran_dry_before_the_turn_ended_closes_the_round_at_the_end() {
    let mut run = Run::asking(0);
    run.send(Event::FirstWords, 500);
    assert!(
        run.send(Event::OutputDone, 900).is_empty(),
        "the turn still runs"
    );
    assert_eq!(run.phase(), Phase::Speaking);
    assert!(
        run.send(Event::FirstWords, 1_200).is_empty(),
        "still the same answer"
    );
    run.send(Event::OutputDone, 1_500);
    assert_eq!(run.send(Event::TurnEnded, 2_000), [Effect::Listen]);
    assert_eq!(run.phase(), Phase::FollowUp);
}

#[test]
fn an_open_question_keeps_listening_for_its_answer() {
    let mut run = Run::asking(0);
    run.send(Event::Question(question("r1")), 1_000);
    assert_eq!(run.phase(), Phase::AwaitingAnswer);
    assert_eq!(run.state.next_deadline(), None);
    assert!(run.send(Event::Tick, HOUR).is_empty());
    assert_eq!(run.state.open_question().map(|q| q.id.as_str()), Some("r1"));
    let effects = run.send(request("ja"), HOUR + 1);
    assert!(effects.contains(&Effect::AnswerQuestion {
        id: "r1".to_owned(),
        text: "ja".to_owned()
    }));
}

#[test]
fn an_open_question_is_asked_first_on_the_next_wake_word_and_answered_by_voice() {
    let mut run = Run::asking(0);
    assert_eq!(
        run.send(Event::Question(question("r1")), 1_000),
        [
            Effect::StopListening,
            Effect::Tone(ToneId::Question),
            Effect::AskQuestion("r1".to_owned()),
            Effect::Listen,
        ]
    );
    assert_eq!(
        run.send(Event::Wake, 60_000),
        [
            Effect::Tone(ToneId::Question),
            Effect::AskQuestion("r1".to_owned()),
            Effect::Listen,
        ]
    );
    assert_eq!(
        run.send(request("ja, senden"), 62_000),
        [
            Effect::Tone(ToneId::Understood),
            Effect::StopListening,
            Effect::AnswerQuestion {
                id: "r1".to_owned(),
                text: "ja, senden".to_owned()
            },
            Effect::Listen,
        ]
    );
    assert_eq!(run.phase(), Phase::Thinking, "the turn goes on");
    assert_eq!(run.state.open_question(), None);
}

#[test]
fn questions_are_asked_in_order() {
    let mut run = Run::asking(0);
    run.send(Event::Question(question("r1")), 1_000);
    assert!(run.send(Event::Question(question("r2")), 1_100).is_empty());
    let effects = run.send(request("ja"), 2_000);
    assert!(effects.contains(&Effect::AskQuestion("r2".to_owned())));
    assert_eq!(run.phase(), Phase::AwaitingAnswer);
}

#[test]
fn resolving_a_queued_question_does_not_repeat_the_current_question() {
    let mut run = Run::asking(0);
    run.send(Event::Question(question("r1")), 1_000);
    run.send(Event::Question(question("r2")), 1_100);

    assert!(run
        .send(Event::QuestionResolved("r2".to_owned()), 1_200)
        .is_empty());
    assert_eq!(run.state.open_question().map(|q| q.id.as_str()), Some("r1"));
}

#[test]
fn a_late_question_from_an_aborted_turn_is_ignored() {
    let mut run = Run::asking(0);
    run.send(Event::Stop, 900);

    assert!(run
        .send(Event::Question(question("stale")), 1_000)
        .is_empty());
    assert_eq!(run.state.open_question(), None);
    assert_eq!(run.phase(), Phase::FollowUp);
}

#[test]
fn a_question_answered_in_the_window_lets_the_turn_go_on() {
    let mut run = Run::asking(0);
    run.send(Event::Question(question("r1")), 1_000);
    assert!(run
        .send(Event::QuestionResolved("other".to_owned()), 1_500)
        .is_empty());
    assert_eq!(
        run.send(Event::QuestionResolved("r1".to_owned()), 2_000),
        [Effect::Listen],
        "listening goes on while the round runs"
    );
    assert_eq!(run.phase(), Phase::Thinking);
}

#[test]
fn a_question_asked_while_speaking_comes_after_the_answer_so_far() {
    let mut run = Run::asking(0);
    run.send(Event::FirstWords, 500);
    assert!(run.send(Event::Question(question("r1")), 800).is_empty());
    assert_eq!(run.phase(), Phase::Speaking);
    assert_eq!(
        run.send(Event::OutputDone, 2_000),
        [
            Effect::Tone(ToneId::Question),
            Effect::AskQuestion("r1".to_owned()),
            Effect::Listen,
        ]
    );
    assert_eq!(run.phase(), Phase::AwaitingAnswer);
}

#[test]
fn continuation_reuses_the_thread_within_the_window() {
    let (mut run, thread) = Run::answered(1_000);
    let ended = 9_000;
    run.send(pause(), ended);
    assert_eq!(run.phase(), Phase::Ready);

    run.send(Event::Wake, ended + 5 * 60_000);
    let effects = run.send(request("Und morgen?"), ended + 5 * 60_000);
    assert!(effects.contains(&Effect::StartTurn {
        thread_id: Some(thread),
        text: "Und morgen?".to_owned()
    }));
}

#[test]
fn after_the_continuation_window_a_new_conversation_begins() {
    let (mut run, _) = Run::answered(1_000);
    let ended = 9_000;
    run.send(pause(), ended);
    run.send(Event::Wake, ended + 5 * 60_000 + 1);
    let effects = run.send(request("Und morgen?"), ended + 5 * 60_000 + 1);
    assert!(effects.contains(&Effect::StartTurn {
        thread_id: None,
        text: "Und morgen?".to_owned()
    }));
}

#[test]
fn a_continuation_window_of_zero_never_continues() {
    let mut run = Run::with(Timing::from_prefs(Some(0)));
    run.send(Event::SwitchOn, 0);
    run.send(Event::Wake, 0);
    run.send(request("Hallo"), 0);
    run.send(Event::ThreadStarted(Uuid::new_v4()), 1);
    run.send(Event::EndConversation, 2);
    run.send(Event::Wake, 2);
    assert_eq!(
        run.state.conversation.as_ref().and_then(|c| c.thread_id),
        None
    );
}

#[test]
fn timing_takes_defaults_and_clamps() {
    let default = Timing::default();
    assert_eq!(default.continue_for, Duration::from_secs(5 * 60));
    assert_eq!(Timing::from_prefs(Some(-4)).continue_for, Duration::ZERO);
    assert_eq!(
        Timing::from_prefs(Some(999)).continue_for,
        Duration::from_secs(60 * 60)
    );
}

#[test]
fn an_empty_utterance_asks_to_repeat_and_leaves_no_thread() {
    let mut run = Run::new();
    run.send(Event::SwitchOn, 0);
    run.send(Event::Wake, 0);
    assert_eq!(
        run.send(Event::Utterance(Utterance::Empty), 3_000),
        [
            Effect::Tone(ToneId::NotUnderstood),
            Effect::Say(Line::Repeat),
            Effect::Listen,
        ]
    );
    assert_eq!(run.phase(), Phase::FollowUp);
    assert!(run.send(Event::Tick, HOUR).is_empty());
    assert_eq!(
        run.send(pause(), HOUR + 1),
        [
            Effect::StopListening,
            Effect::Ended { thread_id: None },
            Effect::Say(Line::Goodbye),
            Effect::Tone(ToneId::ConversationEnd),
        ]
    );
    assert_eq!(run.state.last_ended, None);
}

#[test]
fn an_end_phrase_says_goodbye_and_ends() {
    let (mut run, thread) = Run::answered(1_000);
    assert_eq!(
        run.send(Event::Utterance(Utterance::End), 2_000),
        [
            Effect::StopListening,
            Effect::Ended {
                thread_id: Some(thread)
            },
            Effect::Say(Line::Goodbye),
            Effect::Tone(ToneId::ConversationEnd),
        ]
    );
    assert_eq!(run.phase(), Phase::Ready);
    assert_eq!(run.state.last_ended, Some((thread, run.at(2_000))));
}

#[test]
fn a_stop_word_while_speaking_aborts_the_round_and_keeps_the_conversation() {
    let mut run = Run::asking(0);
    run.send(Event::FirstWords, 500);
    assert_eq!(
        run.send(Event::Stop, 900),
        [
            Effect::StopOutput,
            Effect::AbortTurn,
            Effect::Tone(ToneId::Aborted),
            Effect::Listen,
        ]
    );
    assert_eq!(run.phase(), Phase::FollowUp);
    assert!(
        run.send(Event::TurnEnded, 950).is_empty(),
        "already aborted"
    );
}

#[test]
fn a_stop_word_without_a_running_round_resumes_listening() {
    let mut run = Run::new();
    run.send(Event::SwitchOn, 0);
    run.send(Event::Wake, 100);

    assert_eq!(
        run.send(Event::Utterance(Utterance::Stop), 900),
        [Effect::Listen]
    );
    assert_eq!(run.phase(), Phase::FollowUp);
    assert_eq!(run.state.next_deadline(), None);
}

#[test]
fn a_stop_word_while_thinking_aborts_without_output() {
    let mut run = Run::asking(0);
    assert_eq!(
        run.send(Event::Stop, 900),
        [
            Effect::AbortTurn,
            Effect::Tone(ToneId::Aborted),
            Effect::Listen
        ]
    );
    assert_eq!(run.phase(), Phase::FollowUp);
}

#[test]
fn a_stop_word_while_awaiting_an_answer_aborts_the_round_with_its_question() {
    let mut run = Run::asking(0);
    run.send(Event::Question(question("r1")), 1_000);
    let effects = run.send(Event::Utterance(Utterance::Stop), 2_000);
    assert_eq!(
        effects,
        [
            Effect::AbortTurn,
            Effect::Tone(ToneId::Aborted),
            Effect::Listen
        ]
    );
    assert_eq!(run.phase(), Phase::FollowUp);
    assert_eq!(run.state.open_question(), None);
}

#[test]
fn barging_in_stops_the_output_and_listens() {
    let mut run = Run::asking(0);
    run.send(Event::FirstWords, 500);
    assert_eq!(
        run.send(Event::BargeIn, 900),
        [
            Effect::StopOutput,
            Effect::AbortTurn,
            Effect::Tone(ToneId::Listening),
            Effect::Listen,
        ]
    );
    assert_eq!(run.phase(), Phase::Listening);
    assert!(
        run.send(Event::BargeIn, 950).is_empty(),
        "only while speaking"
    );
}

#[test]
fn barging_in_discards_questions_from_the_aborted_turn() {
    let mut run = Run::asking(0);
    run.send(Event::FirstWords, 500);
    run.send(Event::Question(question("r1")), 600);

    run.send(Event::BargeIn, 900);

    assert_eq!(run.state.open_question(), None);
}

#[test]
fn ending_actively_aborts_a_running_round_and_says_what_was_done() {
    let mut run = Run::asking(0);
    run.send(Event::Tick, 5_000);
    assert_eq!(
        run.send(Event::EndConversation, 6_000),
        [
            Effect::AbortTurn,
            Effect::StopListening,
            Effect::Ended { thread_id: None },
            Effect::Say(Line::DoneSoFar),
            Effect::Tone(ToneId::ConversationEnd),
        ]
    );
    assert_eq!(run.phase(), Phase::Ready);
}

#[test]
fn muting_without_a_round_ends_the_conversation() {
    let (mut run, thread) = Run::answered(1_000);
    assert_eq!(
        run.send(Event::Mute(true), 2_000),
        [
            Effect::Tone(ToneId::Muted),
            Effect::StopListening,
            Effect::Ended {
                thread_id: Some(thread)
            },
        ]
    );
    assert_eq!(run.phase(), Phase::Ready);
    assert!(run.state.muted());
    assert!(
        run.send(Event::Wake, 3_000).is_empty(),
        "muted haex does not listen"
    );
}

#[test]
fn muted_with_a_round_shows_the_answer_and_listens_again_after_unmuting() {
    let mut run = Run::asking(0);
    run.send(Event::Mute(true), 500);
    assert_eq!(run.phase(), Phase::Thinking, "the round goes on");
    assert_eq!(run.send(Event::FirstWords, 1_000), [Effect::ShowAnswer]);
    assert!(run.send(Event::TurnEnded, 2_000).is_empty());
    assert_eq!(run.phase(), Phase::FollowUp);
    assert!(run.send(Event::Tick, 600_000).is_empty());

    assert_eq!(
        run.send(Event::Mute(false), 600_000),
        [Effect::Tone(ToneId::Unmuted), Effect::Listen]
    );
    assert_eq!(run.state.next_deadline(), None);
    assert!(run.send(Event::Tick, HOUR).is_empty());
    assert_eq!(run.phase(), Phase::FollowUp);
}

#[test]
fn muting_while_speaking_stops_the_output() {
    let mut run = Run::asking(0);
    run.send(Event::FirstWords, 500);
    let effects = run.send(Event::Mute(true), 900);
    assert!(effects.contains(&Effect::StopOutput));
    assert_eq!(run.phase(), Phase::Working);
}

#[test]
fn a_question_while_muted_is_only_shown_and_asked_on_the_next_wake_word() {
    let mut run = Run::asking(0);
    run.send(Event::Mute(true), 500);
    assert!(run.send(Event::Question(question("r1")), 1_000).is_empty());
    assert_eq!(run.phase(), Phase::AwaitingAnswer);
    // Unmuting listens again but does not repeat the question on its own.
    assert_eq!(
        run.send(Event::Mute(false), 5_000),
        [Effect::Tone(ToneId::Unmuted), Effect::Listen]
    );
    let effects = run.send(Event::Wake, 20_000);
    assert!(effects.contains(&Effect::AskQuestion("r1".to_owned())));
}

#[test]
fn pausing_holds_the_conversation_and_resuming_listens_again() {
    let mut run = Run::asking(0);
    run.send(Event::FirstWords, 500);
    run.send(Event::TurnEnded, 600);
    let effects = run.send(Event::Pause(true), 900);
    assert_eq!(effects, [Effect::StopListening, Effect::StopOutput]);
    assert!(run.state.paused());
    assert_eq!(run.phase(), Phase::FollowUp);
    assert!(run.send(Event::Tick, 600_000).is_empty());
    assert_eq!(run.send(Event::Pause(false), 600_000), [Effect::Listen]);
    assert_eq!(run.phase(), Phase::FollowUp);
}

#[test]
fn closing_the_vault_ends_everything_from_any_phase() {
    let mut run = Run::asking(0);
    run.send(Event::FirstWords, 500);
    assert_eq!(
        run.send(Event::VaultClosed, 900),
        [
            Effect::StopOutput,
            Effect::AbortTurn,
            Effect::StopListening,
            Effect::Ended { thread_id: None },
        ]
    );
    assert_eq!(run.phase(), Phase::Off);
}

#[test]
fn switching_off_mid_round_aborts_it() {
    let mut run = Run::asking(0);
    let effects = run.send(Event::SwitchOff, 900);
    assert_eq!(
        effects,
        [
            Effect::AbortTurn,
            Effect::StopListening,
            Effect::Ended { thread_id: None },
            Effect::Tone(ToneId::ModeOff),
        ]
    );
    assert_eq!(run.phase(), Phase::Off);
}

#[test]
fn phases_go_to_the_window_in_camel_case() {
    let names: Vec<String> = [
        Phase::Off,
        Phase::Ready,
        Phase::Listening,
        Phase::Thinking,
        Phase::Working,
        Phase::Speaking,
        Phase::FollowUp,
        Phase::AwaitingAnswer,
    ]
    .iter()
    .map(|p| {
        serde_json::to_value(p)
            .expect("phase")
            .as_str()
            .expect("text")
            .to_owned()
    })
    .collect();
    assert_eq!(
        names,
        [
            "off",
            "ready",
            "listening",
            "thinking",
            "working",
            "speaking",
            "followUp",
            "awaitingAnswer"
        ]
    );
}

/// Operator 2026-11-06: haex keeps listening while a round runs. A further request waits and is the
/// next round as soon as the answer is over (spec edge case "Neues Anliegen, während eine Aufgabe
/// läuft").
#[test]
fn a_request_while_the_round_runs_is_the_next_round_once_the_answer_is_over() {
    let mut run = Run::asking(0);
    let thread = Uuid::new_v4();
    run.send(Event::ThreadStarted(thread), 10);

    assert_eq!(
        run.send(request("Und wie viele Einwohner hat das Land?"), 1_000),
        [Effect::Tone(ToneId::Understood), Effect::Listen],
        "acknowledged and noted, nothing starts yet"
    );
    assert_eq!(run.phase(), Phase::Thinking, "the first round goes on");

    assert_eq!(
        run.send(Event::FirstWords, 2_000),
        [Effect::StopListening, Effect::SpeakAnswer],
        "no listening while haex speaks"
    );
    run.send(Event::TurnEnded, 2_500);
    assert_eq!(
        run.send(Event::OutputDone, 4_000),
        [
            Effect::StartTurn {
                thread_id: Some(thread),
                text: "Und wie viele Einwohner hat das Land?".to_owned()
            },
            Effect::Listen,
        ]
    );
    assert_eq!(run.phase(), Phase::Thinking);
    assert_eq!(
        run.state.next_deadline(),
        Some(run.at(4_000) + WORKING_AFTER)
    );
}

#[test]
fn requests_said_while_working_go_out_together_in_order() {
    let mut run = Run::asking(0);
    run.send(Event::Tick, 4_000);
    assert_eq!(run.phase(), Phase::Working);
    run.send(request("Und wie viele Einwohner?"), 5_000);
    run.send(request("Und welche Sprache spricht man dort?"), 6_000);

    let effects = run.send(Event::TurnEnded, 7_000);
    assert_eq!(
        effects,
        [
            Effect::StartTurn {
                thread_id: None,
                text: "Und wie viele Einwohner? Und welche Sprache spricht man dort?".to_owned()
            },
            Effect::Listen,
        ],
        "a round without words is over at once"
    );
}

#[test]
fn a_stop_word_while_the_round_runs_aborts_it_and_what_was_noted() {
    let mut run = Run::asking(0);
    run.send(request("Und wie viele Einwohner?"), 1_000);
    let effects = run.send(Event::Utterance(Utterance::Stop), 2_000);
    assert_eq!(
        effects,
        [
            Effect::AbortTurn,
            Effect::Tone(ToneId::Aborted),
            Effect::Listen
        ]
    );
    assert_eq!(run.phase(), Phase::FollowUp);
    assert!(
        run.send(Event::TurnEnded, 2_500).is_empty(),
        "the noted request is gone too"
    );
}

#[test]
fn an_empty_utterance_while_the_round_runs_is_ignored_without_asking_again() {
    let mut run = Run::asking(0);
    assert_eq!(
        run.send(Event::Utterance(Utterance::Empty), 1_000),
        [Effect::Listen]
    );
    assert_eq!(run.phase(), Phase::Thinking);
}

#[test]
fn an_end_phrase_while_the_round_runs_ends_the_conversation() {
    let mut run = Run::asking(0);
    let effects = run.send(pause(), 1_000);
    assert!(effects.contains(&Effect::AbortTurn));
    assert!(effects.contains(&Effect::Say(Line::Goodbye)));
    assert_eq!(run.phase(), Phase::Ready);
}

#[test]
fn listening_pauses_for_the_progress_line_and_goes_on_after_it() {
    let mut run = Run::asking(0);
    assert_eq!(
        run.send(Event::Tick, 4_000),
        [
            Effect::Tone(ToneId::Working),
            Effect::Say(Line::Progress),
            Effect::Listen
        ]
    );
}
