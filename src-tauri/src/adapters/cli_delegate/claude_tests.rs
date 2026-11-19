//! Unit tests for `claude.rs`'s NDJSON `stream-json` line parser
//! (tasks.md T011). Event shapes are the real ones captured live in
//! research.md §1, not invented fixtures.

use super::DelegateVendor;
use super::autonomy::AutonomyMode;
use super::claude::{LineOutcome, build_command, parse_line};
use super::subagents::Tracker;
use crate::adapters::types::{StreamChunk, StreamError};
use crate::cli_tools::runtime::{DelegateRuntime, RunFolder};

/// Convenience wrapper for tests that don't care about cross-line
/// sub-agent tracking state — a fresh `Tracker` per call.
fn parse(line: &str, ttft_ms: Option<u64>, total_ms: u64) -> LineOutcome {
    parse_line(line, ttft_ms, total_ms, &mut Tracker::new())
}

#[test]
fn text_delta_becomes_a_content_chunk() {
    let line = r#"{"type":"stream_event","event":{"type":"content_block_delta","delta":{"type":"text_delta","text":"hello"}}}"#;
    match parse(line, None, 10) {
        LineOutcome::Chunk(StreamChunk::Delta { content, reasoning }) => {
            assert_eq!(content, "hello");
            assert!(reasoning.is_none());
        }
        _ => panic!("expected a Delta chunk"),
    }
}

#[test]
fn thinking_delta_becomes_a_reasoning_chunk() {
    let line = r#"{"type":"stream_event","event":{"type":"content_block_delta","delta":{"type":"thinking_delta","text":"pondering"}}}"#;
    match parse(line, None, 10) {
        LineOutcome::Chunk(StreamChunk::Delta { content, reasoning }) => {
            assert_eq!(content, "");
            assert_eq!(reasoning.as_deref(), Some("pondering"));
        }
        _ => panic!("expected a Delta chunk"),
    }
}

#[test]
fn successful_result_becomes_done() {
    let line = r#"{"is_error":false,"subtype":"success","result":"Done.","usage":{"input_tokens":12,"output_tokens":34}}"#.replace(
        r#""is_error":false"#,
        r#""type":"result","is_error":false"#,
    );
    match parse(&line, Some(5), 42) {
        LineOutcome::Chunk(StreamChunk::Done {
            finish_reason,
            prompt_tokens,
            completion_tokens,
            ttft_ms,
            total_ms,
        }) => {
            assert_eq!(finish_reason.as_deref(), Some("complete"));
            assert_eq!(prompt_tokens, Some(12));
            assert_eq!(completion_tokens, Some(34));
            assert_eq!(ttft_ms, Some(5));
            assert_eq!(total_ms, 42);
        }
        _ => panic!("expected a Done chunk"),
    }
}

#[test]
fn errored_result_becomes_a_model_error() {
    let line = r#"{"type":"result","is_error":true,"subtype":"error","result":"something broke"}"#;
    match parse(line, None, 10) {
        LineOutcome::Error(StreamError::Model(message)) => {
            assert_eq!(message, "something broke");
        }
        _ => panic!("expected a Model error"),
    }
}

#[test]
fn unrelated_system_events_are_ignored() {
    let line = r#"{"type":"system","subtype":"status","status":"requesting"}"#;
    assert!(matches!(parse(line, None, 10), LineOutcome::Ignore));
}

#[test]
fn blank_lines_are_ignored() {
    assert!(matches!(parse("", None, 10), LineOutcome::Ignore));
    assert!(matches!(parse("   ", None, 10), LineOutcome::Ignore));
}

#[test]
fn malformed_json_surfaces_as_an_internal_stream_error_not_a_panic() {
    let line = r#"{"type":"stream_event", this is not valid json"#;
    match parse(line, None, 10) {
        LineOutcome::Error(StreamError::Internal(_)) => {}
        _ => panic!("expected an Internal stream error for malformed JSON"),
    }
}

#[test]
fn a_top_level_assistant_message_with_no_tool_use_is_ignored() {
    let line = r#"{"type":"assistant","parent_tool_use_id":null,"message":{"role":"assistant","content":[{"type":"text","text":"hi"}]}}"#;
    assert!(matches!(parse(line, None, 10), LineOutcome::Ignore));
}

#[test]
fn a_top_level_tool_use_dispatch_alone_is_ignored_until_referenced() {
    // Dispatching a tool_use (of any kind — this parser doesn't need to
    // know it's specifically the `Agent` tool) doesn't yet mean a
    // sub-agent started; only a later message referencing it as
    // `parent_tool_use_id` confirms that (research.md §2).
    let line = r#"{"type":"assistant","parent_tool_use_id":null,"message":{"role":"assistant","content":[{"type":"tool_use","id":"toolu_1","name":"Agent","input":{}}]}}"#;
    assert!(matches!(parse(line, None, 10), LineOutcome::Ignore));
}

