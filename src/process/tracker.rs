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
    ProcError, WaitOutcome, create_time, pids_by_exe, proc_kill, proc_wait, shell_open, snapshot,
    spawn_detached,
};

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
                create_time: c.create_time,
            }))
            .collect();
        for t in &targets {
            terminate(t);
        }

        // Some descendants (Stream Deck's uiAccess-related helpers) deny
        // SYNCHRONIZE, so waiting fails; Python then treats nothing as alive and
        // skips the kill pass entirely. terminate() above already signalled them.
        let alive = wait_procs(&targets, CLOSE_WAIT).unwrap_or_default();
        for t in &alive {
            terminate(t);
        }

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
            .and_then(|pid| create_time(pid).ok().map(|ct| (pid, ct)));
        match live {
            Some((pid, ct)) if (ct - entry.create_time).abs() <= CREATE_TIME_EPSILON => {
                Some(Target {
                    pid,
                    create_time: ct,
                })
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
        let mut tmp = OsString::from(self.state_file.as_os_str());
        tmp.push(".tmp");
        let tmp = PathBuf::from(tmp);
        if std::fs::write(&tmp, to_python_json(&self.state)).is_ok() {
            let _ = std::fs::rename(&tmp, &self.state_file);
        }
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
fn to_python_json(state: &State) -> String {
    let pretty = serde_json::to_string_pretty(state).unwrap_or_else(|_| "{}".to_owned());
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
    out
}

/// `ntpath.basename`.
fn basename(path: &str) -> &str {
    path.rsplit(['\\', '/', ':']).next().unwrap_or(path)
}

// ---------------------------------------------------------------------------
// Process tree (psutil semantics)
// ---------------------------------------------------------------------------

/// A process identified by PID and creation time.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Target {
    pid: u32,
    create_time: f64,
}

#[derive(Debug, Clone)]
struct Descendant {
    pid: u32,
    create_time: f64,
    name: String,
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
            match create_time(child.pid) {
                Ok(ct) if root_ctime <= ct => {
                    out.push(Descendant {
                        pid: child.pid,
                        create_time: ct,
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

/// psutil's `terminate()`/`kill()` (identical on Windows): skip a PID now owned
/// by another process, otherwise `TerminateProcess`. Errors are swallowed.
fn terminate(t: &Target) {
    if matches!(create_time(t.pid), Ok(ct) if ct != t.create_time) {
        return;
    }
    let _ = proc_kill(t.pid);
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
            to_python_json(&state),
            "{\r\n  \"Crew Chief\": {\r\n    \"pid\": 1234,\r\n    \"create_time\": 1762851234.5678234,\r\n    \"exe_path\": \"C:\\\\Caf\\u00e9\\\\a.exe\"\r\n  }\r\n}"
        );
        assert_eq!(to_python_json(&State::default()), "{}");
    }

    #[test]
    fn escapes_astral_and_del_like_python() {
        let state = parse(r#"{"k": {"pid": 1, "create_time": 2.0, "exe_path": "\u007f😀"}}"#);
        let out = to_python_json(&state);
        assert!(out.contains(r#""exe_path": "\u007f\ud83d\ude00""#), "{out}");
        assert!(out.contains(r#""create_time": 2.0"#), "{out}");
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
    fn basename_matches_ntpath() {
        assert_eq!(
            basename(r"C:\Program Files\Elgato\StreamDeck.exe"),
            "StreamDeck.exe"
        );
        assert_eq!(basename("C:/a/b.exe"), "b.exe");
        assert_eq!(basename("C:b.exe"), "b.exe");
        assert_eq!(basename("b.exe"), "b.exe");
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
}
