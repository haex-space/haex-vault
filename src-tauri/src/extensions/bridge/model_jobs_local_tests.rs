//! A job on a model on this device (spec 055 T045 (3), SC-003): after the spike T057 local vision
//! runs as an OpenAI-compatible server on loopback (research R8). Such a job asks for no consent
//! and its image reaches only that server, through holzi's real adapter.

use serde_json::json;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::dispatch::tests::setup;
use super::model_jobs_run_tests::{allow, completion, image_step, job, models, start, GOOD};
use super::permissions::PERMISSION_REQUEST;

#[test]
fn a_job_on_this_device_asks_no_consent_and_its_image_reaches_only_the_local_server() {
    let runtime = tokio::runtime::Runtime::new().expect("runtime");
    let server = runtime.block_on(MockServer::start());
    let event = json!({ "choices": [{ "delta": { "content": GOOD } }] });
    let body = format!("data: {event}\n\ndata: [DONE]\n\n");
    runtime.block_on(
        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_raw(body, "text/event-stream"))
            .mount(&server),
    );
    let s = setup();
    let (_, rows) = models(&s, &format!("{}/v1", server.uri()), &["vision"], true);
    allow(&s, "model", "low", &rows[0].id, "granted");

    start(&s, &job(vec![image_step()])).expect("started without a question");
    let data = completion(&s);

    assert_eq!(data["status"], "completed", "{data}");
    assert_eq!(data["provenance"][0]["attempts"][0]["place"], "this_device");
    let asked = s.recorded.0.lock().unwrap().clone();
    assert!(
        !asked.iter().any(|(name, _)| name == PERMISSION_REQUEST),
        "no consent for this device"
    );
    let requests = runtime
        .block_on(server.received_requests())
        .expect("recorded");
    assert_eq!(requests.len(), 1);
    let sent = String::from_utf8_lossy(&requests[0].body);
    assert!(
        sent.contains("iVBORw0KGgo="),
        "the image went to the local server"
    );
}
