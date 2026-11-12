//! Process layer against dummy processes only (see `dummy_support`).

#![allow(clippy::unwrap_used, clippy::expect_used)] // helpers outside #[test] functions; a panic there fails the test

mod dummy_support;

use std::cell::RefCell;
use std::path::Path;
use std::time::{Duration, Instant};

use dummy_support::{Dummies, wait_until};
use iracing_companion_launcher::process::tracker::{LaunchMethod, ProcessTracker};
use iracing_companion_launcher::process::{
    is_process_running, kill_process, launch_process, shell_open,
};
use serde_json::Value;
use windows::Win32::Foundation::{CloseHandle, HANDLE, LUID};
use windows::Win32::Security::{
    AdjustTokenPrivileges, LUID_AND_ATTRIBUTES, LookupPrivilegeValueW, SE_DEBUG_NAME,
    TOKEN_ADJUST_PRIVILEGES, TOKEN_PRIVILEGES, TOKEN_PRIVILEGES_ATTRIBUTES,
};
use windows::Win32::System::Threading::{
    GetCurrentProcess, OpenProcess, OpenProcessToken, PROCESS_ACCESS_RIGHTS,
    PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_SYNCHRONIZE, PROCESS_TERMINATE,
};
use windows::core::PCWSTR;

const WAIT: Duration = Duration::from_millis(1000);
const KEY: &str = "Dummy App";

// ---------------------------------------------------------------------------
// Support
// ---------------------------------------------------------------------------

