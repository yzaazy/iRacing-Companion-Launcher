//! Pointer and keyboard input for the window's controls.
//!
//! The pointer follows CustomTkinter: a control acts on the press of the left button (its
//! `<Button-1>` binding), not on release; a pressed button shows its normal colour for 100 ms
//! before the hover colour returns (`CTkButton._clicked`); a checkbox or radio button that
//! changed state redraws in its plain colours and shows hover again only once the pointer has
//! left and re-entered (`_draw` overwrites what `_on_enter` set).
//!
//! The keyboard is an addition: the Python window takes no keys. Nothing has focus until Tab is
//! pressed; Tab and Shift+Tab move through the controls that can act, in reading order; Space or
//! Enter activates the focused control exactly as a press would; Escape or any mouse press drops
//! the focus, so a stray key never reaches Launch or Close. The focused control wears a ring
//! outside its painted shape, which moves and resizes nothing.
//!
//! egui's own focus is not used: no control senses focus, so egui's Tab handling and its
//! Space/Enter "fake clicks" never reach them.

use crate::model::UiAction;
use crate::ui::pack::Bounds;
use crate::ui::theme;
use egui::{CursorIcon, Id, Key, PointerButton, Sense, Stroke, StrokeKind, Ui, WidgetInfo};
use std::collections::{HashMap, HashSet};

/// How long a pressed button shows its normal colour (`self.after(100, ...)`).
const FLASH_SECONDS: f64 = 0.1;

/// The outline the focus ring follows.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Ring {
    /// A painted rectangle and its corner radius.
    Rounded { rect: Bounds, radius: f32 },
    /// A painted circle, by its bounding box.
    Circle(Bounds),
}

/// How a control looks after a press.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// A CTkButton: flashes its normal colour.
    Button,
    /// A CTkCheckBox or CTkRadioButton: loses its hover look if the press changed its state.
    Toggle { changes_state: bool },
}

/// One control as the frame draws it.
pub struct Control {
    pub id: Id,
    /// Where the pointer reaches it: the bound Tk widgets (for a toggle, its box and its empty
    /// text label, not the gap between them). The first rect carries the accessibility info.
    pub hit: Vec<Bounds>,
    /// Whether it acts at all: false for disabled buttons and the toggles of entries not found.
    pub enabled: bool,
    /// What a press emits, if anything.
    pub action: Option<UiAction>,
    pub kind: Kind,
    pub ring: Ring,
    pub info: WidgetInfo,
}

/// What the pointer does to a control this frame.
pub struct Pointer {
    /// Show the hover colours.
    pub hover_look: bool,
}

/// Remembered between frames.
#[derive(Clone, Default)]
struct Memory {
    /// The control with keyboard focus.
    focused: Option<Id>,
    /// Its position among the tab stops when it was last one, so Tab continues from there when
    /// it stops being one (a button that became disabled).
    focused_index: Option<usize>,
    /// Buttons flashing their normal colour, until this time.
    flash: HashMap<Id, f64>,
    /// Toggles that redrew without hover; they keep it off until the pointer leaves.
    no_hover: HashSet<Id>,
}

/// A key the controls react to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Press {
    Next,
    Previous,
    Activate,
    Drop,
}

/// Collects this frame's controls, resolves pointer presses as they are drawn and the keyboard
/// once all are known.
pub struct Controls {
    memory: Memory,
    /// Keyboard focus as the frame began.
    focus_at_start: Option<Id>,
    keys: Vec<Press>,
    primary_pressed: bool,
    time: f64,
    /// Tab stops in order: id, ring, action, kind.
    stops: Vec<(Id, Ring, Option<UiAction>, Kind)>,
    actions: Vec<UiAction>,
    /// A tab stop that is not a control: the log, when it can scroll.
    extra_stop: Option<(usize, Id, Ring)>,
}

fn memory_id() -> Id {
    Id::unique("launcher-controls")
}

