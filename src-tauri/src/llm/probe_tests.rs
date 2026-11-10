use super::probe::decide;
use crate::adapters::types::ToolTemplateProbe::{IgnoresTools, Inconclusive};

#[test]
fn the_same_text_with_and_without_a_tool_means_the_template_ignores_tools() {
    assert_eq!(decide(Ok("<user>hi"), Ok("<user>hi")), IgnoresTools);
}

#[test]
fn a_template_that_shows_the_tool_is_inconclusive() {
    assert_eq!(
        decide(
            Ok("<user>hi"),
            Ok("<tools>holzi_probe_tool</tools><user>hi")
        ),
        Inconclusive
    );
}

#[test]
fn the_refusal_of_a_template_without_a_tool_part_means_it_ignores_tools() {
    let refusal = "chat template does not handle tool usage";
    assert_eq!(decide(Ok("<user>hi"), Err(refusal)), IgnoresTools);
}

#[test]
fn any_other_error_or_a_failing_plain_render_tells_nothing() {
    assert_eq!(decide(Ok("x"), Err("channel closed")), Inconclusive);
    assert_eq!(decide(Err("template broke"), Ok("x")), Inconclusive);
    assert_eq!(
        decide(Err("a"), Err("does not handle tool usage")),
        Inconclusive,
        "no plain rendering, no comparison"
    );
}
