//! Every character a Windows user could have in a path or a name renders, from the fonts Windows
//! links to Segoe UI, and startup reads none of those fallback fonts.
//!
//! The harness panics on any glyph no font has (egui_kittest's `MissingGlyphPolicy::Panic`), so
//! rendering a line is the test that it has no box in it.

use crate::support::{WINDOW, builder, fixtures};
use egui::epaint::text::{FontFamily, FontId, MissingGlyphPolicy};
use iracing_companion_launcher::model::{AppCard, GameCard, LogLevel, LogLine, Status, ViewState};
use iracing_companion_launcher::ui::{self, fonts, theme};
use std::path::Path;

/// Lines with characters none of Segoe UI and Consolas has, or that only some Windows fonts do.
const RARE: &[&str] = &[
    "⚠ iRacing is still running - please close it manually",
    "✓ All apps launched ✔ ✗",
    "C:\\Users\\田中太郎\\AppData\\Roaming\\SimHub\\SimHubWPF.exe",
    "C:\\Users\\张伟\\桌面\\游戏\\赛车.lnk",
    "C:\\Users\\すずき\\ドキュメント\\レース",
    "C:\\Users\\김민준\\바탕 화면\\레이싱",
    "C:\\Users\\Иван Петров\\Документы",
    "C:\\Users\\Γιώργος\\Έγγραφα",
    "C:\\Users\\דוד כהן\\מסמכים",
    "C:\\Users\\محمد\\المستندات",
    "C:\\Users\\สมชาย\\เอกสาร",
    "C:\\Users\\राहुल\\दस्तावेज़",
    "C:\\Users\\Đặng Thị Hồng\\Tài liệu",
    "C:\\Users\\Łukasz Żółć\\Pulpit ½ ™ € £ ¥ ° ± × ÷",
];

fn rare_state() -> ViewState {
    ViewState {
        version: "3.0.0".into(),
        apps: vec![
            AppCard {
                name: "SimHub ⚠ 田中".into(),
                status: Status::Running,
                checked: true,
                helper_count: 1,
            },
            AppCard {
                name: "Crew Chief ✓ 김민준".into(),
                status: Status::NotFound,
                checked: false,
                helper_count: 0,
            },
        ],
        games: vec![GameCard {
            name: "iRacing ✓ すずき".into(),
            status: Status::Idle,
        }],
        selected_game: None,
        log: RARE
            .iter()
            .map(|text| LogLine {
                level: LogLevel::Warning,
                text: format!("[12:00:00] {text}"),
            })
            .collect(),
        select_all_label: "Deselect All".into(),
        launch_label: "Launch apps (1)".into(),
        close_label: "Close all (1)".into(),
        launch_enabled: true,
        close_enabled: true,
    }
}

/// A context with the app's fonts, failing on any glyph no font has.
fn strict_ctx() -> egui::Context {
    let ctx = egui::Context::default();
    ui::install_fonts(&ctx);
    ctx.set_missing_glyph_policy(MissingGlyphPolicy::Panic);
    ctx
}

fn frame(ctx: &egui::Context, state: &ViewState) {
    let input = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, WINDOW)),
        ..Default::default()
    };
    let mut output = ctx.run_ui(input, |ui| {
        ui::draw(ui, state);
    });
    output.textures_delta.clear();
}

#[test]
fn startup_reads_no_fallback_font() {
    let ctx = strict_ctx();
    for state in fixtures().values() {
        frame(&ctx, state);
        frame(&ctx, state);
    }
    let discovered: Vec<String> = ctx.fonts(|f| {
        f.discovered_fonts()
            .iter()
            .map(|f| f.name.clone())
            .collect()
    });
    assert!(
        discovered.is_empty(),
        "the fixture states needed fallback fonts: {discovered:?}"
    );
}

#[test]
fn every_family_has_every_rare_character() {
    let ctx = strict_ctx();
    frame(&ctx, &rare_state());
    let families = [theme::SEGOE, theme::SEGOE_BOLD, theme::CONSOLAS];
    let missing: Vec<(char, &str)> = ctx.fonts_mut(|f| {
        RARE.iter()
            .flat_map(|line| line.chars())
            .filter(|c| !c.is_control() && !c.is_whitespace())
            .flat_map(|c| families.iter().map(move |fam| (c, *fam)))
            .filter(|(c, fam)| {
                !f.has_glyph(&FontId::new(14.0, FontFamily::Name((*fam).into())), *c)
            })
            .collect()
    });
    assert!(missing.is_empty(), "no glyph for {missing:?}");

    // ⚠ and ✓ come from Segoe UI Symbol, as GDI links them for Segoe UI; CJK from a font on
    // Segoe UI's link list.
    let discovered: Vec<String> = ctx.fonts(|f| {
        f.discovered_fonts()
            .iter()
            .map(|f| f.name.to_lowercase())
            .collect()
    });
    assert!(
        discovered.iter().any(|n| n.contains("seguisym")),
        "Segoe UI Symbol not used: {discovered:?}"
    );
    let loaded = fonts::loaded_fallback_files();
    println!(
        "discovered: {discovered:?}
read: {loaded:?}"
    );
    assert!(!loaded.is_empty());
}

/// The harness, like the app, lays out every line and card: a glyph no font has panics it.
#[test]
fn rare_characters_lay_out_without_a_missing_glyph() {
    let state = rare_state();
    let mut h = builder().build_ui(|ui| {
        ui::draw(ui, &state);
    });
    h.run();
}

/// The same window rendered, for a look: `target/ui-snapshots/other/rare-glyphs.png`.
#[test]
#[ignore = "visual inspection"]
fn render_rare_characters() {
    let image = crate::support::render(&rare_state());
    let out = crate::support::repo_path("target/ui-snapshots/other");
    std::fs::create_dir_all(&out).unwrap();
    image.save(out.join("rare-glyphs.png")).unwrap();
}

#[test]
fn cmap_probe_reads_coverage() {
    let dir = Path::new(&std::env::var_os("WINDIR").unwrap_or_else(|| "C:\\Windows".into()))
        .join("Fonts");
    let segoe = dir.join("segoeui.ttf");
    let symbol = dir.join("seguisym.ttf");
    for file in [&segoe, &symbol] {
        assert!(file.exists(), "{} is part of every Windows", file.display());
    }
    assert_eq!(
        fonts::cmap::probe_file(&segoe, "Segoe UI", 'A'),
        Some((0, true))
    );
    assert_eq!(
        fonts::cmap::probe_file(&segoe, "Segoe UI", '⚠'),
        Some((0, false))
    );
    assert_eq!(
        fonts::cmap::probe_file(&symbol, "Segoe UI Symbol", '⚠'),
        Some((0, true))
    );
    // A collection: the face is found by name.
    let yahei = dir.join("msyh.ttc");
    assert!(
        yahei.exists(),
        "{} is part of every Windows",
        yahei.display()
    );
    let (index, has) = fonts::cmap::probe_file(&yahei, "Microsoft YaHei UI", '张').unwrap();
    assert!(has);
    assert_eq!(
        index, 1,
        "Microsoft YaHei UI is the second face of msyh.ttc"
    );
}
