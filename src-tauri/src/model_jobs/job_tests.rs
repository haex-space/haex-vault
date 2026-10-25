//! Tests for parsing and checking a model job (spec 055, `contracts/job-format.md`).

use base64::Engine;
use serde_json::{json, Value};

use super::job::{parse, JobError, Need};
use super::limits::{MAX_IMAGES, MAX_STEPS};

fn b64(bytes: &[u8]) -> String {
    base64::engine::general_purpose::STANDARD.encode(bytes)
}

fn read_step() -> Value {
    json!({
        "id": "read",
        "needs": ["text", "image"],
        "instruction": "Zerlege die Seite in Regionen. Gib nur JSON aus.",
        "input": { "images": [ { "mimeType": "image/jpeg", "data": b64(&[1, 2, 3]) } ] },
        "format": {
            "name": "page-recognition", "version": 1,
            "schema": { "type": "object", "required": ["regions"], "properties": {
                "regions": { "type": "array", "items": { "type": "object", "properties": {
                    "text": { "type": "string" } } } } } }
        },
        "maxOutputTokens": 8192
    })
}

fn fix_step() -> Value {
    json!({
        "id": "fix",
        "needs": ["text"],
        "instruction": "Korrigiere nur Rechtschreibung.",
        "input": { "from": { "step": "read", "pointer": "/regions" } },
        "format": { "name": "text-fix", "version": 1,
                    "schema": { "type": "object", "properties": { "texts": { "type": "array", "items": { "type": "string" } } } } }
    })
}

fn job(steps: Vec<Value>) -> Value {
    json!({ "steps": steps })
}

/// A step changed by `change`.
fn step_with(base: Value, change: impl FnOnce(&mut Value)) -> Value {
    let mut step = base;
    change(&mut step);
    step
}

fn invalid(value: &Value) -> String {
    match parse(value) {
        Err(JobError::Invalid(why)) => why,
        other => panic!("expected an invalid job, got {other:?}"),
    }
}

#[test]
fn a_two_step_job_is_parsed_and_decoded() {
    let parsed = parse(&job(vec![read_step(), fix_step()])).expect("valid");

    assert_eq!(parsed.steps.len(), 2);
    let read = &parsed.steps[0];
    assert_eq!(read.id, "read");
    assert_eq!(read.needs, [Need::Text, Need::Image]);
    assert_eq!(read.input.images[0].bytes, [1, 2, 3]);
    assert_eq!(read.input.images[0].mime_type, "image/jpeg");
    assert_eq!(read.max_output_tokens, 8192);
    assert_eq!(read.format.name, "page-recognition");

    let fix = &parsed.steps[1];
    let source = fix.input.from.as_ref().expect("from");
    assert_eq!(
        (source.step.as_str(), source.pointer.as_str()),
        ("read", "/regions")
    );
    assert_eq!(fix.max_output_tokens, 4096);
}

#[test]
fn the_form_must_be_exact() {
    assert!(parse(&json!({})).is_err());
    assert!(parse(&json!({"steps": "x"})).is_err());
    assert!(matches!(parse(&job(vec![])), Err(JobError::Invalid(_))));
    // A key nobody knows is refused, not ignored.
    let mut extra = job(vec![read_step()]);
    extra["tools"] = json!(["shell"]);
    assert!(parse(&extra).is_err());
    let step = step_with(read_step(), |s| s["tools"] = json!([]));
    assert!(parse(&job(vec![step])).is_err());
}

#[test]
fn more_than_four_steps_is_a_limit() {
    let steps: Vec<Value> = (0..=MAX_STEPS)
        .map(|n| {
            step_with(fix_step(), |s| {
                s["id"] = json!(format!("s{n}"));
                s["input"] = json!({"text": "x"});
            })
        })
        .collect();
    assert_eq!(parse(&job(steps)), Err(JobError::Limit("steps")));
}

#[test]
fn step_ids_are_short_lowercase_and_unique() {
    for bad in ["", "Read", "1read", "re ad", "read.1", &"a".repeat(33)] {
        let step = step_with(read_step(), |s| s["id"] = json!(bad));
        assert!(invalid(&job(vec![step])).contains("step id"), "{bad:?}");
    }
    let twice = job(vec![
        read_step(),
        step_with(fix_step(), |s| s["id"] = json!("read")),
    ]);
    assert!(invalid(&twice).contains("twice"));
}

