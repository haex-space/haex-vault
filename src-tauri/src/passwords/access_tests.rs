//! Tests for the access rules of the password manager (spec 034, `contracts/access.md` Z1–Z9 and
//! Z11–Z13). The module is pure, so the cases need no database; the same table is run for every
//! caller that is not `User` (FR-032).

use super::access::{
    authorize_create, authorize_delete, authorize_list, authorize_read, authorize_unassigned,
    authorize_update, require_user, sees_owned, Caller, Denied, Grant, GrantAction, ItemState,
    ListView, Scope,
};

fn names(list: &[&str]) -> Vec<String> {
    list.iter().map(|name| name.to_string()).collect()
}

fn read(tags: &[&str]) -> Grant {
    Grant::new(GrantAction::Read, Scope::tags(tags.iter().copied()))
}

fn write(tags: &[&str]) -> Grant {
    Grant::new(GrantAction::ReadWrite, Scope::tags(tags.iter().copied()))
}

fn item<'a>(tags: &'a [String]) -> ItemState<'a> {
    ItemState {
        tags,
        in_trash: false,
        owner: None,
    }
}

fn trashed<'a>(tags: &'a [String]) -> ItemState<'a> {
    ItemState {
        tags,
        in_trash: true,
        owner: None,
    }
}

fn owned<'a>(tags: &'a [String], owner: &'a str) -> ItemState<'a> {
    ItemState {
        tags,
        in_trash: false,
        owner: Some(owner),
    }
}

/// The callers that are not the user and not the built-in agent: they follow the same grant rules.
fn outside_callers() -> Vec<Caller> {
    vec![
        Caller::Extension {
            id: "ext-1".to_string(),
        },
        Caller::ExternalAgent {
            id: "agent-1".to_string(),
        },
        Caller::Internal { feature: "test" },
        // Spec 049: a browser extension through the External Bridge.
        Caller::BrowserClient {
            id: "3f1c0000000000000000000000000000".to_string(),
        },
    ]
}

// Z1: the user needs no grant.
#[test]
fn the_user_may_do_everything_without_a_grant() {
    let user = Caller::User;
    let tags = names(&["a"]);
    assert_eq!(authorize_list(&user, &[]), Ok(ListView::Items(Scope::All)));
    assert_eq!(authorize_read(&user, &[], &item(&tags)), Ok(()));
    assert_eq!(authorize_create(&user, &[], &names(&[])), Ok(()));
    assert_eq!(
        authorize_update(&user, &[], &item(&tags), Some(&names(&["x", "y"]))),
        Ok(names(&["x", "y"]))
    );
    assert_eq!(authorize_delete(&user, &[], &item(&tags)), Ok(()));
    assert_eq!(authorize_read(&user, &[], &trashed(&tags)), Ok(()));
    assert_eq!(
        authorize_unassigned(&user, &[], GrantAction::ReadWrite),
        Ok(())
    );
    assert_eq!(require_user(&user), Ok(()));
}

// Z2: the built-in agent gets the agent headers and nothing else, whatever the grants say.
#[test]
fn the_built_in_agent_gets_only_the_agent_headers() {
    let agent = Caller::BuiltinAgent;
    let everything = [write(&["*"])];
    let tags = names(&["s3"]);
    assert_eq!(authorize_list(&agent, &[]), Ok(ListView::Agent));
    assert_eq!(authorize_list(&agent, &everything), Ok(ListView::Agent));
    assert_eq!(
        authorize_read(&agent, &everything, &item(&tags)),
        Err(Denied::Forbidden)
    );
    assert_eq!(
        authorize_create(&agent, &everything, &tags),
        Err(Denied::Forbidden)
    );
    assert_eq!(
        authorize_update(&agent, &everything, &item(&tags), None),
        Err(Denied::Forbidden)
    );
    assert_eq!(
        authorize_delete(&agent, &everything, &item(&tags)),
        Err(Denied::Forbidden)
    );
    assert_eq!(
        authorize_unassigned(&agent, &everything, GrantAction::Read),
        Err(Denied::Forbidden)
    );
    assert_eq!(require_user(&agent), Err(Denied::Forbidden));
}

