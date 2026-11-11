//! Ports `managers/process_manager.py`: name-based process checks, kills and launches.
//!
//! The Python delegates to `psutil`; the private helpers here reproduce what
//! psutil does on Windows (which Win32 calls, which access rights, which error
//! counts as "no such process" and which as "access denied"), because those
//! details decide what the launcher sees.

pub mod tracker;

use std::ffi::c_void;
use std::os::windows::ffi::OsStrExt;
use std::time::Duration;

use windows::Wdk::System::SystemInformation::{NtQuerySystemInformation, SystemProcessInformation};
use windows::Win32::Foundation::{
    CloseHandle, ERROR_ACCESS_DENIED, ERROR_INVALID_PARAMETER, FILETIME, GENERIC_READ,
    GENERIC_WRITE, HANDLE, WAIT_ABANDONED, WAIT_OBJECT_0, WAIT_TIMEOUT,
};
use windows::Win32::Security::SECURITY_ATTRIBUTES;
use windows::Win32::Storage::FileSystem::{
    CreateFileW, FILE_ATTRIBUTE_NORMAL, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
};
use windows::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW, TH32CS_SNAPPROCESS,
};
use windows::Win32::System::ProcessStatus::EnumProcesses;
use windows::Win32::System::Threading::{
    CREATE_NEW_PROCESS_GROUP, CREATE_UNICODE_ENVIRONMENT, CreateProcessW, DETACHED_PROCESS,
    DeleteProcThreadAttributeList, EXTENDED_STARTUPINFO_PRESENT, GetExitCodeProcess,
    GetProcessTimes, InitializeProcThreadAttributeList, LPPROC_THREAD_ATTRIBUTE_LIST, OpenProcess,
    PROC_THREAD_ATTRIBUTE_HANDLE_LIST, PROCESS_ACCESS_RIGHTS, PROCESS_INFORMATION,
    PROCESS_QUERY_INFORMATION, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_SYNCHRONIZE,
    PROCESS_TERMINATE, STARTF_USESTDHANDLES, STARTUPINFOEXW, TerminateProcess,
    UpdateProcThreadAttribute, WaitForSingleObject,
};
use windows::Win32::UI::Shell::ShellExecuteW;
use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
use windows::core::{PCWSTR, PWSTR};

/// `GetExitCodeProcess` value for a process that has not exited.
const STILL_ACTIVE: u32 = 259;

/// Exit code psutil passes to `TerminateProcess` for both `terminate()` and `kill()`.
const SIGTERM: u32 = 15;

pub fn is_process_running(exe_name: &str) -> bool {
    let wanted = exe_name.to_lowercase();
    snapshot().iter().any(|p| p.name.to_lowercase() == wanted)
}

/// Kills every process with this exe name. True if at least one kill went through.
pub fn kill_process(exe_name: &str) -> bool {
    let wanted = exe_name.to_lowercase();
    let mut killed = false;
    for p in snapshot() {
        if p.name.to_lowercase() == wanted && proc_kill(p.pid).is_ok() {
            killed = true;
        }
    }
    killed
}

/// Detached and untracked. True once the process was created and `wait` has passed;
/// Python does not check that it is still alive.
pub fn launch_process(exe_path: &str, wait: Duration) -> bool {
    match spawn_detached(exe_path) {
        Ok(_) => {
            std::thread::sleep(wait);
            true
        }
        Err(_) => false,
    }
}

/// `os.startfile(target)`: `ShellExecuteW` with no verb (the default verb, normally
/// "open"), no parameters, no working directory, `SW_SHOWNORMAL`.
pub fn shell_open(target: &str) -> std::io::Result<()> {
    let file = wide(target);
    // SAFETY: `file` is NUL-terminated and outlives the call.
    let rc = unsafe {
        ShellExecuteW(
            None,
            PCWSTR::null(),
            PCWSTR(file.as_ptr()),
            PCWSTR::null(),
            PCWSTR::null(),
            SW_SHOWNORMAL,
        )
    };
    if rc.0 as usize > 32 {
        return Ok(());
    }
    // Python raises from GetLastError(); fall back to the return code when it is unset.
    let err = std::io::Error::last_os_error();
    match err.raw_os_error() {
        Some(0) | None => Err(std::io::Error::from_raw_os_error(rc.0 as i32)),
        Some(_) => Err(err),
    }
}

