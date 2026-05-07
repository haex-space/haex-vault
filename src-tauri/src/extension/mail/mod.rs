//! Extension-facing wrapper for the core mail module.
//!
//! Adds permission checks (`ResourceType::Mail`, action `Fetch` or
//! `Send`) and extension-id resolution (window vs. iframe parameters)
//! around the general-purpose IMAP/SMTP API in `crate::mail`.
//!
//! Account credentials are NOT stored here — extensions load them from
//! the core passwords vault (filtered by tag scope) and pass them in
//! per call. The wrapper has no notion of "accounts" beyond this.

pub mod commands;
