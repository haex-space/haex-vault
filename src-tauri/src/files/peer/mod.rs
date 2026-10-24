//! File access between devices over `haex-vault-files/1` (spec 056): the serving side
//! ([`serve`]), the requesting side ([`client`], [`store`]), and what both share ([`wire`],
//! [`handshake`]). Devices prove themselves with [`crate::sync::foreign_auth`] (spec 057); the
//! serving device decides on every request (FR-005).

pub mod client;
pub mod connect;
pub mod contacts;
pub mod exact;
pub mod handshake;
pub mod serve;
pub mod store;
pub mod wire;

#[cfg(test)]
mod contact_tests;
#[cfg(test)]
mod own_device_tests;
#[cfg(test)]
pub(crate) mod test_support;
