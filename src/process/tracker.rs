//! Ports `managers/process_tracker.py`: launches apps and tracks their process trees across restarts.
//!
//! State lives in `process_state.json`, shared with the Python version so an
//! upgrade keeps tracking the apps the old launcher started:
//! `{ "<App Name>": { "pid": int, "create_time": float, "exe_path": str } }`.
//! Each entry is valid only while its PID is alive with the same creation time,
//! which guards against PID reuse after a restart.

use std::collections::{HashMap, HashSet};
use std::ffi::OsString;
use std::fmt;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use serde::de::{MapAccess, Visitor};
use serde::ser::SerializeMap;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::Value;

use super::{
    ProcError, WaitOutcome, create_time, identity, kill_if_same, pids_by_exe, proc_wait,
    shell_open, snapshot, spawn_detached,
};
use crate::win::basename;

/// Tolerance (seconds) when comparing persisted vs. live creation time. Python
/// stores wall-clock seconds, so a small NTP correction can shift it slightly.
const CREATE_TIME_EPSILON: f64 = 0.5;

/// How long `close_tracked` waits for terminated processes before killing survivors.
const CLOSE_WAIT: Duration = Duration::from_secs(3);

#[derive(Debug)]
pub struct ProcessTracker {
    state_file: PathBuf,
    state: State,
}

impl ProcessTracker {
    /// Loads the state file and drops every entry whose process is gone or whose
    /// PID was recycled. A missing or unreadable file means nothing is tracked.
    pub fn new(state_file: PathBuf) -> Self {
        let mut tracker = Self {
            state: load(&state_file),
            state_file,
        };
        let keys: Vec<String> = tracker.state.0.iter().map(|(k, _)| k.clone()).collect();
        for key in keys {
            // Called for its side effect of dropping stale entries.
            tracker.proc_for(&key);
        }
        tracker
    }