// Z3: no matching grant of the asked kind is `Forbidden`.
#[test]
fn without_a_grant_of_the_asked_kind_everything_is_forbidden() {
    for caller in outside_callers() {
        let tags = names(&["s3"]);
        assert_eq!(authorize_list(&caller, &[]), Err(Denied::Forbidden));
        assert_eq!(
            authorize_read(&caller, &[], &item(&tags)),
            Err(Denied::Forbidden)
        );
        // A read grant does not write.
        let reading = [read(&["s3"])];
        assert_eq!(
            authorize_create(&caller, &reading, &tags),
            Err(Denied::Forbidden)
        );
        assert_eq!(
            authorize_update(&caller, &reading, &item(&tags), None),
            Err(Denied::Forbidden)
        );
        assert_eq!(
            authorize_delete(&caller, &reading, &item(&tags)),
            Err(Denied::Forbidden)
        );
    }
}

// Z4, Z5: a read grant for one tag shows and reads only the items that carry it.
#[test]
fn a_read_grant_for_a_tag_covers_the_items_with_that_tag() {
    for caller in outside_callers() {
        let grants = [read(&["s3"])];
        assert_eq!(
            authorize_list(&caller, &grants),
            Ok(ListView::Items(Scope::tags(["s3"])))
        );
        let with = names(&["s3", "bank"]);
        let without = names(&["bank"]);
        assert_eq!(authorize_read(&caller, &grants, &item(&with)), Ok(()));
        assert_eq!(
            authorize_read(&caller, &grants, &item(&without)),
            Err(Denied::NotFound),
            "an item outside the scope is indistinguishable from a missing one"
        );
        assert_eq!(
            authorize_read(&caller, &grants, &item(&names(&[]))),
            Err(Denied::NotFound)
        );
    }
}

#[test]
fn tag_comparison_goes_through_fold() {
    let caller = Caller::Extension {
        id: "ext".to_string(),
    };
    let grants = [read(&["S3"])];
    assert_eq!(
        authorize_read(&caller, &grants, &item(&names(&["s3 "]))),
        Ok(())
    );
    // A decomposed and a precomposed umlaut are one tag.
    let grants = [read(&["m\u{fc}ller"])];
    assert_eq!(
        authorize_read(&caller, &grants, &item(&names(&["mu\u{308}ller"]))),
        Ok(())
    );
}

#[test]
fn read_write_covers_read_and_grants_union() {
    let caller = Caller::ExternalAgent {
        id: "a".to_string(),
    };
    let s3 = names(&["s3"]);
    let bank = names(&["bank"]);
    // ReadWrite covers Read.
    let grants = [write(&["s3"])];
    assert_eq!(authorize_read(&caller, &grants, &item(&s3)), Ok(()));
    // Two grants: the scope of each kind is the union of the grants that cover it.
    let grants = [read(&["s3"]), write(&["bank"])];
    assert_eq!(authorize_read(&caller, &grants, &item(&s3)), Ok(()));
    assert_eq!(authorize_read(&caller, &grants, &item(&bank)), Ok(()));
    assert_eq!(
        authorize_list(&caller, &grants),
        Ok(ListView::Items(Scope::tags(["s3", "bank"])))
    );
    assert_eq!(authorize_delete(&caller, &grants, &item(&bank)), Ok(()));
    assert_eq!(
        authorize_delete(&caller, &grants, &item(&s3)),
        Err(Denied::NotFound),
        "s3 is readable but not writable"
    );
}

#[test]
fn a_star_grant_covers_everything() {
    let caller = Caller::Internal { feature: "t" };
    let grants = [write(&["*"])];
    assert_eq!(
        authorize_list(&caller, &grants),
        Ok(ListView::Items(Scope::All))
    );
    assert_eq!(authorize_read(&caller, &grants, &item(&names(&[]))), Ok(()));
    assert_eq!(authorize_create(&caller, &grants, &names(&[])), Ok(()));
    let all = [Grant::new(GrantAction::Read, Scope::All), write(&["s3"])];
    assert_eq!(
        authorize_list(&caller, &all),
        Ok(ListView::Items(Scope::All)),
        "one grant for all makes the whole scope all"
    );
}

