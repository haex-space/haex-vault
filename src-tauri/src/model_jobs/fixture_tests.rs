use super::{run_fixture, validate_paper_note_result};
use crate::model_jobs::model::{ImageInput, PaperNoteInput, PaperNoteOptions};

fn input() -> PaperNoteInput {
    PaperNoteInput {
        image: ImageInput {
            bytes: vec![0, 1, 2, 3],
            mime_type: "image/png".to_owned(),
            width: 100,
            height: 100,
        },
        locale: "de-DE".to_owned(),
        options: PaperNoteOptions::default(),
    }
}

#[test]
fn fixture_produces_schema_valid_regions() {
    let result = run_fixture(&input());
    validate_paper_note_result(&result).expect("fixture result validates");
    assert_eq!(result.regions.len(), 2);
}

#[test]
fn fixture_honours_drawing_option() {
    let mut input = input();
    input.options.detect_drawings = false;
    assert_eq!(run_fixture(&input).regions.len(), 1);
}
