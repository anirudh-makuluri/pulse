//! Service PID file helpers.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde::{Deserialize, Serialize};

use crate::error::{PulseError, Result};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ServicePidFile {
    pub pid: u32,
    pub started_at: String,
    pub exe_path: String,
    pub pipe_name: String,
}

/// Atomically write the PID file (temp + rename).
pub fn write_pid_file(path: &Path, info: &ServicePidFile) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension("pid.tmp");
    let text = serde_json::to_string_pretty(info)
        .map_err(|e| PulseError::Ipc(format!("pid serialize: {e}")))?;
    fs::write(&tmp, text)?;
    fs::rename(&tmp, path)?;
    Ok(())
}

pub fn read_pid_file(path: &Path) -> Result<Option<ServicePidFile>> {
    if !path.exists() {
        return Ok(None);
    }
    let text = fs::read_to_string(path)?;
    let info: ServicePidFile = serde_json::from_str(&text)
        .map_err(|e| PulseError::Ipc(format!("pid parse: {e}")))?;
    Ok(Some(info))
}

/// Remove PID file only if it matches this process (pid + optional exe).
pub fn remove_pid_file_if_matches(path: &Path, pid: u32, exe_path: Option<&str>) -> Result<bool> {
    match read_pid_file(path)? {
        None => Ok(false),
        Some(info) => {
            let exe_ok = exe_path
                .map(|e| paths_equal(&info.exe_path, e))
                .unwrap_or(true);
            if info.pid == pid && exe_ok {
                let _ = fs::remove_file(path);
                Ok(true)
            } else {
                Ok(false)
            }
        }
    }
}

fn paths_equal(a: &str, b: &str) -> bool {
    let na = a.replace('/', "\\").to_ascii_lowercase();
    let nb = b.replace('/', "\\").to_ascii_lowercase();
    na == nb
}

/// Returns true only when the live process still matches the executable that
/// wrote the PID file. This guards against PID reuse before terminating a
/// service that stopped serving its IPC pipe.
pub fn service_process_matches(info: &ServicePidFile) -> bool {
    if !process_is_live(info.pid) {
        return false;
    }
    #[cfg(windows)]
    {
        process_executable_path(info.pid)
            .map(|path| paths_equal(&path.to_string_lossy(), &info.exe_path))
            .unwrap_or(false)
    }
    #[cfg(not(windows))]
    {
        true
    }
}

/// Force-stop a verified unhealthy service. Callers must first confirm that
/// its IPC endpoint is unavailable; healthy services should shut down through
/// their normal RPC instead.
pub fn terminate_service_process(info: &ServicePidFile) -> Result<()> {
    if !service_process_matches(info) {
        return Err(PulseError::Ipc(format!(
            "refusing to terminate pid {} because it no longer matches the Pulse service PID file",
            info.pid
        )));
    }

    #[cfg(windows)]
    let status = Command::new("taskkill")
        .args(["/PID", &info.pid.to_string(), "/F"])
        .status()
        .map_err(|error| PulseError::Ipc(format!("terminate Pulse service: {error}")))?;
    #[cfg(not(windows))]
    let status = Command::new("kill")
        .args(["-TERM", &info.pid.to_string()])
        .status()
        .map_err(|error| PulseError::Ipc(format!("terminate Pulse service: {error}")))?;

    if status.success() {
        Ok(())
    } else {
        Err(PulseError::Ipc(format!(
            "terminate Pulse service exited with {status}"
        )))
    }
}

/// Returns true if a process with this PID appears to be running.
pub fn process_is_live(pid: u32) -> bool {
    #[cfg(windows)]
    {
        windows_process_live(pid)
    }
    #[cfg(not(windows))]
    {
        // Best-effort: signal 0.
        unsafe { libc_kill(pid as i32) }
    }
}

#[cfg(windows)]
fn windows_process_live(pid: u32) -> bool {
    use windows_sys::Win32::Foundation::{CloseHandle, WAIT_TIMEOUT};
    use windows_sys::Win32::System::Threading::{
        OpenProcess, WaitForSingleObject, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_SYNCHRONIZE,
    };

    unsafe {
        let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_SYNCHRONIZE, 0, pid);
        if handle.is_null() || handle == windows_sys::Win32::Foundation::INVALID_HANDLE_VALUE {
            return false;
        }
        let live = WaitForSingleObject(handle, 0) == WAIT_TIMEOUT;
        CloseHandle(handle);
        live
    }
}

#[cfg(windows)]
fn process_executable_path(pid: u32) -> Option<PathBuf> {
    use std::ffi::OsString;
    use std::os::windows::ffi::OsStringExt;
    use windows_sys::Win32::Foundation::CloseHandle;
    use windows_sys::Win32::System::Threading::{
        OpenProcess, QueryFullProcessImageNameW, PROCESS_QUERY_LIMITED_INFORMATION,
    };

    unsafe {
        let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if handle.is_null() || handle == windows_sys::Win32::Foundation::INVALID_HANDLE_VALUE {
            return None;
        }
        let mut buffer = vec![0u16; 32_768];
        let mut len = buffer.len() as u32;
        let ok = QueryFullProcessImageNameW(handle, 0, buffer.as_mut_ptr(), &mut len);
        CloseHandle(handle);
        if ok == 0 {
            return None;
        }
        buffer.truncate(len as usize);
        Some(PathBuf::from(OsString::from_wide(&buffer)))
    }
}

#[cfg(not(windows))]
fn libc_kill(pid: i32) -> bool {
    // Avoid libc dep: try reading /proc or just assume unknown.
    std::path::Path::new(&format!("/proc/{pid}")).exists()
}

/// Read PID file and return it only if the process is still live.
pub fn live_service_pid(path: &Path) -> Result<Option<ServicePidFile>> {
    match read_pid_file(path)? {
        None => Ok(None),
        Some(info) => {
            if process_is_live(info.pid) {
                Ok(Some(info))
            } else {
                // Stale — clean up.
                let _ = fs::remove_file(path);
                Ok(None)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn write_read_roundtrip() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("service.pid");
        let info = ServicePidFile {
            pid: 42,
            started_at: "2026-07-21T00:00:00Z".into(),
            exe_path: r"C:\pulse\pulse-service.exe".into(),
            pipe_name: "pulse-service".into(),
        };
        write_pid_file(&path, &info).unwrap();
        let got = read_pid_file(&path).unwrap().unwrap();
        assert_eq!(got, info);
    }

    #[test]
    fn path_comparison_is_case_and_separator_insensitive() {
        assert!(paths_equal(
            r"C:\Pulse\pulse-service.exe",
            "c:/pulse/PULSE-SERVICE.EXE"
        ));
    }
}
