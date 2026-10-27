//! Sharing a contact (spec 060): the text that goes out as link, clipboard and QR code, and the
//! rules every share keeps. [`envelope`] is its format, [`name`] the name a share carries, [`model`]
//! the types of the dialog, [`build`] makes the dialog's rows and the text from a contact, [`preview`]
//! shows what reading a share in would do and [`merge`] does it, adding and never removing.

pub mod build;
pub mod envelope;
pub mod merge;
pub mod model;
pub mod name;
pub mod preview;
#[cfg(test)]
pub(crate) mod test_support;
