use super::*;

fn groups(ids: &[&str]) -> Vec<String> {
    ids.iter().map(|id| id.to_string()).collect()
}

fn in_groups(ids: &[&str]) -> Scope {
    Scope::Groups(ids.iter().map(|id| id.to_string()).collect())
}

fn outside_callers() -> Vec<Caller> {
    vec![
        Caller::BuiltinAgent,
        Caller::Extension { id: "ext".into() },
        Caller::ExternalAgent { id: "mcp".into() },
    ]
}

#[test]
fn the_user_and_holzi_functions_need_no_grant() {
    for caller in [Caller::User, Caller::Internal { feature: "phone" }] {
        assert_eq!(authorize_list(&caller, &[]), Ok(Scope::All));
        assert_eq!(authorize_read(&caller, &[], &[]), Ok(()));
        assert_eq!(authorize_write(&caller, &[], &groups(&["g1"])), Ok(()));
        assert_eq!(authorize_create(&caller, &[], &[]), Ok(()));
    }
}

#[test]
fn without_a_grant_every_other_caller_is_forbidden_even_the_built_in_agent() {
    for caller in outside_callers() {
        assert_eq!(authorize_list(&caller, &[]), Err(Denied::Forbidden));
        assert_eq!(authorize_read(&caller, &[], &[]), Err(Denied::Forbidden));
        assert_eq!(authorize_write(&caller, &[], &[]), Err(Denied::Forbidden));
        assert_eq!(authorize_create(&caller, &[], &[]), Err(Denied::Forbidden));
    }
}

#[test]
fn a_read_grant_reads_but_never_writes() {
    let read = [Grant::new(GrantAction::Read, Scope::All)];
    for caller in outside_callers() {
        assert_eq!(authorize_read(&caller, &read, &[]), Ok(()));
        assert_eq!(authorize_write(&caller, &read, &[]), Err(Denied::Forbidden));
        assert_eq!(
            authorize_create(&caller, &read, &[]),
            Err(Denied::Forbidden)
        );
    }
}

#[test]
fn a_write_grant_covers_reading() {
    let write = [Grant::new(GrantAction::ReadWrite, Scope::All)];
    let caller = Caller::ExternalAgent { id: "mcp".into() };
    assert_eq!(authorize_read(&caller, &write, &[]), Ok(()));
    assert_eq!(authorize_write(&caller, &write, &[]), Ok(()));
}

#[test]
fn a_group_grant_covers_only_its_members_and_hides_the_rest() {
    let work = [Grant::new(GrantAction::Read, in_groups(&["work"]))];
    let caller = Caller::Extension { id: "mail".into() };
    assert_eq!(authorize_list(&caller, &work), Ok(in_groups(&["work"])));
    assert_eq!(
        authorize_read(&caller, &work, &groups(&["work", "x"])),
        Ok(())
    );
    // Outside the scope looks exactly like missing (FR-031).
    assert_eq!(
        authorize_read(&caller, &work, &groups(&["family"])),
        Err(Denied::NotFound)
    );
    assert_eq!(authorize_read(&caller, &work, &[]), Err(Denied::NotFound));
}

#[test]
fn grants_add_up() {
    let grants = [
        Grant::new(GrantAction::Read, in_groups(&["work"])),
        Grant::new(GrantAction::Read, in_groups(&["family"])),
    ];
    let caller = Caller::BuiltinAgent;
    assert_eq!(
        authorize_list(&caller, &grants),
        Ok(in_groups(&["work", "family"]))
    );
    let all = [
        Grant::new(GrantAction::Read, in_groups(&["work"])),
        Grant::new(GrantAction::Read, Scope::All),
    ];
    assert_eq!(authorize_list(&caller, &all), Ok(Scope::All));
}

#[test]
fn creating_with_a_group_scope_needs_a_group_of_the_scope() {
    let grants = [Grant::new(GrantAction::ReadWrite, in_groups(&["work"]))];
    let caller = Caller::ExternalAgent { id: "mcp".into() };
    assert_eq!(
        authorize_create(&caller, &grants, &groups(&["work"])),
        Ok(())
    );
    assert_eq!(
        authorize_create(&caller, &grants, &[]),
        Err(Denied::Forbidden)
    );
}

#[test]
fn only_the_user_passes_require_user() {
    assert_eq!(require_user(&Caller::User), Ok(()));
    assert_eq!(
        require_user(&Caller::Internal { feature: "phone" }),
        Err(Denied::Forbidden)
    );
    assert_eq!(require_user(&Caller::BuiltinAgent), Err(Denied::Forbidden));
}

#[test]
fn denials_map_to_the_contacts_errors() {
    assert!(matches!(
        HolziError::from(Denied::NotFound),
        HolziError::ContactsNotFound
    ));
    assert!(matches!(
        HolziError::from(Denied::Forbidden),
        HolziError::ContactsForbidden
    ));
}
