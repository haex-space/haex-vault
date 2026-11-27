//! Spec 102 US2 (FR-004, FR-011): haex-vault's server serves the delegates in the chat whether or
//! not the device's external access is on. The access only decides whether programs outside
//! haex-vault may connect: without it the program way admits delegates' passes alone and the HTTP
//! way is closed; switching it changes that without a restart, and running delegates stay.

#[path = "common/mcp_fixture.rs"]
mod mcp_fixture;

use serde_json::json;
use uuid::Uuid;

use haex_lib::adapters::cli_delegate::haex_mcp::HaexAccess;
use haex_lib::adapters::cli_delegate::DelegateVendor;
use haex_lib::mcp_server::hello::CloseReason;
use mcp_fixture::*;

const THREAD: Uuid = Uuid::from_u128(0x7e57_0102);
const MODEL: &str = "6f3c0a52-0a47-4c0a-9a61-5d2d1a5b7e01:claude-sonnet";

fn without_access() -> Fixture {
    let mut fx = fixture();
    fx.external = false;
    fx.http_port = Some(0);
    fx
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn without_the_access_a_delegate_comes_in_and_a_program_outside_does_not() {
    let mut fx = without_access();
    let server = fx.server().await;

    let haex = fx
        .delegates
        .server_for(DelegateVendor::Claude, Some(THREAD), MODEL)
        .await
        .expect("a delegate gets the server without the external access");
    let (agent, _helper) = fx.delegate(&haex.args).await.expect("connected");
    wait_for_tools(&agent).await;

    let (reply, _stream) = fx.greet_raw("an-editor", None).await;
    assert_eq!(reply, json!({ "status": "rejected", "reason": "disabled" }));
    fx.no_question().await;

    assert_eq!(server.http_address(), None, "the HTTP way is closed");
    server.stop(CloseReason::Disabled).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn switching_the_access_opens_and_closes_the_server_without_ending_a_delegate() {
    let mut fx = without_access();
    let mut server = fx.server().await;
    let haex = fx
        .delegates
        .server_for(DelegateVendor::Claude, Some(THREAD), MODEL)
        .await
        .expect("a delegate gets the server");
    let (delegate, _delegate_helper) = fx.delegate(&haex.args).await.expect("connected");
    wait_for_tools(&delegate).await;

    // On: a program outside may ask to be registered, and the HTTP way listens.
    server
        .set_external(true, Some(0))
        .await
        .expect("the access turns on");
    assert!(server.http_address().is_some(), "the HTTP way listens");
    let (_outside, outside_helper) = fx.registered_in_standard("an-editor").await;

    // Off: the program outside is cut off, the HTTP way closes, the delegate stays.
    server
        .set_external(false, None)
        .await
        .expect("the access turns off");
    assert_eq!(server.http_address(), None);
    let exited = tokio::time::timeout(std::time::Duration::from_secs(10), outside_helper)
        .await
        .expect("the program outside ends in time")
        .expect("its helper");
    assert_ne!(exited, 0, "the program outside was cut off");
    // A request of the delegate still gets its answer: its connection stayed.
    tool_names(&delegate).await;
    let (reply, _stream) = fx.greet_raw("an-editor", None).await;
    assert_eq!(reply, json!({ "status": "rejected", "reason": "disabled" }));

    server.stop(CloseReason::Disabled).await;
}
