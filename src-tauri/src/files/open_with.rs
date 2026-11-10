//! "Öffnen mit": the apps of this device that can open a file, so the person picks one instead of
//! the system's default. The best matches for the file's type come first (`recommended`); every
//! other app that takes files follows. An app is named by an `id` only this module hands out and
//! resolves again, so the window never passes a command line.

use serde::Serialize;
use ts_rs::TS;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub struct OpenApp {
    pub id: String,
    pub name: String,
    pub recommended: bool,
}

/// Splits a command line into its words: spaces separate, double quotes group, a backslash
/// escapes inside quotes.
#[cfg(any(target_os = "linux", target_os = "windows", test))]
fn split_command(line: &str, windows: bool) -> Vec<String> {
    let mut words = Vec::new();
    let mut word = String::new();
    let mut started = false;
    let mut quoted = false;
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '"' => {
                quoted = !quoted;
                started = true;
            }
            '\\' if quoted && !windows => {
                if let Some(next) = chars.next() {
                    word.push(next);
                    started = true;
                }
            }
            c if c.is_whitespace() && !quoted => {
                if started {
                    words.push(std::mem::take(&mut word));
                    started = false;
                }
            }
            c => {
                word.push(c);
                started = true;
            }
        }
    }
    if started {
        words.push(word);
    }
    words
}

