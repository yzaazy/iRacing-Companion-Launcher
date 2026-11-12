# 0004. Win32 directly through the `windows` crate

**Status:** Accepted

## Context

v2.3.2 used psutil, `subprocess`, `os.startfile`, pywin32/winshell, `winreg` and Tk's file dialog. What the launcher observes depends on what those do underneath: which access rights psutil asks for, which error means "no such process" and which "access denied", how a creation time is computed, which flags the dialog sets. Cross-platform crates such as `sysinfo` and `rfd` make their own choices for these, and the app runs only on Windows.

## Decision

Call Win32 directly through the `windows` crate (0.62) for processes (ToolHelp snapshots, `OpenProcess`, `GetProcessTimes`, `NtQuerySystemInformation`, `CreateProcessW`, `TerminateProcess`, waits), the shell (`ShellExecuteW`, `IShellLinkW`), COM, the registry and the Browse dialog (`IFileOpenDialog` with Tk 8.6's options). No `sysinfo`, no `rfd`.

- `process::sys` reproduces psutil's behaviour on Windows; `dialog` reproduces Tk's open dialog.
- Each `unsafe` block covers the call it needs and carries a `SAFETY:` comment (the `undocumented_unsafe_blocks` lint enforces it). Handles, find handles, registry keys and COM apartments are released by small RAII types in `win`.
- Only the `windows` features the code uses are enabled.

## Consequences

- Process and dialog behaviour matches v2.3.2 in the details users notice, and is tested against dummy processes and captured psutil values.
- The crate is Windows-only by design.
- More `unsafe` and FFI code to maintain than a wrapper crate would need, kept in `process::sys`, `process`, `detect`, `dialog` and `win`.
