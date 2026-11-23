//! The Abacus connect flow end to end against a stand-in of the sign-in service (spec 096, T015):
//! prompt, polling, the credential entry and the one provider row, what a failure leaves behind, and
//! that nothing personal reaches the record. No real account is used.

use std::sync::{Arc, Mutex};

use serde_json::json;
use tokio_util::sync::CancellationToken;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use crate::cli_tools::record::Record;
use crate::error::HaexError;
use crate::providers::abacus_signin::{Client, Secret};
use crate::providers::connect_abacus::{connect, failure, save_connection};
use crate::providers::delegate_credential::{self, Read, KIND_DEVICE_CODE, VAULT_SCOPE};
use crate::providers::providers_tests::open_vault;
use crate::storage::providers::{self as providers_store, ProviderKind};
use crate::storage::query;
use crate::vault_gate::{VaultDb, VaultGate};

const EMAIL: &str = "placeholder.person@example.invalid";
const USER_CODE: &str = "PLACE-HOLDER";
const DEVICE_CODE: &str = "placeholder-device-code-of-the-connect-tests";

struct Fixture {
    _dir: tempfile::TempDir,
    db: Arc<haex_crdt::Database>,
    gate: VaultDb,
    record_dir: tempfile::TempDir,
}

impl Fixture {
    fn new() -> Self {
        let dir = tempfile::tempdir().expect("tmp dir");
        let db = open_vault(dir.path());
        let gate = VaultGate::new().vault_db(Arc::clone(&db)).expect("gate");
        Self {
            _dir: dir,
            db,
            gate,
            record_dir: tempfile::tempdir().expect("record dir"),
        }
    }

    fn record(&self) -> Record {
        Record::new(self.record_dir.path().to_path_buf())
    }

    fn rows(&self) -> Vec<providers_store::Provider> {
        query::read(&self.db, |r| {
            Ok(providers_store::list_providers(r).expect("list"))
        })
        .expect("read")
    }

    async fn credential(&self) -> Option<String> {
        match delegate_credential::read(&self.gate, "abacusai", VAULT_SCOPE)
            .await
            .expect("read")
        {
            Read::Ready(credential) => Some(credential.secret().to_owned()),
            Read::Missing => None,
        }
    }

    /// Everything the record of the vault says, as one text.
    fn record_text(&self) -> String {
        std::fs::read_dir(self.record_dir.path())
            .map(|entries| {
                entries
                    .flatten()
                    .filter_map(|entry| std::fs::read_to_string(entry.path()).ok())
                    .collect::<String>()
            })
            .unwrap_or_default()
    }
}

/// A service that confirms at once: the prompt on the first call, the device code on the first poll.
async fn service() -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/_authenticateDeviceForCodeLlm"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "success": true,
            "result": {
                "verificationUri": "https://example.invalid/device",
                "userCode": USER_CODE,
                "pollingInterval": 1,
            }
        })))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/api/_pollDeviceCode"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "success": true,
            "result": { "deviceCode": DEVICE_CODE, "chatWebUrl": "https://example.invalid/chat" }
        })))
        .mount(&server)
        .await;
    server
}

#[tokio::test]
async fn a_connect_shows_the_prompt_then_saves_the_credential_once_and_one_empty_row() {
    let fixture = Fixture::new();
    let server = service().await;
    let client = Client::with_base(&server.uri()).expect("client");
    let shown = Arc::new(Mutex::new(None));
    let seen = Arc::clone(&shown);

    let provider = connect(
        &client,
        &fixture.gate,
        &fixture.record(),
        "Abacus AI".into(),
        &Secret::new(EMAIL.to_owned()),
        &CancellationToken::new(),
        move |prompt| {
            *seen.lock().unwrap() = Some((prompt.uri.clone(), prompt.code.expose().to_owned()));
        },
    )
    .await
    .expect("connected");

    assert_eq!(
        shown.lock().unwrap().clone(),
        Some((
            "https://example.invalid/device".to_owned(),
            USER_CODE.to_owned()
        ))
    );
    assert_eq!(provider.kind, ProviderKind::CliDelegate);
    assert_eq!(provider.adapter.as_deref(), Some("abacusai"));
    assert_eq!(
        provider.credentials,
        Some(Vec::new()),
        "the row holds no secret"
    );
    assert_eq!(fixture.rows().len(), 1);
    assert_eq!(fixture.credential().await.as_deref(), Some(DEVICE_CODE));
}

#[tokio::test]
async fn connecting_again_reuses_the_row_and_the_scope_keeps_one_entry() {
    let fixture = Fixture::new();
    let server = service().await;
    let client = Client::with_base(&server.uri()).expect("client");
    for _ in 0..2 {
        connect(
            &client,
            &fixture.gate,
            &fixture.record(),
            "Abacus AI".into(),
            &Secret::new(EMAIL.to_owned()),
            &CancellationToken::new(),
            |_| {},
        )
        .await
        .expect("connected");
    }

    assert_eq!(fixture.rows().len(), 1);
    assert_eq!(fixture.credential().await.as_deref(), Some(DEVICE_CODE));
}

