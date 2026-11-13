//! The commands of the native camera of the link sheet (spec 085, research R4). Only the main
//! window may start it, and at most one capture runs: starting again replaces it.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

use tauri::ipc::{Channel, InvokeResponseBody};
#[cfg(target_os = "linux")]
use tauri::Emitter;
use tauri::{AppHandle, State, Window};

use super::CameraProblem;
#[cfg(target_os = "linux")]
use super::{CODES_EVENT, ERROR_EVENT, TEXT_EVENT};
use crate::error::{HolziError, Result};

/// The capture that runs, if one does.
#[derive(Default)]
pub struct LinkCamera {
    running: Mutex<Option<Running>>,
    /// One start or stop at a time, from taking the capture that runs to registering the new one:
    /// two starts would otherwise both see none and the first capture be lost, and a stop that
    /// arrives while a start is open would find nothing and leave the camera on.
    turn: tokio::sync::Mutex<()>,
}

struct Running {
    stop: Arc<AtomicBool>,
    thread: JoinHandle<()>,
}

impl Running {
    /// Ends the capture and waits for the camera to be given back.
    fn end(self) {
        self.stop.store(true, Ordering::SeqCst);
        // A panic in the capture thread has nothing left to give back.
        let _ = self.thread.join();
    }
}

impl LinkCamera {
    fn take(&self) -> Option<Running> {
        self.running
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .take()
    }

    fn set(&self, running: Running) {
        *self
            .running
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(running);
    }
}

fn require_main(window: &Window) -> Result<()> {
    if window.label() == "main" {
        Ok(())
    } else {
        Err(HolziError::InvalidInput {
            reason: "the camera is only for the main window".to_string(),
        })
    }
}

/// Whether the camera of the link sheet is read natively here, because the webview's is not
/// usable (Linux). Everywhere else the webview opens the camera itself.
#[tauri::command]
pub fn link_camera_native() -> bool {
    cfg!(target_os = "linux")
}

/// Starts the camera: every picture goes to `on_frame` as the bytes of a JPEG (raw, not text: the
/// preview is as smooth as the camera is), the text of a QR code in them comes as [`TEXT_EVENT`],
/// and a camera that stops by itself as [`ERROR_EVENT`]. A camera that cannot be opened is the
/// error of this call. With `codes` set the search reads every code format of a pass (QR, Code 128,
/// EAN-13, PDF417, Aztec) instead of QR only, and tells them as [`CODES_EVENT`] to the main window.
#[tauri::command]
pub async fn link_camera_start(
    app: AppHandle,
    window: Window,
    state: State<'_, LinkCamera>,
    on_frame: Channel<InvokeResponseBody>,
    codes: Option<bool>,
) -> Result<()> {
    require_main(&window)?;
    let codes = codes.unwrap_or(false);
    let _turn = state.turn.lock().await;
    if let Some(running) = state.take() {
        tokio::task::spawn_blocking(move || running.end())
            .await
            .map_err(|e| HolziError::Io {
                reason: format!("camera task: {e}"),
            })?;
    }
    let running = tokio::task::spawn_blocking(move || spawn(app, on_frame, codes))
        .await
        .map_err(|e| HolziError::Io {
            reason: format!("camera task: {e}"),
        })?
        .map_err(unavailable)?;
    state.set(running);
    Ok(())
}

/// Gives the camera back; safe at any time.
#[tauri::command]
pub async fn link_camera_stop(window: Window, state: State<'_, LinkCamera>) -> Result<()> {
    require_main(&window)?;
    let _turn = state.turn.lock().await;
    if let Some(running) = state.take() {
        tokio::task::spawn_blocking(move || running.end())
            .await
            .map_err(|e| HolziError::Io {
                reason: format!("camera task: {e}"),
            })?;
    }
    Ok(())
}

fn unavailable(problem: CameraProblem) -> HolziError {
    HolziError::CameraUnavailable {
        reason: problem.key().to_string(),
    }
}