// ---------------------------------------------------------------------------
// psutil-equivalent primitives, shared with `tracker`
// ---------------------------------------------------------------------------

/// The two psutil exceptions the Python catches.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ProcError {
    NoSuchProcess,
    AccessDenied,
}

/// One row of a process snapshot.
#[derive(Debug, Clone)]
pub(crate) struct ProcEntry {
    pub pid: u32,
    pub ppid: u32,
    pub name: String,
}

/// Owns a Win32 handle and closes it on drop.
struct Handle(HANDLE);

impl Drop for Handle {
    fn drop(&mut self) {
        // SAFETY: the handle was returned open by Win32 and is closed exactly once.
        let _ = unsafe { CloseHandle(self.0) };
    }
}

fn wide(s: &str) -> Vec<u16> {
    std::ffi::OsStr::new(s)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect()
}

fn open_process(access: PROCESS_ACCESS_RIGHTS, pid: u32) -> windows::core::Result<Handle> {
    // SAFETY: plain call; the returned handle is wrapped immediately.
    unsafe { OpenProcess(access, false, pid) }.map(Handle)
}

fn is_code(err: &windows::core::Error, code: windows::Win32::Foundation::WIN32_ERROR) -> bool {
    err.code() == code.to_hresult()
}

/// Every running process with its parent PID and exe name (Toolhelp, as psutil's `ppid_map`).
pub(crate) fn snapshot() -> Vec<ProcEntry> {
    let mut out = Vec::new();
    // SAFETY: plain call; the handle is wrapped immediately.
    let Ok(snap) = (unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) }).map(Handle) else {
        return out;
    };
    let mut entry = PROCESSENTRY32W {
        dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32,
        ..Default::default()
    };
    // SAFETY: `entry` is a valid PROCESSENTRY32W with dwSize set.
    let mut ok = unsafe { Process32FirstW(snap.0, &mut entry) }.is_ok();
    while ok {
        let len = entry
            .szExeFile
            .iter()
            .position(|&c| c == 0)
            .unwrap_or(entry.szExeFile.len());
        out.push(ProcEntry {
            pid: entry.th32ProcessID,
            ppid: entry.th32ParentProcessID,
            name: String::from_utf16_lossy(&entry.szExeFile[..len]),
        });
        // SAFETY: as above.
        ok = unsafe { Process32NextW(snap.0, &mut entry) }.is_ok();
    }
    out
}

/// PIDs of every process named `exe_name`, case-insensitive (`_pids_by_exe`).
pub(crate) fn pids_by_exe(exe_name: &str) -> Vec<u32> {
    let wanted = exe_name.to_lowercase();
    snapshot()
        .into_iter()
        .filter(|p| p.name.to_lowercase() == wanted)
        .map(|p| p.pid)
        .collect()
}

/// psutil's `psutil_pid_in_pids`: EnumProcesses lists the PID.
fn pid_in_pids(pid: u32) -> bool {
    let mut buf = vec![0u32; 1024];
    loop {
        let mut needed = 0u32;
        let bytes = (buf.len() * 4) as u32;
        // SAFETY: `buf` holds `bytes` bytes.
        if unsafe { EnumProcesses(buf.as_mut_ptr(), bytes, &mut needed) }.is_err() {
            return false;
        }
        if needed < bytes {
            return buf[..needed as usize / 4].contains(&pid);
        }
        buf.resize(buf.len() * 2, 0);
    }
}

/// FILETIME ticks to epoch seconds, as psutil's `psutil_FiletimeToUnixTime`.
fn filetime_ticks_to_epoch(ticks: u64) -> f64 {
    ticks.wrapping_sub(116_444_736_000_000_000) as f64 / 10_000_000.0
}

