//! Tests for the `--mcp-agent` helper (spec 021, T022) against a fake app endpoint in a temporary
//! directory; stdin and stdout are in-memory pipes.

use serde_json::Value;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, DuplexStream};
use uuid::Uuid;

use super::helper::{credential_path, exit, parse_args, run, HelperArgs, HelperEnv};

fn env(dir: &std::path::Path) -> HelperEnv {
    HelperEnv {
        socket_dir: dir.join("sockets"),
        credential_dir: dir.join("config").join("mcp-agents"),
    }
}

fn args(slug: &str) -> HelperArgs {
    HelperArgs {
        slug: slug.to_owned(),
        vault: None,
        delegate_token: None,
    }
}

const INITIALIZE: &str = r#"{"jsonrpc":"2.0","id":7,"method":"initialize","params":{}}"#;

async fn read_json(reader: &mut BufReader<DuplexStream>) -> Value {
    let mut line = String::new();
    reader.read_line(&mut line).await.expect("a line");
    serde_json::from_str(line.trim_end()).expect("json")
}

#[test]
fn arguments_are_read_and_checked() {
    let parsed = parse_args(
        ["codex", "--vault", "0123456789ab4def8123456789abcdef"]
            .into_iter()
            .map(String::from),
    )
    .expect("valid");
    assert_eq!(parsed.slug, "codex");
    assert!(parsed.vault.is_some());
    assert!(parse_args(["Bad Slug"].into_iter().map(String::from)).is_err());
    assert!(parse_args(["x", "--vault"].into_iter().map(String::from)).is_err());
    assert!(parse_args(["x", "--other", "1"].into_iter().map(String::from)).is_err());
    let secret = parse_args(
        ["x", "--delegate-token", "secret-credential"]
            .into_iter()
            .map(String::from),
    )
    .expect("valid");
    assert!(!format!("{secret:?}").contains("secret-credential"));
}

/// FR-002 clarification: holzi is not running, the agent hears so on its first request, and
/// nothing is started.
#[tokio::test]
async fn without_holzi_the_first_request_gets_an_error_and_the_helper_ends() {
    let temp = tempfile::tempdir().expect("tempdir");
    let (agent_in, helper_stdin) = tokio::io::duplex(4096);
    let (helper_stdout, agent_out) = tokio::io::duplex(4096);
    let mut agent_in = agent_in;
    agent_in
        .write_all(format!("{INITIALIZE}\n").as_bytes())
        .await
        .expect("write");
    let code = run(args("codex"), env(temp.path()), helper_stdin, helper_stdout).await;
    assert_eq!(code, exit::NOT_OPEN);
    let mut reader = BufReader::new(agent_out);
    let answer = read_json(&mut reader).await;
    assert_eq!(answer["id"], 7);
    assert_eq!(answer["error"]["code"], -32000);
    assert_eq!(answer["error"]["message"], "holzi is not open");
}

#[cfg(unix)]
mod with_endpoint {
    use std::os::unix::fs::PermissionsExt;

    use super::*;
    use crate::mcp_server::endpoint::{bind, endpoint_name, BoxStream};

    async fn read_line(reader: &mut BufReader<tokio::io::ReadHalf<BoxStream>>) -> Value {
        let mut line = String::new();
        reader.read_line(&mut line).await.expect("a line");
        serde_json::from_str(line.trim_end()).expect("json")
    }

    #[tokio::test]
    async fn a_rejected_agent_learns_why_on_its_first_request() {
        let temp = tempfile::tempdir().expect("tempdir");
        let env = env(temp.path());
        let device = Uuid::new_v4();
        let mut listener = bind(&env.socket_dir, device).await.expect("bind");
        let app = tokio::spawn(async move {
            let stream = listener.accept().await.expect("accept").expect("allowed");
            let (read, mut write) = tokio::io::split(stream);
            let mut reader = BufReader::new(read);
            let greeting = read_line(&mut reader).await;
            assert_eq!(greeting["slug"], "codex");
            assert_eq!(greeting["token"], Value::Null);
            write
                .write_all(b"{\"status\":\"rejected\",\"reason\":\"blocked\"}\n")
                .await
                .expect("reply");
        });
        let (mut agent_in, helper_stdin) = tokio::io::duplex(4096);
        let (helper_stdout, agent_out) = tokio::io::duplex(4096);
        agent_in
            .write_all(format!("{INITIALIZE}\n").as_bytes())
            .await
            .expect("write");
        let code = run(args("codex"), env.clone(), helper_stdin, helper_stdout).await;
        app.await.expect("app");
        assert_eq!(code, exit::REJECTED);
        let answer = read_json(&mut BufReader::new(agent_out)).await;
        assert_eq!(answer["error"]["message"], "holzi blocked this agent");
    }

