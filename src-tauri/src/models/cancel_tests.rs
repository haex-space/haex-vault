use super::DownloadRegistry;

#[test]
fn cancelling_a_running_download_fires_its_token() {
    let registry = DownloadRegistry::default();
    let download = registry.begin("qwen3-4b");
    assert!(!download.token().is_cancelled());

    assert!(registry.cancel("qwen3-4b"));

    assert!(download.token().is_cancelled());
}

#[test]
fn cancelling_a_download_that_does_not_run_does_nothing() {
    let registry = DownloadRegistry::default();
    assert!(!registry.cancel("qwen3-4b"));
}

#[test]
fn only_the_named_download_is_cancelled() {
    let registry = DownloadRegistry::default();
    let wanted = registry.begin("a");
    let other = registry.begin("b");

    assert!(registry.cancel("a"));

    assert!(wanted.token().is_cancelled());
    assert!(!other.token().is_cancelled());
}

#[test]
fn a_finished_download_can_no_longer_be_cancelled() {
    let registry = DownloadRegistry::default();
    drop(registry.begin("qwen3-4b"));
    assert!(!registry.cancel("qwen3-4b"));
}