// Spec 017, FR-017: a denied tag hides its entries from a grant for all, unless the entry also
// carries a granted tag.
#[test]
fn a_scope_for_all_but_denied_tags_yields_to_a_granted_tag() {
    let caller = Caller::Extension {
        id: "ext-1".to_string(),
    };
    let grants = [
        Grant::new(GrantAction::ReadWrite, Scope::all_except(["Private"])),
        read(&["bank"]),
    ];
    for (tags, visible) in [
        (&[][..], true),
        (&["other"][..], true),
        (&["private"][..], false),
        (&["private", "other"][..], false),
        (&["private", "bank"][..], true),
    ] {
        let tags = names(tags);
        assert_eq!(
            authorize_read(&caller, &grants, &item(&tags)).is_ok(),
            visible,
            "{tags:?}"
        );
    }
    assert_eq!(authorize_create(&caller, &grants, &names(&[])), Ok(()));
    assert_eq!(
        authorize_create(&caller, &grants, &names(&["other", "PRIVATE"])),
        Err(Denied::Forbidden)
    );
    assert_eq!(
        authorize_update(
            &caller,
            &grants,
            &item(&names(&["other"])),
            Some(&names(&["private"]))
        ),
        Err(Denied::Forbidden),
        "a denied tag cannot be added"
    );
    assert_eq!(
        authorize_update(
            &caller,
            &grants,
            &item(&names(&["other"])),
            Some(&names(&[]))
        ),
        Ok(names(&[])),
        "an entry without tags stays in a scope for all"
    );
    assert_eq!(
        authorize_unassigned(&caller, &grants, GrantAction::Read),
        Ok(())
    );
    assert_eq!(Scope::all_except(Vec::<String>::new()), Scope::All);
}

// Z6: create with a tag scope.
#[test]
fn create_needs_tags_of_the_scope_and_only_those() {
    for caller in outside_callers() {
        let grants = [write(&["s3"])];
        assert_eq!(authorize_create(&caller, &grants, &names(&["s3"])), Ok(()));
        assert_eq!(authorize_create(&caller, &grants, &names(&["S3"])), Ok(()));
        assert_eq!(
            authorize_create(&caller, &grants, &names(&["s3", "other"])),
            Err(Denied::Forbidden)
        );
        assert_eq!(
            authorize_create(&caller, &grants, &names(&["other"])),
            Err(Denied::Forbidden)
        );
        assert_eq!(
            authorize_create(&caller, &grants, &names(&[])),
            Err(Denied::Forbidden)
        );
        let all = [write(&["*"])];
        assert_eq!(authorize_create(&caller, &all, &names(&[])), Ok(()));
    }
}

// Z7, Z12: update keeps tags outside the scope and never adds one.
#[test]
fn update_keeps_the_tags_outside_the_scope() {
    for caller in outside_callers() {
        let grants = [write(&["s3"])];
        let tags = names(&["s3", "bank"]);
        // No tag list sent: nothing changes.
        assert_eq!(
            authorize_update(&caller, &grants, &item(&tags), None),
            Ok(tags.clone())
        );
        // Only the in-scope tag sent: the out-of-scope tag stays.
        assert_eq!(
            authorize_update(&caller, &grants, &item(&tags), Some(&names(&["s3"]))),
            Ok(names(&["bank", "s3"]))
        );
        // Sending the existing out-of-scope tag again is not adding it.
        assert_eq!(
            authorize_update(
                &caller,
                &grants,
                &item(&tags),
                Some(&names(&["s3", "bank"]))
            ),
            Ok(names(&["bank", "s3"]))
        );
        // A new tag outside the scope would lift the item into another caller's scope.
        assert_eq!(
            authorize_update(
                &caller,
                &grants,
                &item(&tags),
                Some(&names(&["s3", "other"]))
            ),
            Err(Denied::Forbidden)
        );
        // Removing the last in-scope tag would write the item out of the scope.
        assert_eq!(
            authorize_update(&caller, &grants, &item(&tags), Some(&names(&["bank"]))),
            Err(Denied::Forbidden)
        );
        assert_eq!(
            authorize_update(&caller, &grants, &item(&tags), Some(&names(&[]))),
            Err(Denied::Forbidden)
        );
        // An item outside the scope is not there.
        assert_eq!(
            authorize_update(&caller, &grants, &item(&names(&["bank"])), None),
            Err(Denied::NotFound)
        );
    }
}

#[test]
fn update_with_a_scope_for_all_replaces_the_tags() {
    let caller = Caller::Extension {
        id: "e".to_string(),
    };
    let grants = [write(&["*"])];
    let tags = names(&["a", "b"]);
    assert_eq!(
        authorize_update(&caller, &grants, &item(&tags), Some(&names(&["c"]))),
        Ok(names(&["c"]))
    );
    assert_eq!(
        authorize_update(&caller, &grants, &item(&tags), Some(&names(&[]))),
        Ok(names(&[]))
    );
}

// Z8: delete needs a write grant and an item in the scope.
#[test]
fn delete_needs_a_write_grant_and_an_item_in_the_scope() {
    for caller in outside_callers() {
        let tags = names(&["s3"]);
        assert_eq!(
            authorize_delete(&caller, &[read(&["s3"])], &item(&tags)),
            Err(Denied::Forbidden)
        );
        assert_eq!(
            authorize_delete(&caller, &[write(&["s3"])], &item(&tags)),
            Ok(())
        );
        assert_eq!(
            authorize_delete(&caller, &[write(&["bank"])], &item(&tags)),
            Err(Denied::NotFound)
        );
    }
}

