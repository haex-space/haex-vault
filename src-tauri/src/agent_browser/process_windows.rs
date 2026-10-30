//! The pipe on Windows (spec 065, research R2): Chromium reads its pipe ends as the C runtime's
//! descriptors 3 and 4, which a child inherits through `STARTUPINFOW.lpReserved2` (the way
//! Puppeteer's Node starts it). Only the two pipe ends are inherited (`PROC_THREAD_ATTRIBUTE_
//! HANDLE_LIST`), so no other handle of holzi leaks into the browser.
//!
//! The command line and the `lpReserved2` block are plain functions, tested on every system; the
//! system calls exist only on Windows.

/// The C runtime's flags for an open pipe descriptor (`FOPEN | FPIPE`).
pub const CRT_OPEN_PIPE: u8 = 0x01 | 0x08;

/// The `lpReserved2` block for descriptors `0..handles.len()`: their count, one flag byte each,
/// then each handle as a pointer-sized value. A descriptor with flag 0 is closed.
pub fn crt_block(entries: &[(u8, usize)]) -> Vec<u8> {
    let count = u32::try_from(entries.len()).unwrap_or(u32::MAX);
    let mut block = count.to_le_bytes().to_vec();
    block.extend(entries.iter().map(|(flags, _)| *flags));
    for (_, handle) in entries {
        block.extend_from_slice(&handle.to_le_bytes());
    }
    block
}

/// One argument quoted the way `CommandLineToArgvW` reads it back.
fn quote(arg: &str, line: &mut String) {
    if !arg.is_empty() && !arg.contains([' ', '\t', '"']) {
        line.push_str(arg);
        return;
    }
    line.push('"');
    let mut backslashes = 0;
    for c in arg.chars() {
        match c {
            '\\' => backslashes += 1,
            '"' => {
                line.extend(std::iter::repeat_n('\\', backslashes * 2 + 1));
                line.push('"');
                backslashes = 0;
            }
            _ => {
                line.extend(std::iter::repeat_n('\\', backslashes));
                line.push(c);
                backslashes = 0;
            }
        }
    }
    line.extend(std::iter::repeat_n('\\', backslashes * 2));
    line.push('"');
}

/// The command line of `program` with `args`.
pub fn command_line(program: &str, args: &[String]) -> String {
    let mut line = String::new();
    // The program name is read by other rules: quoted whole, never escaped.
    line.push('"');
    line.push_str(program);
    line.push('"');
    for arg in args {
        line.push(' ');
        quote(arg, &mut line);
    }
    line
}

#[cfg(windows)]
pub use self::system::spawn;

#[cfg(windows)]
mod system {
    use std::ffi::{c_void, OsString};
    use std::os::windows::ffi::OsStrExt;
    use std::os::windows::io::AsRawHandle;
    use std::path::Path;

    use tokio::sync::watch;
    use windows_sys::Win32::Foundation::{
        CloseHandle, SetHandleInformation, HANDLE, HANDLE_FLAG_INHERIT, INVALID_HANDLE_VALUE,
    };
    use windows_sys::Win32::System::Threading::{
        CreateProcessW, DeleteProcThreadAttributeList, GetExitCodeProcess,
        InitializeProcThreadAttributeList, UpdateProcThreadAttribute, WaitForSingleObject,
        EXTENDED_STARTUPINFO_PRESENT, INFINITE, PROCESS_INFORMATION,
        PROC_THREAD_ATTRIBUTE_HANDLE_LIST, STARTUPINFOEXW,
    };

    use super::{command_line, crt_block, CRT_OPEN_PIPE};
    use crate::agent_browser::cdp::Connection;
    use crate::agent_browser::error::{BrowserError, Result};
    use crate::agent_browser::process::BrowserProcess;

    fn last_error(what: &str) -> BrowserError {
        BrowserError::Unavailable(format!("{what}: {}", std::io::Error::last_os_error()))
    }

    fn wide(text: &str) -> Vec<u16> {
        std::ffi::OsStr::new(text)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect()
    }

    /// A process handle that is closed when it goes.
    struct Owned(HANDLE);

    // SAFETY: a process handle may be used and closed from any thread.
    unsafe impl Send for Owned {}

    impl Drop for Owned {
        fn drop(&mut self) {
            // SAFETY: the handle came from `CreateProcessW` and is closed only here.
            unsafe {
                CloseHandle(self.0);
            }
        }
    }

