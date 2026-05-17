//! Streaming layer for remote storage.
//!
//! Defines a small [`StreamingSource`] trait that knows how to:
//!   - report a resource's total size
//!   - read a single byte range from it
//!   - report a content type
//!
//! Per-target adapters (S3 here, local/p2p later) implement the trait.
//! The [`protocol`] module exposes them through a Tauri custom URI scheme
//! (`haex-stream://...`) that the WebView can point an HTML5 `<video>` or
//! `<audio>` element at — the browser then drives Range requests and we
//! translate each one into a call into the matching adapter.

pub mod peer_source;
pub mod protocol;
pub mod s3_source;
pub mod source;

pub use protocol::stream_protocol_handler;
