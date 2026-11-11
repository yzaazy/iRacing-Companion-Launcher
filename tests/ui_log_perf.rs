//! The log's cost per frame must not grow with its length, nor the cost of adding a line with the
//! number of lines before it.
//!
//! `log_cost_does_not_grow_with_length` guards that in every `cargo test`. The numbers for a
//! report come from the ignored benchmark, in a release build:
//!
//! ```text
//! cargo test --release --test ui_log_perf -- --ignored --nocapture
//! ```

use iracing_companion_launcher::contract::{AppCard, LogLevel, LogLine, Status, ViewState};
use iracing_companion_launcher::ui;
use std::time::{Duration, Instant};

const WINDOW: egui::Vec2 = egui::vec2(1200.0, 700.0);

fn state(lines: usize) -> ViewState {
    let levels = [
        LogLevel::Info,
        LogLevel::Success,
        LogLevel::Launch,
        LogLevel::Warning,
        LogLevel::Close,
        LogLevel::Error,
    ];
    ViewState {
        version: "3.0.0".into(),
        apps: [
            "Fanatec",
            "Crew Chief V4",
            "Trading Paints",
            "SimHub",
            "Garage61",
        ]
        .into_iter()
        .map(|name| AppCard {
            name: name.into(),
            status: Status::Running,
            checked: true,
            helper_count: 1,
        })
        .collect(),
        games: Vec::new(),
        selected_game: None,
        log: (0..lines).map(|i| line(&levels, i)).collect(),
        select_all_label: "Deselect All".into(),
        launch_label: "Launch apps (5)".into(),
        close_label: "Close all (5)".into(),
        launch_enabled: true,
        close_enabled: true,
    }
}

fn line(levels: &[LogLevel], i: usize) -> LogLine {
    // Every fourth line is long enough to wrap.
    let tail = if i.is_multiple_of(4) {
        " from C:\\Program Files (x86)\\Some Vendor\\Some Companion App\\bin\\app.exe"
    } else {
        ""
    };
    LogLine {
        level: levels[i % levels.len()].clone(),
        text: format!(
            "[12:{:02}:{:02}] Line {i}: something happened{tail}",
            i / 60 % 60,
            i % 60
        ),
    }
}

struct Frames {
    ctx: egui::Context,
}

impl Frames {
    fn new() -> Self {
        let ctx = egui::Context::default();
        ui::install_fonts(&ctx);
        Self { ctx }
    }

    /// One frame without a renderer: layout and tessellation, which is where the log's cost is.
    fn frame(&self, state: &ViewState) -> Duration {
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, WINDOW)),
            ..Default::default()
        };
        let start = Instant::now();
        let mut output = self.ctx.run_ui(input, |ui| {
            ui::draw(ui, state);
        });
        let _ = self.ctx.tessellate(output.shapes, output.pixels_per_point);
        let elapsed = start.elapsed();
        // No renderer here: the font atlas upload is dropped on purpose.
        output.textures_delta.clear();
        elapsed
    }
}

/// The cheapest of `n` steady frames, and the cheapest of `n` first frames after a new line.
fn measure(lines: usize, n: usize) -> (Duration, Duration) {
    let frames = Frames::new();
    let mut s = state(lines);
    for _ in 0..3 {
        frames.frame(&s);
    }
    let steady = (0..n).map(|_| frames.frame(&s)).min().unwrap();
    let levels = [LogLevel::Info];
    let append = (0..n)
        .map(|i| {
            s.log.push(line(&levels, lines + i));
            frames.frame(&s)
        })
        .min()
        .unwrap();
    (steady, append)
}

#[test]
fn log_cost_does_not_grow_with_length() {
    // Warm the process (font files, allocator) before timing anything.
    measure(10, 3);
    let (steady_short, append_short) = measure(100, 15);
    let (steady_long, append_long) = measure(2000, 15);
    // Twenty times the lines; anything that scales with them shows as a ratio far above this.
    let limit = |short: Duration| short * 3 + Duration::from_millis(2);
    assert!(
        steady_long <= limit(steady_short),
        "steady frame: {steady_short:?} at 100 lines, {steady_long:?} at 2000"
    );
    assert!(
        append_long <= limit(append_short),
        "frame after a new line: {append_short:?} at 100 lines, {append_long:?} at 2000"
    );
}

#[test]
#[ignore = "benchmark: run in release with --ignored --nocapture"]
fn log_benchmark() {
    measure(10, 3);
    println!("lines  steady frame  first frame after a new line");
    for lines in [100, 500, 2000] {
        let (steady, append) = measure(lines, 20);
        println!("{lines:>5}  {steady:>12.2?}  {append:>12.2?}");
    }
}
