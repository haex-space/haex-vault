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

    #[error("Platform not supported for desktop shortcuts")]
    PlatformNotSupported,

    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
}

impl serde::Serialize for ShortcutError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
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

    // Deep-link URL
    let deep_link_url = format!("haexvault://extension/{extension_id}");

    #[cfg(target_os = "linux")]
    {
        create_linux_shortcut(&app_handle, &app_path, extension_name, extension_icon, &deep_link_url, &extension_id)?;
    }

    #[cfg(target_os = "windows")]
    {
        create_windows_shortcut(&app_path, extension_name, extension_icon, &deep_link_url, &extension_id)?;
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

    // Create .desktop file content
    let desktop_content = format!(
        r#"[Desktop Entry]
Type=Application
Name={extension_name} (HaexVault)
Exec="{app_path}" "{deep_link_url}"
Icon={icon}
Terminal=false
Categories=Utility;
Comment=Launch {extension_name} in HaexVault
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
    let shortcut_path = format!("{desktop_path}\\{extension_name} (HaexVault).lnk");

    // Use PowerShell to create the shortcut
    let ps_script = format!(
        r#"
$WshShell = New-Object -ComObject WScript.Shell
$Shortcut = $WshShell.CreateShortcut("{shortcut_path}")
$Shortcut.TargetPath = "{app_path}"
$Shortcut.Arguments = "{deep_link_url}"
$Shortcut.WorkingDirectory = "{working_dir}"
$Shortcut.Description = "Launch {extension_name} in HaexVault"
$Shortcut.Save()
"#,
        shortcut_path = shortcut_path.replace('\\', "\\\\"),
        app_path = app_path.display().to_string().replace('\\', "\\\\"),
        deep_link_url = deep_link_url,
        working_dir = app_path.parent().map(|p| p.display().to_string()).unwrap_or_default().replace('\\', "\\\\"),
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
        let desktop_file_path = format!("{home}/.local/share/applications/haex-vault-ext-{safe_id}.desktop");

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
