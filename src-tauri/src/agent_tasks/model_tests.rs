use super::model::{PaperNoteInput, PaperNoteOptions, TaskInput};

#[test]
fn task_input_round_trips_with_camel_case_wire_names() {
    let input = TaskInput {
        task: "paper-note-recognition".to_owned(),
        schema_version: 1,
        profile_id: Some("local-qwen-vl".to_owned()),
        input: PaperNoteInput {
            image: super::model::ImageInput {
                bytes: vec![1, 2, 3],
                mime_type: "image/jpeg".to_owned(),
                width: 1200,
                height: 800,
            },
            locale: "de-DE".to_owned(),
            options: PaperNoteOptions::default(),
        },
    };

    let wire = serde_json::to_value(&input).expect("serialize task input");
    assert_eq!(wire["schemaVersion"], 1);
    assert_eq!(wire["profileId"], "local-qwen-vl");
    assert_eq!(wire["input"]["image"]["mimeType"], "image/jpeg");
    assert_eq!(serde_json::from_value::<TaskInput>(wire).unwrap(), input);
}
