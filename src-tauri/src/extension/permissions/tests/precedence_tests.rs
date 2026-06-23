use crate::extension::permissions::manager::check::deny_first_precedence;
use crate::extension::permissions::types::PermissionStatus;

#[test]
fn empty_input_returns_none() {
    assert_eq!(deny_first_precedence(std::iter::empty()), None);
}

#[test]
fn single_granted_returns_granted() {
    assert_eq!(
        deny_first_precedence([PermissionStatus::Granted]),
        Some(PermissionStatus::Granted)
    );
}

#[test]
fn single_denied_returns_denied() {
    assert_eq!(
        deny_first_precedence([PermissionStatus::Denied]),
        Some(PermissionStatus::Denied)
    );
}

#[test]
fn denied_wins_over_granted_regardless_of_order() {
    assert_eq!(
        deny_first_precedence([
            PermissionStatus::Granted,
            PermissionStatus::Denied,
            PermissionStatus::Granted,
        ]),
        Some(PermissionStatus::Denied)
    );
    assert_eq!(
        deny_first_precedence([PermissionStatus::Denied, PermissionStatus::Granted,]),
        Some(PermissionStatus::Denied)
    );
}

#[test]
fn granted_wins_over_ask() {
    assert_eq!(
        deny_first_precedence([PermissionStatus::Ask, PermissionStatus::Granted]),
        Some(PermissionStatus::Granted)
    );
}

#[test]
fn ask_only_returns_ask() {
    assert_eq!(
        deny_first_precedence([PermissionStatus::Ask, PermissionStatus::Ask]),
        Some(PermissionStatus::Ask)
    );
}
