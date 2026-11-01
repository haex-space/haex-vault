//! Spec 021 US5: each external agent reaches files as itself, through an executor of its own that
//! is made once; the built-in agent keeps its own, and no other caller gets one.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use serde_json::json;
use tokio_util::sync::CancellationToken;

use super::FilesAgents;
use crate::caller::Caller;
use crate::chat::tools::native_action::{NativeCall, NativeExecutor};
use crate::chat::tools::permission::PermissionMode;
use crate::files::access::DeviceGrant;
use crate::files::agent::exec::FilesAgent;
use crate::files::agent::test_env::{names, Scene};

fn call(caller: Caller) -> NativeCall {
    NativeCall {
        caller,
        mode: PermissionMode::Auto,
    }
}

fn agent(id: &str) -> Caller {
    Caller::ExternalAgent { id: id.to_owned() }
}

#[tokio::test]
async fn an_external_agent_needs_its_own_device_grant_and_keeps_its_executor() {
    let builtin = Scene::new(None);
    let builtin_root = builtin.at("");
    let external = Scene::new(None);
    let external_root = external.at("");
    let env = Mutex::new(Some(external.agent.env));
    let made = Arc::new(AtomicUsize::new(0));
    let counted = Arc::clone(&made);
    let agents = FilesAgents::new(Arc::new(builtin.agent), move |id| {
        counted.fetch_add(1, Ordering::SeqCst);
        FilesAgent::external(env.lock().unwrap().take().expect("one external agent"), id)
    });
    let cancel = CancellationToken::new();
    let list = |root: &str| json!({ "source": "device", "path": root });

    // The built-in agent reaches the device unless revoked; an external one has no grant yet.
    let listed = agents
        .run(
            "files.list",
            list(&builtin_root),
            &call(Caller::BuiltinAgent),
            &cancel,
        )
        .await
        .expect("the built-in agent lists");
    assert!(names(&listed.result, "entries").contains(&"notiz.txt".to_owned()));
    let refused = agents
        .run(
            "files.list",
            list(&external_root),
            &call(agent("a")),
            &cancel,
        )
        .await
        .expect_err("no device grant");
    assert_eq!(refused.code, "files_not_granted");

    // "Dateien des Geräts" for this agent: listing and reading work, from the same executor.
    let own = agents.agent_for(&agent("a")).expect("its executor");
    own.env.grants.lock().unwrap().device = Some(DeviceGrant::Granted);
    let listed = agents
        .run(
            "files.list",
            list(&external_root),
            &call(agent("a")),
            &cancel,
        )
        .await
        .expect("listed");
    assert!(names(&listed.result, "entries").contains(&"notiz.txt".to_owned()));
    let read = agents
        .run(
            "files.read",
            json!({ "source": "device", "path": format!("{external_root}/notiz.txt") }),
            &call(agent("a")),
            &cancel,
        )
        .await
        .expect("read");
    assert_eq!(read.result["text"], "Hallo Welt");
    assert_eq!(made.load(Ordering::SeqCst), 1, "made once per agent");
}

#[tokio::test]
async fn an_executor_refuses_a_caller_it_was_not_made_for() {
    let scene = Scene::new(None);
    let root = scene.at("");
    let external = FilesAgent::external(scene.agent.env, "a");
    let cancel = CancellationToken::new();
    for caller in [
        Caller::BuiltinAgent,
        agent("b"),
        Caller::Extension { id: "x".to_owned() },
    ] {
        let refused = external
            .run(
                "files.list",
                json!({ "source": "device", "path": root }),
                &call(caller),
                &cancel,
            )
            .await
            .expect_err("refused");
        assert_eq!(refused.code, "forbidden_for_agents");
    }
}

#[tokio::test]
async fn no_other_caller_gets_an_executor() {
    let scene = Scene::new(None);
    let agents = FilesAgents::new(Arc::new(scene.agent), |_| unreachable!("no external agent"));
    for caller in [Caller::User, Caller::Extension { id: "x".to_owned() }] {
        let refused = agents
            .run(
                "files.sources",
                json!({}),
                &call(caller),
                &CancellationToken::new(),
            )
            .await
            .expect_err("refused");
        assert_eq!(refused.code, "forbidden_for_agents");
    }
}