    /// Starts `executable` with `args`, the browser's pipe ends as descriptors 3 and 4.
    pub fn spawn(executable: &Path, args: &[OsString]) -> Result<(Connection, BrowserProcess)> {
        let (child_reads, holzi_writes) =
            std::io::pipe().map_err(|e| BrowserError::Unavailable(e.to_string()))?;
        let (holzi_reads, child_writes) =
            std::io::pipe().map_err(|e| BrowserError::Unavailable(e.to_string()))?;
        let inherited: [HANDLE; 2] = [
            child_reads.as_raw_handle() as HANDLE,
            child_writes.as_raw_handle() as HANDLE,
        ];
        for handle in inherited {
            // SAFETY: both handles are open pipe ends owned by this function.
            if unsafe { SetHandleInformation(handle, HANDLE_FLAG_INHERIT, HANDLE_FLAG_INHERIT) }
                == 0
            {
                return Err(last_error("SetHandleInformation"));
            }
        }
        let closed = INVALID_HANDLE_VALUE as usize;
        let block = crt_block(&[
            (0, closed),
            (0, closed),
            (0, closed),
            (CRT_OPEN_PIPE, inherited[0] as usize),
            (CRT_OPEN_PIPE, inherited[1] as usize),
        ]);
        let args: Vec<String> = args
            .iter()
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect();
        let mut line = wide(&command_line(&executable.to_string_lossy(), &args));
        let program = wide(&executable.to_string_lossy());

        // The attribute list that names the two handles as the only inherited ones.
        let mut size = 0usize;
        // SAFETY: the first call only asks for the size of a list with one attribute.
        unsafe {
            InitializeProcThreadAttributeList(std::ptr::null_mut(), 1, 0, &mut size);
        }
        let mut list = vec![0u8; size];
        let attributes = list.as_mut_ptr() as *mut c_void;
        // SAFETY: `list` has the size the system asked for and outlives every use below.
        if unsafe { InitializeProcThreadAttributeList(attributes, 1, 0, &mut size) } == 0 {
            return Err(last_error("InitializeProcThreadAttributeList"));
        }
        // SAFETY: `inherited` outlives `CreateProcessW`; the list was initialised above.
        let updated = unsafe {
            UpdateProcThreadAttribute(
                attributes,
                0,
                PROC_THREAD_ATTRIBUTE_HANDLE_LIST as usize,
                inherited.as_ptr() as *const c_void,
                std::mem::size_of_val(&inherited),
                std::ptr::null_mut(),
                std::ptr::null(),
            )
        };
        if updated == 0 {
            // SAFETY: the list was initialised above.
            unsafe { DeleteProcThreadAttributeList(attributes) };
            return Err(last_error("UpdateProcThreadAttribute"));
        }

        // SAFETY: an all-zero STARTUPINFOEXW is a valid "no settings" value.
        let mut startup: STARTUPINFOEXW = unsafe { std::mem::zeroed() };
        startup.StartupInfo.cb = std::mem::size_of::<STARTUPINFOEXW>() as u32;
        startup.StartupInfo.cbReserved2 = u16::try_from(block.len()).unwrap_or(u16::MAX);
        startup.StartupInfo.lpReserved2 = block.as_ptr() as *mut u8;
        startup.lpAttributeList = attributes;
        // SAFETY: an all-zero PROCESS_INFORMATION is filled by `CreateProcessW`.
        let mut info: PROCESS_INFORMATION = unsafe { std::mem::zeroed() };
        // SAFETY: every pointer refers to a live, NUL-terminated or sized buffer of this frame;
        // `line` is mutable as `CreateProcessW` requires.
        let created = unsafe {
            CreateProcessW(
                program.as_ptr(),
                line.as_mut_ptr(),
                std::ptr::null(),
                std::ptr::null(),
                1,
                EXTENDED_STARTUPINFO_PRESENT,
                std::ptr::null(),
                std::ptr::null(),
                &startup.StartupInfo,
                &mut info,
            )
        };
        // SAFETY: the list was initialised above and is no longer needed.
        unsafe { DeleteProcThreadAttributeList(attributes) };
        if created == 0 {
            return Err(last_error(&format!(
                "{} did not start",
                executable.display()
            )));
        }
        // holzi keeps only its own ends; the browser's are in the child now.
        drop((child_reads, child_writes));
        // SAFETY: the thread handle is not used.
        unsafe { CloseHandle(info.hThread) };
        let process = Owned(info.hProcess);
        let pid = info.dwProcessId;
        let (report, exit) = watch::channel(None);
        std::thread::Builder::new()
            .name("agent-browser-wait".to_owned())
            .spawn(move || {
                let mut code = 1u32;
                // SAFETY: `process` is a live process handle owned by this thread.
                let success = unsafe {
                    WaitForSingleObject(process.0, INFINITE);
                    GetExitCodeProcess(process.0, &mut code) != 0 && code == 0
                };
                drop(process);
                let _ = report.send(Some(success));
            })
            .map_err(|e| BrowserError::Unavailable(e.to_string()))?;
        Ok((
            Connection::new(holzi_reads, holzi_writes),
            BrowserProcess::new(pid, exit),
        ))
    }
}