fn read_state(path: &Path) -> Value {
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

fn tracked_create_time(path: &Path, key: &str) -> f64 {
    read_state(path)[key]["create_time"].as_f64().unwrap()
}

// ---------------------------------------------------------------------------
// Name-based functions
// ---------------------------------------------------------------------------

#[test]
fn name_matching_is_case_insensitive() {
    let mut d = Dummies::new();
    let exe = d.exe("solo");
    assert!(!is_process_running(&d.name("solo")));
    d.spawn(&exe);
    let upper = d.name("solo").to_uppercase();
    assert!(wait_until(WAIT, || is_process_running(&upper)));
    assert!(is_process_running(&d.name("solo")));
    assert!(!is_process_running(&d.name("never-started")));
}

#[test]
fn kill_process_kills_every_instance() {
    let mut d = Dummies::new();
    let exe = d.exe("multi");
    d.spawn(&exe);
    d.spawn(&exe);
    let name = d.name("multi");
    assert!(wait_until(WAIT, || is_process_running(&name)));

    assert!(kill_process(&name.to_uppercase()));

    for child in &mut d.children {
        let exited = wait_until(Duration::from_secs(5), || {
            matches!(child.try_wait(), Ok(Some(_)))
        });
        assert!(exited, "an instance survived kill_process");
    }
    assert!(!is_process_running(&name));
    assert!(!kill_process(&name), "nothing left to kill");
}

#[test]
fn launch_process_is_detached_and_untracked() {
    let mut d = Dummies::new();
    let exe = d.exe("game");
    let started = Instant::now();
    assert!(launch_process(&exe, Duration::from_millis(300)));
    assert!(started.elapsed() >= Duration::from_millis(300), "waits");
    assert!(is_process_running(&d.name("game")));

    let missing = d.dir.path().join("missing.exe");
    assert!(!launch_process(
        &missing.to_string_lossy(),
        Duration::from_secs(5)
    ));
}

#[test]
fn shell_open_reports_a_missing_target() {
    let d = Dummies::new();
    let missing = d.dir.path().join("missing.exe");
    assert!(shell_open(&missing.to_string_lossy()).is_err());
}

// ---------------------------------------------------------------------------
// Tracker
// ---------------------------------------------------------------------------

#[test]
fn launch_and_track_then_close() {
    let mut d = Dummies::new();
    let exe = d.exe("app");
    let mut t = ProcessTracker::new(d.state_file());

    assert!(t.launch_and_track(KEY, &exe, WAIT, LaunchMethod::Direct));
    assert!(t.is_tracked(KEY));
    assert_eq!(t.is_tracked_running(KEY), (true, 0));
    assert!(t.child_names(KEY).is_empty());
    assert_eq!(read_state(&d.state_file())[KEY]["exe_path"], exe.as_str());

    assert!(t.close_tracked(KEY));
    assert!(!t.is_tracked(KEY));
    assert!(!is_process_running(&d.name("app")));
    assert_eq!(read_state(&d.state_file()), serde_json::json!({}));
    assert!(!t.close_tracked(KEY), "already closed");
}

#[test]
fn tree_is_counted_named_and_closed_together() {
    let mut d = Dummies::new();
    let root = d.tree();
    let mut t = ProcessTracker::new(d.state_file());

    assert!(t.launch_and_track(KEY, &root, WAIT, LaunchMethod::Direct));
    assert!(
        wait_until(Duration::from_secs(15), || t.is_tracked_running(KEY)
            == (true, 2)),
        "expected child and grandchild"
    );
    // Depth first, as psutil's children(recursive=True).
    assert_eq!(t.child_names(KEY), [d.name("child"), d.name("grandchild")]);

    assert!(t.close_tracked(KEY));
    for role in ["root", "child", "grandchild"] {
        assert!(!is_process_running(&d.name(role)), "{role} survived");
    }
    assert!(!t.is_tracked(KEY));
}

#[test]
fn spared_exe_and_its_subtree_are_not_counted_named_or_closed() {
    let mut d = Dummies::new();
    let root = d.tree();
    let mut t = ProcessTracker::new(d.state_file());
    // In any case, as every other name match.
    t.spare([d.name("child").to_uppercase().as_str()]);

    assert!(t.launch_and_track(KEY, &root, WAIT, LaunchMethod::Direct));
    assert!(
        wait_until(Duration::from_secs(15), || is_process_running(
            &d.name("grandchild")
        )),
        "expected child and grandchild"
    );
    assert_eq!(t.is_tracked_running(KEY), (true, 0));
    assert!(t.child_names(KEY).is_empty());
    assert_eq!(t.spared_names(KEY), [d.name("child")]);

    assert!(t.close_tracked(KEY));
    assert!(!is_process_running(&d.name("root")), "root survived");
    assert!(!t.is_tracked(KEY));
    // Long enough for a terminated process to be gone.
    std::thread::sleep(Duration::from_millis(500));
    for role in ["child", "grandchild"] {
        assert!(is_process_running(&d.name(role)), "{role} was closed");
    }
}

#[test]
fn process_that_exits_by_itself_is_dropped() {
    let mut d = Dummies::new();
    let exe = d.exe("quits");
    let mut t = ProcessTracker::new(d.state_file());
    assert!(t.launch_and_track(KEY, &exe, WAIT, LaunchMethod::Direct));

    d.stop("quits");
    assert!(wait_until(Duration::from_secs(10), || !t
        .is_tracked_running(KEY)
        .0));
    assert!(!t.is_tracked(KEY));
    assert_eq!(read_state(&d.state_file()), serde_json::json!({}));
    assert!(!t.close_tracked(KEY));
}

thread_local! {
    /// The exe name [`until_awaited_exits`] waits for, per test thread.
    static AWAITED: RefCell<String> = const { RefCell::new(String::new()) };
}

/// A launch wait that ends when no process named [`AWAITED`] is left, instead
/// of after a fixed time: a dummy that exits as soon as it starts can take
/// longer than any fixed wait to get that far on a loaded machine.
fn until_awaited_exits(_: Duration) {
    AWAITED.with_borrow(|name| {
        assert!(
            wait_until(Duration::from_secs(60), || !is_process_running(name)),
            "{name} never exited"
        );
    });
}

#[test]
fn launch_that_dies_during_the_wait_fails() {
    let mut d = Dummies::new();
    let exe = d.exe("dies");
    d.stop("dies"); // the dummy exits as soon as it starts
    AWAITED.set(d.name("dies"));
    let mut t = ProcessTracker::with_sleep(d.state_file(), until_awaited_exits);
    assert!(!t.launch_and_track(KEY, &exe, WAIT, LaunchMethod::Direct));
    assert!(!t.is_tracked(KEY));
}

#[test]
fn pid_reuse_is_detected_by_creation_time() {
    let mut d = Dummies::new();
    let exe = d.exe("reused");
    let state = d.state_file();
    {
        let mut t = ProcessTracker::new(state.clone());
        assert!(t.launch_and_track(KEY, &exe, WAIT, LaunchMethod::Direct));
    }
    let original = read_state(&state);
    let ct = tracked_create_time(&state, KEY);

    // Same PID, alive, but created at another time: an unrelated process.
    let mut reused = original.clone();
    reused[KEY]["create_time"] = (ct - 10.0).into();
    std::fs::write(&state, reused.to_string()).unwrap();
    let t = ProcessTracker::new(state.clone());
    assert!(!t.is_tracked(KEY));
    assert_eq!(
        read_state(&state),
        serde_json::json!({}),
        "dropped and saved"
    );
    assert!(is_process_running(&d.name("reused")), "never killed");

    // Within the 0.5 s tolerance it is still ours.
    for delta in [0.3, -0.3] {
        let mut close = original.clone();
        close[KEY]["create_time"] = (ct + delta).into();
        std::fs::write(&state, close.to_string()).unwrap();
        let mut t = ProcessTracker::new(state.clone());
        assert!(t.is_tracked(KEY), "delta {delta}");
        assert_eq!(t.is_tracked_running(KEY), (true, 0));
    }
}

#[test]
fn state_survives_a_restart() {
    let mut d = Dummies::new();
    let root = d.tree();
    {
        let mut t = ProcessTracker::new(d.state_file());
        assert!(t.launch_and_track(KEY, &root, WAIT, LaunchMethod::Direct));
    }
    let mut t = ProcessTracker::new(d.state_file());
    assert!(t.is_tracked(KEY));
    assert!(wait_until(Duration::from_secs(15), || t
        .is_tracked_running(KEY)
        == (true, 2)));
    assert!(t.close_tracked(KEY));
    assert!(!is_process_running(&d.name("grandchild")));
}

#[test]
fn shell_launch_finds_the_new_pid() {
    let mut d = Dummies::new();
    let exe = d.exe("shell");
    // An instance that was already running must not be taken for the new one.
    d.spawn(&exe);
    assert!(wait_until(WAIT, || is_process_running(&d.name("shell"))));

    let mut t = ProcessTracker::new(d.state_file());
    assert!(t.launch_and_track(KEY, &exe, WAIT, LaunchMethod::Shell));
    let pid = read_state(&d.state_file())[KEY]["pid"].as_u64().unwrap();
    assert_ne!(pid, u64::from(d.children[0].id()));
    assert_eq!(t.is_tracked_running(KEY), (true, 0));

    assert!(t.close_tracked(KEY));
    assert!(
        matches!(d.children[0].try_wait(), Ok(None)),
        "the older instance must survive"
    );
}

#[test]
fn shell_launch_without_a_new_process_fails() {
    let mut d = Dummies::new();
    let mut t = ProcessTracker::with_sleep(d.state_file(), until_awaited_exits);
    let missing = d.dir.path().join("missing.exe");
    assert!(!t.launch_and_track(KEY, &missing.to_string_lossy(), WAIT, LaunchMethod::Shell));

    // Starts, but exits before the wait ends: no new PID is found.
    let exe = d.exe("brief");
    d.stop("brief");
    AWAITED.set(d.name("brief"));
    assert!(!t.launch_and_track(KEY, &exe, WAIT, LaunchMethod::Shell));
    assert!(!t.is_tracked(KEY));
}

fn can_open(pid: u64, access: PROCESS_ACCESS_RIGHTS) -> bool {
    // SAFETY: OpenProcess takes only values and returns a new handle or an
    // error; a returned handle is closed at once, below.
    match unsafe { OpenProcess(access, false, pid as u32) } {
        Ok(h) => {
            // SAFETY: `h` was just opened by this function and is closed exactly once.
            let _ = unsafe { CloseHandle(h) };
            true
        }
        Err(_) => false,
    }
}

/// Disables `SeDebugPrivilege` in this test process. An elevated administrator with it
/// enabled (a GitHub runner) opens any process whatever its DACL, so a dummy that refuses
/// access would not refuse this process; the launcher runs without it. Process-wide and
/// idempotent; no test relies on the privilege, and disabling one the token lacks is a no-op.
fn drop_debug_privilege() {
    let mut token = HANDLE::default();
    // SAFETY: GetCurrentProcess returns a pseudo-handle that needs no closing;
    // `token` is a live out-pointer for the new token handle.
    if unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_ADJUST_PRIVILEGES, &raw mut token) }
        .is_err()
    {
        return;
    }
    let mut luid = LUID::default();
    // SAFETY: SE_DEBUG_NAME is a static NUL-terminated string; `luid` is a live out-pointer.
    if unsafe { LookupPrivilegeValueW(PCWSTR::null(), SE_DEBUG_NAME, &raw mut luid) }.is_ok() {
        let disabled = TOKEN_PRIVILEGES {
            PrivilegeCount: 1,
            Privileges: [LUID_AND_ATTRIBUTES {
                Luid: luid,
                Attributes: TOKEN_PRIVILEGES_ATTRIBUTES(0),
            }],
        };
        // SAFETY: `token` was opened above with TOKEN_ADJUST_PRIVILEGES; `disabled` is a
        // complete one-entry TOKEN_PRIVILEGES alive across the call; no previous state is asked for.
        let _ = unsafe {
            AdjustTokenPrivileges(token, false, Some(&raw const disabled), 0, None, None)
        };
    }
    // SAFETY: `token` was opened above and is closed exactly once.
    let _ = unsafe { CloseHandle(token) };
}

