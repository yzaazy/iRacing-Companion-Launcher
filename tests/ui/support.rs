//! What the UI tests share: the fixture states and the measured spec, harnesses that render on one
//! wgpu device, the resolved geometry, and pixel probes.

use eframe::egui_wgpu::{self, WgpuSetup, WgpuSetupExisting};
use egui::{Color32, Key, Pos2, Vec2};
use egui_kittest::wgpu::{WgpuTestRenderer, create_render_state, default_wgpu_setup};
use egui_kittest::{Harness, HarnessBuilder, LazyRenderer};
use iracing_companion_launcher::contract::{UiAction, ViewState};
use iracing_companion_launcher::ui::{self, sections};
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// The Python window's size, at which the reference screenshots were taken.
pub const WINDOW: Vec2 = egui::vec2(1200.0, 700.0);

/// The fixture states every reference image has, in `fixtures.json`.
pub const STATES: [&str; 5] = ["all-missing", "idle", "log-full", "mixed", "none-selected"];

pub fn repo_path(rel: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(rel)
}

fn read_repo_file(rel: &str) -> String {
    let path = repo_path(rel);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

/// `docs/rewrite/fixtures.json`, read once.
pub fn fixtures() -> &'static BTreeMap<String, ViewState> {
    static FIXTURES: OnceLock<BTreeMap<String, ViewState>> = OnceLock::new();
    FIXTURES.get_or_init(|| {
        let all: BTreeMap<String, ViewState> =
            serde_json::from_str(&read_repo_file("docs/rewrite/fixtures.json"))
                .unwrap_or_else(|e| panic!("fixtures.json does not match ViewState: {e}"));
        assert_eq!(
            all.keys().map(String::as_str).collect::<Vec<_>>(),
            STATES,
            "fixtures.json has other states than the reference images"
        );
        all
    })
}

pub fn fixture(name: &str) -> ViewState {
    fixtures()
        .get(name)
        .unwrap_or_else(|| panic!("no fixture state {name}"))
        .clone()
}

/// `docs/rewrite/ui-spec.json`, read once.
pub fn spec() -> &'static Value {
    static SPEC: OnceLock<Value> = OnceLock::new();
    SPEC.get_or_init(|| serde_json::from_str(&read_repo_file("docs/rewrite/ui-spec.json")).unwrap())
}

/// One wgpu device for the whole process, created on the first render. Every harness renders on
/// it through its own egui renderer. Creating several devices at once crashes the headless
/// adapter now and then; one device used from several threads does not.
fn shared_device() -> WgpuSetupExisting {
    static DEVICE: OnceLock<WgpuSetupExisting> = OnceLock::new();
    DEVICE
        .get_or_init(|| {
            let state = create_render_state(
                default_wgpu_setup(),
                egui_wgpu::RendererOptions::PREDICTABLE,
            );
            WgpuSetupExisting {
                instance: state.instance,
                adapter: state.adapter,
                device: state.device,
                queue: state.queue,
            }
        })
        .clone()
}

/// A renderer on the shared device, made only when the harness first renders.
fn renderer() -> LazyRenderer {
    LazyRenderer::new(|| WgpuTestRenderer::from_setup(WgpuSetup::Existing(shared_device())))
}

/// The window at 1200x700 and one pixel per point, with the app's fonts and the shared device.
pub fn builder<S>() -> HarnessBuilder<S> {
    HarnessBuilder::default()
        .with_size(WINDOW)
        .with_pixels_per_point(1.0)
        .with_render_cursor(false)
        .with_context_setup(ui::install_fonts)
        .renderer(renderer())
}

/// What a harness shows and what the user did in it.
pub struct Probe {
    pub state: ViewState,
    pub actions: Vec<UiAction>,
}

