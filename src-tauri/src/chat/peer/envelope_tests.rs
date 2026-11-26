//! The text limits and the preview of a long text (spec 072 FR-012, research R2).

use super::*;

#[test]
fn a_text_up_to_the_bound_is_accepted_and_one_more_character_is_not() {
    let at_bound = "ä".repeat(MAX_TEXT_CHARS);
    assert_eq!(
        checked_text(&at_bound).expect("at the bound").len(),
        2 * MAX_TEXT_CHARS
    );

    let error = checked_text(&format!("{at_bound}ä")).expect_err("one over");
    assert!(error.to_string().contains("chat.error.tooLong"), "{error}");
}

#[test]
fn the_bound_counts_characters_not_bytes() {
    // 250,001 characters of 4 bytes are over a million bytes but under a million characters.
    let text = "😀".repeat(250_001);
    assert!(text.len() > MAX_TEXT_CHARS);
    assert!(checked_text(&text).is_ok());
}

#[test]
fn an_empty_text_is_refused() {
    assert!(checked_text("  \n ").is_err());
}

#[test]
fn a_short_text_is_its_own_preview() {
    assert_eq!(
        preview("  Kommst du heute Abend?  "),
        "Kommst du heute Abend?"
    );
    let exact = "a".repeat(PREVIEW_CHARS);
    assert_eq!(preview(&exact), exact);
}

#[test]
fn a_long_text_is_cut_at_a_word_boundary_within_the_limit() {
    let text = "wort ".repeat(1_000);
    let preview = preview(&text);
    assert!(
        preview.chars().count() <= PREVIEW_CHARS,
        "{}",
        preview.chars().count()
    );
    assert!(
        preview.ends_with("wort…"),
        "ends at a whole word: {preview:?}"
    );
    assert!(text.starts_with(preview.trim_end_matches('…')));
}

#[test]
fn a_long_text_without_spaces_is_cut_at_the_limit() {
    let text = "x".repeat(PREVIEW_CHARS + 50);
    let preview = preview(&text);
    assert_eq!(preview.chars().count(), PREVIEW_CHARS);
    assert!(preview.ends_with('…'));
}

#[test]
fn the_preview_never_cuts_inside_a_character() {
    let text = "ä😀".repeat(PREVIEW_CHARS);
    let preview = preview(&text);
    assert_eq!(preview.chars().count(), PREVIEW_CHARS);
    assert!(text.starts_with(preview.trim_end_matches('…')));
}

#[test]
fn the_inline_marker_is_left_out_when_false_and_read_when_absent() {
    let file = |inline: bool| FileAttachment {
        name: "message.txt".into(),
        mime: INLINE_MIME.into(),
        size: 5,
        sha256: "aa".into(),
        blossom: BlobRef {
            url: "https://b/aa".into(),
            sha256: "bb".into(),
            key: "cc".into(),
        },
        inline,
    };
    let plain = serde_json::to_string(&file(false)).expect("json");
    assert!(!plain.contains("inline"), "{plain}");
    let marked = serde_json::to_string(&file(true)).expect("json");
    assert!(marked.contains("\"inline\":true"), "{marked}");

    let read: FileAttachment = serde_json::from_str(&plain).expect("an older sender's file");
    assert!(!read.inline);
    let read: FileAttachment = serde_json::from_str(&marked).expect("marked file");
    assert!(read.inline);
}

/// Spec 099 contracts/nostr-chat-gate.md: a notice is a receipt with `answer` and no lists; a
/// receipt without `answer` reads as before.
#[test]
fn a_receipt_carries_an_answer_and_one_without_reads_as_before() {
    let notice = Receipt {
        v: VERSION,
        from: "aa".into(),
        card: "haex:card:x".into(),
        thread: To::Direct("bb".into()),
        delivered: vec![],
        read: vec![],
        answer: Some(Answer::Blocked),
        answer_sequence: Some(1),
        answer_decision_hlc: Some("revision".into()),
    };
    let json = serde_json::to_value(&notice).expect("serialize");
    assert_eq!(json["answer"], "blocked");
    assert_eq!(json["answerSequence"], 1);
    assert_eq!(json["answerDecisionHlc"], "revision");
    assert!(json.get("delivered").is_none() && json.get("read").is_none());
    let back: Receipt = serde_json::from_value(json).expect("parse");
    assert_eq!(back, notice);

    let old: Receipt = serde_json::from_str(
        r#"{"v":1,"from":"aa","card":"c","thread":{"direct":"bb"},"delivered":["e1"]}"#,
    )
    .expect("an old receipt");
    assert_eq!(old.answer, None);
    assert_eq!(old.answer_sequence, None);
    assert_eq!(old.delivered, ["e1"]);
    assert_eq!(
        Answer::parse(Answer::Accepted.as_str()),
        Some(Answer::Accepted)
    );
    assert_eq!(Answer::parse("maybe"), None);
}
