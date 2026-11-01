//! Tauri commands of contact and group conversations (contracts/ipc.md), split by area so no file
//! grows past 500 lines (plan.md): `conversations` for the list and its entries; later slices add
//! messages, requests, files, groups and settings.

pub mod conversations;
