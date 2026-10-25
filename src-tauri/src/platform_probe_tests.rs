use tokio::io::AsyncReadExt;

use crate::files::streaming::StreamingSource;

use super::*;

#[test]
fn connect_src_gets_the_loopback() {
    let csp = "default-src 'self'; connect-src ipc: http://ipc.localhost; img-src 'self'";
    assert_eq!(
        with_loopback_connect(csp),
        "default-src 'self'; connect-src ipc: http://ipc.localhost http://127.0.0.1:*; img-src 'self'"
    );
}

#[test]
fn a_policy_without_connect_src_keeps_what_default_src_allowed() {
    assert_eq!(
        with_loopback_connect("default-src 'self' data:"),
        "default-src 'self' data:; connect-src 'self' data: http://127.0.0.1:*"
    );
}

#[test]
fn directive_names_match_regardless_of_case() {
    assert_eq!(
        with_loopback_connect("Connect-Src ipc:"),
        "Connect-Src ipc: http://127.0.0.1:*"
    );
}

#[test]
fn the_health_route_answers_ok_with_cors() {
    let answer = response_for("GET /__probe HTTP/1.1\r\n");
    assert!(answer.starts_with("HTTP/1.1 200 OK\r\n"));
    assert!(answer.contains("Access-Control-Allow-Origin: *\r\n"));
    assert!(answer.ends_with("\r\n\r\nok"));
}

#[test]
fn the_preflight_is_answered() {
    let answer = response_for("OPTIONS /__probe HTTP/1.1\r\n");
    assert!(answer.starts_with("HTTP/1.1 204 No Content\r\n"));
    assert!(answer.contains("Access-Control-Allow-Origin: *\r\n"));
    assert!(answer.contains("Access-Control-Allow-Methods: GET, OPTIONS\r\n"));
    assert!(answer.contains("Access-Control-Allow-Private-Network: true\r\n"));
}

#[test]
fn a_query_does_not_change_the_route() {
    assert!(response_for("GET /__probe?v=localhost HTTP/1.1").starts_with("HTTP/1.1 200 OK"));
}

#[test]
fn anything_else_is_not_found() {
    for line in [
        "GET / HTTP/1.1",
        "GET /__probe/x HTTP/1.1",
        "POST /__probe HTTP/1.1",
        "",
    ] {
        assert!(
            response_for(line).starts_with("HTTP/1.1 404 Not Found\r\n"),
            "{line:?}"
        );
    }
}

fn step(name: &str, ok: bool) -> Step {
    Step {
        name: name.to_owned(),
        ok,
        detail: None,
    }
}

#[test]
fn every_step_passed_means_exit_zero() {
    let report = Report {
        steps: vec![step("webview", true), step("loopback-fetch", true)],
        user_agent: None,
    };
    assert_eq!(exit_code(&report), 0);
}

#[test]
fn one_failed_step_means_exit_one() {
    let report = Report {
        steps: vec![step("webview", true), step("loopback-fetch", false)],
        user_agent: None,
    };
    assert_eq!(exit_code(&report), 1);
}

#[test]
fn a_report_without_steps_is_a_failure() {
    let report = Report {
        steps: vec![],
        user_agent: None,
    };
    assert_eq!(exit_code(&report), 1);
}

#[test]
fn the_report_reads_the_window_shape() {
    let report: Report = serde_json::from_value(serde_json::json!({
        "steps": [{ "name": "loopback-fetch", "ok": false, "detail": "TypeError: Failed to fetch" }],
        "userAgent": "Mozilla/5.0",
    }))
    .unwrap();
    assert_eq!(
        report.steps[0].detail.as_deref(),
        Some("TypeError: Failed to fetch")
    );
    assert_eq!(report.user_agent.as_deref(), Some("Mozilla/5.0"));
}

#[test]
fn the_result_line_starts_with_the_marker() {
    let line = result_line(&serde_json::json!({ "ok": true }));
    assert_eq!(line, r#"HOLZI_PROBE_RESULT {"ok":true}"#);
}

#[test]
fn the_init_script_names_the_port_and_the_media() {
    assert_eq!(
        init_script(41873, &serde_json::json!({ "pdf": "http://127.0.0.1:1/t" })),
        r#"window.__HOLZI_PROBE__ = Object.freeze({ port: 41873, media: {"pdf":"http://127.0.0.1:1/t"} });"#
    );
}

#[tokio::test]
async fn the_fixtures_get_server_urls_and_are_read_in_ranges() {
    let media = MediaServer::start(tokio_util::sync::CancellationToken::new())
        .await
        .unwrap();
    let urls = serve_fixtures(&media);
    for (name, _, _) in FIXTURES {
        assert!(urls[name]
            .as_str()
            .unwrap()
            .starts_with("http://127.0.0.1:"));
    }
    let (_, bytes, _) = FIXTURES[2];
    let fixture = BytesSource::new(Arc::from(bytes), "application/pdf");
    assert_eq!(fixture.size().await.unwrap(), bytes.len() as u64);
    let mut part = Vec::new();
    fixture
        .open_range(1, 4)
        .await
        .unwrap()
        .read_to_end(&mut part)
        .await
        .unwrap();
    assert_eq!(part, b"PDF-");
    let mut past = Vec::new();
    fixture
        .open_range(bytes.len() as u64 + 10, 4)
        .await
        .unwrap()
        .read_to_end(&mut past)
        .await
        .unwrap();
    assert!(past.is_empty());
}

#[test]
fn the_encrypted_vectors_step_passes_and_says_how_many_checks_ran() {
    let step = encrypted_vectors();
    assert_eq!(step.name, ENCRYPTED_VECTORS_STEP);
    assert!(step.ok, "{:?}", step.detail);
    assert!(step
        .detail
        .as_deref()
        .is_some_and(|detail| detail.ends_with("checks passed")));
}

#[test]
fn all_files_passes_without_the_permission_and_with_a_working_listing() {
    let without = all_files(Ok(false), || {
        panic!("nothing is listed without the permission")
    });
    assert!(without.ok);
    assert_eq!(without.detail.as_deref(), Some("granted: false"));
    let with = all_files(Ok(true), || Ok(7));
    assert!(with.ok);
    assert_eq!(with.detail.as_deref(), Some("granted: true; 7 entries"));
    let refused = all_files(Ok(true), || {
        Err(std::io::Error::from(std::io::ErrorKind::PermissionDenied))
    });
    assert!(!refused.ok);
    assert!(!all_files(Err("no plugin".to_owned()), || Ok(0)).ok);
}
