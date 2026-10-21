//! Search, create mailbox and delete messages (spec 019, `contracts/host-mail-commands.md`) through
//! the bridge, against the test server of `test_server.rs` on this device.

use serde_json::{json, Value};

use super::imap_manage::search_query;
use super::test_server::deliver;
use super::test_setup::{setup, Setup};
use super::SearchCriteria;

const SEARCH: &str = "extension_mail_search";
const CREATE: &str = "extension_mail_create_mailbox";
const DELETE: &str = "extension_mail_delete_messages";

fn found(s: &Setup, mailbox: &str, criteria: Value) -> Value {
    s.call(
        SEARCH,
        json!({ "imap": s.imap(), "mailbox": mailbox, "criteria": criteria }),
    )
    .unwrap()
}

fn uids(answer: &Value) -> Vec<u64> {
    answer["uids"]
        .as_array()
        .unwrap()
        .iter()
        .map(|u| u.as_u64().unwrap())
        .collect()
}

#[test]
fn a_search_builds_the_command_itself_and_quotes_what_it_was_given() {
    let criteria: SearchCriteria = serde_json::from_value(json!({
        "from": "ann\"a\\", "subject": "Rechnung", "since": "2026-01-02", "before": "2026-02-03",
        "unseen": true, "flagged": false,
    }))
    .unwrap();
    assert_eq!(
        search_query(&criteria).unwrap(),
        "FROM \"ann\\\"a\\\\\" SUBJECT \"Rechnung\" SINCE 2-Jan-2026 BEFORE 3-Feb-2026 UNSEEN UNFLAGGED"
    );
    let non_ascii: SearchCriteria = serde_json::from_value(json!({ "text": "grün" })).unwrap();
    assert_eq!(
        search_query(&non_ascii).unwrap(),
        "CHARSET UTF-8 TEXT \"grün\"",
        "text that is not ASCII says its charset"
    );
}

#[test]
fn every_criterion_narrows_the_hits_and_all_of_them_must_hold() {
    let s = Setup::granted();
    // INBOX: 7 "first" (read), 9 "second" (unread, with attachment).
    deliver(&s.server, "INBOX", "Rechnung Elektriker");
    let all = found(&s, "INBOX", json!({ "since": "2026-01-01" }));
    assert_eq!(uids(&all), vec![10, 9, 7], "newest first");
    assert_eq!(all["total"], 3);

    assert_eq!(
        uids(&found(&s, "INBOX", json!({ "unseen": true }))),
        [10, 9]
    );
    assert_eq!(uids(&found(&s, "INBOX", json!({ "unseen": false }))), [7]);
    assert_eq!(
        uids(&found(&s, "INBOX", json!({ "subject": "elektriker" }))),
        [10],
        "case does not matter"
    );
    assert_eq!(
        uids(&found(&s, "INBOX", json!({ "from": "anna" }))),
        [10, 9, 7]
    );
    assert_eq!(
        uids(&found(&s, "INBOX", json!({ "to": "carl" }))),
        [] as [u64; 0]
    );
    assert_eq!(
        uids(&found(&s, "INBOX", json!({ "text": "grün" }))),
        [10, 9, 7]
    );
    assert_eq!(
        uids(&found(&s, "INBOX", json!({ "since": "2026-01-02" }))),
        [] as [u64; 0]
    );
    assert_eq!(
        uids(&found(&s, "INBOX", json!({ "before": "2026-01-02" }))),
        [10, 9, 7]
    );
    s.server.lock().unwrap().boxes.get_mut("INBOX").unwrap()[1]
        .flags
        .push("\\Flagged".into());
    assert_eq!(uids(&found(&s, "INBOX", json!({ "flagged": true }))), [9]);
    assert_eq!(
        uids(&found(&s, "INBOX", json!({ "flagged": false }))),
        [10, 7]
    );
    assert_eq!(
        uids(&found(
            &s,
            "INBOX",
            json!({ "unseen": true, "flagged": false, "subject": "rechnung" })
        )),
        [10],
        "all criteria together"
    );
}

#[test]
fn a_hit_behind_the_newest_fifty_is_found_and_the_limit_cuts_the_answer() {
    let s = Setup::granted();
    deliver(&s.server, "INBOX", "Elektriker");
    for n in 0..60 {
        deliver(&s.server, "INBOX", &format!("Newsletter {n}"));
    }
    let hit = found(&s, "INBOX", json!({ "subject": "Elektriker" }));
    assert_eq!(hit["total"], 1);
    assert_eq!(
        uids(&hit),
        [10],
        "the oldest of 63 messages, 60 newer ones behind it"
    );

    let many = s
        .call(
            SEARCH,
            json!({ "imap": s.imap(), "mailbox": "INBOX", "criteria": { "from": "anna" }, "limit": 5 }),
        )
        .unwrap();
    assert_eq!(many["total"], 63, "all hits are counted");
    assert_eq!(uids(&many).len(), 5);
    assert_eq!(uids(&many)[0], 70, "the newest first");
}

