//! Files in the chat with contacts (spec 072 US3, research R11–R14): every file travels as an
//! encrypted copy over a Blossom-Server ([`blossom`]) in the `HXCF` format ([`crypto`]). Storing
//! received files, the transfers and the deletion of blobs follow in later slices of PR E.

pub mod blossom;
pub mod crypto;

#[cfg(test)]
pub(crate) mod test_server;
