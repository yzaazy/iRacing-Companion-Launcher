//! Process layer against dummy processes only.
//!
//! The developer's real companion apps run on this machine, so every process
//! these tests launch, match by name or kill is a copy of
//! `tests/fixtures/process/dummy.rs` under a unique `ircl-test-<random>-<role>.exe`
//! name in a temp directory. `Dummies` kills its names and deletes the
//! directory on drop, also when a test fails, and every dummy exits by itself
//! after two minutes.

use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use iracing_companion_launcher::process::tracker::ProcessTracker;
use iracing_companion_launcher::process::{
    is_process_running, kill_process, launch_process, shell_open,
};
use serde_json::Value;
use windows::Win32::Foundation::CloseHandle;
use windows::Win32::System::Threading::{
    OpenProcess, PROCESS_ACCESS_RIGHTS, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_SYNCHRONIZE,
    PROCESS_TERMINATE,
};

const WAIT: Duration = Duration::from_millis(1000);
const KEY: &str = "Dummy App";

// ---------------------------------------------------------------------------
// Support
// ---------------------------------------------------------------------------

const DUMMY_SRC: &str = include_str!("fixtures/process/dummy.rs");

/// Compiles the dummy once per test binary, into Cargo's per-target temp dir.
fn dummy_exe() -> &'static Path {
    static EXE: OnceLock<PathBuf> = OnceLock::new();
    EXE.get_or_init(|| {
        let hash = DUMMY_SRC.bytes().fold(0xcbf2_9ce4_8422_2325_u64, |h, b| {
            (h ^ u64::from(b)).wrapping_mul(0x100_0000_01b3)
        });
        let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR"));
        let out = dir.join(format!("ircl-dummy-{hash:016x}.exe"));
        if out.exists() {
            return out;
        }
        let build = dir.join(format!("ircl-dummy-build-{}", std::process::id()));
        std::fs::create_dir_all(&build).unwrap();
        let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/process/dummy.rs");
        let built = build.join("dummy.exe");
        let rustc = Path::new(env!("CARGO")).with_file_name("rustc.exe");
        let rustc = if rustc.exists() {
            rustc
        } else {
            PathBuf::from("rustc")
        };
        let status = Command::new(rustc)
            .args(["--edition", "2021", "-C", "debuginfo=0", "-o"])
            .arg(&built)
            .arg(&src)
            .status()
            .expect("run rustc for the dummy process");
        assert!(status.success(), "dummy process failed to compile");
        // Another test binary may have won the race; either copy is identical.
        let _ = std::fs::rename(&built, &out);
        let _ = std::fs::remove_dir_all(&build);
        out
    })
}

fn unique_prefix() -> String {
    static COUNTER: AtomicU32 = AtomicU32::new(0);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .subsec_nanos();
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    format!(
        "ircl-test-{:08x}",
        nanos ^ std::process::id().rotate_left(16) ^ n.wrapping_mul(0x9E37_79B9)
    )
}

/// Dummy exes in a temp directory whose path contains a space.
struct Dummies {
    dir: tempfile::TempDir,
    prefix: String,
    names: Vec<String>,
    children: Vec<Child>,
}

impl Dummies {
    fn new() -> Self {
        let dir = tempfile::Builder::new()
            .prefix("ircl test ")
            .tempdir()
            .unwrap();
        Self {
            dir,
            prefix: unique_prefix(),
            names: Vec::new(),
            children: Vec::new(),
        }
    }

    fn name(&self, role: &str) -> String {
        format!("{}-{role}.exe", self.prefix)
    }

    /// Copies the dummy under this role's name.
    fn exe(&mut self, role: &str) -> String {
        let name = self.name(role);
        let path = self.dir.path().join(&name);
        std::fs::copy(dummy_exe(), &path).unwrap();
        self.names.push(name);
        path.to_string_lossy().into_owned()
    }

    /// Root, child and grandchild exes; returns the root.
    fn tree(&mut self) -> String {
        self.exe("child");
        self.exe("grandchild");
        self.exe("root")
    }

