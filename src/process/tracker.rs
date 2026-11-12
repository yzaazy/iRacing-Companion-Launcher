//! Launches apps and tracks their process trees across restarts.
//!
//! State lives in `process_state.json` (see [`super::state_file`]), shared with
//! the Python version so an upgrade keeps tracking the apps the old launcher
//! started. Each entry is valid only while its PID is alive with the same
//! creation time, which guards against PID reuse after a restart.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::time::{Duration, Instant};

use super::state_file::{Entry, Slot, State, is_falsy, load, to_python_json, write_state};
use super::sys::{
    ProcError, WaitOutcome, create_time, identity, kill_if_same, proc_wait, snapshot,
    spawn_detached,
};
use super::{pids_by_exe, shell_open};
use crate::win::basename;

/// Tolerance (seconds) when comparing persisted vs. live creation time. Python
/// stores wall-clock seconds, so a small NTP correction can shift it slightly.
const CREATE_TIME_EPSILON: f64 = 0.5;

/// How long `close_tracked` waits for terminated processes before killing survivors.
const CLOSE_WAIT: Duration = Duration::from_secs(3);

/// How [`ProcessTracker::launch_and_track`] starts the process.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LaunchMethod {
    /// `CreateProcess`, detached; the PID comes back from the call.
    Direct,
    /// `ShellExecute` with the default verb, for manifests Windows refuses
    /// to start directly; the PID is found by exe name afterwards.
    Shell,
}

#[derive(Debug)]
pub struct ProcessTracker {
    state_file: PathBuf,
    state: State,
    /// The launch wait: `std::thread::sleep`, as Python's `time.sleep`.
    sleep: fn(Duration),
    /// Lowercased exe names the tree walk never enters; see [`Self::spare`].
    spared: Vec<String>,
}

impl ProcessTracker {
    /// Loads the state file and drops every entry whose process is gone or whose
    /// PID was recycled. A missing or unreadable file means nothing is tracked.
    pub fn new(state_file: PathBuf) -> Self {
        Self::with_sleep(state_file, std::thread::sleep)
    }

    /// [`Self::new`] with `sleep` in place of `std::thread::sleep` for the wait
    /// after a launch, for tests: a test can then wait for an event instead of
    /// a fixed time. `sleep` receives the `wait` passed to the launch.
    pub fn with_sleep(state_file: PathBuf, sleep: fn(Duration)) -> Self {
        let mut tracker = Self {
            state: load(&state_file),
            state_file,
            sleep,
            spared: Vec::new(),
        };
        let keys: Vec<String> = tracker.state.0.iter().map(|(k, _)| k.clone()).collect();
        for key in keys {
            // Called for its side effect of dropping stale entries.
            tracker.proc_for(&key);
        }
        tracker
    }

