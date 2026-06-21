use super::notifier::SingleSourceNotifier;

#[test]
fn first_call_for_did_returns_true() {
    let n = SingleSourceNotifier::new();
    assert!(n.should_notify("did:key:alice"));
}

#[test]
fn second_call_for_same_did_returns_false() {
    let n = SingleSourceNotifier::new();
    n.should_notify("did:key:alice");
    assert!(!n.should_notify("did:key:alice"));
}

#[test]
fn different_dids_each_get_their_first_notify() {
    let n = SingleSourceNotifier::new();
    assert!(n.should_notify("did:key:alice"));
    assert!(n.should_notify("did:key:bob"));
    assert!(!n.should_notify("did:key:alice"));
    assert!(!n.should_notify("did:key:bob"));
}