impl Controls {
    /// Read the remembered state and this frame's keys and pointer press.
    pub fn begin(ui: &Ui) -> Self {
        let mut memory: Memory = ui
            .ctx()
            .data(|d| d.get_temp(memory_id()))
            .unwrap_or_default();
        let (keys, primary_pressed, any_pressed, time) = ui.input(|i| {
            let keys = i
                .events
                .iter()
                .filter_map(|e| match e {
                    egui::Event::Key {
                        key,
                        pressed: true,
                        repeat,
                        modifiers,
                        ..
                    } => {
                        let plain = !modifiers.ctrl && !modifiers.alt && !modifiers.command;
                        match key {
                            Key::Tab if plain && modifiers.shift => Some(Press::Previous),
                            Key::Tab if plain => Some(Press::Next),
                            // Held keys repeat; a held Enter must not launch twice.
                            Key::Space | Key::Enter if plain && !repeat => Some(Press::Activate),
                            Key::Escape => Some(Press::Drop),
                            _ => None,
                        }
                    }
                    _ => None,
                })
                .collect::<Vec<_>>();
            (
                keys,
                i.pointer.button_pressed(PointerButton::Primary),
                i.pointer.any_pressed(),
                i.time,
            )
        });
        // A mouse press anywhere drops the keyboard focus: what the pointer touched does not
        // keep it, and a click on the background is the way to let go of it.
        if any_pressed {
            memory.focused = None;
            memory.focused_index = None;
        }
        Self {
            focus_at_start: memory.focused,
            memory,
            keys,
            primary_pressed,
            time,
            stops: Vec::new(),
            actions: Vec::new(),
            extra_stop: None,
        }
    }

    /// The keyboard focus as the frame began.
    pub fn focused(&self) -> Option<Id> {
        self.focus_at_start
    }

    /// Draw-time handling of one control: its pointer state, and its action if pressed.
    pub fn control(&mut self, ui: &Ui, c: Control) -> Pointer {
        let mut hovered = false;
        for (i, b) in c.hit.iter().enumerate() {
            if !b.is_mapped() {
                continue;
            }
            let id = if i == 0 { c.id } else { c.id.with(i) };
            let sense = if c.enabled {
                Sense::CLICK
            } else {
                Sense::hover()
            };
            let response = ui.interact(b.rect(), id, sense);
            if i == 0 {
                let info = c.info.clone();
                response.widget_info(move || info.clone());
            }
            if response.hovered() {
                hovered = true;
                if c.enabled {
                    ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
                }
            }
        }
        if !hovered {
            self.memory.flash.remove(&c.id);
            self.memory.no_hover.remove(&c.id);
        }
        if c.enabled && hovered && self.primary_pressed {
            self.pressed(&c.id, c.action.clone(), c.kind);
        }
        let flashing = match self.memory.flash.get(&c.id) {
            Some(&until) if until > self.time => {
                ui.ctx()
                    .request_repaint_after_secs((until - self.time) as f32);
                true
            }
            Some(_) => {
                self.memory.flash.remove(&c.id);
                false
            }
            None => false,
        };
        let hover_look = c.enabled && hovered && !flashing && !self.memory.no_hover.contains(&c.id);
        // Everything that can act is a stop; a selected radio button is one too, as a radio
        // button is, though pressing it changes nothing.
        if c.enabled {
            self.stops.push((c.id, c.ring, c.action, c.kind));
        }
        Pointer { hover_look }
    }

    /// Make a tab stop that is not a control (the log), placed after the controls added so far.
    pub fn stop(&mut self, id: Id, ring: Ring) {
        self.extra_stop = Some((self.stops.len(), id, ring));
    }

    fn pressed(&mut self, id: &Id, action: Option<UiAction>, kind: Kind) {
        match kind {
            Kind::Button => {
                self.memory.flash.insert(*id, self.time + FLASH_SECONDS);
            }
            Kind::Toggle { changes_state } => {
                if changes_state {
                    self.memory.no_hover.insert(*id);
                }
            }
        }
        if let Some(action) = action {
            self.actions.push(action);
        }
    }

