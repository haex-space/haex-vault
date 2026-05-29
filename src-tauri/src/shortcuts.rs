/// Desktop shortcut creation for extensions
/// Creates native OS shortcuts that launch HaexVault with a deep-link URL
use crate::AppState;
use tauri::{AppHandle, Manager, State};

#[derive(Debug, thiserror::Error)]
pub enum ShortcutError {
    #[error("Failed to create shortcut: {reason}")]
    CreationFailed { reason: String },

    #[error("Extension not found: {extension_id}")]
    ExtensionNotFound { extension_id: String },

    #[error("Invalid extension id: {0}")]
    InvalidExtensionId(String),

    #[allow(dead_code)]
    #[error("Platform not supported for desktop shortcuts")]
    PlatformNotSupported,

    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
}

/// Validate that `extension_id` is safe to interpolate into a deep-link URL
/// and from there into `.desktop`/PowerShell shortcut arguments.
///
/// Today `extension_id` is always a UUID (see `installer.rs`), but the value
/// flows through Tauri command parameters, configuration, and a sync layer
/// before it lands here — any future code path that produces a non-UUID id
/// would otherwise break out of the quoted `Exec="…"` argument with a
/// newline, double-quote, or backtick. Constraining to URL-safe chars at
/// the point of interpolation is defense in depth.
fn validate_extension_id_for_url(extension_id: &str) -> Result<(), ShortcutError> {
    if extension_id.is_empty() || extension_id.len() > 128 {
        return Err(ShortcutError::InvalidExtensionId(extension_id.to_string()));
    }
    let ok = extension_id
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
    if !ok {
        return Err(ShortcutError::InvalidExtensionId(extension_id.to_string()));
    }
    Ok(())
}

impl serde::Serialize for ShortcutError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

/// Reduce an extension manifest's `name` to a label that is safe to embed
/// in a Linux `.desktop` file and a PowerShell shortcut script.
///
/// The manifest name is signed by the extension's developer, not by the
/// vault — a hostile extension can ship a name containing PowerShell
/// metacharacters (`"`, backtick, `$(...)`), `.desktop` line breaks
/// (`\nExec=…`), or path-separator characters that escape the filename.
/// Sanitizing here is defense-in-depth around an inherently untrusted
/// string.
///
/// Strategy: keep ASCII alphanumerics, space, dash, and underscore;
/// replace anything else with `_`; collapse runs of `_`; trim; and bound
/// the length to 64 chars so a maliciously long name cannot break the
/// generated file. Dots are deliberately excluded so a `..` sequence in
/// the manifest name cannot survive into the Windows filename path.
fn sanitize_shortcut_label(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    let mut last_was_underscore = false;
    for ch in name.chars() {
        let keep = ch.is_ascii_alphanumeric() || ch == ' ' || ch == '-' || ch == '_';
        if keep {
            out.push(ch);
            last_was_underscore = ch == '_';
        } else if !last_was_underscore {
            out.push('_');
            last_was_underscore = true;
        }
    }
    let trimmed = out.trim_matches(|c: char| c == ' ' || c == '_').to_string();
    if trimmed.is_empty() {
        return "extension".to_string();
    }
    trimmed.chars().take(64).collect()
}

/// Creates a native desktop shortcut for an extension
/// - Linux: Creates a .desktop file in ~/.local/share/applications/
/// - Windows: Creates a .lnk file on the Desktop
/// - macOS: Not supported (URL schemes are registered at build time)
#[tauri::command]
pub async fn create_desktop_shortcut(
    app_handle: AppHandle,
    state: State<'_, AppState>,
    extension_id: String,
) -> Result<(), ShortcutError> {
    // Validate before any side effects — refuse to write a shortcut file
    // whose Exec/Arguments line cannot be safely quoted.
    validate_extension_id_for_url(&extension_id)?;

    // Get extension info
    let extension = state
        .extension_manager
        .get_extension(&extension_id)
        .ok_or_else(|| ShortcutError::ExtensionNotFound {
            extension_id: extension_id.clone(),
        })?;

    let extension_name = &extension.manifest.name;
    let extension_icon = extension.manifest.icon.as_deref();

    // Get app path for the shortcut target
    let app_path = std::env::current_exe().map_err(|e| ShortcutError::CreationFailed {
        reason: format!("Could not determine app path: {e}"),
    })?;

    // Deep-link URL — extension_id is constrained to URL-safe chars above,
    // so this format is safe to interpolate into quoted shell arguments.
    let deep_link_url = format!("haexvault://extension/{extension_id}");

    #[cfg(target_os = "linux")]
    {
        create_linux_shortcut(
            &app_handle,
            &app_path,
            extension_name,
            extension_icon,
            &deep_link_url,
            &extension_id,
        )?;
    }

    #[cfg(target_os = "windows")]
    {
        create_windows_shortcut(
            &app_path,
            extension_name,
            extension_icon,
            &deep_link_url,
            &extension_id,
        )?;
    }

    #[cfg(target_os = "macos")]
    {
        // macOS doesn't support runtime URL scheme registration
        // The URL scheme must be registered in Info.plist at build time
        return Err(ShortcutError::PlatformNotSupported);
    }

    #[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "macos")))]
    {
        return Err(ShortcutError::PlatformNotSupported);
    }

    Ok(())
}

