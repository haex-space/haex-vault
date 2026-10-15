//! Visitenkarten (spec 051 US4, research R12, contracts/contact-card.md): the format and the
//! vault's own card. Reading cards in lives in the service (`service/cards.rs`).

pub mod format;
pub mod own_card;
pub mod received;
