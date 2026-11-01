//! Tauri commands of contact and group conversations (contracts/ipc.md), split by area so no file
//! grows past 500 lines (plan.md): `conversations` for the list and its entries, `messages` for sending,
//! listing and reading; later slices add requests, files, groups and settings.

pub mod conversations;
pub mod messages;
