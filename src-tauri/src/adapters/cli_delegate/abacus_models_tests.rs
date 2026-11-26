//! The Abacus model list (spec 096 plan D7) against hand-written answers and a shell stub of the
//! program. No real Abacus account is used.

use serde_json::json;

use super::{catalog_request, initial_request_write_error, parse_catalog_line, CatalogLine};
use crate::adapters::cli_delegate::abacus::CREDENTIAL_REFUSED;
use crate::adapters::AdapterError;

/// An answer in the shape `abacusai` 2.10.23 gave (research §12), shortened.
fn answer(groups: serde_json::Value) -> String {
    json!({
        "type": "control_response",
        "response": {
            "subtype": "success",
            "request_id": "haex-model-catalog",
            "response": { "localProvidersEnabled": true, "groups": groups },
        },
    })
    .to_string()
}

fn ids(line: CatalogLine) -> Vec<(String, String)> {
    match line {
        CatalogLine::Models(models) => models
            .into_iter()
            .map(|m| (m.remote_id, m.display_name))
            .collect(),
        other => panic!("{other:?}"),
    }
}

#[test]
fn the_request_asks_for_the_catalog() {
    let request = catalog_request();
    assert_eq!(request["type"], "control_request");
    assert_eq!(request["request"]["subtype"], "get_model_catalog");
}

#[test]
fn the_account_models_become_rows_in_the_program_order() {
    let line = answer(json!([{
        "id": "abacus", "label": "Abacus.AI", "origin": "abacus",
        "models": [
            { "id": "ROUTE_LLM", "name": "RouteLLM", "origin": "abacus" },
            { "id": "CLAUDE_V5_5_OPUS", "name": "Opus 5.5", "origin": "abacus" },
            { "id": "KIMI_K3", "origin": "abacus" },
        ],
    }]));
    assert_eq!(
        ids(parse_catalog_line(&line)),
        [
            ("ROUTE_LLM".to_owned(), "RouteLLM".to_owned()),
            ("CLAUDE_V5_5_OPUS".to_owned(), "Opus 5.5".to_owned()),
            ("KIMI_K3".to_owned(), "KIMI_K3".to_owned()),
        ]
    );
}

#[test]
fn models_of_local_providers_and_models_without_an_id_are_left_out() {
    let line = answer(json!([
        { "id": "abacus", "models": [
            { "id": "ROUTE_LLM", "name": "RouteLLM", "origin": "abacus" },
            { "name": "no id", "origin": "abacus" },
        ]},
        { "id": "ollama", "models": [{ "id": "llama3", "name": "Llama 3", "origin": "ollama" }] },
    ]));
    assert_eq!(
        ids(parse_catalog_line(&line)),
        [("ROUTE_LLM".to_owned(), "RouteLLM".to_owned())]
    );
}

#[test]
fn an_answer_without_models_or_with_an_error_is_refused_not_empty() {
    assert!(matches!(
        parse_catalog_line(&answer(json!([]))),
        CatalogLine::Refused(_)
    ));
    let no_groups = json!({
        "type": "control_response",
        "response": { "subtype": "success", "request_id": "haex-model-catalog", "response": {} },
    })
    .to_string();
    assert!(matches!(
        parse_catalog_line(&no_groups),
        CatalogLine::Refused(_)
    ));
    let error = json!({
        "type": "control_response",
        "response": { "subtype": "error", "request_id": "haex-model-catalog", "error": "nope" },
    })
    .to_string();
    match parse_catalog_line(&error) {
        CatalogLine::Refused(reason) => assert!(reason.contains("nope")),
        other => panic!("{other:?}"),
    }
}

