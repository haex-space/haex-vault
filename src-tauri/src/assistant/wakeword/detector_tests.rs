use super::super::Sensitivity;
use super::*;

/// Embeddings carry the loudness of their last hop; the score of a window is the loudness of its
/// last embedding, so a test decides the scores with the audio it feeds.
#[derive(Default)]
struct Fake {
    embed_calls: Vec<usize>,
}

impl Scorer for Fake {
    fn embed(&mut self, audio: &[f32]) -> Vec<[f32; EMBEDDING]> {
        self.embed_calls.push(audio.len());
        let frames = audio.len() / HOP - FIRST_REAL;
        (0..frames)
            .map(|k| {
                let end = (k + FIRST_REAL + 1) * HOP;
                let mut frame = [0.0; EMBEDDING];
                frame[0] = audio[end - 1];
                frame
            })
            .collect()
    }

    fn score(&mut self, window: &[[f32; EMBEDDING]]) -> f32 {
        window.last().map_or(0.0, |frame| frame[0])
    }
}

const MEDIUM: Rule = Rule {
    windows: 2,
    threshold: 0.98,
};

fn hops(value: f32, count: usize) -> Vec<f32> {
    vec![value; count * HOP]
}

#[test]
fn nothing_runs_while_nobody_speaks() {
    let mut detector = Detector::new(Fake::default(), MEDIUM);
    assert!(!detector.push(&hops(1.0, 60), false, Echo::None));
    assert!(detector.scorer.embed_calls.is_empty());
}

#[test]
fn speech_starts_with_one_call_for_the_window_before_it() {
    let mut detector = Detector::new(Fake::default(), MEDIUM);
    detector.push(&hops(0.0, 30), false, Echo::None);
    detector.push(&hops(0.0, 1), true, Echo::None);
    assert_eq!(detector.scorer.embed_calls, vec![BACKFILL_SAMPLES]);
    detector.push(&hops(0.0, 2), true, Echo::None);
    assert_eq!(
        detector.scorer.embed_calls,
        vec![BACKFILL_SAMPLES, STREAM_SAMPLES, STREAM_SAMPLES]
    );
}

#[test]
fn it_fires_after_the_rules_number_of_windows() {
    let mut detector = Detector::new(Fake::default(), MEDIUM);
    detector.push(&hops(0.0, 30), true, Echo::None);
    assert!(
        !detector.push(&hops(1.0, 1), true, Echo::None),
        "one window is not enough"
    );
    assert!(
        detector.push(&hops(1.0, 1), true, Echo::None),
        "the second one fires"
    );
}

#[test]
fn a_window_below_the_threshold_starts_the_count_again() {
    let mut detector = Detector::new(Fake::default(), MEDIUM);
    detector.push(&hops(0.0, 30), true, Echo::None);
    assert!(!detector.push(&hops(1.0, 1), true, Echo::None));
    assert!(!detector.push(&hops(0.5, 1), true, Echo::None));
    assert!(!detector.push(&hops(1.0, 1), true, Echo::None));
    assert!(detector.push(&hops(1.0, 1), true, Echo::None));
}

#[test]
fn after_firing_it_stays_quiet_for_two_seconds() {
    let mut detector = Detector::new(Fake::default(), MEDIUM);
    detector.push(&hops(0.0, 30), true, Echo::None);
    assert!(detector.push(&hops(1.0, 2), true, Echo::None));
    assert!(
        !detector.push(&hops(1.0, 20), true, Echo::None),
        "within 2 s"
    );
    assert!(
        detector.push(&hops(1.0, 10), true, Echo::None),
        "after 2 s it may fire again"
    );
}

#[test]
fn it_runs_on_briefly_after_speech_ends() {
    let mut detector = Detector::new(Fake::default(), MEDIUM);
    detector.push(&hops(0.0, 30), true, Echo::None);
    // The word ends as the pause detection stops hearing speech; the classifier is surest later.
    assert!(detector.push(&hops(1.0, 2), false, Echo::None));
    detector.push(&hops(0.0, 40), false, Echo::None);
    let calls = detector.scorer.embed_calls.len();
    detector.push(&hops(1.0, 5), false, Echo::None);
    assert_eq!(
        detector.scorer.embed_calls.len(),
        calls,
        "long after speech nothing runs"
    );
}