    /// Apply this frame's keys in order, paint the focus ring and remember the state. Returns
    /// every action of the frame, pointer and keyboard, in the order they happened.
    pub fn finish(mut self, ui: &Ui) -> Vec<UiAction> {
        // Each stop with what activating it does; the log's stop does nothing on Space or Enter.
        type Stop = (Id, Ring, Option<(Option<UiAction>, Kind)>);
        let mut stops: Vec<Stop> = self
            .stops
            .iter()
            .map(|(id, ring, action, kind)| (*id, *ring, Some((action.clone(), *kind))))
            .collect();
        if let Some((at, id, ring)) = self.extra_stop {
            stops.insert(at.min(stops.len()), (id, ring, None));
        }

        let position = |memory: &Memory| {
            memory
                .focused
                .and_then(|f| stops.iter().position(|(id, _, _)| *id == f))
        };
        if let Some(i) = position(&self.memory) {
            self.memory.focused_index = Some(i);
        }
        for key in std::mem::take(&mut self.keys) {
            match key {
                Press::Next | Press::Previous if !stops.is_empty() => {
                    let n = stops.len();
                    let next = match (position(&self.memory), self.memory.focused_index) {
                        (Some(i), _) if key == Press::Next => (i + 1) % n,
                        (Some(i), _) => (i + n - 1) % n,
                        // The focused control stopped being a stop: continue from its place.
                        (None, Some(i)) if key == Press::Next => i.min(n - 1),
                        (None, Some(i)) => (i.min(n) + n - 1) % n,
                        (None, None) if key == Press::Next => 0,
                        (None, None) => n - 1,
                    };
                    self.memory.focused = Some(stops[next].0);
                    self.memory.focused_index = Some(next);
                }
                Press::Activate => {
                    if let Some(i) = position(&self.memory)
                        && let Some((action, kind)) = stops[i].2.clone()
                    {
                        let id = stops[i].0;
                        self.pressed(&id, action, kind);
                    }
                }
                Press::Drop => {
                    self.memory.focused = None;
                    self.memory.focused_index = None;
                }
                Press::Next | Press::Previous => {}
            }
        }

        if let Some(i) = position(&self.memory) {
            paint_ring(ui.painter(), stops[i].1);
        }
        if !self.actions.is_empty() || self.memory.focused != self.focus_at_start {
            ui.ctx().request_repaint();
        }
        let memory = self.memory;
        ui.ctx().data_mut(|d| d.insert_temp(memory_id(), memory));
        self.actions
    }
}

/// The focus ring: a 2 px white band 2 px outside the control, with a 1 px black band inside
/// it, the dark-theme focus visual of Windows. It stays clear of the control's own pixels.
pub const RING_GAP: f32 = 1.0;
pub const RING_INNER: f32 = 1.0;
pub const RING_OUTER: f32 = 2.0;

pub fn paint_ring(painter: &egui::Painter, ring: Ring) {
    let bands = [
        (RING_GAP + RING_INNER, RING_INNER, theme::FOCUS_RING_INNER),
        (
            RING_GAP + RING_INNER + RING_OUTER,
            RING_OUTER,
            theme::FOCUS_RING,
        ),
    ];
    for (reach, width, color) in bands {
        match ring {
            Ring::Rounded { rect, radius } => {
                let r = rect.rect().expand(reach);
                let corner = (radius + reach).round().clamp(0.0, 255.0) as u8;
                painter.rect_stroke(
                    r,
                    egui::CornerRadius::same(corner),
                    Stroke::new(width, color),
                    StrokeKind::Inside,
                );
            }
            Ring::Circle(b) => {
                // egui strokes a circle outside its radius.
                let r = b.w as f32 / 2.0 + reach - width;
                painter.circle_stroke(b.rect().center(), r, Stroke::new(width, color));
            }
        }
    }
}
