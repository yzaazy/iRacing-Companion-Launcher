//! psutil-equivalent primitives on Win32, shared by the name-based functions
//! in [`super`] and by `tracker`: process snapshots, creation time and its
//! `NtQuerySystemInformation` fallback, open, wait, terminate, and the detached
//! spawn with its attribute list.

use std::ffi::c_void;
use std::time::Duration;

use windows::Wdk::System::SystemInformation::{NtQuerySystemInformation, SystemProcessInformation};
use windows::Win32::Foundation::{
    ERROR_ACCESS_DENIED, ERROR_INVALID_PARAMETER, FILETIME, GENERIC_READ, GENERIC_WRITE,
    STATUS_INFO_LENGTH_MISMATCH, WAIT_ABANDONED, WAIT_OBJECT_0, WAIT_TIMEOUT,
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
use windows::core::{PCWSTR, PWSTR};

use crate::win::{Handle, wide};

/// `GetExitCodeProcess` value for a process that has not exited.
const STILL_ACTIVE: u32 = 259;

/// Exit code psutil passes to `TerminateProcess` for both `terminate()` and `kill()`.
const SIGTERM: u32 = 15;

/// FILETIME ticks (100 ns since 1601-01-01) at the Unix epoch, 1970-01-01.
const UNIX_EPOCH_IN_FILETIME_TICKS: u64 = 116_444_736_000_000_000;

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

fn open_process(access: PROCESS_ACCESS_RIGHTS, pid: u32) -> windows::core::Result<Handle> {
    // SAFETY: OpenProcess takes only values and returns a new handle or an
    // error; a returned handle is owned by `Handle`, which closes it once.
    unsafe { OpenProcess(access, false, pid) }.map(Handle)
}

fn is_code(err: &windows::core::Error, code: windows::Win32::Foundation::WIN32_ERROR) -> bool {
    err.code() == code.to_hresult()
}

/// Every running process with its parent PID and exe name (Toolhelp, as psutil's `ppid_map`).
pub(crate) fn snapshot() -> Vec<ProcEntry> {
    let mut out = Vec::new();
    // SAFETY: CreateToolhelp32Snapshot takes only values and returns a new
    // handle or an error; a returned handle is owned by `Handle`, which closes it once.
    let Ok(snap) = (unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) }).map(Handle) else {
        return out;
    };
    let mut entry = PROCESSENTRY32W {
        dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32,
        ..Default::default()
    };
    // SAFETY: `snap` is an open snapshot handle; `entry` is a live, writable
    // PROCESSENTRY32W whose dwSize is its size, as the call requires.
    let mut ok = unsafe { Process32FirstW(snap.0, &raw mut entry) }.is_ok();
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
        // SAFETY: `snap` is still open; `entry` is the same live PROCESSENTRY32W,
        // its dwSize left as the call requires.
        ok = unsafe { Process32NextW(snap.0, &raw mut entry) }.is_ok();
    }
    out
}