    /// Exe names, in any case, that are never a tracked app's descendant: the
    /// games. A tracked app can start a game (a Stream Deck button does), and a
    /// game is closed by hand only. A process with such a name, and everything
    /// below it, is not opened, counted, named or closed.
    pub fn spare<'a>(&mut self, exe_names: impl IntoIterator<Item = &'a str>) {
        self.spared = exe_names.into_iter().map(str::to_lowercase).collect();
    }

    /// Launches `exe_path` and remembers its PID under `key`. True if the
    /// launched process is alive after `wait`.
    ///
    /// [`LaunchMethod::Shell`] goes through `ShellExecute` instead of a raw
    /// `CreateProcess`, which Windows refuses (error 740) for manifests with
    /// `uiAccess="true"` such as Elgato Stream Deck. The shell does not hand back
    /// a PID, so the new process is found by exe name afterwards.
    pub fn launch_and_track(
        &mut self,
        key: &str,
        exe_path: &str,
        wait: Duration,
        method: LaunchMethod,
    ) -> bool {
        if method == LaunchMethod::Shell {
            let exe_name = basename(exe_path);
            let before: HashSet<u32> = pids_by_exe(exe_name).into_iter().collect();
            if shell_open(exe_path).is_err() {
                return false;
            }
            (self.sleep)(wait);
            // Python takes `next(iter(new_pids))`, an arbitrary member of a set;
            // the lowest PID is the deterministic equivalent.
            let Some(pid) = pids_by_exe(exe_name)
                .into_iter()
                .filter(|pid| !before.contains(pid))
                .min()
            else {
                return false;
            };
            let Ok(create_time) = create_time(pid) else {
                return false;
            };
            self.set(key, pid, create_time, exe_path);
            self.save();
            return true;
        }

        let Ok(pid) = spawn_detached(exe_path) else {
            return false;
        };
        let Ok(create_time) = create_time(pid) else {
            return false;
        };
        // Saved before the wait, so a crash during it still leaves the app tracked.
        self.set(key, pid, create_time, exe_path);
        self.save();

        (self.sleep)(wait);
        self.proc_for(key).is_some()
    }

    /// Whether tracking state is held for `key`. Does not check the process.
    pub fn is_tracked(&self, key: &str) -> bool {
        self.state.get(key).is_some()
    }

    /// `(alive, descendant_count)`. Drops the entry if the process is gone or
    /// its PID was recycled.
    ///
    /// Descendants that re-parent to system processes are invisible here, on
    /// purpose: killing system-managed processes risks Windows instability.
    pub fn is_tracked_running(&mut self, key: &str) -> (bool, usize) {
        let Some(root) = self.proc_for(key) else {
            return (false, 0);
        };
        match children(root.pid, &self.spared) {
            Ok(tree) => (true, tree.descendants.len()),
            Err(_) => {
                self.drop_key(key);
                (false, 0)
            }
        }
    }

    /// Exe names of the tracked process's descendants, depth first, or empty.
    pub fn child_names(&mut self, key: &str) -> Vec<String> {
        let Some(root) = self.proc_for(key) else {
            return Vec::new();
        };
        let Ok(Tree { descendants, .. }) = children(root.pid, &self.spared) else {
            return Vec::new();
        };
        // psutil reads each name afresh, and one descendant vanishing in between
        // makes Python return an empty list.
        if descendants
            .iter()
            .any(|c| create_time(c.pid) == Err(ProcError::NoSuchProcess))
        {
            return Vec::new();
        }
        descendants.into_iter().map(|c| c.name).collect()
    }

    /// Exe names of the spared processes ([`Self::spare`]) running below the
    /// tracked process, which a close of it leaves running. Empty when nothing
    /// is tracked under `key`.
    pub fn spared_names(&mut self, key: &str) -> Vec<String> {
        let Some(root) = self.proc_for(key) else {
            return Vec::new();
        };
        children(root.pid, &self.spared)
            .map(|tree| tree.spared)
            .unwrap_or_default()
    }

    /// Terminates the tracked process and all its descendants, waits up to 3 s,
    /// kills the survivors and forgets the entry. True if the tracked process was
    /// alive at the start. A spared exe ([`Self::spare`]) and everything below
    /// it is no descendant, and keeps running.
    pub fn close_tracked(&mut self, key: &str) -> bool {
        let Some(root) = self.proc_for(key) else {
            return false;
        };
        let Ok(Tree { descendants, .. }) = children(root.pid, &self.spared) else {
            self.drop_key(key);
            return false;
        };

        let targets: Vec<Target> = std::iter::once(root)
            .chain(descendants.iter().map(|c| Target {
                pid: c.pid,
                identity: c.identity,
            }))
            .collect();
        close_tree(&targets);

        self.drop_key(key);
        true
    }

    /// The live root process for `key`, or None. Drops the entry on any mismatch
    /// so callers never act on stale state.
    fn proc_for(&mut self, key: &str) -> Option<Target> {
        let slot = self.state.get(key)?;
        let entry = match slot {
            Slot::Entry(entry) => entry.clone(),
            // `if not entry: return None` in Python: kept, but never valid.
            Slot::Other(value) if is_falsy(value) => return None,
            // Python crashes on these (KeyError, TypeError); dropping is the
            // closest non-crashing behaviour.
            Slot::Other(_) => {
                self.drop_key(key);
                return None;
            }
        };
        let live = u32::try_from(entry.pid)
            .ok()
            .and_then(|pid| identified(pid).ok().map(|(ident, ct)| (pid, ident, ct)));
        match live {
            Some((pid, identity, ct)) if (ct - entry.create_time).abs() <= CREATE_TIME_EPSILON => {
                Some(Target { pid, identity })
            }
            // Gone, inaccessible, or the PID now belongs to another process.
            _ => {
                self.drop_key(key);
                None
            }
        }
    }

    fn set(&mut self, key: &str, pid: u32, create_time: f64, exe_path: &str) {
        let slot = Slot::Entry(Entry {
            pid: u64::from(pid),
            create_time,
            exe_path: Some(exe_path.to_owned()),
        });
        // Assigning an existing dict key keeps its position in Python.
        match self.state.0.iter_mut().find(|(k, _)| k == key) {
            Some((_, existing)) => *existing = slot,
            None => self.state.0.push((key.to_owned(), slot)),
        }
    }

    fn drop_key(&mut self, key: &str) {
        if let Some(i) = self.state.0.iter().position(|(k, _)| k == key) {
            self.state.0.remove(i);
            self.save();
        }
    }

    /// Writes `<file>.tmp` then replaces the state file. Errors are swallowed.
    fn save(&self) {
        write_state(&self.state_file, to_python_json(&self.state));
    }
}