fn probe_harness(
    builder: HarnessBuilder<Probe>,
    state: ViewState,
    follow: bool,
) -> Harness<'static, Probe> {
    let mut h = builder.build_ui_state(
        move |ui, probe: &mut Probe| {
            let actions = ui::draw(ui, &probe.state);
            if follow {
                follow_actions(&mut probe.state, &actions);
            }
            probe.actions.extend(actions);
        },
        Probe {
            state,
            actions: Vec::new(),
        },
    );
    h.run();
    h
}

/// The window on `state`, collecting the actions it emits.
pub fn harness(state: ViewState) -> Harness<'static, Probe> {
    probe_harness(builder(), state, false)
}

/// A harness whose state follows the actions, as the controller would, with frames 25 ms apart.
pub fn live(state: ViewState) -> Harness<'static, Probe> {
    probe_harness(builder().with_step_dt(0.025), state, true)
}

fn follow_actions(state: &mut ViewState, actions: &[UiAction]) {
    for action in actions {
        match action {
            UiAction::ToggleApp { name, checked } => {
                for app in &mut state.apps {
                    if &app.name == name {
                        app.checked = *checked;
                    }
                }
            }
            UiAction::SelectGame(game) => state.selected_game = game.clone(),
            _ => {}
        }
    }
}

/// `state` rendered at `window` and `pixels_per_point`.
pub fn render_at(state: &ViewState, window: Vec2, pixels_per_point: f32) -> image::RgbaImage {
    let state = state.clone();
    let mut h = builder()
        .with_size(window)
        .with_pixels_per_point(pixels_per_point)
        .build_ui(move |ui| {
            ui::draw(ui, &state);
        });
    h.run();
    h.render()
        .unwrap_or_else(|e| panic!("headless wgpu render failed: {e}"))
}

/// `state` rendered at 1200x700, one pixel per point.
pub fn render(state: &ViewState) -> image::RgbaImage {
    render_at(state, WINDOW, 1.0)
}

/// Run `f` inside a frame of a context with the app's fonts installed, without a renderer.
pub fn with_ctx<R>(f: impl FnOnce(&egui::Context) -> R) -> R {
    let ctx = egui::Context::default();
    ui::install_fonts(&ctx);
    let mut f = Some(f);
    let mut out = None;
    let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
        if let Some(f) = f.take() {
            out = Some(f(ui.ctx()));
        }
    });
    // No renderer here: the font atlas upload is dropped on purpose.
    output.textures_delta.clear();
    out.unwrap()
}

/// The geometry `draw` paints `state` with at 1200x700.
pub fn geometry(state: &ViewState) -> sections::Geometry {
    with_ctx(|ctx| sections::geometry(ctx, state, WINDOW))
}

pub fn pixel(image: &image::RgbaImage, p: Pos2) -> Color32 {
    let q = image.get_pixel(p.x as u32, p.y as u32);
    Color32::from_rgb(q[0], q[1], q[2])
}

/// The colour at `p` in a fresh render of the harness.
pub fn pixel_now(h: &mut Harness<'static, Probe>, p: Pos2) -> Color32 {
    pixel(&h.render().expect("headless render"), p)
}

pub fn press(h: &mut Harness<'static, Probe>, key: Key) {
    h.key_press(key);
    h.run();
}

pub fn tabs(h: &mut Harness<'static, Probe>, n: usize) {
    for _ in 0..n {
        press(h, Key::Tab);
    }
}

pub fn button_event(h: &Harness<'static, Probe>, pos: Pos2, pressed: bool) {
    h.event(egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    });
}

/// `notches` wheel notches; positive scrolls towards the top.
pub fn wheel(h: &Harness<'static, Probe>, notches: f32) {
    h.event(egui::Event::MouseWheel {
        unit: egui::MouseWheelUnit::Line,
        delta: egui::vec2(0.0, notches),
        modifiers: egui::Modifiers::default(),
        phase: egui::TouchPhase::Move,
        source: egui::MouseWheelSource::Wheel,
    });
}
