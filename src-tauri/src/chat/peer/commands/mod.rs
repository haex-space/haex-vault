//! Tauri commands of contact and group conversations (contracts/ipc.md), split by area so no file
//! grows past 500 lines (plan.md): `conversations` for the list and its entries, `messages` for sending,
//! listing and reading, `requests` for answering requests, `files` for sending and downloading files; later slices add groups and settings.

pub mod conversations;
pub mod files;
pub mod messages;
pub mod requests;