/// psutil's `psutil_pid_in_pids`: EnumProcesses lists the PID.
fn pid_in_pids(pid: u32) -> bool {
    let mut buf = vec![0u32; 1024];
    loop {
        let mut needed = 0u32;
        let bytes = (buf.len() * 4) as u32;
        // SAFETY: `buf` is a live, writable buffer of `bytes` bytes and `needed`
        // a live u32; the call writes within both.
        if unsafe { EnumProcesses(buf.as_mut_ptr(), bytes, &raw mut needed) }.is_err() {
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
    ticks.wrapping_sub(UNIX_EPOCH_IN_FILETIME_TICKS) as f64 / 10_000_000.0
}

/// `psutil.Process(pid).create_time()` on Windows: `GetProcessTimes` through a
/// query-limited handle, after psutil's "is it still running" check; on access
/// denied, the slower `NtQuerySystemInformation` fallback, which works for every
/// process. A missing PID is `NoSuchProcess`.
pub(crate) fn create_time(pid: u32) -> Result<f64, ProcError> {
    match fast_create_time(pid) {
        Err(ProcError::AccessDenied) => nt_create_time(pid),
        other => other,
    }
}

/// `create_time(fast_only=True)`: the `GetProcessTimes` path alone, access
/// denied where it cannot open the process.
fn fast_create_time(pid: u32) -> Result<f64, ProcError> {
    let h = match open_process(PROCESS_QUERY_LIMITED_INFORMATION, pid) {
        Ok(h) => h,
        Err(e) if is_code(&e, ERROR_INVALID_PARAMETER) => return Err(ProcError::NoSuchProcess),
        Err(_) => return Err(ProcError::AccessDenied),
    };
    let mut code = 0u32;
    // SAFETY: `h` is an open process handle with query-limited access, which
    // the call needs; `code` is a live u32 for the result.
    if unsafe { GetExitCodeProcess(h.0, &raw mut code) }.is_ok()
        && code != STILL_ACTIVE
        && !pid_in_pids(pid)
    {
        return Err(ProcError::NoSuchProcess);
    }
    handle_create_time(&h).ok_or(ProcError::AccessDenied)
}

/// Creation time through a handle opened with at least query-limited access.
fn handle_create_time(h: &Handle) -> Option<f64> {
    let (mut created, mut exited, mut kernel, mut user) = (
        FILETIME::default(),
        FILETIME::default(),
        FILETIME::default(),
        FILETIME::default(),
    );
    // SAFETY: `h` is an open process handle with at least query-limited access;
    // the four out-pointers point to live, writable FILETIMEs.
    unsafe {
        GetProcessTimes(
            h.0,
            &raw mut created,
            &raw mut exited,
            &raw mut kernel,
            &raw mut user,
        )
    }
    .ok()?;
    Some(filetime_ticks_to_epoch(
        (u64::from(created.dwHighDateTime) << 32) | u64::from(created.dwLowDateTime),
    ))
}

/// psutil's `Process._ident` on Windows: the fast creation time, `None` where
/// access is denied. `NoSuchProcess` when the PID is gone.
pub(crate) fn identity(pid: u32) -> Result<Option<f64>, ProcError> {
    match fast_create_time(pid) {
        Ok(created) => Ok(Some(created)),
        Err(ProcError::AccessDenied) => Ok(None),
        Err(ProcError::NoSuchProcess) => Err(ProcError::NoSuchProcess),
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
        // SAFETY: `buf` is a live, writable buffer of `bytes` bytes, 8-byte
        // aligned as SYSTEM_PROCESS_INFORMATION needs, and `needed` a live u32;
        // the kernel writes within both.
        let status = unsafe {
            NtQuerySystemInformation(
                SystemProcessInformation,
                buf.as_mut_ptr().cast::<c_void>(),
                bytes,
                &raw mut needed,
            )
        };
        // The process list grew; retry larger.
        if status == STATUS_INFO_LENGTH_MISMATCH {
            buf.resize((needed as usize / 8 + 1).max(buf.len() * 2), 0);
            continue;
        }
        if status.is_err() {
            return Err(ProcError::AccessDenied);
        }
        // On success the kernel reports how many bytes it wrote; no entry lies beyond them.
        let written = (needed as usize).min(bytes as usize);
        let base = buf.as_ptr().cast::<u8>();
        let mut offset = 0usize;
        loop {
            if offset + std::mem::size_of::<SpiHead>() > written {
                return Err(ProcError::NoSuchProcess);
            }
            // SAFETY: the entry lies inside the bytes the kernel wrote, which lie
            // inside `buf`, checked just above; `read_unaligned` needs no alignment.
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
    terminate(&h)
}

/// `Process.kill()` for a process whose identity was taken when it was listed:
/// psutil's `_raise_if_pid_reused` compares a fresh identity with it and
/// treats a mismatch as `NoSuchProcess`. Here the check and the kill go
/// through one handle, so the PID cannot change hands between them.
pub(crate) fn kill_if_same(pid: u32, identity: Option<f64>) -> Result<(), ProcError> {
    if pid == 0 {
        return Err(ProcError::AccessDenied);
    }
    match open_process(PROCESS_TERMINATE | PROCESS_QUERY_LIMITED_INFORMATION, pid) {
        Ok(h) => {
            if handle_create_time(&h) != identity {
                return Err(ProcError::NoSuchProcess);
            }
            terminate(&h)
        }
        Err(e) if is_code(&e, ERROR_INVALID_PARAMETER) => Err(ProcError::NoSuchProcess),
        // Refused: psutil's fresh identity is then `None` as well, so only a
        // process that was unreadable when listed matches and gets its kill
        // attempt (which normally ends in access denied too).
        Err(_) if identity.is_none() => proc_kill(pid),
        Err(_) => Err(ProcError::NoSuchProcess),
    }
}

/// psutil's `proc_kill` on an open handle: `TerminateProcess(SIGTERM)`, where
/// access denied counts as success (the process already died).
fn terminate(h: &Handle) -> Result<(), ProcError> {
    // SAFETY: `h` is an open process handle, opened with PROCESS_TERMINATE by
    // both callers; the call takes only values besides it.
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
    // SAFETY: `h` is an open process handle with SYNCHRONIZE, which the wait
    // needs; the call takes only values besides it.
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

/// Owns an initialised `PROC_THREAD_ATTRIBUTE_LIST`. A value exists only once
/// `InitializeProcThreadAttributeList` has succeeded, so `Drop` never deletes a
/// list that was not initialised.
struct AttributeList {
    buf: Vec<u64>,
}

impl AttributeList {
    /// Sizes and initialises a list with room for `count` attributes.
    fn new(count: u32) -> std::io::Result<Self> {
        let mut size = 0usize;
        // SAFETY: a size query: with no list the call only writes the required
        // size to `size`, a live usize. It fails by design with
        // ERROR_INSUFFICIENT_BUFFER.
        let _ = unsafe { InitializeProcThreadAttributeList(None, count, None, &raw mut size) };
        // At least one element, so the pointer below is never dangling.
        let mut buf = vec![0u64; size.div_ceil(8).max(1)];
        let list = LPPROC_THREAD_ATTRIBUTE_LIST(buf.as_mut_ptr().cast());
        // SAFETY: `list` points into `buf`, which holds at least `size` bytes,
        // is 8-byte aligned and outlives the call; `size` is a live usize.
        unsafe { InitializeProcThreadAttributeList(Some(list), count, None, &raw mut size) }?;
        Ok(Self { buf })
    }

    fn ptr(&mut self) -> LPPROC_THREAD_ATTRIBUTE_LIST {
        LPPROC_THREAD_ATTRIBUTE_LIST(self.buf.as_mut_ptr().cast())
    }
}

impl Drop for AttributeList {
    fn drop(&mut self) {
        // SAFETY: a value exists only once InitializeProcThreadAttributeList
        // succeeded on `buf`, which is still alive; the list is deleted once, here.
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
    // SAFETY: `nul_name` is a NUL-terminated UTF-16 string and `sa` a live
    // SECURITY_ATTRIBUTES with nLength set, both alive for the whole call; a
    // returned handle is owned by `Handle`, which closes it once.
    let nul = unsafe {
        CreateFileW(
            PCWSTR(nul_name.as_ptr()),
            (GENERIC_READ | GENERIC_WRITE).0,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            Some(&raw const sa),
            OPEN_EXISTING,
            FILE_ATTRIBUTE_NORMAL,
            None,
        )
    }
    .map(Handle)?;

    let mut list = AttributeList::new(1)?;
    let handles = [nul.0];
    // SAFETY: the list is initialised (an `AttributeList` exists only then);
    // the attribute keeps a pointer to `handles`, `size_of_val(&handles)` bytes,
    // which stays alive until after CreateProcessW below has read it.
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

    let mut cmdline = wide(list2cmdline(exe_path));
    let mut pi = PROCESS_INFORMATION::default();
    // SAFETY: `cmdline` is a live, writable, NUL-terminated UTF-16 buffer, as
    // CreateProcessW requires; `si` is a live STARTUPINFOEXW with cb set to
    // its size and a live attribute list and NUL handle, passed through its
    // first field as EXTENDED_STARTUPINFO_PRESENT expects; `pi` is a live
    // PROCESS_INFORMATION for the result, whose two handles `Handle` closes
    // right after.
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
            &raw const si.StartupInfo,
            &raw mut pi,
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
    fn attribute_list_exists_only_when_initialised() {
        // The size spawn_detached asks for: initialised, then deleted on drop.
        let mut list = AttributeList::new(1).unwrap();
        assert!(!list.buf.is_empty());
        assert!(!list.ptr().0.is_null());
        drop(list);
        // A count the API refuses: the second call fails, no value is built,
        // so there is nothing for Drop to delete.
        assert!(AttributeList::new(u32::MAX).is_err());
    }

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
        assert_eq!(filetime_ticks_to_epoch(UNIX_EPOCH_IN_FILETIME_TICKS), 0.0);
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

    /// A dummy child (`ping`), killed by its own PID and never by name.
    struct Dummy(std::process::Child);

    impl Dummy {
        fn spawn() -> Self {
            let child = std::process::Command::new("ping")
                .args(["-n", "60", "127.0.0.1"])
                .stdout(std::process::Stdio::null())
                .spawn()
                .unwrap();
            Self(child)
        }

        fn pid(&self) -> u32 {
            self.0.id()
        }

        fn alive(&mut self) -> bool {
            self.0.try_wait().unwrap().is_none()
        }

        fn exited_within(&mut self, timeout: Duration) -> bool {
            let start = std::time::Instant::now();
            while start.elapsed() < timeout {
                if !self.alive() {
                    return true;
                }
                std::thread::sleep(Duration::from_millis(10));
            }
            false
        }
    }

    impl Drop for Dummy {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }

    #[test]
    fn kill_spares_a_pid_that_now_belongs_to_another_process() {
        let mut dummy = Dummy::spawn();
        let listed = identity(dummy.pid()).unwrap();
        assert!(listed.is_some(), "our own child is readable");

        // The identity of an earlier holder of this PID: what kill_process
        // holds when the listed process exited and the PID was handed on.
        let earlier = listed.map(|t| t - 1.0);
        assert_eq!(
            kill_if_same(dummy.pid(), earlier),
            Err(ProcError::NoSuchProcess)
        );
        // Unreadable when listed, readable now: psutil's identities differ too.
        assert_eq!(
            kill_if_same(dummy.pid(), None),
            Err(ProcError::NoSuchProcess)
        );
        std::thread::sleep(Duration::from_millis(200));
        assert!(dummy.alive(), "a reused PID is never terminated");

        assert_eq!(kill_if_same(dummy.pid(), listed), Ok(()));
        assert!(dummy.exited_within(Duration::from_secs(5)));
    }

    #[test]
    fn kill_of_a_pid_that_is_gone_is_no_such_process() {
        assert_eq!(
            kill_if_same(0xFFFF_FFF1, Some(1.0)),
            Err(ProcError::NoSuchProcess)
        );
        assert_eq!(identity(0xFFFF_FFF1), Err(ProcError::NoSuchProcess));
    }

    #[test]
    fn missing_pid_is_no_such_process() {
        // Far above any PID Windows hands out.
        assert_eq!(create_time(0xFFFF_FFF1), Err(ProcError::NoSuchProcess));
        assert_eq!(nt_create_time(0xFFFF_FFF1), Err(ProcError::NoSuchProcess));
    }
}
