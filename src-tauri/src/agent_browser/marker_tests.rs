use super::MarkerTexts;

#[test]
fn marker_distinguishes_the_builtin_and_unnamed_external_agents() {
    let builtin = MarkerTexts::new("en", true, "");
    assert_eq!(builtin.text, "Built-in agent is steering this tab");

    let unnamed = MarkerTexts::new("en", false, "");
    assert_eq!(unnamed.text, "Agent is steering this tab");

    let named = MarkerTexts::new("en", false, "Extension X");
    assert_eq!(named.text, "Extension X is steering this tab");
}
