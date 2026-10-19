/// Surfaces, pending/current state, and xdg-style shell objects.
use alloc::string::String;
use alloc::vec::Vec;

// ─── Surface ───────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SurfaceRole {
    None,
    Toplevel,
    Popup,
    Subsurface,
    Cursor,
}

#[derive(Debug, Clone)]
pub struct DamageRect {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

#[derive(Debug, Clone)]
pub struct SurfaceState {
    pub buffer_id: Option<u32>,
    pub buffer_x: i32,
    pub buffer_y: i32,
    pub damage: Vec<DamageRect>,
    pub opaque_region: Option<(i32, i32, i32, i32)>,
    pub input_region: Option<(i32, i32, i32, i32)>,
    pub frame_callback: Option<u32>,
    pub scale: i32,
    pub transform: u32,
}

impl Default for SurfaceState {
    fn default() -> Self {
        Self::new()
    }
}

impl SurfaceState {
    pub fn new() -> Self {
        Self {
            buffer_id: None,
            buffer_x: 0,
            buffer_y: 0,
            damage: Vec::new(),
            opaque_region: None,
            input_region: None,
            frame_callback: None,
            scale: 1,
            transform: 0,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Surface {
    pub id: u32,
    pub client_pid: u32,
    pub role: SurfaceRole,
    pub current: SurfaceState,
    pub pending: SurfaceState,
    pub x: i32,
    pub y: i32,
    pub visible: bool,
    pub parent: Option<u32>,
    pub children: Vec<u32>,
}

impl Surface {
    pub fn new(id: u32, client_pid: u32) -> Self {
        Self {
            id,
            client_pid,
            role: SurfaceRole::None,
            current: SurfaceState::new(),
            pending: SurfaceState::new(),
            x: 0,
            y: 0,
            visible: false,
            parent: None,
            children: Vec::new(),
        }
    }

    /// Commit pending state to current
    pub fn commit(&mut self) {
        self.current = self.pending.clone();
        self.pending.damage.clear();
        self.pending.frame_callback = None;
    }
}

// ─── Shell Surface (xdg_surface / xdg_toplevel) ───────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToplevelState {
    Idle,
    Maximized,
    Fullscreen,
    Resizing,
    Activated,
}

#[derive(Debug, Clone)]
pub struct Toplevel {
    pub surface_id: u32,
    pub title: String,
    pub app_id: String,
    pub min_width: u32,
    pub min_height: u32,
    pub max_width: u32,
    pub max_height: u32,
    pub state: ToplevelState,
}

impl Toplevel {
    pub fn new(surface_id: u32) -> Self {
        Self {
            surface_id,
            title: String::new(),
            app_id: String::new(),
            min_width: 0,
            min_height: 0,
            max_width: 0,
            max_height: 0,
            state: ToplevelState::Idle,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Popup {
    pub surface_id: u32,
    pub parent_surface_id: u32,
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}