    /// The credential arrives as a control line, is kept with mode 0600 and never reaches the
    /// agent; MCP lines pass unchanged both ways; a revocation deletes the file.
    #[tokio::test]
    async fn a_registration_keeps_the_credential_and_a_revocation_drops_it() {
        let temp = tempfile::tempdir().expect("tempdir");
        let env = env(temp.path());
        let device = Uuid::new_v4();
        let mut listener = bind(&env.socket_dir, device).await.expect("bind");
        let file = credential_path(&env, &endpoint_name(device), "claude-code");
        let app = tokio::spawn(async move {
            let stream = listener.accept().await.expect("accept").expect("allowed");
            let (read, mut write) = tokio::io::split(stream);
            let mut reader = BufReader::new(read);
            read_line(&mut reader).await;
            write
                .write_all(b"{\"status\":\"pending\"}\n")
                .await
                .expect("reply");
            let request = read_line(&mut reader).await;
            assert_eq!(request["method"], "initialize");
            write
                .write_all(b"{\"jsonrpc\":\"2.0\",\"id\":7,\"result\":{}}\n")
                .await
                .expect("answer");
            write
                .write_all(b"{\"holzi_control\":{\"token\":\"fresh-credential\"}}\n")
                .await
                .expect("token");
            write
                .write_all(
                    b"{\"jsonrpc\":\"2.0\",\"method\":\"notifications/tools/list_changed\"}\n",
                )
                .await
                .expect("notification");
            // Wait until the agent saw the notification, then revoke.
            let next = read_line(&mut reader).await;
            assert_eq!(next["method"], "ping");
            write
                .write_all(b"{\"holzi_control\":{\"closing\":\"revoked\"}}\n")
                .await
                .expect("closing");
        });
        let (mut agent_in, helper_stdin) = tokio::io::duplex(4096);
        let (helper_stdout, agent_out) = tokio::io::duplex(4096);
        let helper = tokio::spawn({
            let env = env.clone();
            async move { run(args("claude-code"), env, helper_stdin, helper_stdout).await }
        });
        agent_in
            .write_all(format!("{INITIALIZE}\n").as_bytes())
            .await
            .expect("write");
        let mut agent_out = BufReader::new(agent_out);
        assert_eq!(read_json(&mut agent_out).await["id"], 7);
        assert_eq!(
            read_json(&mut agent_out).await["method"],
            "notifications/tools/list_changed",
            "the control line never reaches the agent"
        );
        assert_eq!(
            std::fs::read_to_string(&file).expect("kept"),
            "fresh-credential"
        );
        let mode = std::fs::metadata(&file).expect("file").permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
        agent_in
            .write_all(b"{\"jsonrpc\":\"2.0\",\"id\":8,\"method\":\"ping\"}\n")
            .await
            .expect("ping");
        app.await.expect("app");
        assert_eq!(helper.await.expect("helper"), exit::CLOSED);
        assert!(!file.exists(), "a revocation deletes the credential");
    }

    #[tokio::test]
    async fn a_kept_credential_is_presented_and_several_instances_need_a_choice() {
        let temp = tempfile::tempdir().expect("tempdir");
        let env = env(temp.path());
        let device = Uuid::new_v4();
        let mut listener = bind(&env.socket_dir, device).await.expect("bind");
        let file = credential_path(&env, &endpoint_name(device), "codex");
        std::fs::create_dir_all(file.parent().expect("dir")).expect("dir");
        std::fs::write(&file, "kept-credential\n").expect("credential");
        let app = tokio::spawn(async move {
            let stream = listener.accept().await.expect("accept").expect("allowed");
            let (read, _write) = tokio::io::split(stream);
            let mut reader = BufReader::new(read);
            (read_line(&mut reader).await, listener)
        });
        let (_agent_in, helper_stdin) = tokio::io::duplex(4096);
        let (helper_stdout, _agent_out) = tokio::io::duplex(4096);
        let helper = tokio::spawn({
            let env = env.clone();
            async move { run(args("codex"), env, helper_stdin, helper_stdout).await }
        });
        let (greeting, _first) = app.await.expect("app");
        assert_eq!(greeting["token"], "kept-credential");
        helper.abort();

        // A second instance: without --vault the helper cannot choose.
        let _other = bind(&env.socket_dir, Uuid::new_v4()).await.expect("bind");
        let (mut agent_in, helper_stdin) = tokio::io::duplex(4096);
        let (helper_stdout, agent_out) = tokio::io::duplex(4096);
        agent_in
            .write_all(format!("{INITIALIZE}\n").as_bytes())
            .await
            .expect("write");
        let code = run(args("codex"), env, helper_stdin, helper_stdout).await;
        assert_eq!(code, exit::NOT_OPEN);
        let answer = read_json(&mut BufReader::new(agent_out)).await;
        assert!(answer["error"]["message"]
            .as_str()
            .expect("message")
            .contains("--vault"));
    }
}
