//! `files.read` (spec 044 FR-035, FR-035a, research R2, R8): the text of text files, the text of
//! PDFs and office documents, an image sized for the model, and for anything else the details of
//! the file with a note why there is nothing to read.

use std::io::Read;
use std::path::PathBuf;

use serde_json::json;
use tokio_util::sync::CancellationToken;

use super::env::AgentEnv;
use super::exec::FilesAgent;
use super::reach::{entry_json, files_error, PathInput};
use crate::chat::tools::native_action::{NativeError, NativeReply};
use crate::chat::tools::ToolImage;
use crate::files::extract::image::for_model;
use crate::files::extract::{self, Extracted, INPUT_LIMIT, NOTE_TOO_LARGE, TEXT_CHARS};
use crate::files::kind::{category, FileCategory};
use crate::files::local::ops::io_error;
use crate::files::local::text::{read_text, text_from};
use crate::files::storage_source::StorageFiles;
use crate::files::{Entry, EntryKind, FilesError, FilesErrorCode};

/// Bytes read for [`TEXT_CHARS`] of a text file: four per character at most in UTF-8.
const TEXT_BYTES: u64 = 4 * TEXT_CHARS as u64;

const NOTE_MEDIA: &str = "This is a video or audio file; holzi cannot read its content.";
const NOTE_BINARY: &str = "This file holds no text holzi can read.";
const NOTE_IMAGE: &str = "The image follows.";
const NOTE_BROKEN_IMAGE: &str = "holzi could not decode this image.";

/// Where the bytes of the entry are.
enum At {
    Device(PathBuf),
    Storage(Box<StorageFiles>),
}

fn failed() -> NativeError {
    NativeError::new("failed", "the action failed")
}

impl<E: AgentEnv> FilesAgent<E> {
    pub(super) async fn read(
        &self,
        input: PathInput,
        cancel: &CancellationToken,
    ) -> Result<NativeReply, NativeError> {
        let (entry, storage, real) = self.entry(&input, cancel).await?;
        if entry.kind == EntryKind::Dir {
            return Err(NativeError::invalid_input(
                Some("path"),
                "this is a folder; list it with files_list",
            ));
        }
        let at = match (storage, real) {
            (Some(files), _) => At::Storage(Box::new(files)),
            (None, Some(real)) => At::Device(real),
            (None, None) => unreachable!("an entry lies on the device or on a storage"),
        };
        match category(&entry.name) {
            Some(FileCategory::Video | FileCategory::Audio) => Ok(noted(&entry, NOTE_MEDIA)),
            Some(FileCategory::Image) => self.image(&entry, &at).await,
            Some(FileCategory::Document) => self.document(&entry, &at).await,
            Some(FileCategory::Text) | None => text(&entry, &at).await,
        }
    }

    /// The image as a JPEG for the model; the tool round decides whether the model sees it.
    async fn image(&self, entry: &Entry, at: &At) -> Result<NativeReply, NativeError> {
        let Some(bytes) = bytes(entry, at, INPUT_LIMIT).await? else {
            return Ok(noted(entry, NOTE_TOO_LARGE));
        };
        let jpeg = tokio::task::spawn_blocking(move || for_model(&bytes))
            .await
            .map_err(|_| failed())?;
        Ok(match jpeg {
            Ok(jpeg) => NativeReply {
                result: json!({ "entry": entry_json(entry), "note": NOTE_IMAGE }),
                images: vec![ToolImage {
                    name: entry.name.clone(),
                    media_type: "image/jpeg".to_owned(),
                    bytes: jpeg,
                }],
            },
            Err(_) => noted(entry, NOTE_BROKEN_IMAGE),
        })
    }

    /// The text of a PDF or an office document (research R8).
    async fn document(&self, entry: &Entry, at: &At) -> Result<NativeReply, NativeError> {
        if !extract::supports(&entry.name) {
            return Ok(noted(entry, extract::NOTE_FORMAT));
        }
        let Some(bytes) = bytes(entry, at, INPUT_LIMIT).await? else {
            return Ok(noted(entry, NOTE_TOO_LARGE));
        };
        let name = entry.name.clone();
        let extracted = tokio::task::spawn_blocking(move || extract::from_bytes(&name, &bytes))
            .await
            .map_err(|_| failed())?;
        Ok(match extracted {
            Extracted::Text { text, truncated } => {
                json!({ "entry": entry_json(entry), "text": text, "truncated": truncated }).into()
            }
            Extracted::NoText(note) => noted(entry, note),
        })
    }
}

fn noted(entry: &Entry, note: &str) -> NativeReply {
    json!({ "entry": entry_json(entry), "note": note }).into()
}

/// The text of a text file, at most [`TEXT_CHARS`] characters.
async fn text(entry: &Entry, at: &At) -> Result<NativeReply, NativeError> {
    let content = match at {
        At::Storage(files) => {
            let bytes = files
                .read_start(&entry.path, entry.size.unwrap_or(0), TEXT_BYTES)
                .await
                .map_err(files_error)?;
            text_from(bytes, TEXT_BYTES)
        }
        At::Device(real) => {
            let real = real.clone();
            tokio::task::spawn_blocking(move || read_text(&real, TEXT_BYTES))
                .await
                .map_err(|_| failed())?
        }
    };
    match content {
        Ok(content) => match extract::limited(content.text) {
            Extracted::Text { text, truncated } => Ok(json!({
                "entry": entry_json(entry),
                "text": text,
                "truncated": content.truncated || truncated,
            })
            .into()),
            Extracted::NoText(note) => Ok(noted(entry, note)),
        },
        Err(error) if error.code == FilesErrorCode::Binary => Ok(noted(entry, NOTE_BINARY)),
        Err(error) => Err(files_error(error)),
    }
}

/// All bytes of the entry, or `None` when it is larger than `limit`.
async fn bytes(entry: &Entry, at: &At, limit: u64) -> Result<Option<Vec<u8>>, NativeError> {
    if entry.size.is_some_and(|size| size > limit) {
        return Ok(None);
    }
    let bytes = match at {
        At::Storage(files) => files
            .read_start(&entry.path, entry.size.unwrap_or(0), limit)
            .await
            .map_err(files_error)?,
        At::Device(real) => {
            let real = real.clone();
            tokio::task::spawn_blocking(move || -> Result<Vec<u8>, FilesError> {
                let mut bytes = Vec::new();
                std::fs::File::open(&real)
                    .and_then(|file| file.take(limit + 1).read_to_end(&mut bytes))
                    .map_err(|error| io_error(error, &real))?;
                Ok(bytes)
            })
            .await
            .map_err(|_| failed())?
            .map_err(files_error)?
        }
    };
    // The file grew since it was looked at.
    Ok((bytes.len() as u64 <= limit).then_some(bytes))
}