#[test]
fn helper_that_refuses_waiting_is_still_closed() {
    drop_debug_privilege();
    let mut d = Dummies::new();
    let exe = d.exe("locked");
    let mut t = ProcessTracker::new(d.state_file());
    assert!(t.launch_and_track(KEY, &exe, WAIT, LaunchMethod::Direct));
    let pid = read_state(&d.state_file())[KEY]["pid"].as_u64().unwrap();
    assert!(
        !can_open(pid, PROCESS_SYNCHRONIZE),
        "dummy restricted itself"
    );
    assert!(can_open(pid, PROCESS_TERMINATE));

    // Waiting is denied, so the kill pass is skipped; the first terminate is enough.
    assert_eq!(t.is_tracked_running(KEY), (true, 0));
    assert!(t.close_tracked(KEY));
    assert!(!t.is_tracked(KEY));
    assert!(wait_until(Duration::from_secs(5), || !is_process_running(
        &d.name("locked")
    )));
}

#[test]
fn process_that_refuses_everything_is_tracked_and_survives_close() {
    drop_debug_privilege();
    let mut d = Dummies::new();
    let exe = d.exe("opaque");
    let state = d.state_file();
    let mut t = ProcessTracker::new(state.clone());
    assert!(t.launch_and_track(KEY, &exe, WAIT, LaunchMethod::Direct));
    let pid = read_state(&state)[KEY]["pid"].as_u64().unwrap();
    assert!(
        !can_open(pid, PROCESS_QUERY_LIMITED_INFORMATION),
        "dummy restricted itself"
    );

    // The creation time still validates, through the NtQuerySystemInformation
    // fallback psutil also uses.
    let mut t = ProcessTracker::new(state.clone());
    assert_eq!(t.is_tracked_running(KEY), (true, 0));

    // As in Python: close reports true and forgets the entry, but the process
    // refused termination and keeps running. Name-based kill fails too.
    assert!(t.close_tracked(KEY));
    assert!(!t.is_tracked(KEY));
    assert!(is_process_running(&d.name("opaque")));
    assert!(!kill_process(&d.name("opaque")));
}

