use super::turn_model::{decide, TurnModel};

const CLOUD_A: &str = "11111111-1111-1111-1111-111111111111:model-a";
const CLOUD_B: &str = "22222222-2222-2222-2222-222222222222:model-b";

#[test]
fn a_new_conversation_uses_the_open_model() {
    assert_eq!(decide(None, Some(CLOUD_A)), TurnModel::Open);
    assert_eq!(decide(None, None), TurnModel::Open);
}

#[test]
fn a_conversation_whose_model_is_the_open_one_shares_it() {
    assert_eq!(decide(Some(CLOUD_A), Some(CLOUD_A)), TurnModel::Open);
    assert_eq!(decide(Some("qwen3-4b"), Some("qwen3-4b")), TurnModel::Open);
}

#[test]
fn a_conversation_with_another_cloud_model_gets_an_adapter_of_its_own() {
    assert_eq!(
        decide(Some(CLOUD_A), Some(CLOUD_B)),
        TurnModel::Provider(CLOUD_A.to_string())
    );
    assert_eq!(
        decide(Some(CLOUD_A), None),
        TurnModel::Provider(CLOUD_A.to_string())
    );
}

#[test]
fn a_conversation_with_a_local_model_takes_the_engine_in_memory() {
    assert_eq!(
        decide(Some("qwen3-4b"), Some(CLOUD_B)),
        TurnModel::Local("qwen3-4b".to_string())
    );
}

#[test]
fn an_empty_model_of_a_conversation_counts_as_none() {
    assert_eq!(decide(Some(""), Some(CLOUD_B)), TurnModel::Open);
}