#[test]
fn a_search_that_could_smuggle_a_command_or_names_nothing_is_refused() {
    let s = Setup::granted();
    let search = |criteria: Value| {
        s.code(
            SEARCH,
            json!({ "imap": s.imap(), "mailbox": "INBOX", "criteria": criteria }),
        )
    };
    assert_eq!(search(json!({})), 3001, "no criterion");
    assert_eq!(search(json!({ "subject": "x\r\nA9 DELETE INBOX" })), 3001);
    assert_eq!(search(json!({ "text": "   " })), 3001);
    assert_eq!(search(json!({ "text": "a".repeat(201) })), 3001);
    assert_eq!(search(json!({ "since": "2026-02-30" })), 3001);
    assert_eq!(search(json!({ "since": "yesterday" })), 3001);
    assert_eq!(search(json!({ "raw": "ALL" })), 3001, "no IMAP expression");
    assert_eq!(
        s.code(
            SEARCH,
            json!({ "imap": s.imap(), "mailbox": "INBOX", "criteria": { "text": "x" }, "limit": 1001 })
        ),
        3001
    );
    assert!(
        !s.server
            .lock()
            .unwrap()
            .commands
            .iter()
            .any(|c| c.contains("DELETE")),
        "nothing smuggled into the server"
    );
}

#[test]
fn a_mailbox_is_created_once_and_a_second_try_says_it_was_there() {
    let s = Setup::granted();
    let create = |name: &str| s.call(CREATE, json!({ "imap": s.imap(), "mailbox": name }));
    assert_eq!(create("Steuer").unwrap(), json!({ "created": true }));
    assert!(s.server.lock().unwrap().boxes.contains_key("Steuer"));
    assert_eq!(
        create("Steuer").unwrap(),
        json!({ "created": false }),
        "no error, no second mailbox"
    );
    assert_eq!(create("Archive").unwrap(), json!({ "created": false }));
    for bad in ["", "a\r\nb", "Steu*er", "50%"] {
        assert_eq!(
            s.code(CREATE, json!({ "imap": s.imap(), "mailbox": bad })),
            3001,
            "{bad:?}"
        );
    }
}

#[test]
fn only_the_named_messages_go_and_other_deleted_ones_stay() {
    let s = Setup::granted();
    s.server.lock().unwrap().boxes.get_mut("INBOX").unwrap()[0]
        .flags
        .push("\\Deleted".into());
    s.call(
        DELETE,
        json!({ "imap": s.imap(), "mailbox": "INBOX", "uids": [9] }),
    )
    .unwrap();
    assert_eq!(s.uids("INBOX"), [7], "uid 9 is gone");
    let state = s.server.lock().unwrap();
    assert!(
        state.boxes["INBOX"][0]
            .flags
            .contains(&"\\Deleted".to_owned()),
        "uid 7 was only marked by someone else and is not expunged with it"
    );
    drop(state);
    s.call(
        DELETE,
        json!({ "imap": s.imap(), "mailbox": "INBOX", "uids": [] }),
    )
    .unwrap();
    assert_eq!(s.uids("INBOX"), [7], "nothing named, nothing removed");
    assert_eq!(
        s.code(
            DELETE,
            json!({ "imap": s.imap(), "mailbox": "INBOX", "uids": (0..1001).collect::<Vec<u32>>() })
        ),
        7000
    );
}

#[test]
fn a_server_that_cannot_expunge_by_uid_loses_nothing_and_marks_nothing() {
    let s = Setup::granted();
    s.server.lock().unwrap().no_uidplus = true;
    assert_eq!(
        s.code(
            DELETE,
            json!({ "imap": s.imap(), "mailbox": "INBOX", "uids": [9] })
        ),
        2005
    );
    let state = s.server.lock().unwrap();
    assert!(
        state.boxes["INBOX"]
            .iter()
            .all(|m| m.flags != ["\\Deleted"]),
        "nothing was marked"
    );
    assert_eq!(state.boxes["INBOX"].len(), 2);
    assert!(
        !state.commands.iter().any(|c| c.contains("STORE")),
        "the server was asked what it can do before anything changed"
    );
}

#[test]
fn each_of_the_three_needs_fetch_for_its_own_server_and_port() {
    let s = setup();
    let calls = [
        (
            SEARCH,
            json!({ "imap": s.imap(), "mailbox": "INBOX", "criteria": { "text": "x" } }),
        ),
        (CREATE, json!({ "imap": s.imap(), "mailbox": "Neu" })),
        (
            DELETE,
            json!({ "imap": s.imap(), "mailbox": "INBOX", "uids": [9] }),
        ),
    ];
    for (method, params) in &calls {
        let asked = s.call(method, params.clone()).unwrap_err();
        assert_eq!(asked.code.as_u16(), 1004, "{method}");
        assert_eq!(
            asked.details,
            Some(
                json!({ "resourceType": "mail", "action": "fetch", "target": format!("127.0.0.1:{}", s.imap_port) })
            )
        );
    }
    s.grant("fetch", &format!("127.0.0.1:{}", s.imap_port + 1));
    for (method, params) in &calls {
        assert_eq!(
            s.code(method, params.clone()),
            1004,
            "{method}: another port"
        );
    }
    s.grant("send", "*");
    for (method, params) in &calls {
        assert_eq!(
            s.code(method, params.clone()),
            1004,
            "{method}: send is not fetch"
        );
    }
    assert_eq!(
        s.uids("INBOX"),
        [7, 9],
        "nothing changed without permission"
    );
    assert!(!s.server.lock().unwrap().boxes.contains_key("Neu"));
}
