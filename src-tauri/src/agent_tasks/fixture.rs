use super::model::{
    NormalizedBox, PaperNoteInput, PaperNoteRegion, PaperNoteRegionKind, PaperNoteResult,
    SpellingSuggestion,
};

/// Deterministic stand-in for an image-capable model. It deliberately consumes
/// only declared task input and is used by host lifecycle tests before a real
/// Vision/OCR adapter is available.
pub(crate) fn run_fixture(input: &PaperNoteInput) -> PaperNoteResult {
    let mut regions = vec![PaperNoteRegion {
        kind: PaperNoteRegionKind::Text,
        source_box: NormalizedBox {
            x: 0.08,
            y: 0.1,
            width: 0.84,
            height: 0.16,
        },
        text: Some("Fixture text from paper note".to_owned()),
        confidence: Some(0.99),
        suggestions: if input.options.suggest_spelling {
            vec![SpellingSuggestion {
                original: "Fixture text".to_owned(),
                replacement: "Fixture text".to_owned(),
                confidence: Some(1.0),
            }]
        } else {
            Vec::new()
        },
    }];

    if input.options.detect_drawings {
        regions.push(PaperNoteRegion {
            kind: PaperNoteRegionKind::Drawing,
            source_box: NormalizedBox {
                x: 0.1,
                y: 0.4,
                width: 0.3,
                height: 0.3,
            },
            text: None,
            confidence: Some(0.9),
            suggestions: Vec::new(),
        });
    }

    PaperNoteResult::new(regions)
}
