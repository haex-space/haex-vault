//! Shells of extensions (spec 017, US11, T109) through the bridge, on a real PTY.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;

use serde_json::{json, Value};
use uuid::Uuid;

use super::*;
use crate::extensions::bridge::dispatch::{call, Emit};
use crate::extensions::bridge::events::FRAME_EVENT;
use crate::extensions::commands::permissions::{set, PermissionSetArgs};
use crate::extensions::host::ExtensionHost;
use crate::extensions::registry::effective::effective_bundle;
use crate::extensions::registry::install::install;
use crate::passwords::test_support::open_test_vault;
use crate::storage::known_devices;
use crate::vault_gate::{VaultDb, VaultGate};

#[path = "shell_flow_tests.rs"]
mod flow;
#[path = "shell_permissions_tests.rs"]
mod permissions;

/// Only a guard against a hanging test: every wait ends on an event the shell sends.
const PATIENCE: Duration = Duration::from_secs(30);

#[derive(Default)]
struct Recorded {
    events: Mutex<Vec<(String, Value)>>,
    arrived: Condvar,
}

impl Emit for Recorded {
    fn emit(&self, event: &str, payload: Value) {
        self.events
            .lock()
            .unwrap()
            .push((event.to_owned(), payload));
        self.arrived.notify_all();
    }
}

/// The `data` of every frame event of `kind` for `session`.
fn of(events: &[(String, Value)], kind: &str, session: &str) -> Vec<Value> {
    events
        .iter()
        .filter(|(e, p)| e == FRAME_EVENT && p["type"] == kind && p["data"]["sessionId"] == session)
        .map(|(_, p)| p["data"].clone())
        .collect()
}

fn output(events: &[(String, Value)], session: &str) -> String {
    of(events, OUTPUT, session)
        .iter()
        .filter_map(|d| d["data"].as_str().map(str::to_owned))
        .collect()
}

impl Recorded {
    /// Waits until the events so far satisfy `done`.
    fn wait_until(&self, what: &str, done: impl Fn(&[(String, Value)]) -> bool) {
        let events = self.events.lock().unwrap();
        let (_events, timeout) = self
            .arrived
            .wait_timeout_while(events, PATIENCE, |events| !done(events))
            .unwrap();
        assert!(!timeout.timed_out(), "timed out: {what}");
    }

    fn waited_for_output(&self, session: &str, text: &str) {
        self.wait_until(text, |events| output(events, session).contains(text));
    }

    fn waited_for_exit(&self, session: &str) -> Value {
        self.wait_until("the end", |events| !of(events, EXIT, session).is_empty());
        of(&self.events.lock().unwrap(), EXIT, session)[0].clone()
    }

    fn all(&self) -> Vec<(String, Value)> {
        self.events.lock().unwrap().clone()
    }
}

fn alive(pid: i32) -> bool {
    // SAFETY: signal 0 only checks that the process exists.
    unsafe { libc::kill(pid, 0) == 0 }
}

struct Setup {
    _dir: tempfile::TempDir,
    vault: VaultDb,
    recorded: Arc<Recorded>,
    host: Arc<ExtensionHost>,
    notes: CallContext,
    minimal: CallContext,
}

fn installed(vault: &VaultDb, fixture: &str, device: Uuid) -> (Uuid, Uuid) {
    let bytes = std::fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/extension_bundles")
            .join(fixture),
    )
    .unwrap();
    let extension = install(vault, &bytes, vec![], false, device, 1)
        .unwrap()
        .ids
        .extension_id;
    let bundle = vault
        .read_blocking(move |q| effective_bundle(q, extension).map_err(Into::into))
        .unwrap()
        .unwrap()
        .bundle_id;
    (extension, bundle)
}

fn setup() -> Setup {
    let (dir, db) = open_test_vault();
    let vault = VaultGate::new().vault_db(Arc::new(db)).unwrap();
    let device = vault
        .read_blocking(|q| known_devices::list_devices(q))
        .unwrap()[0]
        .vault_device_uuid;
    let host = Arc::new(ExtensionHost::default());
    let recorded = Arc::new(Recorded::default());
    let context = |(extension, bundle): (Uuid, Uuid)| CallContext {
        db: vault.clone(),
        host: Arc::clone(&host),
        session: host.frames.open(extension, bundle, "tab"),
        device,
        emitter: Arc::clone(&recorded) as Arc<dyn Emit>,
    };
    let notes = context(installed(&vault, "good-notes-like.xt", device));
    let minimal = context(installed(&vault, "good-minimal.xt", device));
    Setup {
        _dir: dir,
        vault,
        recorded,
        host,
        notes,
        minimal,
    }
}