#[tokio::test]
async fn a_refusal_stores_nothing_and_says_so_with_the_services_text() {
    let fixture = Fixture::new();
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/_authenticateDeviceForCodeLlm"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "success": false,
            "error": "No such account"
        })))
        .mount(&server)
        .await;
    let client = Client::with_base(&server.uri()).expect("client");

    let error = connect(
        &client,
        &fixture.gate,
        &fixture.record(),
        "Abacus AI".into(),
        &Secret::new(EMAIL.to_owned()),
        &CancellationToken::new(),
        |_| panic!("no prompt after a refusal"),
    )
    .await
    .expect_err("refused");

    match error {
        HaexError::InvalidInput { reason } => {
            assert!(reason.starts_with("delegate.signin_refused"), "{reason}");
            assert!(reason.contains("No such account"), "{reason}");
        }
        other => panic!("{other:?}"),
    }
    assert!(fixture.rows().is_empty());
    assert_eq!(fixture.credential().await, None);
    assert!(fixture.record_text().contains("connect_failed"));
}

#[tokio::test]
async fn a_cancel_while_waiting_stores_nothing() {
    let fixture = Fixture::new();
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/_authenticateDeviceForCodeLlm"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "success": true,
            "result": { "verificationUri": "https://example.invalid/device", "userCode": USER_CODE, "pollingInterval": 1 }
        })))
        .mount(&server)
        .await;
    // Never confirmed.
    Mock::given(method("POST"))
        .and(path("/api/_pollDeviceCode"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "success": false })))
        .mount(&server)
        .await;
    let client = Client::with_base(&server.uri()).expect("client");
    let cancel = CancellationToken::new();
    let stop = cancel.clone();

    let error = connect(
        &client,
        &fixture.gate,
        &fixture.record(),
        "Abacus AI".into(),
        &Secret::new(EMAIL.to_owned()),
        &cancel,
        move |_| stop.cancel(),
    )
    .await
    .expect_err("cancelled");

    assert!(
        matches!(&error, HaexError::InvalidInput { reason } if reason.starts_with("delegate.signin_cancelled")),
        "{error:?}"
    );
    assert!(fixture.rows().is_empty());
    assert_eq!(fixture.credential().await, None);
}

#[tokio::test]
async fn a_failed_row_write_puts_the_prior_credential_back() {
    let fixture = Fixture::new();
    delegate_credential::save(
        &fixture.gate,
        "abacusai",
        KIND_DEVICE_CODE,
        "prior-code",
        VAULT_SCOPE,
    )
    .await
    .expect("a prior connection");
    // Make the row write fail, as a full disk or a sync conflict would.
    fixture
        .db
        .write(|tx| {
            tx.execute(
                "CREATE TRIGGER refuse_provider_rows BEFORE INSERT ON providers \
                 BEGIN SELECT RAISE(ABORT, 'refused'); END",
                haex_crdt::rusqlite::params![],
            )?;
            Ok(())
        })
        .expect("a trigger");

    let result = save_connection(
        &fixture.gate,
        "Abacus AI".into(),
        &Secret::new(DEVICE_CODE.to_owned()),
    )
    .await;

    assert!(result.is_err(), "the row write was refused");
    assert_eq!(fixture.credential().await.as_deref(), Some("prior-code"));
    assert!(fixture.rows().is_empty());
}

#[tokio::test]
async fn a_failed_first_connection_leaves_no_orphan_credential() {
    let fixture = Fixture::new();
    fixture
        .db
        .write(|tx| {
            tx.execute(
                "CREATE TRIGGER refuse_provider_rows BEFORE INSERT ON providers \
                 BEGIN SELECT RAISE(ABORT, 'refused'); END",
                haex_crdt::rusqlite::params![],
            )?;
            Ok(())
        })
        .expect("a trigger");

    let result = save_connection(
        &fixture.gate,
        "Abacus AI".into(),
        &Secret::new(DEVICE_CODE.to_owned()),
    )
    .await;

    assert!(result.is_err());
    assert_eq!(
        fixture.credential().await,
        None,
        "nothing of the failed connection stays"
    );
}

#[tokio::test]
async fn the_record_of_a_connect_carries_fixed_text_only() {
    let fixture = Fixture::new();
    let server = service().await;
    let client = Client::with_base(&server.uri()).expect("client");
    connect(
        &client,
        &fixture.gate,
        &fixture.record(),
        "Abacus AI".into(),
        &Secret::new(EMAIL.to_owned()),
        &CancellationToken::new(),
        |_| {},
    )
    .await
    .expect("connected");

    let record = fixture.record_text();
    assert!(
        record.contains("connect_started") && record.contains("connect_finished"),
        "{record}"
    );
    for secret in [EMAIL, USER_CODE, DEVICE_CODE, "example.invalid"] {
        assert!(!record.contains(secret), "{record}");
    }
}

#[test]
fn every_failure_has_a_code_the_frontend_words() {
    use crate::providers::abacus_signin::SigninError;
    for (error, code) in [
        (SigninError::Expired, "delegate.signin_expired"),
        (SigninError::Cancelled, "delegate.signin_cancelled"),
        (SigninError::Network, "delegate.signin_network"),
        (SigninError::Unexpected, "delegate.signin_unexpected"),
        (SigninError::Refused("x".into()), "delegate.signin_refused"),
    ] {
        match failure(&error) {
            HaexError::InvalidInput { reason } => assert!(reason.starts_with(code), "{reason}"),
            other => panic!("{other:?}"),
        }
    }
}
