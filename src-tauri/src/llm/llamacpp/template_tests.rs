use serde_json::json;

use super::{prepare, render, RenderInput, TemplateError};

fn input<'a>(tools: Vec<serde_json::Value>, enable_thinking: bool) -> RenderInput<'a> {
    RenderInput {
        messages: vec![json!({"role": "user", "content": "  hello  "})],
        tools,
        enable_thinking,
        bos_token: "<s>",
        eos_token: "</s>",
    }
}

#[test]
fn the_reversing_slice_becomes_the_reverse_filter() {
    assert_eq!(
        prepare("{% for m in messages[::-1] %}{{ m }}{% endfor %}"),
        "{% for m in messages|reverse %}{{ m }}{% endfor %}"
    );
}

#[test]
fn a_reversing_slice_renders_the_messages_from_the_end() {
    let mut two = input(vec![], false);
    two.messages = vec![json!({"content": "a"}), json!({"content": "b"})];
    let out = render(
        "{% for m in messages[::-1] %}{{ m.content }}{% endfor %}",
        &two,
    )
    .unwrap();
    assert_eq!(out, "ba");
}

#[test]
fn the_context_variables_reach_the_template() {
    let out = render(
        "{{ bos_token }}|{{ eos_token }}|{{ add_generation_prompt }}|{{ enable_thinking }}",
        &input(vec![], true),
    )
    .unwrap();
    // minijinja prints booleans the Python way, as Hugging Face templates expect.
    assert_eq!(out, "<s>|</s>|True|True");
}

#[test]
fn tools_are_undefined_without_tools_and_a_list_with_them() {
    let template = "{% if tools %}Y{{ tools|length }}{% else %}N{% endif %}";
    assert_eq!(render(template, &input(vec![], false)).unwrap(), "N");
    let tools = vec![json!({"type": "function"})];
    assert_eq!(render(template, &input(tools, false)).unwrap(), "Y1");
}

#[test]
fn tojson_is_compact_by_default_and_pretty_with_an_indent() {
    let tools = vec![json!({"a": 1, "b": [true]})];
    let compact = render("{{ tools[0]|tojson }}", &input(tools.clone(), false)).unwrap();
    assert_eq!(compact, r#"{"a":1,"b":[true]}"#);
    let pretty = render(
        "{{ tools[0]|tojson(indent=2) }}",
        &input(tools.clone(), false),
    )
    .unwrap();
    assert_eq!(pretty, "{\n  \"a\": 1,\n  \"b\": [\n    true\n  ]\n}");
    // Keyword arguments the Hugging Face filter takes are accepted.
    render(
        "{{ tools[0]|tojson(ensure_ascii=False, sort_keys=True) }}",
        &input(tools, false),
    )
    .unwrap();
}

#[test]
fn tojson_does_not_escape_non_ascii_text() {
    let tools = vec![json!({"name": "Küche"})];
    let out = render("{{ tools[0]|tojson }}", &input(tools, false)).unwrap();
    assert_eq!(out, r#"{"name":"Küche"}"#);
}

#[test]
fn python_string_methods_work() {
    let out = render(
        "{{ messages[0].content.strip() }}|{{ messages[0].content.strip().startswith('he') }}|{{ 'a,b'.split(',')|length }}",
        &input(vec![], false),
    )
    .unwrap();
    assert_eq!(out, "hello|True|2");
}

#[test]
fn raise_exception_is_a_render_error_with_the_templates_text() {
    match render(
        "{{ raise_exception('no system role') }}",
        &input(vec![], false),
    ) {
        Err(TemplateError::Render(message)) => assert!(message.contains("no system role")),
        other => panic!("expected a render error, got {other:?}"),
    }
}

#[test]
fn a_template_that_does_not_parse_is_a_parse_error_and_never_a_fallback() {
    match render("{% for %}", &input(vec![], false)) {
        Err(TemplateError::Parse(_)) => {}
        other => panic!("expected a parse error, got {other:?}"),
    }
}

#[test]
fn a_function_the_engine_does_not_provide_fails_loudly() {
    match render("{{ strftime_now('%Y') }}", &input(vec![], false)) {
        Err(TemplateError::Render(message)) => assert!(message.contains("strftime_now")),
        other => panic!("expected a render error, got {other:?}"),
    }
}