#[test]
fn failed_direct_launch_tracks_nothing() {
    let d = Dummies::new();
    let mut t = ProcessTracker::new(d.state_file());
    let missing = d.dir.path().join("missing.exe");
    assert!(!t.launch_and_track(KEY, &missing.to_string_lossy(), WAIT, LaunchMethod::Direct));
    assert!(!t.is_tracked(KEY));
    assert!(!d.state_file().exists(), "nothing saved");
}

#[test]
fn state_file_is_written_like_python() {
    let mut d = Dummies::new();
    let exe = d.exe("format");
    let mut t = ProcessTracker::new(d.state_file());
    assert!(t.launch_and_track(KEY, &exe, WAIT, LaunchMethod::Direct));

    let text = std::fs::read_to_string(d.state_file()).unwrap();
    assert!(
        text.starts_with("{\r\n  \"Dummy App\": {\r\n    \"pid\": "),
        "{text}"
    );
    let (pid, ct, path) = (
        text.find("\"pid\"").unwrap(),
        text.find("\"create_time\"").unwrap(),
        text.find("\"exe_path\"").unwrap(),
    );
    assert!(pid < ct && ct < path, "Python's key order");
    assert!(!text.contains("\r\n\r\n") && !text.ends_with('\n'));
    assert!(!d.dir.path().join("process_state.json.tmp").exists());
    // Sub-second precision is kept.
    assert!(tracked_create_time(&d.state_file(), KEY).fract() != 0.0);
}