#[cfg(target_os = "linux")]
fn create_linux_shortcut(
    app_handle: &AppHandle,
    app_path: &std::path::Path,
    extension_name: &str,
    _extension_icon: Option<&str>,
    deep_link_url: &str,
    extension_id: &str,
) -> Result<(), ShortcutError> {
    use std::fs;
    use std::os::unix::fs::PermissionsExt;

    // Get home directory
    let home = std::env::var("HOME").map_err(|_| ShortcutError::CreationFailed {
        reason: "Could not determine HOME directory".to_string(),
    })?;

    // Create applications directory if it doesn't exist
    let applications_dir = format!("{home}/.local/share/applications");
    fs::create_dir_all(&applications_dir)?;

    // Try to get the app icon path
    let icon_path = get_app_icon_path(app_handle);

    // The manifest name is signed by the extension author, not by the
    // vault — sanitize before embedding into a line-based .desktop file
    // so that a name containing "\nExec=…" cannot inject a new entry.
    let safe_name = sanitize_shortcut_label(extension_name);

    // Create .desktop file content
    let desktop_content = format!(
        r#"[Desktop Entry]
Type=Application
Name={safe_name} (HaexVault)
Exec="{app_path}" "{deep_link_url}"
Icon={icon}
Terminal=false
Categories=Utility;
Comment=Launch {safe_name} in HaexVault
StartupWMClass=haex-vault
"#,
        app_path = app_path.display(),
        icon = icon_path.unwrap_or_else(|| "haex-vault".to_string()),
    );

    // Sanitize extension_id for filename
    let safe_id = extension_id.replace(|c: char| !c.is_alphanumeric() && c != '-' && c != '_', "_");
    let desktop_file_path = format!("{applications_dir}/haex-vault-ext-{safe_id}.desktop");

    // Write .desktop file
    fs::write(&desktop_file_path, desktop_content)?;

    // Make it executable
    let mut perms = fs::metadata(&desktop_file_path)?.permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&desktop_file_path, perms)?;

    eprintln!("✅ Created Linux desktop shortcut: {desktop_file_path}");

    Ok(())
}

#[cfg(target_os = "linux")]
fn get_app_icon_path(app_handle: &AppHandle) -> Option<String> {
    // Try to find the app icon in common locations
    let resource_dir = app_handle.path().resource_dir().ok()?;

    // Check for icon in resources
    let icon_paths = [
        resource_dir.join("icons/128x128.png"),
        resource_dir.join("icons/icon.png"),
    ];

    for path in &icon_paths {
        if path.exists() {
            return Some(path.to_string_lossy().to_string());
        }
    }

    // Fallback to app name (system might have it registered)
    Some("haex-vault".to_string())
}