    /// Launches `exe_path` and remembers its PID under `key`. True if the
    /// launched process is alive after `wait`.
    ///
    /// `needs_shell_launch` goes through `ShellExecute` instead of a raw
    /// `CreateProcess`, which Windows refuses (error 740) for manifests with
    /// `uiAccess="true"` such as Elgato Stream Deck. The shell does not hand back
    /// a PID, so the new process is found by exe name afterwards.
    pub fn launch_and_track(
        &mut self,
        key: &str,
        exe_path: &str,
        wait: Duration,
        needs_shell_launch: bool,
    ) -> bool {
        if needs_shell_launch {
            let exe_name = basename(exe_path);
            let before: HashSet<u32> = pids_by_exe(exe_name).into_iter().collect();
            if shell_open(exe_path).is_err() {
                return false;
            }
            std::thread::sleep(wait);
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

        std::thread::sleep(wait);
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
        match children(root.pid) {
            Ok(descendants) => (true, descendants.len()),
            Err(_) => {
                self.drop_key(key);
                (false, 0)
            }
        }
    }

    /// Exe names of the tracked process's descendants, depth first, or empty.
    pub fn get_child_names(&mut self, key: &str) -> Vec<String> {
        let Some(root) = self.proc_for(key) else {
            return Vec::new();
        };
        let Ok(descendants) = children(root.pid) else {
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

    /// Terminates the tracked process and all its descendants, waits up to 3 s,
    /// kills the survivors and forgets the entry. True if the tracked process was
    /// alive at the start.
    pub fn close_tracked(&mut self, key: &str) -> bool {
        let Some(root) = self.proc_for(key) else {
            return false;
        };
        let descendants = match children(root.pid) {
            Ok(d) => d,
            Err(_) => {
                self.drop_key(key);
                return false;
            }
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

    /// Forgets a tracked entry without killing the process.
    pub fn forget(&mut self, key: &str) {
        self.drop_key(key);
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

/// The write half of `save`. A failed serialisation writes nothing: the file on disk keeps the
/// last good state rather than being replaced by an empty object.
fn write_state(state_file: &std::path::Path, json: serde_json::Result<String>) {
    let Ok(json) = json else {
        return;
    };
    let mut tmp = OsString::from(state_file.as_os_str());
    tmp.push(".tmp");
    let tmp = PathBuf::from(tmp);
    if std::fs::write(&tmp, json).is_ok() {
        let _ = std::fs::rename(&tmp, state_file);
    }
}

// ---------------------------------------------------------------------------
// State file
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct Entry {
    pid: u64,
    create_time: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    exe_path: Option<String>,
}

/// An entry as found in the file: well-formed, or anything else Python would
/// have loaded into its dict.
#[derive(Debug, Clone, PartialEq)]
enum Slot {
    Entry(Entry),
    Other(Value),
}

impl Serialize for Slot {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        match self {
            Slot::Entry(e) => e.serialize(s),
            Slot::Other(v) => v.serialize(s),
        }
    }
}

/// Top-level object in file order, like a Python dict.
#[derive(Debug, Default)]
struct State(Vec<(String, Slot)>);

impl State {
    fn get(&self, key: &str) -> Option<&Slot> {
        self.0.iter().find(|(k, _)| k == key).map(|(_, v)| v)
    }
}

impl Serialize for State {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let mut map = s.serialize_map(Some(self.0.len()))?;
        for (k, v) in &self.0 {
            map.serialize_entry(k, v)?;
        }
        map.end()
    }
}

/// Deserialises a JSON object; anything else is "not a dict" and yields None.
struct MaybeState(Option<State>);

impl<'de> Deserialize<'de> for MaybeState {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct V;
        impl<'de> Visitor<'de> for V {
            type Value = MaybeState;
            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("any JSON value")
            }
            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<MaybeState, A::Error> {
                let mut out: Vec<(String, Slot)> = Vec::new();
                while let Some((k, v)) = map.next_entry::<String, Value>()? {
                    let slot = match serde_json::from_value::<Entry>(v.clone()) {
                        Ok(e) if v.is_object() => Slot::Entry(e),
                        _ => Slot::Other(v),
                    };
                    // Duplicate keys: the last value wins at the first position.
                    match out.iter_mut().find(|(key, _)| *key == k) {
                        Some((_, existing)) => *existing = slot,
                        None => out.push((k, slot)),
                    }
                }
                Ok(MaybeState(Some(State(out))))
            }
            fn visit_seq<A: serde::de::SeqAccess<'de>>(
                self,
                mut seq: A,
            ) -> Result<MaybeState, A::Error> {
                while seq.next_element::<serde::de::IgnoredAny>()?.is_some() {}
                Ok(MaybeState(None))
            }
            fn visit_bool<E>(self, _: bool) -> Result<MaybeState, E> {
                Ok(MaybeState(None))
            }
            fn visit_i64<E>(self, _: i64) -> Result<MaybeState, E> {
                Ok(MaybeState(None))
            }
            fn visit_u64<E>(self, _: u64) -> Result<MaybeState, E> {
                Ok(MaybeState(None))
            }
            fn visit_f64<E>(self, _: f64) -> Result<MaybeState, E> {
                Ok(MaybeState(None))
            }
            fn visit_str<E>(self, _: &str) -> Result<MaybeState, E> {
                Ok(MaybeState(None))
            }
            fn visit_unit<E>(self) -> Result<MaybeState, E> {
                Ok(MaybeState(None))
            }
        }
        d.deserialize_any(V)
    }
}

/// Python's `_load`: a missing file, unreadable file, invalid JSON (UTF-8 BOM
/// included) or a non-object all mean an empty state. Nothing is written back.
fn load(path: &std::path::Path) -> State {
    let Ok(bytes) = std::fs::read(path) else {
        return State::default();
    };
    match serde_json::from_slice::<MaybeState>(&bytes) {
        Ok(MaybeState(Some(state))) => state,
        _ => State::default(),
    }
}

/// Python truthiness of a JSON value.
fn is_falsy(v: &Value) -> bool {
    match v {
        Value::Null => true,
        Value::Bool(b) => !b,
        Value::Number(n) => n.as_f64() == Some(0.0),
        Value::String(s) => s.is_empty(),
        Value::Array(a) => a.is_empty(),
        Value::Object(o) => o.is_empty(),
    }
}

/// The bytes Python's `json.dump(state, f, indent=2)` writes through a text-mode
/// file on Windows: non-ASCII escaped (`ensure_ascii`), CRLF line endings.
fn to_python_json(state: &impl Serialize) -> serde_json::Result<String> {
    let pretty = serde_json::to_string_pretty(state)?;
    let mut out = String::with_capacity(pretty.len() + 64);
    for c in pretty.chars() {
        match c {
            '\n' => out.push_str("\r\n"),
            c if (c as u32) < 0x7F => out.push(c),
            c => {
                let mut units = [0u16; 2];
                for u in c.encode_utf16(&mut units) {
                    out.push_str(&format!("\\u{u:04x}"));
                }
            }
        }
    }
    Ok(out)
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
fn children(root_pid: u32) -> Result<Vec<Descendant>, ProcError> {
    let snap = snapshot();
    let root_ctime = create_time(root_pid)?;
    let mut by_parent: HashMap<u32, Vec<usize>> = HashMap::new();
    for (i, p) in snap.iter().enumerate() {
        by_parent.entry(p.ppid).or_default().push(i);
    }
    let mut out = Vec::new();
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
            match identified(child.pid) {
                Ok((identity, ct)) if root_ctime <= ct => {
                    out.push(Descendant {
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

    fn parse(json: &str) -> State {
        match serde_json::from_str::<MaybeState>(json) {
            Ok(MaybeState(Some(s))) => s,
            _ => State::default(),
        }
    }

    #[test]
    fn writes_python_json_bytes() {
        // Expected bytes from Python: json.dump(d, f, indent=2) via open(..., "w").
        let state = parse(
            r#"{"Crew Chief": {"pid": 1234, "create_time": 1762851234.5678234, "exe_path": "C:\\Caf\u00e9\\a.exe"}}"#,
        );
        assert_eq!(
            to_python_json(&state).unwrap(),
            "{\r\n  \"Crew Chief\": {\r\n    \"pid\": 1234,\r\n    \"create_time\": 1762851234.5678234,\r\n    \"exe_path\": \"C:\\\\Caf\\u00e9\\\\a.exe\"\r\n  }\r\n}"
        );
        assert_eq!(to_python_json(&State::default()).unwrap(), "{}");
    }

    #[test]
    fn escapes_astral_and_del_like_python() {
        let state = parse(r#"{"k": {"pid": 1, "create_time": 2.0, "exe_path": "\u007f😀"}}"#);
        let out = to_python_json(&state).unwrap();
        assert!(out.contains(r#""exe_path": "\u007f\ud83d\ude00""#), "{out}");
        assert!(out.contains(r#""create_time": 2.0"#), "{out}");
    }

    /// A value whose serialisation always fails.
    struct Unserialisable;

    impl Serialize for Unserialisable {
        fn serialize<S: Serializer>(&self, _: S) -> Result<S::Ok, S::Error> {
            Err(serde::ser::Error::custom("refused"))
        }
    }

    #[test]
    fn failed_serialisation_skips_the_save() {
        let dir = std::env::temp_dir().join(format!("ircl-tracker-skip-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("process_state.json");
        let tmp = dir.join("process_state.json.tmp");
        let _ = std::fs::remove_file(&tmp);
        let good = "{
  \"a\": 1
}";
        std::fs::write(&file, good).unwrap();

        let json = to_python_json(&Unserialisable);
        assert!(json.is_err());
        write_state(&file, json);
        assert_eq!(std::fs::read_to_string(&file).unwrap(), good);
        assert!(!tmp.exists());

        // A successful serialisation still replaces the file through the temp file.
        write_state(&file, to_python_json(&State::default()));
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "{}");
        assert!(!tmp.exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn keeps_file_order_and_last_duplicate() {
        let s = parse(r#"{"b": 0, "a": {}, "b": {"pid": 5, "create_time": 1}}"#);
        let keys: Vec<&str> = s.0.iter().map(|(k, _)| k.as_str()).collect();
        assert_eq!(keys, ["b", "a"]);
        assert!(matches!(s.0[0].1, Slot::Entry(Entry { pid: 5, .. })));
    }

    #[test]
    fn non_object_top_level_is_empty() {
        for json in ["[1, 2]", "3", "\"x\"", "null", "true"] {
            assert!(matches!(
                serde_json::from_str::<MaybeState>(json),
                Ok(MaybeState(None))
            ));
        }
    }

    #[test]
    fn falsy_matches_python() {
        for v in ["null", "false", "0", "0.0", "\"\"", "[]", "{}"] {
            assert!(is_falsy(&serde_json::from_str(v).unwrap()), "{v}");
        }
        for v in ["true", "1", "\"a\"", "[0]", "{\"a\": 1}"] {
            assert!(!is_falsy(&serde_json::from_str(v).unwrap()), "{v}");
        }
    }

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
            let found = children(root_pid)
                .unwrap()
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
        for c in children(root_pid).unwrap() {
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
