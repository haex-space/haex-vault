use tokio_util::sync::CancellationToken;

use super::cli::CliTool;
use super::Tool;
#[cfg(unix)]
use crate::vault_gate::ChildRegistry;

#[tokio::test]
async fn a_trivial_command_returns_its_stdout() {
    let tool = CliTool::default();
    let result = tool
        .execute(
            serde_json::json!({ "command": "echo hello" }),
            CancellationToken::new(),
        )
        .await;
    assert!(!result.is_error);
    assert_eq!(result.content.trim(), "hello");
}

#[tokio::test]
async fn a_failing_command_returns_is_error_without_panicking() {
    let tool = CliTool::default();
    let result = tool
        .execute(
            serde_json::json!({ "command": "exit 7" }),
            CancellationToken::new(),
        )
        .await;
    assert!(result.is_error);
}

#[tokio::test]
async fn missing_command_input_is_a_tool_error_not_a_panic() {
    let tool = CliTool::default();
    let result = tool
        .execute(serde_json::json!({}), CancellationToken::new())
        .await;
    assert!(result.is_error);
}

#[tokio::test]
async fn a_nonexistent_binary_is_a_tool_error_not_a_panic() {
    let tool = CliTool::default();
    let result = tool
        .execute(
            serde_json::json!({ "command": "definitely-not-a-real-binary-xyz" }),
            CancellationToken::new(),
        )
        .await;
    assert!(result.is_error);
}

#[tokio::test]
async fn excessive_output_is_rejected_without_buffering_unbounded_data() {
    let tool = CliTool::default();
    let result = tool
        .execute(
            serde_json::json!({
                "command": "head -c 1048577 /dev/zero"
            }),
            CancellationToken::new(),
        )
        .await;
    assert!(result.is_error);
    assert!(result.content.contains("output exceeded"));
}

#[tokio::test]
async fn cancellation_kills_the_process_and_reaps_it_without_a_zombie() {
    let tool = CliTool::default();
    let cancel = CancellationToken::new();
    let cancel_for_task = cancel.clone();
    tokio::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        cancel_for_task.cancel();
    });

    let started = std::time::Instant::now();
    let result = tool
        .execute(serde_json::json!({ "command": "sleep 5" }), cancel)
        .await;
    let elapsed = started.elapsed();

    assert!(result.is_error);
    assert_eq!(result.content, "tool_call_cancelled");
    assert!(
        elapsed < std::time::Duration::from_secs(2),
        "cancellation must kill the process instead of waiting out the full sleep: took {elapsed:?}"
    );
}

#[cfg(unix)]
#[tokio::test]
async fn cancellation_kills_a_descendant_process_too() {
    let tool = CliTool::default();
    let cancel = CancellationToken::new();
    let cancel_for_execute = cancel.clone();
    let pid_file = tempfile::NamedTempFile::new().expect("pid file");
    let pid_path = pid_file.path().display().to_string();
    let command = format!("sleep 5 & echo $! > '{pid_path}'; wait");
    let execution = tokio::spawn(async move {
        tool.execute(
            serde_json::json!({ "command": command }),
            cancel_for_execute,
        )
        .await
    });

    let descendant_pid = tokio::time::timeout(std::time::Duration::from_secs(2), async {
        loop {
            if let Ok(pid) = tokio::fs::read_to_string(&pid_path).await {
                if let Ok(pid) = pid.trim().parse::<libc::pid_t>() {
                    break pid;
                }
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("command must start its descendant");

    cancel.cancel();
    let result = tokio::time::timeout(std::time::Duration::from_secs(2), execution)
        .await
        .expect("cancellation must finish the command")
        .expect("command task must not panic");
    assert!(result.is_error);
    assert_eq!(result.content, "tool_call_cancelled");

    let descendant_gone = tokio::time::timeout(std::time::Duration::from_secs(2), async {
        loop {
            // `kill(pid, 0)` checks existence without sending a signal.
            if unsafe { libc::kill(descendant_pid, 0) } != 0 {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    })
    .await;
    assert!(
        descendant_gone.is_ok(),
        "cancellation must terminate the shell's descendant process"
    );
}

#[cfg(unix)]
async fn wait_until_registered(registry: &ChildRegistry, count: usize) {
    tokio::time::timeout(std::time::Duration::from_secs(2), async {
        while registry.live() != count {
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap_or_else(|_| panic!("expected {count} registered, found {}", registry.live()));
}

#[cfg(unix)]
#[tokio::test]
async fn the_shell_is_registered_while_it_runs_and_unregistered_once_reaped() {
    let registry = ChildRegistry::default();
    let tool = CliTool::new(registry.clone());
    let cancel = CancellationToken::new();
    let cancel_for_execute = cancel.clone();
    let execution = tokio::spawn(async move {
        tool.execute(
            serde_json::json!({ "command": "sleep 30" }),
            cancel_for_execute,
        )
        .await
    });

    wait_until_registered(&registry, 1).await;
    cancel.cancel();
    let result = execution.await.expect("the tool task ends");

    assert_eq!(result.content, "tool_call_cancelled");
    assert_eq!(registry.live(), 0, "the entry goes with the reaped shell");
}

#[cfg(unix)]
#[tokio::test]
async fn ending_the_registry_ends_a_running_command_without_the_tools_own_cancellation() {
    let registry = ChildRegistry::default();
    let tool = CliTool::new(registry.clone());
    let execution = tokio::spawn(async move {
        tool.execute(
            serde_json::json!({ "command": "sleep 30" }),
            CancellationToken::new(),
        )
        .await
    });
    wait_until_registered(&registry, 1).await;

    // What the forced end does: the process ends without any Drop running, so the registry has to
    // reach the shell by itself.
    registry.kill_all();

    let result = tokio::time::timeout(std::time::Duration::from_secs(3), execution)
        .await
        .expect("the command ended once its group was killed")
        .expect("the tool task ends");
    assert!(result.is_error, "a killed command is not a success");
}

#[test]
fn the_description_keeps_the_model_from_reaching_holzi_data_through_the_shell() {
    let description = CliTool::default().description().to_owned();
    assert!(
        description.contains("only when the user asks"),
        "{description}"
    );
    assert!(
        description.contains("not holzi's apps or extensions"),
        "{description}"
    );
    assert!(
        description.contains("Never invent a command"),
        "{description}"
    );
}
