//! Runs the Python reference next to the Rust port. Ignored by default because
//! it needs Python with winshell; run with `cargo test --test
//! detect_python_crosscheck -- --ignored --nocapture`.

mod detect_support;

use std::path::{Path, PathBuf};
use std::process::Command;

use detect_support::Tree;
use iracing_companion_launcher::detect::paths::find_path_in_list;
use iracing_companion_launcher::detect::shortcuts::{
    find_shortcut_target, find_shortcut_target_in,
};
use iracing_companion_launcher::detect::steam::check_steam_game_installed;

fn repo() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// Runs `body` (Python using `json`, `sys` and the `utils` functions) with the
/// given extra environment and returns its stdout parsed as JSON.
fn python(body: &str, env: &[(&str, &Path)]) -> serde_json::Value {
    let script = format!(
        "import json, sys\nsys.path.insert(0, {repo:?})\n\
         from iracing_launcher_app.utils.path_finder import find_shortcut_target, find_path_in_list\n\
         from iracing_launcher_app.utils.steam_registry import check_steam_game_installed\n\
         import contextlib, io\n{body}\n",
        repo = repo().to_string_lossy()
    );
    let mut command = Command::new("python");
    command
        .arg("-c")
        .arg(script)
        .env("PYTHONIOENCODING", "utf-8");
    for (key, value) in env {
        command.env(key, value);
    }
    let out = command.output().expect("python on PATH");
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice(&out.stdout).unwrap()
}

#[test]
#[ignore = "needs Python with winshell"]
fn fake_tree_matches_python() {
    let tree = Tree::with_prefix("ircl-crosscheck-ü-");
    // Python reads the roots from %APPDATA% and %ProgramData%.
    let appdata = tree.dir.path().join("appdata");
    let programdata = tree.dir.path().join("programdata");
    let suffix = r"Microsoft\Windows\Start Menu\Programs";
    let user = appdata.join(suffix);
    let common = programdata.join(suffix);
    let roots = vec![user.clone(), common.clone()];

    let a = tree.target("a.exe");
    let b = tree.target(r"Prögrâm\b.exe");
    let gone = tree.target("gone.exe");
    let c = tree.target("c.exe");
    tree.lnk(&user, r"A\X\Fanatec.lnk", &a);
    tree.lnk(&user, r"B\Y\Fanatec.lnk", &b);
    tree.lnk(&user, r"B\Fanatec Control Panel.lnk", &a);
    tree.lnk(&user, r"Z\Crew Chief V4.lnk", &gone);
    tree.lnk(&common, r"Britton\CrewChiefV4.lnk", &c);
    tree.lnk(&common, r"Deep\Er\Still\CREWCHIEFV4.LNK", &a);
    std::fs::create_dir_all(user.join("Junk")).unwrap();
    std::fs::write(user.join(r"Junk\SimHub.lnk"), b"corrupt").unwrap();
    tree.lnk(&common, r"Sim\SimHub.lnk", &c);
    std::fs::remove_file(&gone).unwrap();

    let cases: &[&[&str]] = &[
        &["Fanatec.lnk", "Fanatec Control Panel.lnk"],
        &["Fanatec Control Panel.lnk", "Fanatec.lnk"],
        &["Crew Chief V4.lnk", "CrewChiefV4.lnk"],
        &["SimHub.lnk"],
        &["Nothing.lnk"],
    ];
    let mut rust = Vec::new();
    for names in cases {
        for is_game in [false, true] {
            rust.push(find_shortcut_target_in(&roots, names, is_game));
        }
    }
    let body = format!(
        "cases = {cases:?}\nout = []\n\
         for names in cases:\n    for g in (False, True):\n        out.append(find_shortcut_target(names, g))\n\
         print(json.dumps(out))"
    );
    let expected: Vec<Option<String>> = serde_json::from_value(python(
        &body,
        &[("APPDATA", &appdata), ("ProgramData", &programdata)],
    ))
    .unwrap();
    for (i, (r, p)) in rust.iter().zip(&expected).enumerate() {
        println!("case {i}: rust={r:?} python={p:?}");
    }
    assert_eq!(rust, expected);
}

