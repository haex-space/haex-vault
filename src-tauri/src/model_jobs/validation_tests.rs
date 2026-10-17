use super::model::{
    NormalizedBox, PaperNoteRegion, PaperNoteRegionKind, PaperNoteResult, SpellingSuggestion,
};
use super::validate_paper_note_result;

fn text_region() -> PaperNoteRegion {
    PaperNoteRegion {
        kind: PaperNoteRegionKind::Text,
        source_box: NormalizedBox {
            x: 0.1,
            y: 0.1,
            width: 0.2,
            height: 0.2,
        },
        text: Some("valid".to_owned()),
        confidence: Some(0.8),
        suggestions: Vec::new(),
    }
}

#[test]
fn rejects_text_without_text() {
    let mut region = text_region();
    region.text = None;
    let result = PaperNoteResult::new(vec![region]);
    assert!(validate_paper_note_result(&result).is_err());
}

#[test]
fn rejects_regions_outside_normalized_canvas() {
    let mut region = text_region();
    region.source_box.x = 0.9;
    let result = PaperNoteResult::new(vec![region]);
    assert!(validate_paper_note_result(&result).is_err());
}

#[test]
fn rejects_invalid_suggestion_confidence() {
    let mut region = text_region();
    region.suggestions.push(SpellingSuggestion {
        original: "a".to_owned(),
        replacement: "b".to_owned(),
        confidence: Some(2.0),
    });
    let result = PaperNoteResult::new(vec![region]);
    assert!(validate_paper_note_result(&result).is_err());
}