/// `psutil.Process(pid).create_time()` on Windows: `GetProcessTimes` through a
/// query-limited handle, after psutil's "is it still running" check; on access
/// denied, the slower `NtQuerySystemInformation` fallback, which works for every
/// process. A missing PID is `NoSuchProcess`.
pub(crate) fn create_time(pid: u32) -> Result<f64, ProcError> {
    let h = match open_process(PROCESS_QUERY_LIMITED_INFORMATION, pid) {
        Ok(h) => h,
        Err(e) if is_code(&e, ERROR_INVALID_PARAMETER) => return Err(ProcError::NoSuchProcess),
        Err(_) => return nt_create_time(pid),
    };
    let mut code = 0u32;
    // SAFETY: `h` is an open process handle.
    if unsafe { GetExitCodeProcess(h.0, &mut code) }.is_ok()
        && code != STILL_ACTIVE
        && !pid_in_pids(pid)
    {
        return Err(ProcError::NoSuchProcess);
    }
    let (mut created, mut exited, mut kernel, mut user) = (
        FILETIME::default(),
        FILETIME::default(),
        FILETIME::default(),
        FILETIME::default(),
    );
    // SAFETY: `h` is open; the four out-pointers are valid.
    match unsafe { GetProcessTimes(h.0, &mut created, &mut exited, &mut kernel, &mut user) } {
        Ok(()) => Ok(filetime_ticks_to_epoch(
            (u64::from(created.dwHighDateTime) << 32) | u64::from(created.dwLowDateTime),
        )),
        Err(_) => nt_create_time(pid),
    }
}

/// The leading fields of `SYSTEM_PROCESS_INFORMATION` (ntdll's layout; the
/// public header hides `CreateTime` inside a reserved block).
#[repr(C)]
#[derive(Clone, Copy)]
struct SpiHead {
    next_entry_offset: u32,
    number_of_threads: u32,
    working_set_private_size: i64,
    hard_fault_count: u32,
    number_of_threads_high_watermark: u32,
    cycle_time: u64,
    create_time: i64,
    user_time: i64,
    kernel_time: i64,
    image_name_length: u16,
    image_name_maximum_length: u16,
    image_name_buffer: *mut u16,
    base_priority: i32,
    unique_process_id: usize,
}

/// psutil's `_proc_info` fallback for `create_time`.
fn nt_create_time(pid: u32) -> Result<f64, ProcError> {
    let mut buf: Vec<u64> = vec![0; 64 * 1024];
    loop {
        let bytes = (buf.len() * 8) as u32;
        let mut needed = 0u32;
        // SAFETY: `buf` holds `bytes` bytes, 8-byte aligned.
        let status = unsafe {
            NtQuerySystemInformation(
                SystemProcessInformation,
                buf.as_mut_ptr().cast::<c_void>(),
                bytes,
                &mut needed,
            )
        };
        // STATUS_INFO_LENGTH_MISMATCH: the process list grew; retry larger.
        if status.0 == 0xC000_0004_u32 as i32 {
            buf.resize((needed as usize / 8 + 1).max(buf.len() * 2), 0);
            continue;
        }
        if status.is_err() {
            return Err(ProcError::AccessDenied);
        }
        let base = buf.as_ptr().cast::<u8>();
        let mut offset = 0usize;
        loop {
            if offset + std::mem::size_of::<SpiHead>() > bytes as usize {
                return Err(ProcError::NoSuchProcess);
            }
            // SAFETY: the entry lies inside `buf`, checked just above.
            let head = unsafe { std::ptr::read_unaligned(base.add(offset).cast::<SpiHead>()) };
            if head.unique_process_id == pid as usize {
                return Ok(filetime_ticks_to_epoch(head.create_time as u64));
            }
            if head.next_entry_offset == 0 {
                return Err(ProcError::NoSuchProcess);
            }
            offset += head.next_entry_offset as usize;
        }
    }
}

