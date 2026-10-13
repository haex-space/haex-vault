//! Ending the process when the vault closes (spec 013 FR-006, spec 043 research R4).

/// Ends the process with `code`.
///
/// On Android without the C exit handlers: `exit` runs the destructors of the system's UI
/// renderer (libhwui) while its threads still draw, and each of them then aborts with
/// "FORTIFY: pthread_mutex_lock called on a destroyed mutex". The log and the standard streams
/// are flushed first, as `std::process::exit` does.
pub fn end_process(code: i32) -> ! {
    #[cfg(target_os = "android")]
    {
        use std::io::Write;
        log::logger().flush();
        let _ = std::io::stdout().flush();
        let _ = std::io::stderr().flush();
        // SAFETY: FFI call with a plain integer; `_exit` ends the process at once and touches no
        // Rust state.
        unsafe { libc::_exit(code) }
    }
    #[cfg(not(target_os = "android"))]
    std::process::exit(code)
}
