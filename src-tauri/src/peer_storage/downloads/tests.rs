use super::sanitize_folder_segment;

#[test]
fn sanitize_strips_path_separators() {
    assert_eq!(sanitize_folder_segment("foo/bar", "fallback"), "foo_bar");
    assert_eq!(sanitize_folder_segment("a\\b", "fallback"), "a_b");
}

#[test]
fn sanitize_strips_windows_reserved_chars() {
    assert_eq!(
        sanitize_folder_segment("name:with*reserved?chars\"<>|", "fallback"),
        "name_with_reserved_chars____",
    );
}

#[test]
fn sanitize_strips_control_and_null() {
    assert_eq!(sanitize_folder_segment("a\nb\0c", "fb"), "a_b_c");
}

#[test]
fn sanitize_trims_dots_and_whitespace() {
    assert_eq!(sanitize_folder_segment("  ...space...  ", "fb"), "space");
}

#[test]
fn sanitize_empty_falls_back() {
    assert_eq!(sanitize_folder_segment("", "fallback-id"), "fallback-id");
    assert_eq!(sanitize_folder_segment("   ", "fallback-id"), "fallback-id");
    assert_eq!(sanitize_folder_segment("...", "fallback-id"), "fallback-id");
}

#[test]
fn sanitize_keeps_unicode() {
    assert_eq!(
        sanitize_folder_segment("Mein Räumchen 🌱", "fb"),
        "Mein Räumchen 🌱"
    );
}