// ---------------------------------------------------------------------------
// Process tree (psutil semantics)
// ---------------------------------------------------------------------------

/// A `psutil.Process` as the Python holds it: the PID and the identity taken
/// when the object was made (`_ident`, the fast creation time or `None`).
#[derive(Debug, Clone, Copy, PartialEq)]
struct Target {
    pid: u32,
    identity: Option<f64>,
}

#[derive(Debug, Clone)]
struct Descendant {
    pid: u32,
    identity: Option<f64>,
    name: String,
}

/// What [`children`] found below a root.
#[derive(Debug, Default)]
struct Tree {
    /// Depth first.
    descendants: Vec<Descendant>,
    /// Exe names of the spared processes the walk stopped at.
    spared: Vec<String>,
}

/// `psutil.Process(pid)` then `.create_time()`: the identity, and the creation
/// time, which psutil caches from the identity or reads through the slower
/// fallback when the fast path was refused.
fn identified(pid: u32) -> Result<(Option<f64>, f64), ProcError> {
    let ident = identity(pid)?;
    let ct = match ident {
        Some(ct) => ct,
        None => create_time(pid)?,
    };
    Ok((ident, ct))
}

/// psutil's `Process.children(recursive=True)`: walk a snapshot of parent PIDs
/// depth first, keeping a process only if it was created at or after the root.
/// That guard (against the root, not each parent) skips processes whose parent
/// PID was recycled by the root.
///
/// Unlike psutil, the walk stops at a process whose lowercased exe name is in
/// `spared`: it is noted by name before it is opened, and nothing below it is
/// visited.
fn children(root_pid: u32, spared: &[String]) -> Result<Tree, ProcError> {
    let snap = snapshot();
    let root_ctime = create_time(root_pid)?;
    let mut by_parent: HashMap<u32, Vec<usize>> = HashMap::new();
    for (i, p) in snap.iter().enumerate() {
        by_parent.entry(p.ppid).or_default().push(i);
    }
    let mut out = Tree::default();
    let mut seen = HashSet::new();
    let mut stack = vec![root_pid];
    while let Some(pid) = stack.pop() {
        // PIDs can be reused while the snapshot is taken, so the "tree" may
        // contain a cycle.
        if !seen.insert(pid) {
            continue;
        }
        for &i in by_parent.get(&pid).map(Vec::as_slice).unwrap_or_default() {
            let child = &snap[i];
            if spared.contains(&child.name.to_lowercase()) {
                out.spared.push(child.name.clone());
                continue;
            }
            match identified(child.pid) {
                Ok((identity, ct)) if root_ctime <= ct => {
                    out.descendants.push(Descendant {
                        pid: child.pid,
                        identity,
                        name: child.name.clone(),
                    });
                    stack.push(child.pid);
                }
                Ok(_) | Err(ProcError::NoSuchProcess) => {}
                Err(ProcError::AccessDenied) => return Err(ProcError::AccessDenied),
            }
        }
    }
    Ok(out)
}

/// The terminate, wait and kill passes of `close_tracked`.
fn close_tree(targets: &[Target]) {
    for t in targets {
        terminate(t);
    }

    // Some descendants (Stream Deck's uiAccess-related helpers) deny
    // SYNCHRONIZE, so waiting fails; Python then treats nothing as alive and
    // skips the kill pass entirely. terminate() above already signalled them.
    let alive = wait_procs(targets, CLOSE_WAIT).unwrap_or_default();
    for t in &alive {
        terminate(t);
    }
}

