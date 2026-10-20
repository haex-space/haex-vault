//! The consent to send a job outside this device (spec 055 US4, T046, T048): the question names
//! the provider, its host and place and what would be sent; "for the session" holds until the vault
//! closes, which ends the process (spec 013), so the next vault session asks again; a remembered
//! consent writes a row that the settings can revoke; a denial is remembered only when asked to.

use std::sync::Arc;

use serde_json::{json, Value};

use super::dispatch::tests::{setup, Setup};
use super::dispatch::{call, CallContext};
use super::events::FRAME_EVENT;
use super::model_jobs_run_tests::{
    allow, image_step, job, models, prompt_of, refused, scripted, step, GOOD,
};
use super::permissions::PERMISSION_REQUEST;
use crate::extensions::commands::permissions::{
    list, remove, resolve, PermissionRemoveArgs, PermissionResolveArgs,
};
use crate::extensions::host::ExtensionHost;
use crate::extensions::permissions::prompts::PermissionDecision;
use crate::extensions::permissions::store::rows_of;
use crate::extensions::registry::effective::effective_bundle;
use crate::model_jobs::bridge::COMPLETED_EVENT;
use crate::model_jobs::test_support::{FakeSource, Script};
use crate::storage::models::ModelRow;
use crate::storage::providers::Provider;

struct Consent {
    s: Setup,
    server: Provider,
    rows: Vec<ModelRow>,
    source: Arc<FakeSource>,
}

/// One granted model on a server in the own network, without a consent yet.
fn consent() -> Consent {
    let s = setup();
    let (server, rows) = models(&s, "http://192.168.1.20:11434/v1", &["school"], true);
    let source = scripted(&s, &[(&rows[0].id, Script::Answer(GOOD.into()))]);
    allow(&s, "model", "low", &rows[0].id, "granted");
    Consent {
        s,
        server,
        rows,
        source,
    }
}

impl Consent {
    fn start(&self, ctx: &CallContext) -> Result<Value, crate::extensions::error::BridgeError> {
        call(ctx, "extension_model_job_start", &job(vec![step()]))
    }

    /// Waits for the end of the job `answer` started.
    fn completed(&self, answer: &Value) -> Value {
        let id = answer["jobId"].clone();
        self.s.recorded.wait_for(|(name, payload)| {
            name == FRAME_EVENT
                && payload["type"] == COMPLETED_EVENT
                && payload["data"]["jobId"] == id
        })["data"]
            .clone()
    }

    /// The last question holzi's window was shown.
    fn question(&self) -> Value {
        let recorded = self.s.recorded.0.lock().unwrap();
        recorded
            .iter()
            .rev()
            .find(|(name, _)| name == PERMISSION_REQUEST)
            .map(|(_, payload)| payload.clone())
            .expect("a question")
    }

    fn answer(&self, ctx: &CallContext, decision: PermissionDecision, remember: bool) {
        let request_id = self.question()["requestId"].as_str().unwrap().to_owned();
        resolve(
            &ctx.db,
            &ctx.host,
            ctx.emitter.as_ref(),
            ctx.device,
            PermissionResolveArgs {
                request_id,
                decision,
                remember,
                model: None,
            },
            2,
        )
        .unwrap();
    }

    /// The remembered consent rows of the extension.
    fn rows(&self) -> Vec<(String, String)> {
        let extension = self.s.ctx.session.extension_id;
        self.s
            .ctx
            .db
            .read_blocking(move |q| {
                rows_of(q, extension).map_err(|e| haex_crdt::Error::consumer(e.to_string()))
            })
            .unwrap()
            .into_iter()
            .filter(|r| r.kind == "modelDestination")
            .map(|r| (r.target, r.status))
            .collect()
    }

    /// The next vault session: a close ends the process (spec 013), so a new host on the same
    /// vault, with nothing held in memory.
    fn next_vault_session(&self) -> CallContext {
        let host = Arc::new(ExtensionHost::default());
        host.set_model_source(self.source.clone());
        let extension = self.s.ctx.session.extension_id;
        let bundle = self
            .s
            .ctx
            .db
            .read_blocking(move |q| effective_bundle(q, extension).map_err(Into::into))
            .unwrap()
            .unwrap()
            .bundle_id;
        let session = host.frames.open(extension, bundle, "tab-1");
        CallContext {
            db: self.s.ctx.db.clone(),
            host,
            session,
            device: self.s.ctx.device,
            emitter: Arc::clone(&self.s.ctx.emitter),
        }
    }
}