const APPS: &[(&str, &[&str], &[&str])] = &[
    (
        "Fanatec",
        &["Fanatec.lnk", "Fanatec Control Panel.lnk"],
        &[
            r"C:\Program Files\Fanatec\FanatecUI\UI\Fanatec.exe",
            r"C:\Program Files (x86)\Fanatec\FanatecUI\UI\Fanatec.exe",
        ],
    ),
    (
        "Crew Chief V4",
        &["Crew Chief V4.lnk", "CrewChiefV4.lnk"],
        &[
            r"C:\Program Files (x86)\Britton IT Ltd\CrewChiefV4\CrewChiefV4.exe",
            r"C:\Program Files\Britton IT Ltd\CrewChiefV4\CrewChiefV4.exe",
        ],
    ),
    (
        "Trading Paints",
        &["Trading Paints.lnk"],
        &[
            r"C:\Program Files (x86)\Rhinode LLC\Trading Paints\Trading Paints.exe",
            r"C:\Program Files\Rhinode LLC\Trading Paints\Trading Paints.exe",
        ],
    ),
    (
        "SimHub",
        &["SimHub.lnk"],
        &[
            r"C:\Program Files (x86)\SimHub\SimHubWPF.exe",
            r"C:\Program Files\SimHub\SimHubWPF.exe",
        ],
    ),
    (
        "Garage61",
        &[
            "Garage 61 Telemetry Agent.lnk",
            "Garage61.lnk",
            "garage61.lnk",
        ],
        &[],
    ),
    (
        "Elgato Stream Deck",
        &["Elgato Stream Deck.lnk", "Stream Deck.lnk"],
        &[r"C:\Program Files\Elgato\StreamDeck\StreamDeck.exe"],
    ),
    (
        "MarvinsAIRA Refactored",
        &[
            "Marvins Awesome iRacing App.lnk",
            "MarvinsAIRARefactored.lnk",
        ],
        &[r"C:\Program Files\Marvins Awesome iRacing App - Refactored\MarvinsAIRARefactored.exe"],
    ),
];

const GAMES: &[(&str, &[&str], &str)] = &[
    (
        "iRacing",
        &["iRacing.lnk", "iRacing Simulator.lnk"],
        "266410",
    ),
    (
        "Assetto Corsa Competizione",
        &["Assetto Corsa Competizione.lnk"],
        "805550",
    ),
    ("Assetto Corsa Evo", &["Assetto Corsa Evo.lnk"], "3058630"),
    ("Automobilista 2", &["Automobilista 2.lnk"], "1066890"),
    ("rFactor 2", &["rFactor 2.lnk"], "365960"),
];

/// Read-only sanity check against this machine's real Start Menu and registry.
#[test]
#[ignore = "needs Python with winshell; reads the real Start Menu and registry"]
fn real_machine_matches_python() {
    let garage61 = std::env::var("APPDATA").unwrap() + r"\garage61-install\garage61-launcher.exe";
    let mut rust = Vec::new();
    for (_, names, paths) in APPS {
        let mut paths: Vec<String> = paths.iter().map(|p| p.to_string()).collect();
        if paths.is_empty() {
            paths.push(garage61.clone());
        }
        rust.push(serde_json::json!([
            find_shortcut_target(names, false),
            find_path_in_list(&paths),
        ]));
    }
    for (_, names, appid) in GAMES {
        rust.push(serde_json::json!([
            find_shortcut_target(names, true),
            check_steam_game_installed(appid),
        ]));
    }

    let apps: Vec<_> = APPS.iter().map(|(_, n, p)| (n, p)).collect();
    let games: Vec<_> = GAMES.iter().map(|(_, n, a)| (n, a)).collect();
    let body = format!(
        "apps = {apps}\ngames = {games}\ng61 = {garage61:?}\nout = []\n\
         with contextlib.redirect_stdout(io.StringIO()):\n\
         \x20   for names, paths in apps:\n\
         \x20       out.append([find_shortcut_target(names, False), find_path_in_list(paths or [g61])])\n\
         \x20   for names, appid in games:\n\
         \x20       out.append([find_shortcut_target(names, True), check_steam_game_installed(appid)])\n\
         print(json.dumps(out))",
        apps = py_list(
            &apps
                .iter()
                .map(|(n, p)| format!("({}, {})", py_strs(n), py_strs(p)))
                .collect::<Vec<_>>()
        ),
        games = py_list(
            &games
                .iter()
                .map(|(n, a)| format!("({}, {a:?})", py_strs(n)))
                .collect::<Vec<_>>()
        ),
    );
    let python = python(&body, &[]);
    let labels = APPS.iter().map(|a| a.0).chain(GAMES.iter().map(|g| g.0));
    for ((label, r), p) in labels.zip(&rust).zip(python.as_array().unwrap()) {
        println!("{label}\n  rust:   {r}\n  python: {p}");
    }
    assert_eq!(serde_json::Value::Array(rust), python);
}

fn py_strs(items: &[&str]) -> String {
    py_list(&items.iter().map(|s| format!("{s:?}")).collect::<Vec<_>>())
}

fn py_list(items: &[String]) -> String {
    format!("[{}]", items.join(", "))
}
