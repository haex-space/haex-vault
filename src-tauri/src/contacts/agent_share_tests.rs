//! `contacts.share` for agents (spec 060 US5, T058): the permission "Kontakte lesen", the dialog
//! every time, the selection the user left standing, and nothing of the contact for a "no".

use std::sync::{Arc, Mutex};
use std::time::Duration;

use super::*;
use crate::contacts::access::Caller;
use crate::contacts::service::test_support::{person, test_vault, value, TestVault};
use crate::contacts::share::envelope::decode;
use crate::files::permissions::{self, AgentFileKind, AgentFilePermission, AgentFileStatus};

const READER: [u8; 32] = [7; 32];

struct Env {
    db: VaultDb,
    prompt: PermissionPrompt,
    confirm: ContactShareConfirm,
}

impl ContactsShareEnv for Env {
    fn database(&self) -> Result<VaultDb, HolziError> {
        Ok(self.db.clone())
    }

    fn prompt(&self) -> PermissionPrompt {
        self.prompt.clone()
    }

    fn confirm(&self) -> &ContactShareConfirm {
        &self.confirm
    }
}

type Asked = Arc<Mutex<Vec<ContactShareConfirmRequest>>>;

/// What the dialog's user does with a request.
type Answer =
    Arc<dyn Fn(&ContactShareConfirmRequest) -> Option<ContactShareSelection> + Send + Sync>;

fn answering(confirm: &ContactShareConfirm, answer: Answer) -> Asked {
    let asked: Asked = Arc::default();
    let (seen, parked) = (asked.clone(), confirm.clone());
    confirm.set_emitter(Arc::new(move |request: &ContactShareConfirmRequest| {
        seen.lock().unwrap().push(request.clone());
        let (parked, id, chosen) = (parked.clone(), request.request_id.clone(), answer(request));
        tokio::spawn(async move {
            parked.answer(&id, chosen);
        });
        true
    }));
    asked
}

fn agent(vault: &TestVault, grant: Option<AgentFileStatus>) -> ContactsShareAgent<Env> {
    if let Some(status) = grant {
        let row = AgentFilePermission {
            agent_id: BUILTIN_AGENT.to_owned(),
            kind: AgentFileKind::Contacts,
            target: "*".to_owned(),
            status,
            updated_at: 1,
            reach: None,
        };
        vault
            .db
            .write_blocking(move |tx| Ok(permissions::set(tx, &row)?))
            .expect("grant");
    }
    ContactsShareAgent::builtin(Env {
        db: vault.db.clone(),
        prompt: PermissionPrompt::with_timeout(Duration::from_secs(5)),
        confirm: ContactShareConfirm::with_timeout(Duration::from_secs(5)),
    })
}

fn env_of(agent: &ContactsShareAgent<Env>) -> &Env {
    &agent.env
}

async fn anna(vault: &TestVault) -> String {
    let mut input = person(
        "Anna",
        "Beck",
        vec![
            value("phone", "mobile", "0170 1234567"),
            value("email", "home", "anna@example.org"),
        ],
    );
    input.head.note = Some("Kennengelernt auf der Messe".to_owned());
    vault
        .service
        .create(&Caller::User, &[], input)
        .await
        .expect("create")
        .id
}

async fn call(
    agent: &ContactsShareAgent<Env>,
    input: Value,
    cancel: &CancellationToken,
) -> Result<Value, NativeError> {
    agent
        .run("contacts.share", input, &builtin_call(), cancel)
        .await
        .map(|reply| reply.result)
}

/// The values of the text the agent got.
fn shared_values(result: &Value) -> Vec<String> {
    let link = result["link"].as_str().expect("a link");
    decode(link, &READER)
        .expect("a valid share")
        .envelope
        .values
        .into_iter()
        .map(|value| value.value)
        .collect()
}

fn leaks_nothing(error: &NativeError) {
    let said = format!("{} {}", error.code, error.message);
    for secret in ["Anna", "Beck", "0170", "anna@example.org", "Messe", "link"] {
        assert!(!said.contains(secret), "{said}");
    }
}

#[tokio::test]
async fn without_the_permission_to_read_the_contact_looks_missing_and_nothing_is_asked() {
    let vault = test_vault();
    let id = anna(&vault).await;
    let agent = agent(&vault, None);
    let asked = answering(&env_of(&agent).confirm, Arc::new(|_| None));
    // No window for the permission question: the refusal stands.
    let error = call(
        &agent,
        json!({ "contactId": id }),
        &CancellationToken::new(),
    )
    .await
    .expect_err("refused");
    assert_eq!(error.code, "not_found");
    leaks_nothing(&error);
    assert!(asked.lock().unwrap().is_empty());

    let denied = agent_with_denied_star(&vault);
    let error = call(
        &denied,
        json!({ "contactId": id }),
        &CancellationToken::new(),
    )
    .await
    .expect_err("refused");
    assert_eq!(error.code, "not_found");
    let missing = call(
        &denied,
        json!({ "contactId": "no-such-contact" }),
        &CancellationToken::new(),
    )
    .await
    .expect_err("refused");
    assert_eq!((error.code, error.message), (missing.code, missing.message));
}

