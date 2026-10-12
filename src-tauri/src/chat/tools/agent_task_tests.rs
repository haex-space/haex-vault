use super::*;

#[test]
fn a_model_is_named_by_its_remote_id_or_its_catalog_id() {
    assert_eq!(model_name("8f3c:claude-sonnet"), "claude-sonnet");
    assert_eq!(model_name("qwen3-4b-q4"), "qwen3-4b-q4");
}

#[tokio::test]
async fn the_task_holds_only_inside_its_scope() {
    assert_eq!(current(), None);
    let task = AgentTask {
        id: "turn-1".to_owned(),
        model: "qwen3".to_owned(),
        locality: Locality::Local,
    };
    let seen = within(task.clone(), async { current() }).await;
    assert_eq!(seen, Some(task));
    assert_eq!(current(), None);
}