/// Replaces `%NAME%` with the value of the environment variable `lookup` knows; unknown names
/// and placeholders such as `%1` stay as they are.
#[cfg(any(target_os = "windows", test))]
fn expand_env(word: &str, lookup: impl Fn(&str) -> Option<String>) -> String {
    let mut out = String::new();
    let mut rest = word;
    while let Some(start) = rest.find('%') {
        out.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        match after.find('%') {
            Some(end) => match lookup(&after[..end]).filter(|_| end > 0) {
                Some(value) => {
                    out.push_str(&value);
                    rest = &after[end + 1..];
                }
                None => {
                    out.push('%');
                    rest = after;
                }
            },
            None => {
                out.push('%');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

#[cfg(target_os = "linux")]
mod platform {
    use super::{split_command, OpenApp};
    use std::collections::{HashMap, HashSet};
    use std::path::{Path, PathBuf};

    struct Entry {
        id: String,
        name: String,
        exec: String,
        mimes: Vec<String>,
        icon: Option<String>,
    }

    fn data_dirs() -> Vec<PathBuf> {
        let mut dirs = Vec::new();
        match std::env::var_os("XDG_DATA_HOME").filter(|v| !v.is_empty()) {
            Some(home) => dirs.push(PathBuf::from(home)),
            None => {
                if let Some(home) = std::env::var_os("HOME") {
                    dirs.push(PathBuf::from(home).join(".local/share"));
                }
            }
        }
        let system = std::env::var("XDG_DATA_DIRS")
            .ok()
            .filter(|v| !v.is_empty())
            .unwrap_or_else(|| "/usr/local/share:/usr/share".to_string());
        dirs.extend(
            system
                .split(':')
                .filter(|d| !d.is_empty())
                .map(PathBuf::from),
        );
        if let Some(home) = std::env::var_os("HOME") {
            dirs.push(PathBuf::from(home).join(".local/share/flatpak/exports/share"));
        }
        dirs.push(PathBuf::from("/var/lib/flatpak/exports/share"));
        dirs
    }

    /// The mime type of a file name from the shared mime database's glob list.
    fn mime_of(name: &str) -> String {
        let lower = name.to_lowercase();
        let mut best: Option<(u32, usize, String)> = None;
        for dir in data_dirs() {
            let Ok(text) = std::fs::read_to_string(dir.join("mime/globs2")) else {
                continue;
            };
            for line in text.lines() {
                let mut parts = line.splitn(3, ':');
                let (Some(weight), Some(mime), Some(glob)) =
                    (parts.next(), parts.next(), parts.next())
                else {
                    continue;
                };
                let Some(suffix) = glob.strip_prefix('*') else {
                    continue;
                };
                if suffix.contains(['*', '?', '[']) || !lower.ends_with(&suffix.to_lowercase()) {
                    continue;
                }
                let weight = weight.parse().unwrap_or(50);
                let rank = (weight, suffix.len());
                if best.as_ref().is_none_or(|b| rank > (b.0, b.1)) {
                    best = Some((rank.0, rank.1, mime.to_string()));
                }
            }
        }
        best.map_or_else(|| "application/octet-stream".to_string(), |b| b.2)
    }

    fn locale_keys() -> Vec<String> {
        let raw = std::env::var("LC_MESSAGES")
            .or_else(|_| std::env::var("LANG"))
            .unwrap_or_default();
        let base = raw.split('.').next().unwrap_or("").to_string();
        let mut keys = Vec::new();
        if !base.is_empty() {
            keys.push(base.clone());
            if let Some((lang, _)) = base.split_once('_') {
                keys.push(lang.to_string());
            }
        }
        keys
    }

    fn parse(id: &str, text: &str, locales: &[String]) -> Option<Entry> {
        let mut in_entry = false;
        let (mut name, mut exec, mut icon, mut mimes) = (None, None, None, Vec::new());
        let mut localized: HashMap<String, String> = HashMap::new();
        for line in text.lines() {
            let line = line.trim();
            if line.starts_with('[') {
                in_entry = line == "[Desktop Entry]";
                continue;
            }
            if !in_entry {
                continue;
            }
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            match key.trim() {
                "Type" if value != "Application" => return None,
                "NoDisplay" | "Hidden" | "Terminal" if value == "true" => return None,
                "Name" => name = Some(value.to_string()),
                "Exec" => exec = Some(value.to_string()),
                "Icon" => icon = Some(value.to_string()),
                "MimeType" => {
                    mimes = value
                        .split(';')
                        .filter(|m| !m.is_empty())
                        .map(str::to_string)
                        .collect()
                }
                key => {
                    if let Some(locale) = key
                        .strip_prefix("Name[")
                        .and_then(|rest| rest.strip_suffix(']'))
                    {
                        localized.insert(locale.to_string(), value.to_string());
                    }
                }
            }
        }
        let name = locales
            .iter()
            .find_map(|l| localized.get(l).cloned())
            .or(name)?;
        let exec = exec?;
        if !exec.contains("%f")
            && !exec.contains("%F")
            && !exec.contains("%u")
            && !exec.contains("%U")
        {
            return None;
        }
        Some(Entry {
            id: id.to_string(),
            name,
            exec,
            mimes,
            icon,
        })
    }

    fn entries() -> Vec<Entry> {
        let locales = locale_keys();
        let mut seen = HashSet::new();
        let mut found = Vec::new();
        for dir in data_dirs() {
            let apps = dir.join("applications");
            let Ok(read) = std::fs::read_dir(&apps) else {
                continue;
            };
            for file in read.flatten() {
                let path = file.path();
                if path.extension().and_then(|e| e.to_str()) != Some("desktop") {
                    continue;
                }
                let Some(id) = path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .map(str::to_string)
                else {
                    continue;
                };
                if !seen.insert(id.clone()) {
                    continue;
                }
                if let Some(entry) = std::fs::read_to_string(&path)
                    .ok()
                    .and_then(|text| parse(&id, &text, &locales))
                {
                    found.push(entry);
                }
            }
        }
        found
    }

    fn matches(entry: &Entry, mime: &str) -> bool {
        let group = mime.split('/').next().unwrap_or("");
        entry.mimes.iter().any(|m| {
            m == mime
                || (mime.starts_with("text/") && m == "text/plain")
                || m.strip_suffix("/*").is_some_and(|g| g == group)
        })
    }

    pub fn apps(file_name: &str) -> Vec<OpenApp> {
        let mime = mime_of(file_name);
        let mut recommended = Vec::new();
        let mut others = Vec::new();
        for entry in entries() {
            let fits = matches(&entry, &mime);
            let app = OpenApp {
                id: entry.id,
                name: entry.name,
                recommended: fits,
            };
            if fits {
                recommended.push(app);
            } else {
                others.push(app);
            }
        }
        recommended.sort_by_cached_key(|a| a.name.to_lowercase());
        others.sort_by_cached_key(|a| a.name.to_lowercase());
        recommended.extend(others);
        recommended
    }

    fn command_of(entry: &Entry, path: &Path) -> Vec<String> {
        let target = path.to_string_lossy();
        let mut words = Vec::new();
        let mut used = false;
        for word in split_command(&entry.exec, false) {
            let mut out = String::new();
            let mut chars = word.chars();
            while let Some(c) = chars.next() {
                if c != '%' {
                    out.push(c);
                    continue;
                }
                match chars.next() {
                    Some('f' | 'F' | 'u' | 'U') => {
                        out.push_str(&target);
                        used = true;
                    }
                    Some('c') => out.push_str(&entry.name),
                    Some('%') => out.push('%'),
                    _ => {}
                }
            }
            if !out.is_empty() || !word.contains('%') {
                words.push(out);
            }
        }
        if !used {
            words.push(target.into_owned());
        }
        words
    }

    fn icon_file(name: &str) -> Option<PathBuf> {
        let direct = Path::new(name);
        if direct.is_absolute() {
            return direct.is_file().then(|| direct.to_path_buf());
        }
        let mut best: Option<(u32, PathBuf)> = None;
        let mut consider = |path: PathBuf, size: u32| {
            let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
            let rank = match ext {
                "svg" => 1000,
                "png" => 1000 - size.abs_diff(64).min(900),
                _ => return,
            };
            if best.as_ref().is_none_or(|b| rank > b.0) {
                best = Some((rank, path));
            }
        };
        for dir in data_dirs() {
            let icons = dir.join("icons");
            let Ok(themes) = std::fs::read_dir(&icons) else {
                continue;
            };
            for theme in themes.flatten() {
                for first in std::fs::read_dir(theme.path())
                    .into_iter()
                    .flatten()
                    .flatten()
                {
                    let first_name = first.file_name().to_string_lossy().into_owned();
                    let size_of = |s: &str| s.split('x').next().and_then(|n| n.parse::<u32>().ok());
                    for second in std::fs::read_dir(first.path())
                        .into_iter()
                        .flatten()
                        .flatten()
                    {
                        let second_name = second.file_name().to_string_lossy().into_owned();
                        let (apps, size) = if second_name == "apps" {
                            (second.path(), size_of(&first_name))
                        } else if first_name == "apps" {
                            (second.path(), size_of(&second_name))
                        } else {
                            continue;
                        };
                        let size = size.unwrap_or(64);
                        for ext in ["svg", "png"] {
                            let file = apps.join(format!("{name}.{ext}"));
                            if file.is_file() {
                                consider(file, size);
                            }
                        }
                    }
                }
            }
        }
        for dir in data_dirs() {
            for ext in ["svg", "png"] {
                let file = dir.join("pixmaps").join(format!("{name}.{ext}"));
                if file.is_file() {
                    consider(file, 48);
                }
            }
        }
        best.map(|b| b.1)
    }

    pub fn icon(id: &str) -> Option<(&'static str, Vec<u8>)> {
        let entry = entries().into_iter().find(|e| e.id == id)?;
        let file = icon_file(&entry.icon?)?;
        let mime = if file.extension().and_then(|e| e.to_str()) == Some("svg") {
            "image/svg+xml"
        } else {
            "image/png"
        };
        let bytes = std::fs::read(file).ok()?;
        (bytes.len() <= 1 << 20).then_some((mime, bytes))
    }

    pub fn open(file_name: &str, path: &Path, id: &str) -> Result<(), ()> {
        let _ = file_name;
        let entry = entries().into_iter().find(|e| e.id == id).ok_or(())?;
        let words = command_of(&entry, path);
        let (program, args) = words.split_first().ok_or(())?;
        let mut child = std::process::Command::new(program)
            .args(args)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .map_err(|_| ())?;
        std::thread::spawn(move || {
            let _ = child.wait();
        });
        Ok(())
    }
}

#[cfg(target_os = "macos")]
mod platform {
    use super::OpenApp;
    use std::path::{Path, PathBuf};

    struct Bundle {
        path: PathBuf,
        name: String,
        extensions: Vec<String>,
        icon: Option<String>,
    }

    fn bundles() -> Vec<Bundle> {
        let mut roots = vec![
            PathBuf::from("/Applications"),
            PathBuf::from("/System/Applications"),
            PathBuf::from("/Applications/Utilities"),
        ];
        if let Some(home) = std::env::var_os("HOME") {
            roots.push(PathBuf::from(home).join("Applications"));
        }
        let mut found = Vec::new();
        for root in roots {
            let Ok(read) = std::fs::read_dir(&root) else {
                continue;
            };
            for entry in read.flatten() {
                let path = entry.path();
                if path.extension().and_then(|e| e.to_str()) != Some("app") {
                    continue;
                }
                let Ok(info) = plist::Value::from_file(path.join("Contents/Info.plist")) else {
                    continue;
                };
                let Some(dict) = info.as_dictionary() else {
                    continue;
                };
                let name = ["CFBundleDisplayName", "CFBundleName"]
                    .iter()
                    .find_map(|k| dict.get(k).and_then(|v| v.as_string()))
                    .map(str::to_string)
                    .or_else(|| path.file_stem().map(|s| s.to_string_lossy().into_owned()))
                    .unwrap_or_default();
                let extensions = dict
                    .get("CFBundleDocumentTypes")
                    .and_then(|v| v.as_array())
                    .into_iter()
                    .flatten()
                    .filter_map(|t| t.as_dictionary())
                    .filter_map(|t| t.get("CFBundleTypeExtensions"))
                    .filter_map(|v| v.as_array())
                    .flatten()
                    .filter_map(|v| v.as_string())
                    .map(str::to_lowercase)
                    .collect();
                let icon = dict
                    .get("CFBundleIconFile")
                    .and_then(|v| v.as_string())
                    .map(str::to_string);
                found.push(Bundle {
                    path,
                    name,
                    extensions,
                    icon,
                });
            }
        }
        found
    }

    fn extension_of(file_name: &str) -> String {
        Path::new(file_name)
            .extension()
            .map(|e| e.to_string_lossy().to_lowercase())
            .unwrap_or_default()
    }

    pub fn apps(file_name: &str) -> Vec<OpenApp> {
        let ext = extension_of(file_name);
        let mut recommended = Vec::new();
        let mut others = Vec::new();
        for bundle in bundles() {
            let fits = !ext.is_empty() && bundle.extensions.iter().any(|e| *e == ext || e == "*");
            let app = OpenApp {
                id: bundle.path.to_string_lossy().into_owned(),
                name: bundle.name,
                recommended: fits,
            };
            if fits {
                recommended.push(app);
            } else {
                others.push(app);
            }
        }
        recommended.sort_by_cached_key(|a| a.name.to_lowercase());
        others.sort_by_cached_key(|a| a.name.to_lowercase());
        recommended.extend(others);
        recommended
    }

    pub fn icon(id: &str) -> Option<(&'static str, Vec<u8>)> {
        let bundle = bundles()
            .into_iter()
            .find(|b| b.path.to_string_lossy() == id)?;
        let mut file = bundle.path.join("Contents/Resources").join(bundle.icon?);
        if file.extension().is_none() {
            file.set_extension("icns");
        }
        let out = std::env::temp_dir().join(format!("holzi-icon-{}.png", uuid::Uuid::new_v4()));
        let converted = std::process::Command::new("sips")
            .args(["-s", "format", "png", "-Z", "64"])
            .arg(&file)
            .arg("--out")
            .arg(&out)
            .output()
            .ok()
            .filter(|o| o.status.success());
        let bytes = converted.and_then(|_| std::fs::read(&out).ok());
        let _ = std::fs::remove_file(&out);
        bytes.map(|b| ("image/png", b))
    }

    pub fn open(file_name: &str, path: &Path, id: &str) -> Result<(), ()> {
        let _ = file_name;
        if !bundles().iter().any(|b| b.path.to_string_lossy() == id) {
            return Err(());
        }
        let status = std::process::Command::new("open")
            .arg("-a")
            .arg(id)
            .arg(path)
            .status()
            .map_err(|_| ())?;
        if status.success() {
            Ok(())
        } else {
            Err(())
        }
    }
}

#[cfg(target_os = "windows")]
mod platform {
    use super::{expand_env, split_command, OpenApp};
    use std::os::windows::process::CommandExt;
    use std::path::Path;

    const CREATE_NO_WINDOW: u32 = 0x0800_0000;

    struct Program {
        exe: String,
        name: String,
        command: String,
    }

    fn reg(args: &[&str]) -> String {
        std::process::Command::new("reg")
            .arg("query")
            .args(args)
            .creation_flags(CREATE_NO_WINDOW)
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
            .unwrap_or_default()
    }

    /// The `(name, value)` pairs of the values `reg query` lists under one key.
    fn values(output: &str) -> Vec<(String, String)> {
        output
            .lines()
            .filter_map(|line| {
                let line = line.trim_start();
                let (name, rest) = line.split_once("    REG_")?;
                let (_, value) = rest.split_once("    ")?;
                Some((name.trim().to_string(), value.trim().to_string()))
            })
            .collect()
    }

    fn subkeys(output: &str, parent: &str) -> Vec<String> {
        output
            .lines()
            .filter_map(|line| {
                let line = line.trim();
                let line = line
                    .strip_prefix("HKEY_CLASSES_ROOT")
                    .map(|rest| format!("HKCR{rest}"))
                    .or_else(|| {
                        line.strip_prefix("HKEY_CURRENT_USER")
                            .map(|rest| format!("HKCU{rest}"))
                    })
                    .unwrap_or_else(|| line.to_string());
                line.strip_prefix(parent).map(str::to_string)
            })
            .map(|rest| rest.trim_start_matches('\\').to_string())
            .filter(|rest| !rest.is_empty() && !rest.contains('\\'))
            .collect()
    }

    fn program(exe: &str) -> Option<Program> {
        let key = format!("HKCR\\Applications\\{exe}");
        let command = values(&reg(&[&format!("{key}\\shell\\open\\command"), "/ve"]))
            .into_iter()
            .next()?
            .1;
        let name = values(&reg(&[&key, "/v", "FriendlyAppName"]))
            .into_iter()
            .next()
            .map(|(_, v)| v)
            .filter(|v| !v.starts_with('@'))
            .unwrap_or_else(|| {
                Path::new(exe)
                    .file_stem()
                    .map(|s| s.to_string_lossy().into_owned())
                    .unwrap_or_else(|| exe.to_string())
            });
        Some(Program {
            exe: exe.to_string(),
            name,
            command,
        })
    }

    fn recommended_exes(ext: &str) -> Vec<String> {
        let mut exes = Vec::new();
        for key in [
            format!("HKCU\\Software\\Microsoft\\Windows\\CurrentVersion\\Explorer\\FileExts\\.{ext}\\OpenWithList"),
            format!("HKCR\\.{ext}\\OpenWithList"),
        ] {
            for (name, value) in values(&reg(&[&key])) {
                if name.len() == 1 && value.to_lowercase().ends_with(".exe") {
                    exes.push(value);
                }
            }
            for sub in subkeys(&reg(&[&key]), &key) {
                if sub.to_lowercase().ends_with(".exe") {
                    exes.push(sub);
                }
            }
        }
        exes
    }

    fn all_exes() -> Vec<String> {
        let parent = "HKCR\\Applications";
        subkeys(&reg(&[parent]), parent)
            .into_iter()
            .filter(|e| e.to_lowercase().ends_with(".exe"))
            .collect()
    }

    fn extension_of(file_name: &str) -> String {
        Path::new(file_name)
            .extension()
            .map(|e| e.to_string_lossy().to_lowercase())
            .unwrap_or_default()
    }

    pub fn apps(file_name: &str) -> Vec<OpenApp> {
        let ext = extension_of(file_name);
        let fitting: Vec<String> = if ext.is_empty() {
            Vec::new()
        } else {
            recommended_exes(&ext)
        };
        let mut seen = std::collections::HashSet::new();
        let mut recommended = Vec::new();
        let mut others = Vec::new();
        let candidates = fitting
            .iter()
            .map(|e| (e.clone(), true))
            .chain(all_exes().into_iter().map(|e| (e, false)));
        for (exe, fits) in candidates {
            if !seen.insert(exe.to_lowercase()) {
                continue;
            }
            let Some(found) = program(&exe) else {
                continue;
            };
            let app = OpenApp {
                id: found.exe,
                name: found.name,
                recommended: fits,
            };
            if fits {
                recommended.push(app);
            } else {
                others.push(app);
            }
        }
        recommended.sort_by_cached_key(|a| a.name.to_lowercase());
        others.sort_by_cached_key(|a| a.name.to_lowercase());
        recommended.extend(others);
        recommended
    }

    pub fn icon(id: &str) -> Option<(&'static str, Vec<u8>)> {
        if !id.to_lowercase().ends_with(".exe") || id.contains(['\\', '/']) {
            return None;
        }
        let found = program(id)?;
        let exe = expand_env(
            &split_command(&found.command, true).into_iter().next()?,
            |name| std::env::var(name).ok(),
        );
        if !Path::new(&exe).is_file() {
            return None;
        }
        let script = "Add-Type -AssemblyName System.Drawing; \
            $i = [System.Drawing.Icon]::ExtractAssociatedIcon($env:HOLZI_ICON_EXE); \
            $m = New-Object System.IO.MemoryStream; \
            $i.ToBitmap().Save($m, [System.Drawing.Imaging.ImageFormat]::Png); \
            [Convert]::ToBase64String($m.ToArray())";
        let output = std::process::Command::new("powershell")
            .args(["-NoProfile", "-NonInteractive", "-Command", script])
            .env("HOLZI_ICON_EXE", &exe)
            .creation_flags(CREATE_NO_WINDOW)
            .output()
            .ok()
            .filter(|o| o.status.success())?;
        let text = String::from_utf8_lossy(&output.stdout);
        let bytes =
            base64::Engine::decode(&base64::engine::general_purpose::STANDARD, text.trim()).ok()?;
        Some(("image/png", bytes))
    }

    pub fn open(file_name: &str, path: &Path, id: &str) -> Result<(), ()> {
        let _ = file_name;
        if !id.to_lowercase().ends_with(".exe") || id.contains(['\\', '/']) {
            return Err(());
        }
        let found = program(id).ok_or(())?;
        let target = path.to_string_lossy();
        let mut words: Vec<String> = Vec::new();
        let mut used = false;
        for (index, word) in split_command(&found.command, true).into_iter().enumerate() {
            let word = expand_env(&word, |name| std::env::var(name).ok());
            if index == 0 {
                words.push(word);
            } else if word.contains("%1") || word.contains("%L") || word.contains("%l") {
                words.push(
                    word.replace("%1", &target)
                        .replace("%L", &target)
                        .replace("%l", &target),
                );
                used = true;
            } else if !word.starts_with('%') {
                words.push(word);
            }
        }
        if !used {
            words.push(target.into_owned());
        }
        let (program, args) = words.split_first().ok_or(())?;
        let is_exe = program.to_lowercase().ends_with(".exe") && Path::new(program).is_file();
        if !is_exe || Path::new(program) == path {
            return Err(());
        }
        std::process::Command::new(program)
            .args(args)
            .creation_flags(CREATE_NO_WINDOW)
            .spawn()
            .map(|_| ())
            .map_err(|_| ())
    }
}

#[cfg(desktop)]
pub fn apps_for(file_name: &str) -> Vec<OpenApp> {
    platform::apps(file_name)
}

/// The icon of an app as a `data:` URL; none when the app has no icon this module can read.
#[cfg(desktop)]
pub fn icon_of(id: &str) -> Option<String> {
    use base64::Engine;
    let (mime, bytes) = platform::icon(id)?;
    Some(format!(
        "data:{mime};base64,{}",
        base64::engine::general_purpose::STANDARD.encode(bytes)
    ))
}

#[cfg(desktop)]
#[allow(clippy::result_unit_err)]
pub fn open_with(file_name: &str, path: &std::path::Path, id: &str) -> Result<(), ()> {
    platform::open(file_name, path, id)
}

#[cfg(test)]
mod tests {
    use super::{expand_env, split_command};

    #[test]
    fn expands_known_environment_variables_only() {
        let lookup = |name: &str| (name == "SystemRoot").then(|| "C:\\Windows".to_string());
        assert_eq!(
            expand_env("%SystemRoot%\\system32\\NOTEPAD.EXE", lookup),
            "C:\\Windows\\system32\\NOTEPAD.EXE"
        );
        assert_eq!(expand_env("%1", lookup), "%1");
        assert_eq!(expand_env("100%", lookup), "100%");
    }

    #[test]
    fn splits_quoted_words() {
        assert_eq!(
            split_command(r#"code --new "a b" %F"#, false),
            ["code", "--new", "a b", "%F"]
        );
        assert_eq!(
            split_command(r#""C:\Program Files\App\app.exe" "%1""#, true),
            [r"C:\Program Files\App\app.exe", "%1"]
        );
    }
}
