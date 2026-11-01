use super::*;

#[test]
fn only_a_tap_or_a_button_is_a_click() {
    assert_eq!(response("tap"), NotificationResponse::Body);
    assert_eq!(
        response("button:tap"),
        NotificationResponse::Button("tap".to_owned())
    );
    // Android and iOS report swiping a notification away as `dismiss`.
    assert_eq!(response("dismiss"), NotificationResponse::Closed);
    assert_eq!(response("something"), NotificationResponse::Closed);
}

#[test]
fn an_icon_is_written_once_under_its_content_and_complete() {
    let dir = tempfile::tempdir().unwrap();
    let icons = dir.path().join("notification-icons");
    let first = icon_file(&icons, b"png bytes", "png").unwrap();
    assert_eq!(std::fs::read(&first).unwrap(), b"png bytes");
    assert_eq!(icon_file(&icons, b"png bytes", "png").unwrap(), first);
    // Nothing half written stays behind.
    assert_eq!(std::fs::read_dir(&icons).unwrap().count(), 1);
}