#[test]
fn needs_name_text_and_image_exactly_once_and_match_the_images() {
    for needs in [
        json!([]),
        json!(["image"]),
        json!(["text", "text"]),
        json!(["text", "audio"]),
    ] {
        let step = step_with(read_step(), |s| s["needs"] = needs.clone());
        assert!(parse(&job(vec![step])).is_err(), "{needs}");
    }
    // Images but no "image" need, and an "image" need without images.
    let no_need = step_with(read_step(), |s| s["needs"] = json!(["text"]));
    assert!(invalid(&job(vec![no_need])).contains("exactly when"));
    let no_images = step_with(read_step(), |s| s["input"] = json!({"text": "hallo"}));
    assert!(invalid(&job(vec![no_images])).contains("exactly when"));
}

#[test]
fn the_instruction_and_the_input_must_not_be_empty_and_have_a_size() {
    let blank = step_with(read_step(), |s| s["instruction"] = json!("   "));
    assert!(invalid(&job(vec![blank])).contains("instruction"));
    let long = step_with(read_step(), |s| {
        s["instruction"] = json!("x".repeat(16 * 1024 + 1))
    });
    assert_eq!(
        parse(&job(vec![long])),
        Err(JobError::Limit("instruction_bytes"))
    );
    let nothing = step_with(fix_step(), |s| s["input"] = json!({}));
    assert!(invalid(&job(vec![read_step(), nothing])).contains("input is empty"));
    let big_text = step_with(fix_step(), |s| {
        s["input"] = json!({"text": "x".repeat(256 * 1024 + 1)})
    });
    assert_eq!(
        parse(&job(vec![big_text])),
        Err(JobError::Limit("input_bytes"))
    );
}

#[test]
fn images_are_limited_in_count_size_type_and_form() {
    let image = || json!({ "mimeType": "image/png", "data": b64(&[9]) });
    let nine: Vec<Value> = (0..=MAX_IMAGES).map(|_| image()).collect();
    let step = step_with(read_step(), |s| s["input"] = json!({ "images": nine }));
    assert_eq!(parse(&job(vec![step])), Err(JobError::Limit("image_count")));

    // The count is over all steps.
    let five = |id: &str| {
        step_with(read_step(), |s| {
            s["id"] = json!(id);
            s["input"] = json!({ "images": (0..5).map(|_| image()).collect::<Vec<_>>() });
        })
    };
    assert_eq!(
        parse(&job(vec![five("a"), five("b")])),
        Err(JobError::Limit("image_count"))
    );

    let huge = b64(&vec![0u8; 25 * 1024 * 1024 + 1]);
    let step = step_with(read_step(), |s| {
        s["input"] = json!({ "images": [{"mimeType": "image/jpeg", "data": huge}] })
    });
    assert_eq!(parse(&job(vec![step])), Err(JobError::Limit("image_bytes")));

    let gif =
        step_with(
            read_step(),
            |s| s["input"] = json!({ "images": [{"mimeType": "image/gif", "data": b64(&[1])}] }),
        );
    assert!(invalid(&job(vec![gif])).contains("not supported"));
    let junk = step_with(read_step(), |s| {
        s["input"] = json!({ "images": [{"mimeType": "image/png", "data": "***"}] })
    });
    assert!(invalid(&job(vec![junk])).contains("Base64"));
    let empty = step_with(read_step(), |s| {
        s["input"] = json!({ "images": [{"mimeType": "image/png", "data": ""}] })
    });
    assert!(invalid(&job(vec![empty])).contains("empty"));
}

#[test]
fn from_must_name_an_earlier_step_and_a_pointer_that_can_occur() {
    let later = step_with(read_step(), |s| {
        s["input"] = json!({ "from": { "step": "fix", "pointer": "" }, "images": [{"mimeType": "image/png", "data": b64(&[1])}] });
    });
    assert!(invalid(&job(vec![later, fix_step()])).contains("not an earlier step"));
    let unknown = step_with(fix_step(), |s| s["input"]["from"]["step"] = json!("ghost"));
    assert!(invalid(&job(vec![read_step(), unknown])).contains("not an earlier step"));

    for pointer in [
        "regions",
        "/colour",
        "/regions/x",
        "/regions/01/text",
        "/regions/0/colour",
        "/regions/0/text/1",
    ] {
        let step = step_with(fix_step(), |s| {
            s["input"]["from"]["pointer"] = json!(pointer)
        });
        assert!(parse(&job(vec![read_step(), step])).is_err(), "{pointer}");
    }
    for pointer in ["", "/regions", "/regions/0", "/regions/12/text"] {
        let step = step_with(fix_step(), |s| {
            s["input"]["from"]["pointer"] = json!(pointer)
        });
        assert!(parse(&job(vec![read_step(), step])).is_ok(), "{pointer}");
    }

    let malformed_source = step_with(read_step(), |s| {
        s["format"]["schema"] = json!({
            "type": "object",
            "properties": { "a~2": { "type": "string" } }
        })
    });
    let malformed_pointer = step_with(fix_step(), |s| {
        s["input"]["from"]["pointer"] = json!("/a~2")
    });
    assert!(parse(&job(vec![malformed_source, malformed_pointer])).is_err());

    let open_source = step_with(read_step(), |s| {
        s["format"]["schema"] = json!({
            "type": "object",
            "additionalProperties": true
        })
    });
    let arbitrary_pointer = step_with(fix_step(), |s| {
        s["input"]["from"]["pointer"] = json!("/created-by-extension")
    });
    assert!(parse(&job(vec![open_source, arbitrary_pointer])).is_ok());
}

