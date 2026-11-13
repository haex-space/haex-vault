//! The commands that read codes for a pass from an image or a PDF (spec 092, FR-034, FR-035,
//! research R15). They read what the window sends or a file the person chose, decode in Rust and
//! return what was found; nothing is saved and nothing is logged. The window shows format and
//! value, the person picks, and the pass is written by the commands of `pass.rs`.

use serde::Deserialize;
use tauri::ipc::{InvokeBody, Request, Response};
use tauri::AppHandle;
use ts_rs::TS;

use crate::error::{HolziError, Result};
use crate::files::picked::{self, PickedFile};
use crate::passwords::pass::scan::{is_pdf, scan_image, ScannedCode, MAX_INPUT_BYTES};

#[derive(Debug, Deserialize, TS)]
#[ts(export, export_to = "../../src/types/bindings/")]
#[serde(rename_all = "camelCase")]
pub struct PassScanFileArgs {
    pub file: PickedFile,
}

fn too_large() -> HolziError {
    HolziError::InvalidInput {
        reason: "tooLarge".to_owned(),
    }
}

/// The codes in an image the window sends as the raw body (a rendered page of a PDF, or a frame).
/// An image over the limit is refused; one that holds no code gives an empty list.
#[tauri::command]
pub async fn passwords_pass_codes_scan(request: Request<'_>) -> Result<Vec<ScannedCode>> {
    let InvokeBody::Raw(bytes) = request.body() else {
        return Err(HolziError::InvalidInput {
            reason: "the image must arrive as raw bytes".to_owned(),
        });
    };
    if bytes.len() as u64 > MAX_INPUT_BYTES {
        return Err(too_large());
    }
    let bytes = bytes.clone();
    tauri::async_runtime::spawn_blocking(move || scan_image(&bytes))
        .await
        .map_err(|e| HolziError::InvalidInput {
            reason: format!("scan: {e}"),
        })
}

/// The codes in a photo the person chose. The file is read into memory and not kept.
#[tauri::command]
pub async fn passwords_pass_codes_scan_file(
    app: AppHandle,
    args: PassScanFileArgs,
) -> Result<Vec<ScannedCode>> {
    let bytes = picked::read(&app, &args.file, MAX_INPUT_BYTES)?;
    if bytes.len() as u64 > MAX_INPUT_BYTES {
        return Err(too_large());
    }
    tauri::async_runtime::spawn_blocking(move || scan_image(&bytes))
        .await
        .map_err(|e| HolziError::InvalidInput {
            reason: format!("scan: {e}"),
        })
}

/// The bytes of a PDF the person chose, for the window to render page by page. A file that is not
/// a PDF is refused. The bytes go to the window and are kept nowhere (FR-035).
#[tauri::command]
pub async fn passwords_pass_capture_pdf(
    app: AppHandle,
    args: PassScanFileArgs,
) -> Result<Response> {
    let bytes = picked::read(&app, &args.file, MAX_INPUT_BYTES)?;
    if bytes.len() as u64 > MAX_INPUT_BYTES {
        return Err(too_large());
    }
    if !is_pdf(&bytes) {
        return Err(HolziError::InvalidInput {
            reason: "notAPdf".to_owned(),
        });
    }
    Ok(Response::new(bytes))
}