fn agent_with_denied_star(vault: &TestVault) -> ContactsShareAgent<Env> {
    agent(vault, Some(AgentFileStatus::Denied))
}

#[tokio::test]
async fn the_text_carries_exactly_what_the_user_confirmed() {
    let vault = test_vault();
    let id = anna(&vault).await;
    let agent = agent(&vault, Some(AgentFileStatus::Read));
    let asked = answering(
        &env_of(&agent).confirm,
        Arc::new(|request| Some(request.proposed.clone())),
    );
    let result = call(
        &agent,
        json!({ "contactId": id }),
        &CancellationToken::new(),
    )
    .await
    .expect("shared");
    assert_eq!(asked.lock().unwrap().len(), 1);
    let mut values = shared_values(&result);
    values.sort();
    assert_eq!(
        values,
        ["0170 1234567".to_owned(), "anna@example.org".to_owned()]
    );
    assert!(result["size"].as_u64().unwrap() > 0);
    assert_eq!(result["qrAvailable"], json!(true));
}

#[tokio::test]
async fn the_user_removing_a_number_in_the_dialog_removes_it_from_the_text() {
    let vault = test_vault();
    let id = anna(&vault).await;
    let agent = agent(&vault, Some(AgentFileStatus::Read));
    let asked = answering(
        &env_of(&agent).confirm,
        Arc::new(|request| {
            let phone = request
                .options
                .rows
                .iter()
                .find(|row| row.kind == "phone")
                .expect("a phone")
                .id
                .clone();
            let mut chosen = request.proposed.clone();
            chosen.values.retain(|value| value != &phone);
            chosen.note = false;
            Some(chosen)
        }),
    );
    let result = call(
        &agent,
        json!({ "contactId": id }),
        &CancellationToken::new(),
    )
    .await
    .expect("shared");
    assert_eq!(shared_values(&result), ["anna@example.org".to_owned()]);
    let link = result["link"].as_str().unwrap();
    let text = format!("{:?}", decode(link, &READER).unwrap().envelope);
    assert!(!text.contains("Messe"), "{text}");
    // The dialog was shown with what the agent proposed, which was everything.
    assert_eq!(asked.lock().unwrap()[0].proposed.values.len(), 2);
}

#[tokio::test]
async fn a_narrower_proposal_is_what_the_dialog_starts_with_and_a_wider_one_is_cut() {
    let vault = test_vault();
    let id = anna(&vault).await;
    let agent = agent(&vault, Some(AgentFileStatus::Read));
    let asked = answering(
        &env_of(&agent).confirm,
        Arc::new(|request| Some(request.proposed.clone())),
    );
    let email = {
        let options = vault
            .db
            .read_blocking({
                let id = id.clone();
                move |q| build::options(q, &id).map_err(haex_crdt::Error::from)
            })
            .unwrap();
        options
            .rows
            .iter()
            .find(|row| row.kind == "email")
            .unwrap()
            .id
            .clone()
    };
    // Proposes the e-mail, an id the dialog does not offer, and the note it has not got offered
    // under a field that is no row.
    let input = json!({
        "contactId": id,
        "selection": { "values": [email, "not-an-offered-row"], "note": false, "displayName": true },
    });
    let result = call(&agent, input, &CancellationToken::new())
        .await
        .expect("shared");
    let first = asked.lock().unwrap()[0].clone();
    assert_eq!(first.proposed.values, [email]);
    assert!(!first.proposed.note);
    assert_eq!(shared_values(&result), ["anna@example.org".to_owned()]);
}

#[tokio::test]
async fn an_answer_with_more_than_the_dialog_offers_adds_nothing() {
    let vault = test_vault();
    let id = anna(&vault).await;
    let agent = agent(&vault, Some(AgentFileStatus::Read));
    answering(
        &env_of(&agent).confirm,
        Arc::new(|request| {
            let mut chosen = request.proposed.clone();
            chosen.values.push("another-contacts-row".to_owned());
            chosen.holzi = ContactShareHolziChoice::Own {
                devices: vec!["a-device-nobody-offered".to_owned()],
                relays: vec!["wss://nobody-offered.example".to_owned()],
            };
            Some(chosen)
        }),
    );
    let result = call(
        &agent,
        json!({ "contactId": id }),
        &CancellationToken::new(),
    )
    .await
    .expect("shared");
    assert_eq!(shared_values(&result).len(), 2);
    let link = result["link"].as_str().unwrap();
    assert!(decode(link, &READER).unwrap().card.is_none());
}

