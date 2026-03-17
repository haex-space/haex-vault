//! PTY Manager - manages pseudo-terminal sessions for extensions.
//!
//! Desktop: Uses portable-pty for full interactive shell support.
//! Android/iOS: Stub implementation (SSH via russh planned for future).

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;

use super::types::{ShellCreateOptions, ShellExitEvent, ShellOutputEvent, SHELL_OUTPUT_EVENT};

#[cfg(desktop)]
use portable_pty::{native_pty_system, CommandBuilder, MasterPty, PtySize};

pub const SHELL_EXIT_EVENT: &str = "shell:exit";

/// Manages active PTY sessions per extension
pub struct PtyManager {
    sessions: Arc<Mutex<HashMap<String, PtySession>>>,
}

struct PtySession {
    #[cfg(desktop)]
    master: Box<dyn MasterPty + Send>,
    #[cfg(desktop)]
    writer: Box<dyn std::io::Write + Send>,
    extension_id: String,
}

impl PtyManager {
    pub fn new() -> Self {
        Self {
            sessions: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Create a new PTY session and start streaming output via Tauri events
    #[cfg(desktop)]
    pub async fn create_session(
        &self,
        app_handle: &tauri::AppHandle,
        extension_id: &str,
        options: ShellCreateOptions,
    ) -> Result<String, String> {
        let session_id = uuid::Uuid::new_v4().to_string();

        let cols = options.cols.unwrap_or(80);
        let rows = options.rows.unwrap_or(24);

        let pty_system = native_pty_system();
        let pair = pty_system
            .openpty(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|e| format!("Failed to open PTY: {e}"))?;

        // Determine shell
        let shell = options
            .shell
            .or_else(|| std::env::var("SHELL").ok())
            .unwrap_or_else(|| "/bin/sh".to_string());

        // Build command
        let mut cmd = CommandBuilder::new(&shell);

        // Set working directory
        if let Some(cwd) = &options.cwd {
            cmd.cwd(cwd);
        } else if let Ok(home) = std::env::var("HOME") {
            cmd.cwd(home);
        }

        // Set environment variables
        if let Some(env) = &options.env {
            for (key, value) in env {
                cmd.env(key, value);
            }
        }

        // Set TERM for proper terminal support
        cmd.env("TERM", "xterm-256color");

        // Spawn child process
        let _child = pair
            .slave
            .spawn_command(cmd)
            .map_err(|e| format!("Failed to spawn shell: {e}"))?;

        // Get writer for stdin
        let writer = pair
            .master
            .take_writer()
            .map_err(|e| format!("Failed to get PTY writer: {e}"))?;

        // Get reader for stdout
        let mut reader = pair
            .master
            .try_clone_reader()
            .map_err(|e| format!("Failed to get PTY reader: {e}"))?;

        // Store session
        let session = PtySession {
            master: pair.master,
            writer,
            extension_id: extension_id.to_string(),
        };

        self.sessions
            .lock()
            .await
            .insert(session_id.clone(), session);

        // Spawn background task to read PTY output and emit events
        let app_handle = app_handle.clone();
        let sid = session_id.clone();
        let sessions = self.sessions.clone();

        tokio::task::spawn_blocking(move || {
            use tauri::Emitter;

            let mut buf = [0u8; 4096];
            loop {
                match reader.read(&mut buf) {
                    Ok(0) => {
                        // PTY closed
                        let _ = app_handle.emit(
                            SHELL_EXIT_EVENT,
                            &ShellExitEvent {
                                session_id: sid.clone(),
                                exit_code: None,
                            },
                        );
                        // Clean up session
                        let sessions = sessions.clone();
                        let sid = sid.clone();
                        tokio::task::block_in_place(|| {
                            tokio::runtime::Handle::current().block_on(async {
                                sessions.lock().await.remove(&sid);
                            });
                        });
                        break;
                    }
                    Ok(n) => {
                        let data = String::from_utf8_lossy(&buf[..n]).to_string();
                        let _ = app_handle.emit(
                            SHELL_OUTPUT_EVENT,
                            &ShellOutputEvent {
                                session_id: sid.clone(),
                                data,
                            },
                        );
                    }
                    Err(e) => {
                        eprintln!("[Shell] PTY read error for session {sid}: {e}");
                        let _ = app_handle.emit(
                            SHELL_EXIT_EVENT,
                            &ShellExitEvent {
                                session_id: sid.clone(),
                                exit_code: None,
                            },
                        );
                        let sessions = sessions.clone();
                        let sid = sid.clone();
                        tokio::task::block_in_place(|| {
                            tokio::runtime::Handle::current().block_on(async {
                                sessions.lock().await.remove(&sid);
                            });
                        });
                        break;
                    }
                }
            }
        });

        Ok(session_id)
    }

    /// Write data to a PTY session's stdin
    #[cfg(desktop)]
    pub async fn write_to_session(&self, session_id: &str, data: &str) -> Result<(), String> {
        use std::io::Write;

        let mut sessions = self.sessions.lock().await;
        let session = sessions
            .get_mut(session_id)
            .ok_or_else(|| format!("Session {session_id} not found"))?;

        session
            .writer
            .write_all(data.as_bytes())
            .map_err(|e| format!("Failed to write to PTY: {e}"))?;
        session
            .writer
            .flush()
            .map_err(|e| format!("Failed to flush PTY: {e}"))?;

        Ok(())
    }

    /// Resize a PTY session
    #[cfg(desktop)]
    pub async fn resize_session(
        &self,
        session_id: &str,
        cols: u16,
        rows: u16,
    ) -> Result<(), String> {
        let sessions = self.sessions.lock().await;
        let session = sessions
            .get(session_id)
            .ok_or_else(|| format!("Session {session_id} not found"))?;

        session
            .master
            .resize(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|e| format!("Failed to resize PTY: {e}"))?;

        Ok(())
    }

    /// Close a PTY session
    pub async fn close_session(&self, session_id: &str) -> Result<(), String> {
        self.sessions
            .lock()
            .await
            .remove(session_id)
            .ok_or_else(|| format!("Session {session_id} not found"))?;
        // Dropping the session closes the PTY master, which terminates the child
        Ok(())
    }

    /// Close all sessions for an extension
    pub async fn close_extension_sessions(&self, extension_id: &str) {
        let mut sessions = self.sessions.lock().await;
        sessions.retain(|_, s| s.extension_id != extension_id);
    }

    /// Check if a session belongs to an extension
    pub async fn session_belongs_to(&self, session_id: &str, extension_id: &str) -> bool {
        self.sessions
            .lock()
            .await
            .get(session_id)
            .map(|s| s.extension_id == extension_id)
            .unwrap_or(false)
    }
}

// Android/iOS stub - no local PTY support
#[cfg(not(desktop))]
impl PtyManager {
    pub async fn create_session(
        &self,
        _app_handle: &tauri::AppHandle,
        _extension_id: &str,
        _options: ShellCreateOptions,
    ) -> Result<String, String> {
        Err("Local shell is not available on this platform. Use SSH to connect to a remote server.".to_string())
    }

    pub async fn write_to_session(&self, _session_id: &str, _data: &str) -> Result<(), String> {
        Err("Local shell is not available on this platform.".to_string())
    }

    pub async fn resize_session(
        &self,
        _session_id: &str,
        _cols: u16,
        _rows: u16,
    ) -> Result<(), String> {
        Err("Local shell is not available on this platform.".to_string())
    }
}
