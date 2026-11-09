//! The commands of the native camera of the link sheet (spec 085, research R4). Only the main
//! window may start it, and at most one capture runs: starting again replaces it.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

use tauri::{AppHandle, Emitter, State, Window};

use super::{CameraProblem, ERROR_EVENT, FRAME_EVENT, TEXT_EVENT};
use crate::error::{HolziError, Result};

/// The capture that runs, if one does.
#[derive(Default)]
pub struct LinkCamera(Mutex<Option<Running>>);

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
        self.0
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .take()
    }

    fn set(&self, running: Running) {
        *self
            .0
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

/// Starts the camera: pictures come as [`FRAME_EVENT`], the text of a QR code in them as
/// [`TEXT_EVENT`], and a camera that stops by itself as [`ERROR_EVENT`]. A camera that cannot be
/// opened is the error of this call.
#[tauri::command]
pub async fn link_camera_start(
    app: AppHandle,
    window: Window,
    state: State<'_, LinkCamera>,
) -> Result<()> {
    require_main(&window)?;
    if let Some(running) = state.take() {
        tokio::task::spawn_blocking(move || running.end())
            .await
            .map_err(|e| HolziError::Io {
                reason: format!("camera task: {e}"),
            })?;
    }
    let running = tokio::task::spawn_blocking(move || spawn(app))
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
/// and returns once it is running or says why it is not.
#[cfg(target_os = "linux")]
fn spawn(app: AppHandle) -> std::result::Result<Running, CameraProblem> {
    use base64::engine::general_purpose::STANDARD;
    use base64::Engine;

    use super::{capture, qr, session};

    let stop = Arc::new(AtomicBool::new(false));
    let (opened, wait) = std::sync::mpsc::channel();
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
            let started = std::time::Instant::now();
            let ended = session::run(
                &mut source,
                &thread_stop,
                &mut || started.elapsed().as_millis() as u64,
                &qr::read_qr,
                &mut |jpeg| {
                    let url = format!("data:image/jpeg;base64,{}", STANDARD.encode(jpeg));
                    let _ = app.emit(FRAME_EVENT, url);
                },
                &mut |text| {
                    let _ = app.emit(TEXT_EVENT, text);
                },
            );
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

#[cfg(not(target_os = "linux"))]
fn spawn(_app: AppHandle) -> std::result::Result<Running, CameraProblem> {
    let _ = (FRAME_EVENT, TEXT_EVENT, ERROR_EVENT);
    Err(CameraProblem::Unsupported)
}