#[cfg(target_os = "windows")]
fn create_windows_shortcut(
    app_path: &std::path::Path,
    extension_name: &str,
    _extension_icon: Option<&str>,
    deep_link_url: &str,
    extension_id: &str,
) -> Result<(), ShortcutError> {
    use std::process::Command;

    // Get Desktop path
    let desktop_path = std::env::var("USERPROFILE")
        .map(|p| format!("{p}\\Desktop"))
        .map_err(|_| ShortcutError::CreationFailed {
            reason: "Could not determine Desktop path".to_string(),
        })?;

    // Sanitize extension_id for filename
    let safe_id = extension_id.replace(|c: char| !c.is_alphanumeric() && c != '-' && c != '_', "_");

    // The manifest name is signed by the extension author, not by the
    // vault — sanitize before embedding into a double-quoted PowerShell
    // string and into the filename. Without this, a hostile name like
    // `Foo"; Invoke-Expression "bad"; "Bar` would execute arbitrary code.
    let safe_name = sanitize_shortcut_label(extension_name);
    let shortcut_path = format!("{desktop_path}\\{safe_name} (HaexVault).lnk");

    // Use PowerShell to create the shortcut
    let ps_script = format!(
        r#"
$WshShell = New-Object -ComObject WScript.Shell
$Shortcut = $WshShell.CreateShortcut("{shortcut_path}")
$Shortcut.TargetPath = "{app_path}"
$Shortcut.Arguments = "{deep_link_url}"
$Shortcut.WorkingDirectory = "{working_dir}"
$Shortcut.Description = "Launch {safe_name} in HaexVault"
$Shortcut.Save()
"#,
        shortcut_path = shortcut_path.replace('\\', "\\\\"),
        app_path = app_path.display().to_string().replace('\\', "\\\\"),
        deep_link_url = deep_link_url,
        working_dir = app_path
            .parent()
            .map(|p| p.display().to_string())
            .unwrap_or_default()
            .replace('\\', "\\\\"),
    );

    let output = Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", &ps_script])
        .output()?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(ShortcutError::CreationFailed {
            reason: format!("PowerShell failed: {stderr}"),
        });
    }

    eprintln!("✅ Created Windows desktop shortcut: {shortcut_path}");

    Ok(())
}