// Z13: an entry in the trash does not exist for a caller from outside; Z3 comes first.
#[test]
fn an_entry_in_the_trash_does_not_exist_for_callers_from_outside() {
    for caller in outside_callers() {
        let tags = names(&["s3"]);
        for grants in [
            vec![read(&["s3"])],
            vec![write(&["s3"])],
            vec![write(&["*"])],
        ] {
            assert_eq!(
                authorize_read(&caller, &grants, &trashed(&tags)),
                Err(Denied::NotFound)
            );
            let can_write = grants[0].action == GrantAction::ReadWrite;
            let denied = if can_write {
                Denied::NotFound
            } else {
                Denied::Forbidden
            };
            assert_eq!(
                authorize_update(&caller, &grants, &trashed(&tags), None),
                Err(denied)
            );
            assert_eq!(
                authorize_delete(&caller, &grants, &trashed(&tags)),
                Err(denied)
            );
        }
        // Without a grant of the asked kind the answer is `Forbidden`, trash or not.
        assert_eq!(
            authorize_read(&caller, &[], &trashed(&tags)),
            Err(Denied::Forbidden)
        );
        assert_eq!(
            authorize_delete(&caller, &[], &trashed(&tags)),
            Err(Denied::Forbidden)
        );
    }
}

// Z9: a passkey without an entry belongs to no tag scope.
#[test]
fn an_item_less_record_is_covered_only_by_a_grant_for_all() {
    for caller in outside_callers() {
        assert_eq!(
            authorize_unassigned(&caller, &[], GrantAction::Read),
            Err(Denied::Forbidden)
        );
        assert_eq!(
            authorize_unassigned(&caller, &[read(&["s3"])], GrantAction::Read),
            Err(Denied::NotFound)
        );
        assert_eq!(
            authorize_unassigned(&caller, &[read(&["*"])], GrantAction::Read),
            Ok(())
        );
        assert_eq!(
            authorize_unassigned(&caller, &[read(&["*"])], GrantAction::ReadWrite),
            Err(Denied::Forbidden),
            "a read grant does not write"
        );
    }
}

// Z11: everything beyond the five item methods is for the user alone.
#[test]
fn everything_beyond_the_item_methods_is_for_the_user_only() {
    for caller in outside_callers() {
        assert_eq!(require_user(&caller), Err(Denied::Forbidden));
    }
    assert_eq!(require_user(&Caller::BuiltinAgent), Err(Denied::Forbidden));
    assert_eq!(require_user(&Caller::User), Ok(()));
}

// Z14 (spec 038): an entry that belongs to a holzi function exists only for the user and that
// function, whatever the grants say.
#[test]
fn an_owned_entry_exists_only_for_the_user_and_its_owner() {
    let tags = names(&["s3"]);
    let entry = owned(&tags, "storage");
    let all = [Grant::new(GrantAction::ReadWrite, Scope::All)];
    let mut strangers = outside_callers();
    strangers.retain(|c| *c != Caller::Internal { feature: "test" });
    strangers.push(Caller::Internal { feature: "other" });
    for caller in &strangers {
        assert_eq!(
            authorize_read(caller, &all, &entry),
            Err(Denied::NotFound),
            "{caller:?}"
        );
        assert_eq!(
            authorize_update(caller, &all, &entry, None),
            Err(Denied::NotFound),
            "{caller:?}"
        );
        assert_eq!(
            authorize_delete(caller, &all, &entry),
            Err(Denied::NotFound),
            "{caller:?}"
        );
        assert_eq!(
            authorize_read(caller, &[], &entry),
            Err(Denied::Forbidden),
            "without a grant Z3 answers first: {caller:?}"
        );
        assert!(!sees_owned(caller, Some("storage")), "{caller:?}");
        assert!(sees_owned(caller, None), "{caller:?}");
    }
    let owner = Caller::Internal { feature: "storage" };
    assert_eq!(authorize_read(&owner, &all, &entry), Ok(()));
    assert_eq!(authorize_delete(&owner, &all, &entry), Ok(()));
    assert_eq!(authorize_read(&Caller::User, &[], &entry), Ok(()));
    assert!(sees_owned(&Caller::User, Some("storage")));
    assert!(sees_owned(&owner, Some("storage")));
    assert!(!sees_owned(&Caller::BuiltinAgent, Some("storage")));
}