fn sh() -> PathBuf {
    resolve_program("/bin/sh").expect("/bin/sh")
}

impl Setup {
    fn allow(&self, ctx: &CallContext, program: &Path) {
        set(
            &self.vault,
            ctx.device,
            PermissionSetArgs {
                extension_id: ctx.session.extension_id.to_string(),
                kind: "shell".into(),
                action: "execute".into(),
                target: program.to_string_lossy().into_owned(),
                status: "granted".into(),
                replaces: None,
            },
            2,
        )
        .unwrap();
    }

    fn start(&self, ctx: &CallContext) -> String {
        let started = call(
            ctx,
            "extension_shell_create",
            &json!({ "options": { "shell": "/bin/sh", "cols": 100, "rows": 30, "env": { "PROBE": "on" } } }),
        )
        .unwrap();
        assert_eq!(
            started["shellName"],
            sh().file_stem().unwrap().to_string_lossy().as_ref()
        );
        started["sessionId"].as_str().unwrap().to_owned()
    }

    fn write(&self, ctx: &CallContext, session: &str, data: &str) -> Result<Value, BridgeError> {
        call(
            ctx,
            "extension_shell_write",
            &json!({ "sessionId": session, "data": data }),
        )
    }
}

fn code(result: Result<Value, BridgeError>) -> u16 {
    result.map_or_else(|e| e.code.as_u16(), |_| 0)
}

#[test]
fn a_granted_shell_runs_echo_resizes_and_reports_its_end() {
    let s = setup();
    s.allow(&s.notes, &sh());
    let session = s.start(&s.notes);
    s.write(&s.notes, &session, "echo \"probe-$PROBE\"; stty size\n")
        .unwrap();
    s.recorded.waited_for_output(&session, "probe-on");
    s.recorded.waited_for_output(&session, "30 100");

    call(
        &s.notes,
        "extension_shell_resize",
        &json!({ "sessionId": session, "cols": 120, "rows": 40 }),
    )
    .unwrap();
    s.write(&s.notes, &session, "stty size; exit 3\n").unwrap();
    s.recorded.waited_for_output(&session, "40 120");
    assert_eq!(s.recorded.waited_for_exit(&session)["exitCode"], 3);
    assert_eq!(s.host.shells.count(s.notes.session.extension_id), 0);
    assert_eq!(code(s.write(&s.notes, &session, "x")), 1001, "gone");
}

#[test]
fn a_shell_does_not_inherit_haexs_own_variables() {
    // Names nobody else sets; the test process is holzi's process here.
    std::env::set_var("WEBKIT_HAEX_SHELL_PROBE", "leak");
    std::env::set_var("LD_HAEX_SHELL_PROBE", "leak");
    let s = setup();
    s.allow(&s.notes, &sh());
    let session = s.start(&s.notes);
    s.write(
        &s.notes,
        &session,
        "echo \"gone=[$WEBKIT_HAEX_SHELL_PROBE$LD_HAEX_SHELL_PROBE] term=[$TERM] kept=[${HOME:+home}${PATH:+path}]\"\n",
    )
    .unwrap();
    // The terminal echoes the typed line too; only the printed one has the values.
    s.recorded
        .waited_for_output(&session, &format!("gone=[] term=[{TERM}] kept=[homepath]"));
    for name in [
        "LD_PRELOAD",
        "gdk_backend",
        "GIO_EXTRA_MODULES",
        "__NV_PRIME_RENDER_OFFLOAD",
        "RUST_LOG",
    ] {
        assert!(not_inherited(name), "{name}");
    }
    for name in [
        "HOME",
        "USER",
        "PATH",
        "LANG",
        "LC_ALL",
        "SHELL",
        "DISPLAY",
        "XDG_RUNTIME_DIR",
    ] {
        assert!(!not_inherited(name), "{name}");
    }
    call(
        &s.notes,
        "extension_shell_close",
        &json!({ "sessionId": session }),
    )
    .unwrap();
}