#[test]
fn the_format_needs_a_name_a_version_and_a_schema_in_the_subset() {
    let nameless = step_with(read_step(), |s| s["format"]["name"] = json!(" "));
    assert!(invalid(&job(vec![nameless])).contains("name"));
    let version = step_with(read_step(), |s| s["format"]["version"] = json!(0));
    assert!(invalid(&job(vec![version])).contains("version"));
    for keyword in ["pattern", "$ref", "oneOf"] {
        let schema = step_with(read_step(), |s| {
            s["format"]["schema"] =
                json!({"type": "object", "properties": {"a": {"type": "string", keyword: "x"}}});
        });
        let why = invalid(&job(vec![schema]));
        assert!(
            why.contains("format schema") && why.contains(keyword),
            "{why}"
        );
    }
}

#[test]
fn the_extension_cannot_wish_an_effort_and_the_answer_length_is_checked() {
    // The user chooses the effort with the model (spec 055 FR-036), the extension has no say.
    for wish in ["low", "max"] {
        let step = step_with(read_step(), |s| s["effort"] = json!(wish));
        assert!(invalid(&job(vec![step])).contains("effort"), "{wish}");
    }
    for tokens in [json!(0), json!(16_385), json!(-1), json!("many")] {
        let step = step_with(read_step(), |s| s["maxOutputTokens"] = tokens.clone());
        assert!(parse(&job(vec![step])).is_err(), "{tokens}");
    }
    let edge = step_with(read_step(), |s| s["maxOutputTokens"] = json!(16_384));
    assert!(parse(&job(vec![edge])).is_ok());
}

#[test]
fn a_pointer_into_an_open_object_or_an_array_without_items_fits() {
    for (schema, pointer) in [
        (json!({"type": "object"}), "/anything"),
        (json!({"type": "array"}), "/0"),
    ] {
        let source = step_with(read_step(), |s| s["format"]["schema"] = schema.clone());
        let pointing = step_with(fix_step(), |s| {
            s["input"]["from"]["pointer"] = json!(pointer)
        });
        assert!(parse(&job(vec![source, pointing])).is_ok(), "{pointer}");
    }
    // A closed object knows no other names.
    let closed = step_with(read_step(), |s| {
        s["format"]["schema"] = json!({"type": "object", "additionalProperties": false})
    });
    let pointing = step_with(fix_step(), |s| s["input"]["from"]["pointer"] = json!("/x"));
    assert!(parse(&job(vec![closed, pointing])).is_err());
}

#[test]
fn blank_input_text_alone_is_no_input() {
    let blank = step_with(fix_step(), |s| s["input"] = json!({"text": "  "}));
    assert!(matches!(
        parse(&job(vec![blank])),
        Err(JobError::Invalid(_))
    ));
}

#[test]
fn a_format_name_is_counted_in_characters_and_a_bad_value_is_not_echoed_whole() {
    let umlauts = step_with(fix_step(), |s| {
        s["input"] = json!({"text": "x"});
        s["format"]["name"] = json!("ä".repeat(64))
    });
    assert!(parse(&job(vec![umlauts])).is_ok());

    let huge = step_with(fix_step(), |s| {
        s["format"]["version"] = json!("x".repeat(100_000))
    });
    let Err(JobError::Invalid(message)) = parse(&job(vec![huge])) else {
        panic!("a string version must be refused");
    };
    assert!(
        message.chars().count() <= 301,
        "{}",
        message.chars().count()
    );
}
