//! Local named-pipe transport (Windows) / placeholder for other platforms.

use std::io;

use crate::error::{PulseError, Result};

/// Full pipe path for Win32 APIs: `\\.\pipe\{name}`.
pub fn pipe_path(pipe_name: &str) -> String {
    format!(r"\\.\pipe\{pipe_name}")
}

#[cfg(windows)]
mod win {
    use super::*;
    use std::fs::File;
    use std::os::windows::io::{FromRawHandle, RawHandle};
    use std::ptr;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;
    use std::thread;
    use std::time::Duration;

    use windows_sys::Win32::Foundation::{
        CloseHandle, LocalFree, ERROR_IO_PENDING, ERROR_PIPE_BUSY, ERROR_PIPE_CONNECTED, FALSE,
        HANDLE, INVALID_HANDLE_VALUE,
    };
    use windows_sys::Win32::Security::Authorization::{
        ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1,
    };
    use windows_sys::Win32::Security::SECURITY_ATTRIBUTES;
    use windows_sys::Win32::Storage::FileSystem::{
        CreateFileW, FILE_ATTRIBUTE_NORMAL, FILE_GENERIC_READ, FILE_GENERIC_WRITE, FILE_SHARE_NONE,
        OPEN_EXISTING, PIPE_ACCESS_DUPLEX,
    };
    use windows_sys::Win32::System::Pipes::{
        ConnectNamedPipe, CreateNamedPipeW, DisconnectNamedPipe, PIPE_READMODE_BYTE,
        PIPE_REJECT_REMOTE_CLIENTS, PIPE_TYPE_BYTE, PIPE_UNLIMITED_INSTANCES, PIPE_WAIT,
    };
    use windows_sys::Win32::System::Threading::GetCurrentProcessId;

    use crate::ipc::rpc::{serve_one, RpcHandler};

    fn wide(s: &str) -> Vec<u16> {
        use std::os::windows::ffi::OsStrExt;
        std::ffi::OsStr::new(s)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect()
    }

    /// Build SECURITY_ATTRIBUTES: current user full control only (SDDL).
    ///
    /// `D:P(A;;GA;;;OW)` — protected DACL, generic-all for owner.
    /// We also try creator-owner style via `O:OWNG:OWND:P(A;;FA;;;OW)`.
    fn owner_only_security_attributes() -> Result<(SECURITY_ATTRIBUTES, *mut std::ffi::c_void)> {
        // SDDL: Owner = current user via OW; DACL allow full access to owner only.
        // FA = File All; OW = Owner Rights.
        let sddl = "D:P(A;;FA;;;OW)";
        let mut sd: *mut core::ffi::c_void = ptr::null_mut();
        let ok = unsafe {
            ConvertStringSecurityDescriptorToSecurityDescriptorW(
                wide(sddl).as_ptr(),
                SDDL_REVISION_1,
                &mut sd,
                ptr::null_mut(),
            )
        };
        if ok == 0 || sd.is_null() {
            return Err(PulseError::Ipc(format!(
                "failed to build pipe security descriptor (err={})",
                io::Error::last_os_error()
            )));
        }
        let sa = SECURITY_ATTRIBUTES {
            nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: sd,
            bInheritHandle: FALSE,
        };
        Ok((sa, sd))
    }

    fn create_pipe_instance(pipe_name: &str, sa: &SECURITY_ATTRIBUTES) -> Result<HANDLE> {
        let path = pipe_path(pipe_name);
        let handle = unsafe {
            CreateNamedPipeW(
                wide(&path).as_ptr(),
                PIPE_ACCESS_DUPLEX,
                PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT | PIPE_REJECT_REMOTE_CLIENTS,
                PIPE_UNLIMITED_INSTANCES,
                64 * 1024,
                64 * 1024,
                0,
                sa as *const _ as *const _,
            )
        };
        if handle == INVALID_HANDLE_VALUE {
            return Err(PulseError::Ipc(format!(
                "CreateNamedPipeW failed: {}",
                io::Error::last_os_error()
            )));
        }
        Ok(handle)
    }

    /// Connect as a client. Retries briefly if the pipe is busy.
    pub fn connect(pipe_name: &str, timeout: Duration) -> Result<File> {
        let path = pipe_path(pipe_name);
        let deadline = std::time::Instant::now() + timeout;
        loop {
            let handle = unsafe {
                CreateFileW(
                    wide(&path).as_ptr(),
                    FILE_GENERIC_READ | FILE_GENERIC_WRITE,
                    FILE_SHARE_NONE,
                    ptr::null(),
                    OPEN_EXISTING,
                    FILE_ATTRIBUTE_NORMAL,
                    ptr::null_mut(),
                )
            };
            if handle != INVALID_HANDLE_VALUE {
                let file = unsafe { File::from_raw_handle(handle as RawHandle) };
                return Ok(file);
            }
            let err = io::Error::last_os_error();
            if err.raw_os_error() == Some(ERROR_PIPE_BUSY as i32)
                && std::time::Instant::now() < deadline
            {
                thread::sleep(Duration::from_millis(20));
                continue;
            }
            if std::time::Instant::now() >= deadline {
                return Err(PulseError::Ipc(format!(
                    "connect to {path} timed out: {err}"
                )));
            }
            return Err(PulseError::Ipc(format!("connect to {path} failed: {err}")));
        }
    }

