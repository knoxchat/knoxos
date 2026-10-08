/// Immediate-Mode UI Context — The heart of the egui-inspired UI layer.
///
/// `Ui` is what you use to place widgets. It manages layout, input interaction,
/// and drawing. Inspired by egui's `Ui`, adapted for `no_std` bare-metal with
/// integer coordinates and direct framebuffer rendering.
///
/// # Usage pattern (inside a window content draw):
/// ```ignore
/// let mut ui = Ui::new(fb, content_rect, &input_state);
/// ui.heading("Settings");
/// ui.horizontal(|ui| {
///     ui.label("Name:");
///     if ui.button("Click").clicked() { /* ... */ }
/// });
/// ui.add_space(8);
/// ui.slider_u8("Volume", &mut volume, 0, 100);
/// ui.checkbox("Enable", &mut flag);
/// ```
use crate::gui::framebuffer::{FrameBuffer, Rect};
use crate::gui::id::Id;
use crate::gui::layout::{Layout, Region};

mod buttons;
mod choice;
mod collapsing;
mod combo;
mod construct;
mod containers;
mod input_state;
mod interact;
mod memory;
mod overlay;
mod slider;
mod style;
mod text;
mod text_edit;

pub use input_state::InputState;
pub use memory::{UI_MEMORY, UiMemory, WidgetState};
pub use style::Style;

// ═══════════════════════════════════════════════════════════════════════
// Ui — the main immediate-mode context
// ═══════════════════════════════════════════════════════════════════════

/// The primary immediate-mode UI context.
///
/// You place widgets by calling methods like `label()`, `button()`, `slider()`.
/// Each method draws the widget, checks input, and returns a `Response`.
pub struct Ui<'a> {
    /// Reference to the framebuffer we draw into.
    pub fb: &'a mut FrameBuffer,
    /// Input state for this frame.
    pub input: &'a InputState,
    /// Layout configuration.
    pub layout: Layout,
    /// Current region (cursor, bounds).
    pub region: Region,
    /// Visual style.
    pub style: Style,
    /// Unique ID for this Ui scope.
    pub id: Id,
    /// Next auto-generated ID counter.
    next_auto_id: u64,
    /// Whether this Ui is enabled for interaction.
    pub enabled: bool,
    /// Clip rectangle — widgets outside this are not drawn or interacted with.
    pub clip_rect: Rect,
}
