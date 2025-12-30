pub mod filesystem;
pub mod helpers;
pub mod manager;
pub mod web;

#[cfg(test)]
mod tests;

// Re-export manager types
pub use manager::ExtensionWebviewManager;