#[test]
fn missing_or_corrupt_state_file_is_tolerated() {
    let d = Dummies::new();
    let state = d.state_file();

    let t = ProcessTracker::new(state.clone());
    assert!(!t.is_tracked(KEY));
    assert!(!state.exists(), "loading never writes");

    for corrupt in [
        &b"not json{"[..],
        b"",
        b"[1, 2]",
        b"\xff\xfe\x00",
        b"\xef\xbb\xbf{}", // Python's json rejects a BOM
    ] {
        std::fs::write(&state, corrupt).unwrap();
        let mut t = ProcessTracker::new(state.clone());
        assert!(!t.is_tracked(KEY));
        assert_eq!(t.is_tracked_running(KEY), (false, 0));
        assert_eq!(std::fs::read(&state).unwrap(), corrupt, "left as is");
    }
}

#[test]
fn malformed_entries_follow_python() {
    let d = Dummies::new();
    let state = d.state_file();
    // A falsy entry is kept but never valid (`if not entry: return None`).
    // A dead PID is dropped. A malformed entry crashes Python; it is dropped here.
    std::fs::write(
        &state,
        r#"{"Empty": {}, "Dead": {"pid": 4294967295, "create_time": 1.0}, "Bad": {"pid": "x"}}"#,
    )
    .unwrap();
    let mut t = ProcessTracker::new(state.clone());
    assert!(t.is_tracked("Empty"));
    assert_eq!(t.is_tracked_running("Empty"), (false, 0));
    assert!(!t.close_tracked("Empty"));
    assert!(t.is_tracked("Empty"));
    assert!(!t.is_tracked("Dead"));
    assert!(!t.is_tracked("Bad"));
    assert_eq!(read_state(&state), serde_json::json!({"Empty": {}}));
}

// ---------------------------------------------------------------------------
// process_state.json as the Python tracker wrote it (tests/fixtures/process)
// ---------------------------------------------------------------------------

/// The second key in the captured files: non-ASCII and an astral character.
const GONE: &str = "Dümmy Gone 😀";