/// Removes a desktop shortcut for an extension
#[tauri::command]
pub async fn remove_desktop_shortcut(extension_id: String) -> Result<(), ShortcutError> {
    // Sanitize extension_id for filename
    let safe_id = extension_id.replace(|c: char| !c.is_alphanumeric() && c != '-' && c != '_', "_");

    #[cfg(target_os = "linux")]
    {
        let home = std::env::var("HOME").map_err(|_| ShortcutError::CreationFailed {
            reason: "Could not determine HOME directory".to_string(),
        })?;
        let desktop_file_path =
            format!("{home}/.local/share/applications/haex-vault-ext-{safe_id}.desktop");

        if std::path::Path::new(&desktop_file_path).exists() {
            std::fs::remove_file(&desktop_file_path)?;
            eprintln!("✅ Removed Linux desktop shortcut: {desktop_file_path}");
        }
    }

    #[cfg(target_os = "windows")]
    {
        // On Windows, we'd need to search for the shortcut by name
        // This is more complex as we don't store the exact filename
        // For now, just log that this should be handled manually
        eprintln!("⚠️ Windows shortcut removal not implemented - user should delete manually");
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    // ------------------------------------------------------------------
    // sanitize_shortcut_label
    // ------------------------------------------------------------------

    #[test]
    fn sanitize_keeps_safe_chars() {
        assert_eq!(sanitize_shortcut_label("My App"), "My App");
        assert_eq!(sanitize_shortcut_label("haex-pass"), "haex-pass");
        assert_eq!(sanitize_shortcut_label("foo_bar"), "foo_bar");
        assert_eq!(sanitize_shortcut_label("Demo123"), "Demo123");
    }

    #[test]
    fn sanitize_strips_powershell_metacharacters() {
        // The Windows shortcut path embeds the label inside double-quoted
        // PowerShell strings; quotes, backticks, $(...) must be neutralised.
        let injected = r#"Foo"; Invoke-Expression "bad"; "Bar"#;
        let cleaned = sanitize_shortcut_label(injected);
        assert!(!cleaned.contains('"'), "double-quote must be stripped");
        assert!(!cleaned.contains(';'), "semicolon must be stripped");
        assert!(!cleaned.contains('`'), "backtick must be stripped");
        assert!(!cleaned.contains('$'), "dollar must be stripped");
        assert!(!cleaned.contains('('), "paren must be stripped");
    }

    #[test]
    fn sanitize_strips_desktop_file_breakers() {
        // Linux .desktop format is line-based; a newline lets a hostile
        // name inject an Exec= line.
        let injected = "Innocent\nExec=rm -rf $HOME";
        let cleaned = sanitize_shortcut_label(injected);
        assert!(!cleaned.contains('\n'), "newline must be stripped");
        assert!(!cleaned.contains('='), "equals sign must be stripped");
    }

    #[test]
    fn sanitize_strips_path_separators() {
        // The Windows path is built as "{desktop}\\{label} (HaexVault).lnk"
        // — embedded slashes/backslashes would escape the desktop dir.
        let injected = "../../etc/passwd";
        let cleaned = sanitize_shortcut_label(injected);
        assert!(!cleaned.contains('/'), "forward slash must be stripped");
        assert!(!cleaned.contains('\\'), "backslash must be stripped");
        assert!(!cleaned.contains(".."), "parent-dir sequence must not survive");
    }

    #[test]
    fn sanitize_handles_pure_garbage() {
        // All-bad input should produce a safe fallback, not empty.
        assert_eq!(sanitize_shortcut_label(r#"""#), "extension");
        assert_eq!(sanitize_shortcut_label(""), "extension");
        assert_eq!(sanitize_shortcut_label(r#"\\\"#), "extension");
    }

    #[test]
    fn sanitize_bounds_length() {
        let huge: String = "a".repeat(10_000);
        let cleaned = sanitize_shortcut_label(&huge);
        assert!(cleaned.len() <= 64, "label must be bounded to prevent oversized files");
    }

    #[test]
    fn sanitize_collapses_consecutive_replacements() {
        // Two adjacent unsafe chars should not produce __ — that is just
        // cosmetic but documents the intended shape.
        let cleaned = sanitize_shortcut_label(r#""""""#);
        assert_eq!(cleaned, "extension");
    }

    // ------------------------------------------------------------------
    // Regression guards: the platform implementations must use the
    // sanitiser. Without this check the hardening is silently bypassed
    // the moment somebody copies a `{extension_name}` interpolation.
    // ------------------------------------------------------------------

    // ------------------------------------------------------------------
    // validate_extension_id_for_url
    // ------------------------------------------------------------------

    #[test]
    fn validate_extension_id_accepts_uuid_and_simple_ids() {
        assert!(validate_extension_id_for_url("550e8400-e29b-41d4-a716-446655440000").is_ok());
        assert!(validate_extension_id_for_url("simple_id-123").is_ok());
    }

    #[test]
    fn validate_extension_id_rejects_empty_or_too_long() {
        assert!(matches!(
            validate_extension_id_for_url(""),
            Err(ShortcutError::InvalidExtensionId(_))
        ));
        let huge = "a".repeat(129);
        assert!(matches!(
            validate_extension_id_for_url(&huge),
            Err(ShortcutError::InvalidExtensionId(_))
        ));
    }

    #[test]
    fn validate_extension_id_rejects_url_breakers() {
        // The interpolated deep_link_url ends up inside quoted Exec="…"
        // arguments — a newline, double-quote, or backtick would close the
        // quoted argument and inject a new `.desktop` key or PowerShell
        // statement. Reject every non-URL-safe char outright.
        for breaker in ["with space", "ext\"id", "ext\nid", "ext`id", "ext$id", "ext/id", "ext\\id", "ext;id", "ext id\nExec=evil"] {
            assert!(
                matches!(
                    validate_extension_id_for_url(breaker),
                    Err(ShortcutError::InvalidExtensionId(_))
                ),
                "expected validate_extension_id_for_url to reject {breaker:?}"
            );
        }
    }

    #[test]
    fn shortcuts_must_sanitize_extension_name() {
        // Only inspect the non-test region of the file: the tests above
        // call sanitize_shortcut_label many times and would mask a missing
        // production call. Cut at the `#[cfg(test)]` marker.
        let source = include_str!("shortcuts.rs");
        let production = source
            .split_once("#[cfg(test)]")
            .map(|(prod, _)| prod)
            .unwrap_or(source);

        // Production must reference the sanitizer at least three times:
        // 1. the definition (`fn sanitize_shortcut_label(`)
        // 2. inside create_linux_shortcut (Desktop entry interpolation)
        // 3. inside create_windows_shortcut (PowerShell + filename)
        let calls = production.matches("sanitize_shortcut_label").count();
        assert!(
            calls >= 3,
            "expected sanitize_shortcut_label to be referenced by the \
             definition plus both create_linux_shortcut and \
             create_windows_shortcut; found {} in production source",
            calls
        );
    }
}