/// psutil's `proc_kill`: `TerminateProcess(pid, SIGTERM)`. Access denied from
/// `TerminateProcess` itself counts as success (the process already died).
pub(crate) fn proc_kill(pid: u32) -> Result<(), ProcError> {
    if pid == 0 {
        return Err(ProcError::AccessDenied);
    }
    let h = match open_process(PROCESS_TERMINATE, pid) {
        Ok(h) => h,
        Err(e) if is_code(&e, ERROR_INVALID_PARAMETER) => return Err(ProcError::NoSuchProcess),
        Err(_) => return Err(ProcError::AccessDenied),
    };
    // SAFETY: `h` is an open handle with PROCESS_TERMINATE.
    match unsafe { TerminateProcess(h.0, SIGTERM) } {
        Ok(()) => Ok(()),
        Err(e) if is_code(&e, ERROR_ACCESS_DENIED) => Ok(()),
        Err(_) => Err(ProcError::AccessDenied),
    }
}

/// Outcome of psutil's `proc_wait`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum WaitOutcome {
    Exited,
    TimedOut,
}

/// psutil's `proc_wait`: wait on a `SYNCHRONIZE | PROCESS_QUERY_INFORMATION`
/// handle. A PID that is already gone counts as exited; any other failure to open
/// is access denied (some helpers refuse SYNCHRONIZE).
pub(crate) fn proc_wait(pid: u32, timeout: Duration) -> Result<WaitOutcome, ProcError> {
    let h = match open_process(PROCESS_SYNCHRONIZE | PROCESS_QUERY_INFORMATION, pid) {
        Ok(h) => h,
        Err(e) if is_code(&e, ERROR_INVALID_PARAMETER) => return Ok(WaitOutcome::Exited),
        Err(_) => return Err(ProcError::AccessDenied),
    };
    let ms = u32::try_from(timeout.as_millis()).unwrap_or(u32::MAX - 1);
    // SAFETY: `h` is an open handle with SYNCHRONIZE.
    let r = unsafe { WaitForSingleObject(h.0, ms) };
    if r == WAIT_OBJECT_0 || r == WAIT_ABANDONED {
        Ok(WaitOutcome::Exited)
    } else if r == WAIT_TIMEOUT {
        Ok(WaitOutcome::TimedOut)
    } else {
        Err(ProcError::AccessDenied)
    }
}

/// `subprocess.list2cmdline` for one argument.
fn list2cmdline(arg: &str) -> String {
    let mut out = String::new();
    let need_quote = arg.is_empty() || arg.contains(' ') || arg.contains('\t');
    if need_quote {
        out.push('"');
    }
    let mut backslashes = 0usize;
    for c in arg.chars() {
        match c {
            '\\' => backslashes += 1,
            '"' => {
                out.push_str(&"\\".repeat(backslashes * 2));
                backslashes = 0;
                out.push_str("\\\"");
            }
            _ => {
                out.push_str(&"\\".repeat(backslashes));
                backslashes = 0;
                out.push(c);
            }
        }
    }
    out.push_str(&"\\".repeat(backslashes));
    if need_quote {
        out.push_str(&"\\".repeat(backslashes));
        out.push('"');
    }
    out
}

/// Owns an initialised `PROC_THREAD_ATTRIBUTE_LIST`.
struct AttributeList {
    buf: Vec<u64>,
}

impl AttributeList {
    fn ptr(&mut self) -> LPPROC_THREAD_ATTRIBUTE_LIST {
        LPPROC_THREAD_ATTRIBUTE_LIST(self.buf.as_mut_ptr().cast())
    }
}

impl Drop for AttributeList {
    fn drop(&mut self) {
        // SAFETY: the list was initialised by InitializeProcThreadAttributeList.
        unsafe { DeleteProcThreadAttributeList(self.ptr()) };
    }
}