#[test]
fn the_question_names_the_provider_its_host_and_place_and_what_would_be_sent() {
    let c = consent();

    let details = prompt_of(call(
        &c.s.ctx,
        "extension_model_job_start",
        &job(vec![image_step()]),
    ));
    let question = c.question();

    assert_eq!(question["kind"], "modelDestination");
    assert_eq!(question["target"], c.server.id.to_string());
    assert_eq!(
        question["destination"],
        json!({ "provider": "Test provider", "host": "192.168.1.20:11434", "place": "network" })
    );
    assert_eq!(
        question["transfer"],
        json!({ "kinds": ["text", "image"], "textBytes": 49, "imageCount": 1, "imageBytes": 8,
                "earlierResult": false })
    );
    // The extension hears what the user is asked about, never where it would go.
    assert_eq!(details["transfer"], question["transfer"]);
    assert!(!details.to_string().contains("192.168.1.20"));
    assert!(c.source.asked().is_empty());
}

#[test]
fn a_consent_for_the_session_holds_until_the_vault_closes_and_writes_no_row() {
    let c = consent();
    prompt_of(c.start(&c.s.ctx));

    c.answer(&c.s.ctx, PermissionDecision::Allow, false);
    let data = c.completed(&c.start(&c.s.ctx).unwrap());

    assert_eq!(data["status"], "completed");
    assert_eq!(c.source.asked(), [c.rows[0].id.clone()]);
    assert!(c.rows().is_empty(), "held in memory only");
    let listed = list(&c.s.ctx.db, &c.s.ctx.host, c.s.ctx.session.extension_id).unwrap();
    let held = listed
        .iter()
        .find(|p| p.kind == "modelDestination")
        .expect("listed for the settings");
    assert!(held.temporary);
    assert_eq!(
        held.destination.as_ref().map(|d| d.provider.as_str()),
        Some("Test provider")
    );

    let next = c.next_vault_session();
    let details = prompt_of(c.start(&next));
    assert_eq!(details["resourceType"], "modelDestination");
    assert_eq!(
        c.source.asked().len(),
        1,
        "nothing sent in the next session"
    );
}

#[test]
fn a_remembered_consent_holds_in_the_next_session_until_it_is_revoked() {
    let c = consent();
    prompt_of(c.start(&c.s.ctx));

    c.answer(&c.s.ctx, PermissionDecision::Allow, true);

    assert_eq!(c.rows(), [(c.server.id.to_string(), "granted".to_owned())]);
    let next = c.next_vault_session();
    let data = c.completed(&c.start(&next).unwrap());
    assert_eq!(data["status"], "completed");

    let extension = c.s.ctx.session.extension_id;
    let row = list(&next.db, &next.host, extension)
        .unwrap()
        .into_iter()
        .find(|p| p.kind == "modelDestination")
        .unwrap();
    assert!(!row.temporary);
    assert_eq!(
        row.destination.map(|d| (d.provider, d.place)),
        Some(("Test provider".to_owned(), "network".to_owned()))
    );
    remove(
        &next.db,
        &next.host,
        next.device,
        PermissionRemoveArgs {
            extension_id: extension.to_string(),
            permission_id: row.id,
            temporary_key: None,
        },
    )
    .unwrap();

    assert!(c.rows().is_empty());
    prompt_of(c.start(&next));
    assert_eq!(c.source.asked().len(), 1);
}

#[test]
fn a_denial_is_remembered_only_when_asked_to() {
    let c = consent();
    prompt_of(c.start(&c.s.ctx));

    c.answer(&c.s.ctx, PermissionDecision::Deny, false);

    assert!(c.rows().is_empty());
    assert_eq!(refused(c.start(&c.s.ctx)), (1002, json!("consent_denied")));
    let next = c.next_vault_session();
    prompt_of(c.start(&next));

    c.answer(&next, PermissionDecision::Deny, true);

    assert_eq!(c.rows(), [(c.server.id.to_string(), "denied".to_owned())]);
    let after = c.next_vault_session();
    assert_eq!(refused(c.start(&after)), (1002, json!("consent_denied")));
    assert!(c.source.asked().is_empty(), "nothing was ever sent");
}
