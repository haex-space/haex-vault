// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

/// Starts the desktop application through the shared library entry point.
fn main() {
    haex_lib::run();
}
