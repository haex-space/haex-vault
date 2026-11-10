use super::context_budget::{result_chars, within};

#[tokio::test]
async fn a_small_context_gives_a_quarter_of_it_in_characters() {
    assert_eq!(result_chars(), None, "outside a turn there is no budget");
    let inside = within(Some(8192), async { result_chars() }).await;
    assert_eq!(inside, Some(8192 / 4 * 3));
}

#[tokio::test]
async fn without_a_limit_a_tool_gives_its_usual_amount() {
    assert_eq!(within(None, async { result_chars() }).await, None);
}

#[tokio::test]
async fn the_budget_ends_with_the_work_it_was_set_for() {
    within(Some(8192), async {}).await;
    assert_eq!(result_chars(), None);
}