fn fixture(name: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/process")
        .join(name);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

/// What the Python tracker reported and stored while the fixtures were captured.
fn python_view() -> Value {
    serde_json::from_str(&fixture("python_view.json")).unwrap()
}

/// `(pid, create_time)` as written into a state file.
type Numbers = (String, String);

/// The PID and creation time the Python tracker wrote for `key`.
fn python_numbers(view: &Value, key: &str) -> Numbers {
    let entry = &view["entries"][key];
    (
        entry[0].as_u64().unwrap().to_string(),
        entry[1].as_str().unwrap().to_owned(),
    )
}

/// Launches `exe` under `key` through a scratch tracker and returns the PID and
/// creation time of the live process as Rust writes them.
fn live_numbers(d: &Dummies, scratch: &str, key: &str, exe: &str) -> Numbers {
    let scratch = d.dir.path().join(scratch);
    let mut t = ProcessTracker::new(scratch.clone());
    assert!(t.launch_and_track(key, exe, WAIT, LaunchMethod::Direct));
    let entry = &read_state(&scratch)[key];
    (entry["pid"].to_string(), entry["create_time"].to_string())
}

/// A captured file with each Python PID and creation time swapped for a live
/// process's. Every other byte is Python's.
fn with_live(text: &str, swaps: &[(&Numbers, &Numbers)]) -> String {
    let mut out = text.to_owned();
    for (python, live) in swaps {
        for (field, from, to) in [
            ("pid", &python.0, &live.0),
            ("create_time", &python.1, &live.1),
        ] {
            let from = format!("\"{field}\": {from},");
            assert_eq!(out.matches(&from).count(), 1, "{from} in {text}");
            out = out.replace(&from, &format!("\"{field}\": {to},"));
        }
    }
    out
}

#[test]
fn python_numbers_are_formatted_like_rust() {
    // Python writes `repr(float)` and `int`. Rust formats the same values to the
    // same text, so swapping in live numbers below keeps each file Python's.
    let view = python_view();
    let written = fixture("python_written.json");
    for key in [GONE, KEY] {
        let (pid, ct) = python_numbers(&view, key);
        let value: f64 = ct.parse().unwrap();
        assert!(value.fract() != 0.0, "sub-second precision: {ct}");
        assert_eq!(serde_json::to_string(&value).unwrap(), ct);
        assert_eq!(
            serde_json::to_string(&pid.parse::<u64>().unwrap()).unwrap(),
            pid
        );
        assert!(
            written.contains(&format!(
                "\"pid\": {pid},\r\n    \"create_time\": {ct},\r\n"
            )),
            "{written}"
        );
    }
}

#[test]
fn python_state_file_is_read_and_rewritten_byte_for_byte() {
    let mut d = Dummies::new();
    let root = d.tree();
    let solo = d.exe("gone");
    let view = python_view();
    let (py_app, py_gone) = (python_numbers(&view, KEY), python_numbers(&view, GONE));
    let app = live_numbers(&d, "scratch app.json", KEY, &root);
    let gone = live_numbers(&d, "scratch gone.json", GONE, &solo);
    let state = d.state_file();

    // Python's file, with both of its processes alive again.
    let written = with_live(
        &fixture("python_written.json"),
        &[(&py_app, &app), (&py_gone, &gone)],
    );
    std::fs::write(&state, &written).unwrap();
    let mut t = ProcessTracker::new(state.clone());
    for key in ["Empty", GONE, KEY] {
        assert!(t.is_tracked(key), "{key}");
    }
    assert_eq!(std::fs::read_to_string(&state).unwrap(), written);
    assert!(wait_until(Duration::from_secs(15), || t
        .is_tracked_running(KEY)
        == (true, 2)));
    assert_eq!(t.is_tracked_running(GONE), (true, 0));
    // The same tree as Python saw it, under this test's prefix.
    assert_eq!(view["running"], serde_json::json!([true, 2]));
    let python_prefix = view["prefix"].as_str().unwrap();
    let python_names: Vec<String> = view["names"]
        .as_array()
        .unwrap()
        .iter()
        .map(|n| n.as_str().unwrap().replace(python_prefix, &d.prefix))
        .collect();
    assert_eq!(t.child_names(KEY), python_names);

    // Every save writes the bytes Python wrote at the same step.
    assert!(t.close_tracked(KEY));
    assert_eq!(
        std::fs::read_to_string(&state).unwrap(),
        with_live(&fixture("python_closed.json"), &[(&py_gone, &gone)])
    );
    for role in ["root", "child", "grandchild"] {
        assert!(!is_process_running(&d.name(role)), "{role} survived");
    }

    d.stop("gone");
    assert!(wait_until(Duration::from_secs(10), || !is_process_running(
        &d.name("gone")
    )));
    let t = ProcessTracker::new(state.clone());
    assert!(!t.is_tracked(GONE));
    assert!(t.is_tracked("Empty"));
    assert_eq!(
        std::fs::read_to_string(&state).unwrap(),
        fixture("python_dropped.json")
    );
}
