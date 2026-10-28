//! The address book (spec 051): contacts as core data of the vault, synced to every own device,
//! shown in the built-in app "Kontakte". [`service::ContactsService`] is the single entrance;
//! [`access`] holds its rules, [`store`] its SQL, [`kinds`] the open set of value kinds.

pub mod access;
pub mod agent;
pub mod agent_share;
pub mod agent_share_confirm;
pub mod card;
pub mod commands;
pub mod ids;
pub mod kinds;
pub mod maintenance;
pub mod model;
pub mod outside;
pub mod phone;
pub mod photos;
pub mod search;
pub mod service;
pub mod share;
mod store;
pub mod targets;