/// Opens the camera in a thread of its own (the capture borrows the device, so both live there)
/// and returns once it is running or says why it is not. The search for a QR code has a thread of
/// its own too and is offered only the newest picture: while it is busy, pictures are not queued.
#[cfg(target_os = "linux")]
fn spawn(
    app: AppHandle,
    on_frame: Channel<InvokeResponseBody>,
    codes: bool,
) -> std::result::Result<Running, CameraProblem> {
    use std::sync::mpsc::{channel, sync_channel, TrySendError};
    use std::time::Instant;

    use super::session::Reported;
    use super::{capture, qr, session};

    let stop = Arc::new(AtomicBool::new(false));
    let (opened, wait) = channel();
    let thread_stop = Arc::clone(&stop);
    let thread = std::thread::Builder::new()
        .name("link-camera".to_string())
        .spawn(move || {
            let device = match capture::find_camera() {
                Ok(device) => device,
                Err(problem) => {
                    let _ = opened.send(Err(problem));
                    return;
                }
            };
            let mut source = match capture::start(&device) {
                Ok(source) => source,
                Err(problem) => {
                    let _ = opened.send(Err(problem));
                    return;
                }
            };
            let _ = opened.send(Ok(()));
            let started = Instant::now();

            let (offer, receive) = sync_channel::<Vec<u8>>(1);
            let search_app = app.clone();
            let search = std::thread::Builder::new()
                .name("link-camera-qr".to_string())
                .spawn(move || {
                    let mut reported = Reported::default();
                    while let Ok(jpeg) = receive.recv() {
                        let now = started.elapsed().as_millis() as u64;
                        if codes {
                            let found = crate::passwords::pass::scan::scan_image(&jpeg);
                            if !found.is_empty() && reported.should_tell(&codes_key(&found), now) {
                                let _ = search_app.emit_to("main", CODES_EVENT, found);
                            }
                            continue;
                        }
                        let Some(text) = qr::read_qr(&jpeg) else {
                            continue;
                        };
                        if reported.should_tell(&text, now) {
                            let _ = search_app.emit(TEXT_EVENT, text);
                        }
                    }
                });

            let ended = session::run(
                &mut source,
                &thread_stop,
                &mut || started.elapsed().as_millis() as u64,
                &mut |jpeg| {
                    let _ = on_frame.send(InvokeResponseBody::Raw(jpeg.to_vec()));
                },
                &mut |jpeg| {
                    // Busy with the last one: this one is dropped, the next will do.
                    if let Err(TrySendError::Disconnected(_)) = offer.try_send(jpeg.to_vec()) {
                        log::warn!("link camera: the search for a QR code ended");
                    }
                },
            );
            drop(offer);
            if let Ok(search) = search {
                let _ = search.join();
            }
            if let Err(problem) = ended {
                let _ = app.emit(ERROR_EVENT, problem.key());
            }
        })
        .map_err(|_| CameraProblem::Failed)?;
    match wait.recv() {
        Ok(Ok(())) => Ok(Running { stop, thread }),
        Ok(Err(problem)) => {
            let _ = thread.join();
            Err(problem)
        }
        Err(_) => Err(CameraProblem::Failed),
    }
}

/// What tells two reads of the same codes apart from a new one, for the repeat guard. Never shown
/// or logged: it holds the values.
#[cfg(target_os = "linux")]
fn codes_key(found: &[crate::passwords::pass::scan::ScannedCode]) -> String {
    let mut parts: Vec<String> = found
        .iter()
        .map(|code| format!("{}:{}", code.format, code.value))
        .collect();
    parts.sort();
    parts.join("\n")
}

#[cfg(not(target_os = "linux"))]
fn spawn(
    _app: AppHandle,
    _on_frame: Channel<InvokeResponseBody>,
    _codes: bool,
) -> std::result::Result<Running, CameraProblem> {
    Err(CameraProblem::Unsupported)
}