#[test]
fn a_session_belongs_to_its_extension() {
    let s = setup();
    s.allow(&s.notes, &sh());
    let session = s.start(&s.notes);
    assert_eq!(code(s.write(&s.minimal, &session, "echo x\n")), 1001);
    assert_eq!(
        code(call(
            &s.minimal,
            "extension_shell_resize",
            &json!({ "sessionId": session, "cols": 10, "rows": 10 })
        )),
        1001
    );
    assert_eq!(
        code(call(
            &s.minimal,
            "extension_shell_close",
            &json!({ "sessionId": session })
        )),
        1001
    );
    assert_eq!(s.host.shells.count(s.notes.session.extension_id), 1);
    call(
        &s.notes,
        "extension_shell_close",
        &json!({ "sessionId": session }),
    )
    .unwrap();
    s.recorded.waited_for_exit(&session);
    let heard: Vec<String> = s
        .recorded
        .all()
        .iter()
        .filter(|(_, p)| p["data"]["sessionId"] == session.as_str())
        .map(|(_, p)| p["frame"].as_str().unwrap().to_owned())
        .collect();
    assert!(
        heard.iter().all(|f| *f == s.notes.session.frame),
        "only the frames of the owning extension hear it"
    );
}

#[test]
fn ending_all_shells_of_an_extension_kills_every_job_of_the_shell() {
    let s = setup();
    s.allow(&s.notes, &sh());
    let session = s.start(&s.notes);
    // A job in a process group of its own (job control) that ignores the hangup: only a kill of
    // the shell's whole session ends it, whether or not the shell passes the hangup on.
    s.write(
        &s.notes,
        &session,
        "sh -c 'trap \"\" HUP; exec sleep 100' & echo \"child=$!.\"\n",
    )
    .unwrap();
    // The terminal echoes the typed line too; only the printed pid ends in a dot.
    let pid = |events: &[(String, Value)]| -> Option<i32> {
        let output = output(events, &session);
        let (_, rest) = output.rsplit_once("child=")?;
        rest.split_once('.')?.0.parse().ok()
    };
    s.recorded
        .wait_until("the background job", |events| pid(events).is_some());
    let child = pid(&s.recorded.all()).unwrap();
    assert!(alive(child));

    s.host.shells.end_all(s.notes.session.extension_id);

    // The job holds the terminal: the session reports its end only once the job is gone.
    s.recorded.waited_for_exit(&session);
    assert_eq!(s.host.shells.count(s.notes.session.extension_id), 0);
}

#[test]
fn the_list_of_shells_names_existing_programs() {
    let s = setup();
    let shells = call(&s.notes, "extension_shell_list_available", &json!({})).unwrap();
    let shells = shells.as_array().unwrap();
    assert!(!shells.is_empty());
    for shell in shells {
        assert!(
            Path::new(shell["path"].as_str().unwrap()).is_file(),
            "{shell}"
        );
    }
}

#[test]
fn malformed_options_are_refused() {
    let s = setup();
    s.allow(&s.notes, &sh());
    for options in [
        json!({ "shell": 7 }),
        json!({ "shell": format!("/bin/{}sh", "/".repeat(MAX_PROGRAM_BYTES)) }),
        json!({ "cols": 0 }),
        json!({ "rows": 5000 }),
        json!({ "env": { "A=B": "x" } }),
        json!({ "env": { "A": 1 } }),
    ] {
        assert_eq!(
            code(call(
                &s.notes,
                "extension_shell_create",
                &json!({ "options": options.clone() })
            )),
            3001,
            "{options}"
        );
    }
    assert_eq!(
        code(call(
            &s.notes,
            "extension_shell_create",
            &json!({ "options": { "shell": "/bin/sh", "cwd": "/no/such/dir" } })
        )),
        2003
    );
}

#[test]
fn output_decodes_characters_split_across_reads() {
    let text = "grün ✓ 🦀";
    let bytes = text.as_bytes();
    for split in 0..=bytes.len() {
        let mut stream = Utf8Stream::default();
        let mut out = stream.push(&bytes[..split]);
        out.push_str(&stream.push(&bytes[split..]));
        assert_eq!(out, text, "split at {split}");
    }
    let mut stream = Utf8Stream::default();
    assert_eq!(stream.push(&[b'a', 0xff, b'b']), "a\u{fffd}b");
}