#[test]
fn a_sub_agents_first_message_confirms_one_active_agent() {
    let mut tracker = Tracker::new();
    let dispatch = r#"{"type":"assistant","parent_tool_use_id":null,"message":{"role":"assistant","content":[{"type":"tool_use","id":"toolu_1","name":"Agent","input":{}}]}}"#;
    assert!(matches!(
        parse_line(dispatch, None, 10, &mut tracker),
        LineOutcome::Ignore
    ));

    let sub_agent_prompt = r#"{"type":"user","parent_tool_use_id":"toolu_1","message":{"role":"user","content":[{"type":"text","text":"do the thing"}]}}"#;
    match parse_line(sub_agent_prompt, None, 10, &mut tracker) {
        LineOutcome::Chunk(StreamChunk::AgentActivity {
            active_count,
            batch_size,
        }) => {
            assert_eq!(active_count, 1);
            assert_eq!(batch_size, Some(1));
        }
        _ => panic!("expected an AgentActivity chunk"),
    }

    let nested_dispatch = r#"{"type":"assistant","parent_tool_use_id":"toolu_1","message":{"role":"assistant","content":[{"type":"tool_use","id":"toolu_2","name":"Agent","input":{}}]}}"#;
    assert!(matches!(
        parse_line(nested_dispatch, None, 10, &mut tracker),
        LineOutcome::Ignore
    ));

    let nested_prompt =
        r#"{"type":"user","parent_tool_use_id":"toolu_2","message":{"role":"user","content":[]}}"#;
    match parse_line(nested_prompt, None, 10, &mut tracker) {
        LineOutcome::Chunk(StreamChunk::AgentActivity {
            active_count,
            batch_size,
        }) => {
            assert_eq!(active_count, 2);
            assert_eq!(batch_size, Some(1));
        }
        _ => panic!("expected nested AgentActivity chunk"),
    }
}

#[test]
fn two_sub_agents_dispatched_together_report_one_batch_of_two() {
    let mut tracker = Tracker::new();
    let dispatch = r#"{"type":"assistant","parent_tool_use_id":null,"message":{"role":"assistant","content":[
        {"type":"tool_use","id":"toolu_1","name":"Agent","input":{}},
        {"type":"tool_use","id":"toolu_2","name":"Agent","input":{}}
    ]}}"#;
    assert!(matches!(
        parse_line(dispatch, None, 10, &mut tracker),
        LineOutcome::Ignore
    ));

    let first_prompt =
        r#"{"type":"user","parent_tool_use_id":"toolu_1","message":{"role":"user","content":[]}}"#;
    match parse_line(first_prompt, None, 10, &mut tracker) {
        LineOutcome::Chunk(StreamChunk::AgentActivity {
            active_count,
            batch_size,
        }) => {
            assert_eq!(active_count, 1);
            assert_eq!(batch_size, Some(2));
        }
        _ => panic!("expected an AgentActivity chunk"),
    }

    let second_prompt =
        r#"{"type":"user","parent_tool_use_id":"toolu_2","message":{"role":"user","content":[]}}"#;
    match parse_line(second_prompt, None, 10, &mut tracker) {
        LineOutcome::Chunk(StreamChunk::AgentActivity {
            active_count,
            batch_size,
        }) => {
            assert_eq!(active_count, 2);
            assert_eq!(batch_size, None);
        }
        _ => panic!("expected an AgentActivity chunk"),
    }
}

#[test]
fn a_main_thread_tool_result_ends_the_sub_agent() {
    let mut tracker = Tracker::new();
    let dispatch = r#"{"type":"assistant","parent_tool_use_id":null,"message":{"role":"assistant","content":[{"type":"tool_use","id":"toolu_1","name":"Agent","input":{}}]}}"#;
    parse_line(dispatch, None, 10, &mut tracker);
    let prompt =
        r#"{"type":"user","parent_tool_use_id":"toolu_1","message":{"role":"user","content":[]}}"#;
    parse_line(prompt, None, 10, &mut tracker);

    let result = r#"{"type":"user","parent_tool_use_id":null,"message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"toolu_1","content":"done"}]}}"#;
    match parse_line(result, None, 10, &mut tracker) {
        LineOutcome::Chunk(StreamChunk::AgentActivity {
            active_count,
            batch_size,
        }) => {
            assert_eq!(active_count, 0);
            assert_eq!(batch_size, None);
        }
        _ => panic!("expected an AgentActivity chunk"),
    }
}

