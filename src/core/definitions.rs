//! The companion apps and racing games the launcher knows.

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppDef {
    pub name: &'static str,
    pub exe: &'static str,
    pub shortcut_names: &'static [&'static str],
    pub paths: Vec<String>,
    pub needs_shell_launch: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GameDef {
    pub name: &'static str,
    pub exe: &'static str,
    pub shortcut_names: &'static [&'static str],
    pub steam_appid: &'static str,
    pub steam_folder: &'static str,
    pub paths: &'static [&'static str],
}

const GARAGE61_TAIL: &str = r"garage61-install\garage61-launcher.exe";

/// Companion apps in Python order; Garage61's path is resolved from `%APPDATA%`.
pub fn apps() -> Vec<AppDef> {
    let garage61 = garage61_path(std::env::var("APPDATA").ok().as_deref());
    vec![
        AppDef {
            name: "Fanatec",
            exe: "Fanatec.exe",
            shortcut_names: &["Fanatec.lnk", "Fanatec Control Panel.lnk"],
            paths: strings(&[
                r"C:\Program Files\Fanatec\FanatecUI\UI\Fanatec.exe",
                r"C:\Program Files (x86)\Fanatec\FanatecUI\UI\Fanatec.exe",
            ]),
            needs_shell_launch: false,
        },
        AppDef {
            name: "Crew Chief V4",
            exe: "CrewChiefV4.exe",
            shortcut_names: &["Crew Chief V4.lnk", "CrewChiefV4.lnk"],
            paths: strings(&[
                r"C:\Program Files (x86)\Britton IT Ltd\CrewChiefV4\CrewChiefV4.exe",
                r"C:\Program Files\Britton IT Ltd\CrewChiefV4\CrewChiefV4.exe",
            ]),
            needs_shell_launch: false,
        },
        AppDef {
            name: "Trading Paints",
            exe: "Trading Paints.exe",
            shortcut_names: &["Trading Paints.lnk"],
            paths: strings(&[
                r"C:\Program Files (x86)\Rhinode LLC\Trading Paints\Trading Paints.exe",
                r"C:\Program Files\Rhinode LLC\Trading Paints\Trading Paints.exe",
            ]),
            needs_shell_launch: false,
        },
        AppDef {
            name: "SimHub",
            exe: "SimHubWPF.exe",
            shortcut_names: &["SimHub.lnk"],
            paths: strings(&[
                r"C:\Program Files (x86)\SimHub\SimHubWPF.exe",
                r"C:\Program Files\SimHub\SimHubWPF.exe",
            ]),
            needs_shell_launch: false,
        },
        AppDef {
            name: "Garage61",
            exe: "garage61-launcher.exe",
            shortcut_names: &[
                "Garage 61 Telemetry Agent.lnk",
                "Garage61.lnk",
                "garage61.lnk",
            ],
            // The Python app left this empty and filled it in when its app manager started.
            paths: vec![garage61],
            needs_shell_launch: false,
        },
        AppDef {
            name: "Elgato Stream Deck",
            exe: "StreamDeck.exe",
            shortcut_names: &["Elgato Stream Deck.lnk", "Stream Deck.lnk"],
            paths: strings(&[r"C:\Program Files\Elgato\StreamDeck\StreamDeck.exe"]),
            // StreamDeck.exe's manifest requests uiAccess="true", which Windows
            // only honours through ShellExecute; CreateProcess fails with error 740.
            needs_shell_launch: true,
        },
        AppDef {
            name: "MarvinsAIRA Refactored",
            exe: "MarvinsAIRARefactored.exe",
            shortcut_names: &[
                "Marvins Awesome iRacing App.lnk",
                "MarvinsAIRARefactored.lnk",
            ],
            paths: strings(&[
                r"C:\Program Files\Marvins Awesome iRacing App - Refactored\MarvinsAIRARefactored.exe",
            ]),
            needs_shell_launch: false,
        },
    ]
}

/// Racing games in Python order.
pub fn games() -> Vec<GameDef> {
    vec![
        GameDef {
            name: "iRacing",
            exe: "iRacingUI.exe",
            shortcut_names: &["iRacing.lnk", "iRacing Simulator.lnk"],
            steam_appid: "266410",
            steam_folder: "iRacing",
            paths: &[r"C:\Program Files (x86)\iRacing\iRacingLauncher64.exe"],
        },
        GameDef {
            name: "Assetto Corsa Competizione",
            exe: "AC2-Win64-Shipping.exe",
            shortcut_names: &["Assetto Corsa Competizione.lnk"],
            steam_appid: "805550",
            steam_folder: "Assetto Corsa Competizione",
            paths: &[],
        },
        GameDef {
            name: "Assetto Corsa Evo",
            exe: "AssettoCorsaEVO.exe",
            shortcut_names: &["Assetto Corsa Evo.lnk"],
            steam_appid: "3058630",
            steam_folder: "Assetto Corsa Evo",
            paths: &[],
        },
        GameDef {
            name: "Automobilista 2",
            exe: "AMS2AVX.exe",
            shortcut_names: &["Automobilista 2.lnk"],
            steam_appid: "1066890",
            steam_folder: "Automobilista 2",
            paths: &[],
        },
        GameDef {
            name: "rFactor 2",
            exe: "rFactor2.exe",
            shortcut_names: &["rFactor 2.lnk"],
            steam_appid: "365960",
            steam_folder: "rFactor 2",
            paths: &[],
        },
    ]
}

fn strings(paths: &[&str]) -> Vec<String> {
    paths.iter().map(|p| p.to_string()).collect()
}

/// `os.path.join(os.getenv('APPDATA'), r"garage61-install\garage61-launcher.exe")`
/// with `ntpath.join`'s separator rule. Python raises when `APPDATA` is unset;
/// here an unset variable joins like an empty one.
fn garage61_path(appdata: Option<&str>) -> String {
    let base = appdata.unwrap_or("");
    let bare_drive = base.len() == 2 && base.ends_with(':');
    if base.is_empty() || base.ends_with(['\\', '/']) || bare_drive {
        format!("{base}{GARAGE61_TAIL}")
    } else {
        format!("{base}\\{GARAGE61_TAIL}")
    }
}

#[cfg(test)]
mod tests {
    use super::garage61_path;

    // Expected values printed by Python's ntpath.join for the same inputs.
    #[test]
    fn garage61_path_joins_like_ntpath() {
        let cases = [
            (
                Some(r"C:\Users\x\AppData\Roaming"),
                r"C:\Users\x\AppData\Roaming\garage61-install\garage61-launcher.exe",
            ),
            (
                Some(r"C:\Users\x\"),
                r"C:\Users\x\garage61-install\garage61-launcher.exe",
            ),
            (Some("C:"), r"C:garage61-install\garage61-launcher.exe"),
            (
                Some("D:/data"),
                r"D:/data\garage61-install\garage61-launcher.exe",
            ),
            (Some(""), r"garage61-install\garage61-launcher.exe"),
            (None, r"garage61-install\garage61-launcher.exe"),
        ];
        for (appdata, expected) in cases {
            assert_eq!(garage61_path(appdata), expected, "APPDATA={appdata:?}");
        }
    }
}
