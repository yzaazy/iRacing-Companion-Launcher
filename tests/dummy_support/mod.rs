//! Dummy processes for the tests that launch, match or kill processes.
//!
//! The developer's real companion apps run on this machine, so every process
//! a test launches, matches by name or kills is a copy of
//! `tests/fixtures/process/dummy.rs` under a unique `ircl-test-<random>-<role>.exe`
//! name in a temp directory. `Dummies` kills its names and deletes the
//! directory on drop, also when a test fails, and every dummy exits by itself
//! after two minutes.

#![allow(dead_code)] // each test crate uses a different subset of the helpers
#![allow(clippy::unwrap_used, clippy::expect_used)] // helpers outside #[test] functions; a panic there fails the test

use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use iracing_companion_launcher::process::{is_process_running, kill_process};

const DUMMY_SRC: &str = include_str!("../fixtures/process/dummy.rs");

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
pub(crate) struct Dummies {
    pub(crate) dir: tempfile::TempDir,
    pub(crate) prefix: String,
    names: Vec<String>,
    pub(crate) children: Vec<Child>,
}

impl Dummies {
    pub(crate) fn new() -> Self {
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

    pub(crate) fn name(&self, role: &str) -> String {
        format!("{}-{role}.exe", self.prefix)
    }

    /// Copies the dummy under this role's name.
    pub(crate) fn exe(&mut self, role: &str) -> String {
        let name = self.name(role);
        let path = self.dir.path().join(&name);
        std::fs::copy(dummy_exe(), &path).unwrap();
        self.names.push(name);
        path.to_string_lossy().into_owned()
    }

    /// Root, child and grandchild exes; returns the root.
    pub(crate) fn tree(&mut self) -> String {
        self.exe("child");
        self.exe("grandchild");
        self.exe("root")
    }

    /// Starts an instance outside the tracker.
    pub(crate) fn spawn(&mut self, path: &str) {
        self.children.push(Command::new(path).spawn().unwrap());
    }

    /// Makes every instance of this role exit by itself.
    pub(crate) fn stop(&self, role: &str) {
        let path = self.dir.path().join(format!("{}.stop", self.name(role)));
        std::fs::write(path, b"").unwrap();
    }

    pub(crate) fn state_file(&self) -> PathBuf {
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

pub(crate) fn wait_until(timeout: Duration, mut done: impl FnMut() -> bool) -> bool {
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
