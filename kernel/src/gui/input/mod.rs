/// Input Handling - Mouse and keyboard event processing
/// PS/2 mouse driver and high-level input event dispatching
mod click;
mod config;
mod drag;
mod frame;
mod hover;
mod mouse;
mod ps2;
mod scroll;

pub use config::{
    FocusMode, MOUSE_SETTINGS, MouseSettings, focus_mode, mouse_settings, set_focus_mode,
    set_mouse_settings,
};
pub use frame::{end_frame, snapshot as frame_input, update_pointer};
pub use mouse::{
    MOUSE, MouseState, add_mouse_byte, drain_mouse_queue, init_mouse_queue, process_mouse_events,
    set_absolute_mouse,
};
pub use ps2::init_mouse;