#[test]
fn other_lines_and_answers_to_other_requests_are_skipped() {
    assert_eq!(parse_catalog_line("not json"), CatalogLine::Other);
    assert_eq!(
        parse_catalog_line(r#"{"type":"system","subtype":"init","permission_mode":"supervise"}"#),
        CatalogLine::Other
    );
    let other = json!({
        "type": "control_response",
        "response": { "subtype": "success", "request_id": "m1", "response": { "groups": [] } },
    })
    .to_string();
    assert_eq!(parse_catalog_line(&other), CatalogLine::Other);
}

#[test]
fn a_fatal_authentication_error_overrides_a_broken_initial_pipe_write() {
    let error = initial_request_write_error(
        br#"{"type":"fatal","reason":"not_authenticated"}"#,
        "broken pipe",
    );
    match error {
        AdapterError::Unavailable { reason } => assert_eq!(reason, CREDENTIAL_REFUSED),
        other => panic!("{other:?}"),
    }
}

#[cfg(unix)]
mod program {
    use std::fs;
    use std::os::unix::fs::PermissionsExt;
    use std::path::{Path, PathBuf};
    use std::sync::Arc;
    use std::time::Duration;

    use tempfile::TempDir;

    use super::super::list_models;
    use crate::adapters::cli_delegate::abacus::{CREDENTIAL_REFUSED, NOT_CONNECTED};
    use crate::adapters::AdapterError;
    use crate::cli_tools::runtime::DelegateRuntime;
    use crate::passwords::test_support::open_test_vault;
    use crate::providers::delegate_credential::{self, KIND_DEVICE_CODE, VAULT_SCOPE};
    use crate::vault_gate::{ChildRegistry, VaultDb, VaultGate};

    const CODE: &str = "placeholder-device-code-of-abacus-model-tests";

    async fn vault() -> (TempDir, VaultDb) {
        let (dir, database) = open_test_vault();
        let db = VaultGate::new().vault_db(Arc::new(database)).unwrap();
        delegate_credential::save(&db, "abacusai", KIND_DEVICE_CODE, CODE, VAULT_SCOPE)
            .await
            .unwrap();
        (dir, db)
    }

    /// A stub that writes its environment and first input line to `dir`, then runs `body`.
    fn stub(dir: &Path, body: &str) -> PathBuf {
        let path = dir.join("abacusai");
        fs::write(
            &path,
            format!(
                "#!/bin/sh\nenv > '{env}'\nIFS= read -r first\nprintf '%s\\n' \"$first\" > '{input}'\n{body}\n",
                env = dir.join("env").display(),
                input = dir.join("input").display(),
            ),
        )
        .unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
        path
    }

    #[tokio::test]
    async fn the_program_is_asked_with_the_device_code_and_no_user_line() {
        let (_vault, db) = vault().await;
        let dir = TempDir::new().unwrap();
        let program = stub(
            dir.path(),
            r#"echo '{"type":"system","subtype":"init","permission_mode":"supervise"}'
echo '{"type":"control_response","response":{"subtype":"success","request_id":"haex-model-catalog","response":{"groups":[{"id":"abacus","models":[{"id":"ROUTE_LLM","name":"RouteLLM","origin":"abacus"},{"id":"KIMI_K3","name":"Kimi K3","origin":"abacus"}]}]}}}'
sleep 5"#,
        );
        let runtime = DelegateRuntime::for_stub("abacusai", program, dir.path());

        let children = ChildRegistry::default();
        let models = list_models(runtime, Some(&db), children.clone())
            .await
            .expect("the stub answers");

        let names: Vec<_> = models.iter().map(|m| m.display_name.as_str()).collect();
        assert_eq!(names, ["RouteLLM", "Kimi K3"]);
        let input = fs::read_to_string(dir.path().join("input")).unwrap();
        assert!(input.contains("get_model_catalog"), "{input}");
        assert!(!input.contains("\"user\""), "no model runs: {input}");
        let env = fs::read_to_string(dir.path().join("env")).unwrap();
        assert!(env.contains(&format!("CODELLM_DEVICE_CODE={CODE}")));
        assert!(!env.contains("ABACUS_API_KEY"));
        assert_eq!(children.live(), 0);
    }

    #[tokio::test]
    async fn a_refused_device_code_leads_back_to_connecting() {
        let (_vault, db) = vault().await;
        let dir = TempDir::new().unwrap();
        let program = stub(
            dir.path(),
            r#"echo '{"type":"fatal","reason":"not_authenticated"}' >&2
exit 1"#,
        );
        let runtime = DelegateRuntime::for_stub("abacusai", program, dir.path());

        match list_models(runtime, Some(&db), ChildRegistry::default())
            .await
            .unwrap_err()
        {
            AdapterError::Unavailable { reason } => assert_eq!(reason, CREDENTIAL_REFUSED),
            other => panic!("{other:?}"),
        }
    }

    #[tokio::test]
    async fn without_a_vault_nothing_is_started() {
        let dir = TempDir::new().unwrap();
        let runtime =
            DelegateRuntime::for_stub("abacusai", dir.path().join("does-not-exist"), dir.path());

        match list_models(runtime, None, ChildRegistry::default())
            .await
            .unwrap_err()
        {
            AdapterError::Unavailable { reason } => assert_eq!(reason, NOT_CONNECTED),
            other => panic!("{other:?}"),
        }
    }

    #[tokio::test]
    async fn a_forced_vault_close_kills_the_catalog_child() {
        let (_vault, db) = vault().await;
        let dir = TempDir::new().unwrap();
        let program = stub(
            dir.path(),
            r#"sleep 5; echo '{"type":"control_response","response":{"subtype":"success","request_id":"haex-model-catalog","response":{"groups":[{"id":"abacus","models":[{"id":"ROUTE_LLM","origin":"abacus"}]}]}}}'"#,
        );
        let runtime = DelegateRuntime::for_stub("abacusai", program, dir.path());
        let children = ChildRegistry::default();
        let registry = children.clone();
        let refresh = tokio::spawn(async move { list_models(runtime, Some(&db), registry).await });

        for _ in 0..100 {
            if children.live() == 1 {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        assert_eq!(
            children.live(),
            1,
            "catalog child is registered with the vault"
        );
        children.kill_all();
        assert!(refresh.await.unwrap().is_err());
        assert_eq!(children.live(), 0, "the killed child unregisters");
    }
}
