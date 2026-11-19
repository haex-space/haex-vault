//! Wires the Kotlin module under `android/` into the app's Gradle build.

/// Nothing is called from the web view: the core calls the Kotlin side directly.
const COMMANDS: &[&str] = &[];

fn main() {
    tauri_plugin::Builder::new(COMMANDS)
        .android_path("android")
        .build();
}