#[test]
fn blocks_of_any_size_make_the_same_hops() {
    let mut whole = Detector::new(Fake::default(), MEDIUM);
    let mut pieces = Detector::new(Fake::default(), MEDIUM);
    let mut audio = hops(0.0, 30);
    audio.extend(hops(1.0, 3));
    let fired_whole = whole.push(&audio, true, Echo::None);
    let fired_pieces = audio
        .chunks(333)
        .fold(false, |f, c| pieces.push(c, true, Echo::None) | f);
    assert!(fired_whole && fired_pieces);
    assert_eq!(whole.scorer.embed_calls, pieces.scorer.embed_calls);
}

#[test]
fn sensitivities_go_from_strict_to_lenient() {
    assert_eq!(Sensitivity::parse(None), Sensitivity::Medium);
    assert_eq!(Sensitivity::parse(Some("high")), Sensitivity::High);
    assert_eq!(Sensitivity::parse(Some("loud")), Sensitivity::Medium);
    let (low, medium, high) = (
        Sensitivity::Low.rule(),
        Sensitivity::Medium.rule(),
        Sensitivity::High.rule(),
    );
    // Fewer windows or a lower threshold never makes it stricter.
    assert!(low.windows >= medium.windows && medium.windows >= high.windows);
    assert!(medium.threshold >= high.threshold);
}

#[test]
fn a_wake_word_said_right_after_the_start_is_evaluated() {
    // The watcher has heard no more than the word itself: the audio before it counts as silence.
    let mut detector = Detector::new(Fake::default(), MEDIUM);
    assert!(!detector.push(&hops(1.0, 1), true, Echo::None));
    assert!(detector.push(&hops(1.0, 1), true, Echo::None));
}

#[test]
fn own_output_the_microphone_hears_as_it_is_never_wakes_it() {
    let mut detector = Detector::new(Fake::default(), MEDIUM);
    detector.push(&hops(0.0, 30), true, Echo::None);
    // haex says "Hex" itself and nothing cancels it.
    assert!(!detector.push(&hops(1.0, 10), true, Echo::Raw));
    // The user says it right after: haex is still where the word is.
    assert!(!detector.push(&hops(1.0, 3), true, Echo::None));
    assert!(
        detector.push(&hops(1.0, 10), true, Echo::None),
        "once haex's output left the end of the window, the user wakes it"
    );
}

#[test]
fn own_output_earlier_in_the_window_only_makes_the_rule_strict() {
    let mut detector = Detector::new(Fake::default(), MEDIUM);
    detector.push(&hops(0.0, 30), true, Echo::None);
    // haex at the start of the window, then quiet, then the user.
    detector.push(&hops(0.0, 4), true, Echo::Raw);
    detector.push(&hops(0.0, 8), true, Echo::None);
    assert!(
        !detector.push(&hops(1.0, 2), true, Echo::None),
        "medium's 2 windows are not enough"
    );
    assert!(
        detector.push(&hops(1.0, 1), true, Echo::None),
        "the strict rule's third fires"
    );
}

#[test]
fn over_cancelled_output_the_user_can_still_wake_it_by_the_strict_rule() {
    let mut detector = Detector::new(Fake::default(), MEDIUM);
    detector.push(&hops(0.0, 30), true, Echo::None);
    assert!(!detector.push(&hops(1.0, 2), true, Echo::Cancelled));
    assert!(detector.push(&hops(1.0, 1), true, Echo::Cancelled));

    let mut detector = Detector::new(Fake::default(), Sensitivity::High.rule());
    detector.push(&hops(0.0, 30), true, Echo::None);
    assert!(
        !detector.push(&hops(0.9, 10), true, Echo::Cancelled),
        "high's threshold of 0.5 does not hold while haex plays"
    );
    assert!(
        detector.push(&hops(0.9, 20), true, Echo::None),
        "it does again once haex's output left the window"
    );
}
