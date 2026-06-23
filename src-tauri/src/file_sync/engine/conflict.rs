/// Build a conflict file path: `name.conflict.{timestamp}.ext`
pub(super) fn make_conflict_path(relative_path: &str, timestamp: i64) -> String {
    let path = std::path::Path::new(relative_path);
    let stem = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or(relative_path);
    let extension = path.extension().and_then(|e| e.to_str());
    let parent = path.parent().and_then(|p| p.to_str()).unwrap_or("");

    let conflict_name = match extension {
        Some(ext) => format!("{stem}.conflict.{timestamp}.{ext}"),
        None => format!("{stem}.conflict.{timestamp}"),
    };

    if parent.is_empty() {
        conflict_name
    } else {
        format!("{parent}/{conflict_name}")
    }
}