    /// Starts an instance outside the tracker.
    fn spawn(&mut self, path: &str) {
        self.children.push(Command::new(path).spawn().unwrap());
    }

    /// Makes every instance of this role exit by itself.
    fn stop(&self, role: &str) {
        let path = self.dir.path().join(format!("{}.stop", self.name(role)));
        std::fs::write(path, b"").unwrap();
    }

    fn state_file(&self) -> PathBuf {
        self.dir.path().join("process_state.json")
    }
}

impl Drop for Dummies {
    fn drop(&mut self) {
        for name in &self.names {
            let _ = std::fs::write(self.dir.path().join(format!("{name}.stop")), b"");
            // Only ever names this struct created.
            kill_process(name);
        }
        for child in &mut self.children {
            let _ = child.kill();
            let _ = child.wait();
        }
        wait_until(Duration::from_secs(5), || {
            !self.names.iter().any(|n| is_process_running(n))
        });
    }
}

fn wait_until(timeout: Duration, mut done: impl FnMut() -> bool) -> bool {
    let end = Instant::now() + timeout;
    loop {
        if done() {
            return true;
        }
        if Instant::now() >= end {
            return false;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

fn read_state(path: &Path) -> Value {
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

fn tracked_create_time(path: &Path, key: &str) -> f64 {
    read_state(path)[key]["create_time"].as_f64().unwrap()
}

// ---------------------------------------------------------------------------
// Name-based functions (process_manager.py)
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
// Tracker (process_tracker.py)
// ---------------------------------------------------------------------------

#[test]
fn launch_and_track_then_close() {
    let mut d = Dummies::new();
    let exe = d.exe("app");
    let mut t = ProcessTracker::new(d.state_file());

    assert!(t.launch_and_track(KEY, &exe, WAIT, false));
    assert!(t.is_tracked(KEY));
    assert_eq!(t.is_tracked_running(KEY), (true, 0));
    assert!(t.get_child_names(KEY).is_empty());
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

    assert!(t.launch_and_track(KEY, &root, WAIT, false));
    assert!(
        wait_until(Duration::from_secs(15), || t.is_tracked_running(KEY)
            == (true, 2)),
        "expected child and grandchild"
    );
    // Depth first, as psutil's children(recursive=True).
    assert_eq!(
        t.get_child_names(KEY),
        [d.name("child"), d.name("grandchild")]
    );

    assert!(t.close_tracked(KEY));
    for role in ["root", "child", "grandchild"] {
        assert!(!is_process_running(&d.name(role)), "{role} survived");
    }
    assert!(!t.is_tracked(KEY));
}

#[test]
fn process_that_exits_by_itself_is_dropped() {
    let mut d = Dummies::new();
    let exe = d.exe("quits");
    let mut t = ProcessTracker::new(d.state_file());
    assert!(t.launch_and_track(KEY, &exe, WAIT, false));

    d.stop("quits");
    assert!(wait_until(Duration::from_secs(10), || !t
        .is_tracked_running(KEY)
        .0));
    assert!(!t.is_tracked(KEY));
    assert_eq!(read_state(&d.state_file()), serde_json::json!({}));
    assert!(!t.close_tracked(KEY));
}

#[test]
fn launch_that_dies_during_the_wait_fails() {
    let mut d = Dummies::new();
    let exe = d.exe("dies");
    d.stop("dies"); // the dummy exits as soon as it starts
    let mut t = ProcessTracker::new(d.state_file());
    assert!(!t.launch_and_track(KEY, &exe, WAIT, false));
    assert!(!t.is_tracked(KEY));
}

#[test]
fn pid_reuse_is_detected_by_creation_time() {
    let mut d = Dummies::new();
    let exe = d.exe("reused");
    let state = d.state_file();
    {
        let mut t = ProcessTracker::new(state.clone());
        assert!(t.launch_and_track(KEY, &exe, WAIT, false));
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
        assert!(t.launch_and_track(KEY, &root, WAIT, false));
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
    assert!(t.launch_and_track(KEY, &exe, WAIT, true));
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
    let mut t = ProcessTracker::new(d.state_file());
    let missing = d.dir.path().join("missing.exe");
    assert!(!t.launch_and_track(KEY, &missing.to_string_lossy(), WAIT, true));

    // Starts, but exits before the wait ends: no new PID is found.
    let exe = d.exe("brief");
    d.stop("brief");
    assert!(!t.launch_and_track(KEY, &exe, WAIT, true));
    assert!(!t.is_tracked(KEY));
}

fn can_open(pid: u64, access: PROCESS_ACCESS_RIGHTS) -> bool {
    // SAFETY: plain call; a returned handle is closed at once.
    match unsafe { OpenProcess(access, false, pid as u32) } {
        Ok(h) => {
            // SAFETY: `h` was just opened.
            let _ = unsafe { CloseHandle(h) };
            true
        }
        Err(_) => false,
    }
}

#[test]
fn helper_that_refuses_waiting_is_still_closed() {
    let mut d = Dummies::new();
    let exe = d.exe("locked");
    let mut t = ProcessTracker::new(d.state_file());
    assert!(t.launch_and_track(KEY, &exe, WAIT, false));
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
    let mut d = Dummies::new();
    let exe = d.exe("opaque");
    let state = d.state_file();
    let mut t = ProcessTracker::new(state.clone());
    assert!(t.launch_and_track(KEY, &exe, WAIT, false));
    let pid = read_state(&state)[KEY]["pid"].as_u64().unwrap();
    assert!(
        !can_open(pid, PROCESS_QUERY_LIMITED_INFORMATION),
        "dummy restricted itself"
    );

    // The creation time still validates, through the NtQuerySystemInformation
    // fallback psutil also uses, and agrees with psutil.
    let mut t = ProcessTracker::new(state.clone());
    assert_eq!(t.is_tracked_running(KEY), (true, 0));
    let state_s = state.to_string_lossy().into_owned();
    if let Some(theirs) = python(&d, &["ctime", repo_root(), &state_s, KEY, &pid.to_string()]) {
        let ours = tracked_create_time(&state, KEY);
        let theirs: f64 = theirs.parse().unwrap();
        println!("create_time (fallback) pid {pid}: rust {ours:?} psutil {theirs:?}");
        assert!((ours - theirs).abs() < 1e-3);
    }

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
    assert!(!t.launch_and_track(KEY, &missing.to_string_lossy(), WAIT, false));
    assert!(!t.is_tracked(KEY));
    assert!(!d.state_file().exists(), "nothing saved");
}

#[test]
fn forget_leaves_the_process_running() {
    let mut d = Dummies::new();
    let exe = d.exe("forgotten");
    let mut t = ProcessTracker::new(d.state_file());
    assert!(t.launch_and_track(KEY, &exe, WAIT, false));
    t.forget(KEY);
    assert!(!t.is_tracked(KEY));
    assert!(is_process_running(&d.name("forgotten")));
    assert_eq!(read_state(&d.state_file()), serde_json::json!({}));
}

#[test]
fn state_file_is_written_like_python() {
    let mut d = Dummies::new();
    let exe = d.exe("format");
    let mut t = ProcessTracker::new(d.state_file());
    assert!(t.launch_and_track(KEY, &exe, WAIT, false));

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
// Cross-check with psutil and the Python ProcessTracker
// ---------------------------------------------------------------------------

const PY_SCRIPT: &str = r#"
import importlib.util, json, sys
mode, root, state, key = sys.argv[1:5]
spec = importlib.util.spec_from_file_location(
    "process_tracker", root + "/iracing_launcher_app/managers/process_tracker.py")
pt = importlib.util.module_from_spec(spec)
spec.loader.exec_module(pt)
import psutil
if mode == "ctime":
    print(repr(psutil.Process(int(sys.argv[5])).create_time()))
elif mode == "launch":
    t = pt.ProcessTracker(state)
    print(json.dumps({"ok": t.launch_and_track(key, sys.argv[5], wait_time=1.0)}))
elif mode == "inspect":
    t = pt.ProcessTracker(state)
    alive, count = t.is_tracked_running(key)
    names = t.get_child_names(key)
    t.state_file_path = state + ".py"
    t._save()
    print(json.dumps({"tracked": t.is_tracked(key), "alive": alive,
                      "count": count, "names": names}))
"#;

/// Runs the script, or None when Python with psutil is not available.
fn python(d: &Dummies, args: &[&str]) -> Option<String> {
    let probe = Command::new("python")
        .args(["-c", "import psutil"])
        .output()
        .ok()?;
    if !probe.status.success() {
        eprintln!("skipped: python with psutil not available");
        return None;
    }
    let script = d.dir.path().join("crosscheck.py");
    std::fs::write(&script, PY_SCRIPT).unwrap();
    let out = Command::new("python")
        .arg(&script)
        .args(args)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "python failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    Some(String::from_utf8(out.stdout).unwrap().trim().to_owned())
}

fn repo_root() -> &'static str {
    env!("CARGO_MANIFEST_DIR")
}

#[test]
fn create_time_matches_psutil() {
    let mut d = Dummies::new();
    let exe = d.exe("ctime");
    let state = d.state_file();
    let mut t = ProcessTracker::new(state.clone());
    assert!(t.launch_and_track(KEY, &exe, WAIT, false));
    let pid = read_state(&state)[KEY]["pid"].as_u64().unwrap().to_string();
    let ours = tracked_create_time(&state, KEY);
    let state_s = state.to_string_lossy();
    let Some(theirs) = python(&d, &["ctime", repo_root(), &state_s, KEY, &pid]) else {
        return;
    };
    let theirs: f64 = theirs.parse().unwrap();
    println!(
        "create_time pid {pid}: rust {ours:?} psutil {theirs:?} diff {:e}",
        ours - theirs
    );
    assert!((ours - theirs).abs() < 1e-3);
}

#[test]
fn python_state_file_is_accepted() {
    let mut d = Dummies::new();
    let root = d.tree();
    let state = d.state_file();
    let state_s = state.to_string_lossy().into_owned();
    let Some(out) = python(&d, &["launch", repo_root(), &state_s, KEY, &root]) else {
        return;
    };
    assert_eq!(out, r#"{"ok": true}"#);

    let mut t = ProcessTracker::new(state.clone());
    assert!(
        t.is_tracked(KEY),
        "{}",
        std::fs::read_to_string(&state).unwrap()
    );
    assert!(wait_until(Duration::from_secs(15), || t
        .is_tracked_running(KEY)
        == (true, 2)));
    assert!(t.close_tracked(KEY));
    for role in ["root", "child", "grandchild"] {
        assert!(!is_process_running(&d.name(role)), "{role} survived");
    }
}

#[test]
fn rust_state_file_is_accepted_by_python() {
    let mut d = Dummies::new();
    let root = d.tree();
    let state = d.state_file();
    let state_s = state.to_string_lossy().into_owned();
    let mut t = ProcessTracker::new(state.clone());
    assert!(t.launch_and_track(KEY, &root, WAIT, false));
    assert!(wait_until(Duration::from_secs(15), || t
        .is_tracked_running(KEY)
        == (true, 2)));
    let names = t.get_child_names(KEY);

    let Some(out) = python(&d, &["inspect", repo_root(), &state_s, KEY]) else {
        return;
    };
    let seen: Value = serde_json::from_str(&out).unwrap();
    assert_eq!(
        seen,
        serde_json::json!({"tracked": true, "alive": true, "count": 2, "names": names})
    );
    // Python re-saving the state it loaded produces our bytes exactly.
    assert_eq!(
        std::fs::read(format!("{state_s}.py")).unwrap(),
        std::fs::read(&state).unwrap()
    );

    let mut t = ProcessTracker::new(state);
    assert!(t.close_tracked(KEY));
    assert!(!is_process_running(&d.name("root")));
}
