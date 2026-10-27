//! Sharing a contact (spec 060): the text that goes out as link, clipboard and QR code, and the
//! rules every share keeps. [`envelope`] is its format, [`name`] the name a share carries, [`model`]
//! the types of the dialog and [`build`] makes the dialog's rows and the text from a contact.

pub mod build;
pub mod envelope;
pub mod model;
pub mod name;