/// psutil's `terminate()`/`kill()` (identical on Windows): `_raise_if_pid_reused`
/// compares a fresh identity with the one the object was made with and skips a
/// PID now owned by another process, otherwise `TerminateProcess`. The check and
/// the kill go through one handle. Errors are swallowed.
fn terminate(t: &Target) {
    let _ = kill_if_same(t.pid, t.identity);
}

/// psutil's `wait_procs(targets, timeout)`: returns the processes still alive.
/// Each round waits at most one second in total, spread over the survivors.
fn wait_procs(targets: &[Target], timeout: Duration) -> Result<Vec<Target>, ProcError> {
    let deadline = Instant::now() + timeout;
    let mut alive: Vec<Target> = targets.to_vec();
    'rounds: while !alive.is_empty() {
        let per_proc = Duration::from_secs_f64(1.0 / alive.len() as f64);
        let mut gone = Vec::new();
        for t in &alive {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                alive.retain(|a| !gone.contains(a));
                break 'rounds;
            }
            if proc_wait(t.pid, remaining.min(per_proc))? == WaitOutcome::Exited {
                gone.push(*t);
            }
        }
        alive.retain(|a| !gone.contains(a));
    }
    // Last attempt over the survivors, without waiting.
    let mut survivors = Vec::new();
    for t in alive {
        if proc_wait(t.pid, Duration::ZERO)? == WaitOutcome::TimedOut {
            survivors.push(t);
        }
    }
    Ok(survivors)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A dummy tree the test starts itself, `cmd` running `ping`, so neither
    /// can be a real app. Members are only ever killed by PID.
    struct DummyTree {
        root: std::process::Child,
        members: Vec<Target>,
    }

    impl DummyTree {
        fn spawn() -> Self {
            let root = std::process::Command::new("cmd")
                .args(["/c", "ping -n 60 127.0.0.1 >nul"])
                .stdout(std::process::Stdio::null())
                .spawn()
                .unwrap();
            Self {
                root,
                members: Vec::new(),
            }
        }

        fn alive(pid: u32) -> bool {
            proc_wait(pid, Duration::ZERO) == Ok(WaitOutcome::TimedOut)
        }
    }

    impl Drop for DummyTree {
        fn drop(&mut self) {
            for t in &self.members {
                let _ = kill_if_same(t.pid, t.identity);
            }
            let _ = self.root.kill();
            let _ = self.root.wait();
        }
    }

    #[test]
    fn close_spares_a_member_whose_identity_no_longer_matches() {
        let mut tree = DummyTree::spawn();
        let root_pid = tree.root.id();
        let start = Instant::now();
        let ping = loop {
            let found = children(root_pid, &[])
                .unwrap()
                .descendants
                .into_iter()
                .find(|c| c.name.eq_ignore_ascii_case("ping.exe"));
            if let Some(ping) = found {
                break ping;
            }
            assert!(
                start.elapsed() < Duration::from_secs(10),
                "ping never started"
            );
            std::thread::sleep(Duration::from_millis(20));
        };
        let (root_ident, _) = identified(root_pid).unwrap();
        let mut targets = vec![Target {
            pid: root_pid,
            identity: root_ident,
        }];
        for c in children(root_pid, &[]).unwrap().descendants {
            targets.push(Target {
                pid: c.pid,
                identity: c.identity,
            });
        }
        tree.members = targets.clone();
        assert!(ping.identity.is_some(), "our own child is readable");

        // The ping as an earlier holder of its PID would have recorded it: the
        // PID changed hands after enumeration, so it is now another process.
        for t in &mut targets {
            if t.pid == ping.pid {
                t.identity = ping.identity.map(|ct| ct - 1.0);
            }
        }
        close_tree(&targets);

        assert!(DummyTree::alive(ping.pid), "a reused PID is never killed");
        for t in targets.iter().filter(|t| t.pid != ping.pid) {
            assert!(!DummyTree::alive(t.pid), "tree member {} survived", t.pid);
        }
        assert!(tree.root.try_wait().unwrap().is_some());
    }
}
