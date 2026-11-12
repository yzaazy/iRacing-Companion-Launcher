//! The log's cost per frame must not grow with its length, nor the cost of adding a line with the
//! number of lines before it.
//!
//! `log_cost_does_not_grow_with_length` guards that in every `cargo test`. The numbers for a
//! report come from the ignored benchmark, in a release build:
//!
//! ```text
//! cargo test --release --test ui log_benchmark -- --ignored --nocapture
//! ```

use crate::support::WINDOW;
use iracing_companion_launcher::contract::{AppCard, LogLevel, LogLine, Status, ViewState};
use iracing_companion_launcher::ui;
use std::time::{Duration, Instant};

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
    let mut log = Log::new(lines);
    let steady = (0..n).map(|_| log.frame()).min().unwrap();
    let append = (0..n).map(|_| log.append()).min().unwrap();
    (steady, append)
}

/// A window on a log of some length, warmed up.
struct Log {
    frames: Frames,
    state: ViewState,
}

impl Log {
    fn new(lines: usize) -> Self {
        let log = Self {
            frames: Frames::new(),
            state: state(lines),
        };
        for _ in 0..3 {
            log.frame();
        }
        log
    }

    fn frame(&self) -> Duration {
        self.frames.frame(&self.state)
    }

    /// The first frame after a new line.
    fn append(&mut self) -> Duration {
        let n = self.state.log.len();
        self.state.log.push(line(&[LogLevel::Info], n));
        self.frame()
    }
}

/// Lines in the short and the long log. A cost per line in view is the same for both; a cost per
/// line in the log is 100 times larger in the long one.
const SHORT: usize = 100;
const LONG: usize = 10_000;
/// Frames measured per log; the cheapest counts.
const ROUNDS: usize = 30;

#[test]
fn log_cost_does_not_grow_with_length() {
    // Short and long frames alternate, so whatever else the machine is doing (other tests run
    // beside this one) weighs on both alike, and the cheapest of each is compared.
    let mut short = Log::new(SHORT);
    let mut long = Log::new(LONG);
    let mut steady = (Duration::MAX, Duration::MAX);
    let mut append = (Duration::MAX, Duration::MAX);
    for _ in 0..ROUNDS {
        steady.0 = steady.0.min(short.frame());
        steady.1 = steady.1.min(long.frame());
    }
    for _ in 0..ROUNDS {
        append.0 = append.0.min(short.append());
        append.1 = append.1.min(long.append());
    }
    // A frame costs about 0.5 ms with optimised dependencies, and the long log measures within
    // 5 % of the short one. This limit fails on any per-line cost above about 50 ns.
    let limit = |short: Duration| short * 3 / 2 + Duration::from_micros(250);
    assert!(
        steady.1 <= limit(steady.0),
        "steady frame: {:?} at {SHORT} lines, {:?} at {LONG}",
        steady.0,
        steady.1
    );
    assert!(
        append.1 <= limit(append.0),
        "frame after a new line: {:?} at {SHORT} lines, {:?} at {LONG}",
        append.0,
        append.1
    );
}

#[test]
#[ignore = "benchmark: run in release with --ignored --nocapture"]
fn log_benchmark() {
    measure(10, 3);
    println!("lines  steady frame  first frame after a new line");
    for lines in [SHORT, 2000, LONG] {
        let (steady, append) = measure(lines, 20);
        println!("{lines:>5}  {steady:>12.2?}  {append:>12.2?}");
    }
}