#[tokio::test]
async fn a_no_gives_the_agent_nothing_of_the_contact() {
    let vault = test_vault();
    let id = anna(&vault).await;
    let agent = agent(&vault, Some(AgentFileStatus::Read));
    answering(&env_of(&agent).confirm, Arc::new(|_| None));
    let error = call(
        &agent,
        json!({ "contactId": id }),
        &CancellationToken::new(),
    )
    .await
    .expect_err("declined");
    assert_eq!(error.code, "denied_by_user");
    leaks_nothing(&error);
}

#[tokio::test]
async fn no_window_a_cancelled_turn_and_a_timeout_refuse_the_same_way() {
    let vault = test_vault();
    let id = anna(&vault).await;

    // No window: nobody could be asked.
    let agent_without_window = agent(&vault, Some(AgentFileStatus::Read));
    let error = call(
        &agent_without_window,
        json!({ "contactId": id }),
        &CancellationToken::new(),
    )
    .await
    .expect_err("refused");
    assert_eq!(error.code, "denied_by_user");
    leaks_nothing(&error);

    // A window that never answers: the turn is cancelled.
    let cancelled = agent(&vault, Some(AgentFileStatus::Read));
    env_of(&cancelled).confirm.set_emitter(Arc::new(|_| true));
    let cancel = CancellationToken::new();
    let stop = cancel.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(100)).await;
        stop.cancel();
    });
    let error = call(&cancelled, json!({ "contactId": id }), &cancel)
        .await
        .expect_err("cancelled");
    assert_eq!(error.code, "denied_by_user");
    leaks_nothing(&error);

    // And one that does not answer in time.
    let slow = ContactsShareAgent::builtin(Env {
        db: vault.db.clone(),
        prompt: PermissionPrompt::with_timeout(Duration::from_secs(5)),
        confirm: ContactShareConfirm::with_timeout(Duration::from_millis(100)),
    });
    env_of(&slow).confirm.set_emitter(Arc::new(|_| true));
    let error = call(&slow, json!({ "contactId": id }), &CancellationToken::new())
        .await
        .expect_err("expired");
    assert_eq!(error.code, "denied_by_user");
    leaks_nothing(&error);
}

#[tokio::test]
async fn every_call_asks_again() {
    let vault = test_vault();
    let id = anna(&vault).await;
    let agent = agent(&vault, Some(AgentFileStatus::Read));
    let asked = answering(
        &env_of(&agent).confirm,
        Arc::new(|request| Some(request.proposed.clone())),
    );
    for _ in 0..2 {
        call(
            &agent,
            json!({ "contactId": id }),
            &CancellationToken::new(),
        )
        .await
        .expect("shared");
    }
    let asked = asked.lock().unwrap();
    assert_eq!(asked.len(), 2);
    assert_ne!(asked[0].request_id, asked[1].request_id);
}

#[tokio::test]
async fn a_contact_without_anything_to_share_is_not_put_to_the_user() {
    let vault = test_vault();
    // The own contact, still empty.
    let empty = crate::contacts::ids::SELF_CONTACT_ID.to_owned();
    let agent = agent(&vault, Some(AgentFileStatus::Read));
    let asked = answering(&env_of(&agent).confirm, Arc::new(|_| None));
    let error = call(
        &agent,
        json!({ "contactId": empty }),
        &CancellationToken::new(),
    )
    .await
    .expect_err("nothing");
    assert_eq!(error.code, "nothing_to_share");
    assert!(asked.lock().unwrap().is_empty());
}

#[tokio::test]
async fn bad_input_and_other_actions_are_refused() {
    let vault = test_vault();
    let agent = agent(&vault, Some(AgentFileStatus::Read));
    let error = call(&agent, json!({}), &CancellationToken::new())
        .await
        .expect_err("invalid");
    assert_eq!(error.code, "invalid_input");
    let error = agent
        .run(
            "contacts.get",
            json!({}),
            &builtin_call(),
            &CancellationToken::new(),
        )
        .await
        .expect_err("not ours");
    assert_eq!(error.code, "unknown_action");
}

#[test]
fn a_selection_is_cut_to_the_offer_and_never_widened() {
    let options = ContactShareOptions {
        rows: vec![],
        head: crate::contacts::share::model::ContactShareHead::default(),
        holzi: ContactShareHolzi::None,
        name: String::new(),
        can_share: false,
    };
    let wanted = ContactShareSelection {
        values: vec!["x".to_owned()],
        display_name: true,
        nickname: true,
        organization: true,
        job_title: true,
        note: true,
        holzi: ContactShareHolziChoice::Card,
    };
    let cut = restrict(&options, &wanted);
    assert!(cut.values.is_empty());
    assert!(!cut.display_name && !cut.nickname && !cut.organization && !cut.job_title && !cut.note);
    assert_eq!(cut.holzi, ContactShareHolziChoice::None);
}

fn builtin_call() -> crate::chat::tools::native_action::NativeCall {
    crate::chat::tools::native_action::NativeCall::builtin(
        crate::chat::tools::permission::PermissionMode::Manual,
    )
}
