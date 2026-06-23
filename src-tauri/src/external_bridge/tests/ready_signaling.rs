//! `ExternalBridge::wait_for_extension_ready` / `signal_extension_ready`
//! async behaviour tests — covers the bug-fix scenario where extensions
//! with no pending migrations must still signal ready to unblock waiters.

use std::sync::Arc;

use super::super::server::ExternalBridge;

#[tokio::test]
async fn test_extension_ready_signal_no_waiter() {
    let bridge = ExternalBridge::new();
    let extension_id = "non-existent-extension";

    // Signal ready for an extension that no one is waiting for
    // This should not panic
    bridge.signal_extension_ready(extension_id).await;
}

#[tokio::test]
async fn test_extension_ready_wait_with_immediate_signal() {
    let bridge = Arc::new(ExternalBridge::new());
    let extension_id = "test-extension-456";

    // Spawn a task that waits for the extension to be ready
    let bridge_clone = bridge.clone();
    let ext_id = extension_id.to_string();
    let wait_handle =
        tokio::spawn(async move { bridge_clone.wait_for_extension_ready(&ext_id, 5000).await });

    // Give the wait task time to set up
    tokio::time::sleep(std::time::Duration::from_millis(10)).await;

    // Signal that the extension is ready
    bridge.signal_extension_ready(extension_id).await;

    // The wait should complete successfully
    let result = wait_handle.await.unwrap();
    assert!(
        result,
        "wait_for_extension_ready should return true when signaled"
    );
}

#[tokio::test]
async fn test_extension_ready_wait_timeout() {
    let bridge = ExternalBridge::new();
    let extension_id = "timeout-extension";

    // Wait for an extension that never signals ready (with short timeout)
    let result = bridge.wait_for_extension_ready(extension_id, 50).await;

    assert!(
        !result,
        "wait_for_extension_ready should return false on timeout"
    );
}

#[tokio::test]
async fn test_extension_ready_signal_cleans_up() {
    let bridge = Arc::new(ExternalBridge::new());
    let extension_id = "cleanup-extension";

    // Start waiting (this creates an entry in extension_ready_signals)
    let bridge_clone = bridge.clone();
    let ext_id = extension_id.to_string();
    let wait_handle =
        tokio::spawn(async move { bridge_clone.wait_for_extension_ready(&ext_id, 5000).await });

    // Give the wait task time to set up
    tokio::time::sleep(std::time::Duration::from_millis(10)).await;

    // Signal ready
    bridge.signal_extension_ready(extension_id).await;

    // Wait for the task to complete
    let result = wait_handle.await.unwrap();
    assert!(result, "Extension should have been signaled ready");

    // After wait completes, the entry should be cleaned up
    // We verify this by checking that a new wait would need to set up a new entry
    // (the previous entry was cleaned up)
    let signals = bridge.get_extension_ready_signals();
    let signals_read = signals.read().await;
    assert!(
        !signals_read.contains_key(extension_id),
        "Signal entry should be cleaned up after wait completes"
    );
}

#[tokio::test]
async fn test_multiple_extensions_ready_independently() {
    let bridge = Arc::new(ExternalBridge::new());

    // Start waiting for two different extensions
    let bridge1 = bridge.clone();
    let bridge2 = bridge.clone();

    let wait1 = tokio::spawn(async move { bridge1.wait_for_extension_ready("ext-1", 5000).await });

    let wait2 = tokio::spawn(async move { bridge2.wait_for_extension_ready("ext-2", 5000).await });

    // Give wait tasks time to set up
    tokio::time::sleep(std::time::Duration::from_millis(10)).await;

    // Signal only ext-1
    bridge.signal_extension_ready("ext-1").await;

    // ext-1 should complete successfully
    let result1 = wait1.await.unwrap();
    assert!(result1, "ext-1 should be signaled");

    // Signal ext-2
    bridge.signal_extension_ready("ext-2").await;

    // ext-2 should also complete successfully
    let result2 = wait2.await.unwrap();
    assert!(result2, "ext-2 should be signaled");
}

/// Tests the scenario where an extension signals ready immediately after
/// being set up to wait (simulates the "no pending migrations" case).
///
/// This test verifies the fix for the bug where extensions that had already
/// completed their migrations would never signal ready, causing ExternalBridge
/// to timeout waiting for them.
#[tokio::test]
async fn test_extension_ready_signal_immediate_after_wait_setup() {
    let bridge = Arc::new(ExternalBridge::new());
    let extension_id = "already-migrated-extension";

    // Simulate the ExternalBridge waiting for an extension to be ready
    // (this happens in ensure_extension_loaded)
    let bridge_clone = bridge.clone();
    let ext_id = extension_id.to_string();
    let wait_handle =
        tokio::spawn(async move { bridge_clone.wait_for_extension_ready(&ext_id, 5000).await });

    // Give the wait task time to set up (minimal delay)
    tokio::time::sleep(std::time::Duration::from_millis(5)).await;

    // Immediately signal ready - simulates what happens when
    // extension_database_register_migrations finds no pending migrations
    // and signals ready in the early return path
    bridge.signal_extension_ready(extension_id).await;

    // The wait should complete successfully (not timeout)
    let result = wait_handle.await.unwrap();
    assert!(
        result,
        "Extension with no pending migrations should still signal ready and unblock waiters"
    );
}

/// Tests that signaling ready multiple times for the same extension is safe
/// (idempotent behavior - important for robustness)
#[tokio::test]
async fn test_extension_ready_signal_idempotent() {
    let bridge = Arc::new(ExternalBridge::new());
    let extension_id = "idempotent-extension";

    // Start waiting
    let bridge_clone = bridge.clone();
    let ext_id = extension_id.to_string();
    let wait_handle =
        tokio::spawn(async move { bridge_clone.wait_for_extension_ready(&ext_id, 5000).await });

    tokio::time::sleep(std::time::Duration::from_millis(10)).await;

    // Signal ready multiple times (should not panic or cause issues)
    bridge.signal_extension_ready(extension_id).await;
    bridge.signal_extension_ready(extension_id).await;
    bridge.signal_extension_ready(extension_id).await;

    // Wait should complete on first signal
    let result = wait_handle.await.unwrap();
    assert!(result, "First signal should unblock the waiter");

    // Additional signals after wait completed should be safe (no-op)
    bridge.signal_extension_ready(extension_id).await;
}