#[test]
fn a_main_thread_tool_result_for_a_plain_tool_is_ignored() {
    // Not every tool call is a sub-agent — an ordinary tool's result on the
    // main thread must not be misreported as agent activity.
    let mut tracker = Tracker::new();
    let result = r#"{"type":"user","parent_tool_use_id":null,"message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"toolu_plain","content":"ok"}]}}"#;
    assert!(matches!(
        parse_line(result, None, 10, &mut tracker),
        LineOutcome::Ignore
    ));
}

fn model_args(model: &str) -> Vec<String> {
    args_with_effort(model, None)
}

/// A runtime over a stub program, and the run folder of one run.
fn stub_runtime() -> (tempfile::TempDir, DelegateRuntime, RunFolder) {
    let dir = tempfile::TempDir::new().expect("tempdir");
    let runtime = DelegateRuntime::for_stub("claude", dir.path().join("stub-claude"), dir.path());
    let run = runtime.run_folder().expect("run folder");
    (dir, runtime, run)
}

fn args_with_effort(model: &str, reasoning_option: Option<&str>) -> Vec<String> {
    let (_dir, runtime, run) = stub_runtime();
    let cmd = build_command(
        &runtime,
        model,
        None,
        None,
        &run,
        AutonomyMode::Ungated,
        reasoning_option,
    );
    cmd.as_std()
        .get_args()
        .map(|a| a.to_string_lossy().into_owned())
        .collect()
}

/// Spec 094 FR-003: the program is the vault's own installation, by its full path, never a name
/// the host would look up.
#[test]
fn the_command_runs_the_installed_program_by_its_path() {
    let (dir, runtime, run) = stub_runtime();
    let cmd = build_command(
        &runtime,
        "claude-opus-5",
        None,
        None,
        &run,
        AutonomyMode::Ungated,
        None,
    );
    assert_eq!(cmd.as_std().get_program(), dir.path().join("stub-claude"));
    assert_eq!(cmd.as_std().get_current_dir(), Some(run.path()));
}

/// Spec 094 FR-011: the home, the temp folders and the tool's own configuration variable point
/// into the vault, and its update switch is set; all on the child alone.
#[test]
fn the_command_is_contained_to_the_vault_and_switches_the_tools_update_off() {
    let (dir, runtime, run) = stub_runtime();
    let cmd = build_command(
        &runtime,
        "claude-opus-5",
        None,
        None,
        &run,
        AutonomyMode::Ungated,
        None,
    );
    let envs: std::collections::HashMap<_, _> = cmd
        .as_std()
        .get_envs()
        .map(|(k, v)| {
            (
                k.to_string_lossy().into_owned(),
                v.map(|v| v.to_os_string()),
            )
        })
        .collect();
    let set = |name: &str| {
        envs.get(name)
            .and_then(Option::as_ref)
            .map(std::path::PathBuf::from)
    };
    assert_eq!(set("CLAUDE_CONFIG_DIR").as_deref(), Some(run.path()));
    if !cfg!(windows) {
        assert_eq!(set("HOME").as_deref(), Some(run.home()));
        assert_eq!(set("TMPDIR").as_deref(), Some(run.tmp()));
    }
    assert!(run.home().starts_with(dir.path()));
    assert_eq!(
        envs.get("DISABLE_UPDATES")
            .and_then(Option::as_ref)
            .map(|v| v.to_string_lossy().into_owned()),
        Some("1".to_string())
    );
}

#[test]
fn a_reasoning_option_id_is_passed_through_as_the_effort_flag_unchanged() {
    let args = args_with_effort("claude-opus-5", Some("xhigh"));
    let flag = args.iter().position(|a| a == "--effort");
    assert_eq!(
        flag.and_then(|i| args.get(i + 1)).map(String::as_str),
        Some("xhigh"),
    );
}

#[test]
fn no_reasoning_option_omits_the_effort_flag() {
    let args = model_args("claude-opus-5");
    assert!(!args.iter().any(|a| a == "--effort"));
}

#[test]
fn a_real_model_id_is_passed_through_as_the_model_flag() {
    let args = model_args("claude-opus-5");
    let flag = args.iter().position(|a| a == "--model");
    assert_eq!(
        flag.and_then(|i| args.get(i + 1)).map(String::as_str),
        Some("claude-opus-5"),
    );
}

#[test]
fn an_empty_model_id_omits_the_model_flag() {
    assert!(!model_args("").contains(&"--model".to_string()));
}

#[test]
fn the_legacy_synthetic_vendor_id_omits_the_model_flag() {
    // Pre-refresh sessions persisted `DelegateVendor::Claude.as_str()`
    // ("claude") as their model id, back when `list_models` returned one
    // synthetic entry instead of real Anthropic model ids — that id isn't
    // a real `--model` value the CLI accepts, so it must fall back to the
    // CLI's own default exactly like an empty id does.
    assert!(!model_args(DelegateVendor::Claude.as_str()).contains(&"--model".to_string()));
}