/// `subprocess.Popen([exe_path], close_fds=True, stdin/stdout/stderr=DEVNULL,
/// creationflags=DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP)`: no application
/// name, the command line built by `list2cmdline`, inherited environment and
/// working directory, and only the NUL handle inherited (Python's handle list).
/// Returns the new PID.
pub(crate) fn spawn_detached(exe_path: &str) -> std::io::Result<u32> {
    let sa = SECURITY_ATTRIBUTES {
        nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
        lpSecurityDescriptor: std::ptr::null_mut(),
        bInheritHandle: true.into(),
    };
    let nul_name = wide("NUL");
    // SAFETY: `nul_name` is NUL-terminated and `sa` outlives the call.
    let nul = unsafe {
        CreateFileW(
            PCWSTR(nul_name.as_ptr()),
            (GENERIC_READ | GENERIC_WRITE).0,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            Some(&sa),
            OPEN_EXISTING,
            FILE_ATTRIBUTE_NORMAL,
            None,
        )
    }
    .map(Handle)?;

    let mut size = 0usize;
    // SAFETY: size query; fails by design with ERROR_INSUFFICIENT_BUFFER.
    let _ = unsafe { InitializeProcThreadAttributeList(None, 1, None, &mut size) };
    let mut list = AttributeList {
        buf: vec![0; size.div_ceil(8)],
    };
    // SAFETY: `list.buf` holds `size` bytes.
    unsafe { InitializeProcThreadAttributeList(Some(list.ptr()), 1, None, &mut size) }?;
    let handles = [nul.0];
    // SAFETY: `handles` outlives CreateProcessW below; the list is initialised.
    unsafe {
        UpdateProcThreadAttribute(
            list.ptr(),
            0,
            PROC_THREAD_ATTRIBUTE_HANDLE_LIST as usize,
            Some(handles.as_ptr().cast()),
            std::mem::size_of_val(&handles),
            None,
            None,
        )
    }?;

    let mut si = STARTUPINFOEXW::default();
    si.StartupInfo.cb = std::mem::size_of::<STARTUPINFOEXW>() as u32;
    si.StartupInfo.dwFlags = STARTF_USESTDHANDLES;
    si.StartupInfo.hStdInput = nul.0;
    si.StartupInfo.hStdOutput = nul.0;
    si.StartupInfo.hStdError = nul.0;
    si.lpAttributeList = list.ptr();

    let mut cmdline = wide(&list2cmdline(exe_path));
    let mut pi = PROCESS_INFORMATION::default();
    // SAFETY: every pointer is valid for the duration of the call; `cmdline` is
    // writable as CreateProcessW requires.
    unsafe {
        CreateProcessW(
            PCWSTR::null(),
            Some(PWSTR(cmdline.as_mut_ptr())),
            None,
            None,
            true,
            DETACHED_PROCESS
                | CREATE_NEW_PROCESS_GROUP
                | EXTENDED_STARTUPINFO_PRESENT
                | CREATE_UNICODE_ENVIRONMENT,
            None,
            PCWSTR::null(),
            &si.StartupInfo,
            &mut pi,
        )
    }?;
    drop(Handle(pi.hThread));
    drop(Handle(pi.hProcess));
    Ok(pi.dwProcessId)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn list2cmdline_matches_python() {
        // Expected values from Python's subprocess.list2cmdline([arg]).
        assert_eq!(list2cmdline(r"C:\Apps\app.exe"), r"C:\Apps\app.exe");
        assert_eq!(
            list2cmdline(r"C:\Program Files\App\app.exe"),
            r#""C:\Program Files\App\app.exe""#
        );
        assert_eq!(list2cmdline(""), r#""""#);
        assert_eq!(list2cmdline(r"C:\a b\"), r#""C:\a b\\""#);
        assert_eq!(list2cmdline(r#"a\"b"#), r#"a\\\"b"#);
        assert_eq!(list2cmdline("a\tb"), "\"a\tb\"");
    }

    #[test]
    fn filetime_conversion_is_epoch_seconds() {
        assert_eq!(filetime_ticks_to_epoch(116_444_736_000_000_000), 0.0);
        assert_eq!(filetime_ticks_to_epoch(116_444_736_015_000_000), 1.5);
    }

    #[test]
    fn create_time_of_self_matches_both_sources() {
        let pid = std::process::id();
        let fast = create_time(pid).unwrap();
        let slow = nt_create_time(pid).unwrap();
        assert_eq!(fast, slow);
    }

    #[test]
    fn missing_pid_is_no_such_process() {
        // Far above any PID Windows hands out.
        assert_eq!(create_time(0xFFFF_FFF1), Err(ProcError::NoSuchProcess));
        assert_eq!(nt_create_time(0xFFFF_FFF1), Err(ProcError::NoSuchProcess));
    }
}