    /// Accept loop. Spawns a thread per client. Stops when `shutdown` is set.
    ///
    /// Note: unblocking a waiting `ConnectNamedPipe` requires a dummy client connect on stop;
    /// callers should set shutdown and connect once.
    pub fn serve_loop<H>(
        pipe_name: &str,
        handler: Arc<H>,
        shutdown: Arc<AtomicBool>,
    ) -> Result<()>
    where
        H: RpcHandler + 'static,
    {
        let (sa, sd) = owner_only_security_attributes()?;
        // Keep sd alive for the loop; free at end.
        struct SdGuard(*mut std::ffi::c_void);
        impl Drop for SdGuard {
            fn drop(&mut self) {
                if !self.0.is_null() {
                    unsafe {
                        LocalFree(self.0 as _);
                    }
                }
            }
        }
        let _guard = SdGuard(sd);

        while !shutdown.load(Ordering::SeqCst) {
            let handle = create_pipe_instance(pipe_name, &sa)?;
            let connected = unsafe { ConnectNamedPipe(handle, ptr::null_mut()) };
            let err = io::Error::last_os_error();
            let ok = connected != 0
                || err.raw_os_error() == Some(ERROR_PIPE_CONNECTED as i32)
                || err.raw_os_error() == Some(ERROR_IO_PENDING as i32);

            if shutdown.load(Ordering::SeqCst) {
                unsafe {
                    DisconnectNamedPipe(handle);
                    CloseHandle(handle);
                }
                break;
            }

            if !ok {
                unsafe {
                    CloseHandle(handle);
                }
                // Brief pause to avoid busy-spin on unexpected errors.
                thread::sleep(Duration::from_millis(10));
                continue;
            }

            let h = Arc::clone(&handler);
            // HANDLEs are not Send; pass as usize (exclusive ownership of this instance).
            let handle_bits = handle as usize;
            thread::spawn(move || {
                let mut file =
                    unsafe { File::from_raw_handle(handle_bits as RawHandle) };
                // Handle multiple sequential requests on one connection until client closes.
                loop {
                    if let Err(e) = serve_one(h.as_ref(), &mut file) {
                        // Client disconnected or protocol error — drop connection.
                        let _ = e;
                        break;
                    }
                }
                // File drop closes handle.
            });
        }
        Ok(())
    }

    pub fn current_pid() -> u32 {
        unsafe { GetCurrentProcessId() }
    }

    /// Wake a server blocked in ConnectNamedPipe by connecting and dropping.
    pub fn poke(pipe_name: &str) {
        let _ = connect(pipe_name, Duration::from_millis(200));
    }

    /// Verify we can create a named pipe instance with the intended security, then close it.
    /// Used so the service only writes its PID after a successful bind probe.
    pub fn probe_bind(pipe_name: &str) -> Result<()> {
        let (sa, sd) = owner_only_security_attributes()?;
        let handle = create_pipe_instance(pipe_name, &sa);
        unsafe {
            if !sd.is_null() {
                LocalFree(sd as _);
            }
        }
        let handle = handle?;
        unsafe {
            CloseHandle(handle);
        }
        Ok(())
    }
}

#[cfg(windows)]
pub use win::{connect, current_pid, poke, probe_bind, serve_loop};

#[cfg(not(windows))]
pub fn connect(_pipe_name: &str, _timeout: std::time::Duration) -> Result<std::fs::File> {
    Err(PulseError::Ipc(
        "named pipes are only implemented on Windows in v0".into(),
    ))
}

#[cfg(not(windows))]
pub fn serve_loop<H>(
    _pipe_name: &str,
    _handler: std::sync::Arc<H>,
    _shutdown: std::sync::Arc<std::sync::atomic::AtomicBool>,
) -> Result<()>
where
    H: crate::ipc::rpc::RpcHandler + 'static,
{
    Err(PulseError::Ipc(
        "named pipe server is only implemented on Windows in v0".into(),
    ))
}

#[cfg(not(windows))]
pub fn current_pid() -> u32 {
    std::process::id()
}

#[cfg(not(windows))]
pub fn poke(_pipe_name: &str) {}

#[cfg(not(windows))]
pub fn probe_bind(_pipe_name: &str) -> Result<()> {
    Err(PulseError::Ipc(
        "named pipe server is only implemented on Windows in v0".into(),
    ))
}
