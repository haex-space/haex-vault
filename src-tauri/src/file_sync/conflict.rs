//! The name of a conflict copy (FR-012): `Name (Konflikt Gerätename Datum).Endung` beside the file,
//! with a number added when that name is taken. The word stays German in every language; it is part
//! of a file name, not of the window.

/// The name for the version of `path` that moves aside, made on `device` on `date` (`2026-10-27`).
/// `taken` says whether a path is used already on that side.
pub fn conflict_path(path: &str, device: &str, date: &str, taken: &dyn Fn(&str) -> bool) -> String {
    let (folder, name) = path.rsplit_once('/').map_or(("", path), |(f, n)| (f, n));
    let (stem, ext) = match name.rfind('.') {
        Some(dot) if dot > 0 => name.split_at(dot),
        _ => (name, ""),
    };
    let device = device.replace(['/', '\\'], "-");
    for n in 1.. {
        let counter = if n == 1 {
            String::new()
        } else {
            format!(" {n}")
        };
        let name = format!("{stem} (Konflikt {device} {date}{counter}){ext}");
        let candidate = if folder.is_empty() {
            name
        } else {
            format!("{folder}/{name}")
        };
        if !taken(&candidate) {
            return candidate;
        }
    }
    unreachable!("some number is free")
}

#[cfg(test)]
#[path = "conflict_tests.rs"]
mod tests;
