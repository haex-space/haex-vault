//! Files in the chat with contacts (spec 072 US3, research R11–R14): every file travels as an
//! encrypted copy over a Blossom-Server ([`blossom`]) in the `HXCF` format ([`crypto`]); this device
//! keeps its copies in an encrypted folder ([`store`]); [`transfers`] uploads before a message is
//! queued and downloads what arrived. The deletion of blobs follows in PR E2b.

pub mod blossom;
pub mod crypto;
pub mod store;
pub mod transfers;

#[cfg(test)]
pub(crate) mod test_server;
