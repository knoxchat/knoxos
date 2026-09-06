/// Wayland Protocol Server — Native display server for KnoxOS
///
/// Implements core Wayland protocols for client-server GUI isolation:
///   - wl_display: global registry, event loop, client connection
///   - wl_compositor: surface creation and management
///   - wl_surface: pixel buffer attachment, damage, commit
///   - wl_shm: shared memory buffer management
///   - wl_seat: input device abstraction (keyboard, pointer, touch)
///   - wl_output: monitor geometry and mode advertisement
///   - xdg_shell: window management (toplevel, popup)
///   - wl_data_device: clipboard and drag-and-drop
///
/// This runs inside the kernel as an integrated compositor, but follows
/// the Wayland protocol semantics so that userspace clients (when ready)
/// can connect via a socket and receive proper Wayland events.
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use spin::Mutex;

use super::framebuffer::{FrameBuffer, Pixel, Rect};
use super::window::WindowId;

// ═══════════════════════════════════════════════════════════════════════
// WAYLAND OBJECT IDS
// ═══════════════════════════════════════════════════════════════════════

/// Wayland object ID (unique per client)
pub type ObjectId = u32;

/// Client connection ID
pub type ClientId = u32;

static NEXT_CLIENT_ID: AtomicU32 = AtomicU32::new(1);

// ═══════════════════════════════════════════════════════════════════════
// WAYLAND OPCODES
// ═══════════════════════════════════════════════════════════════════════

/// Wayland wire protocol message header
#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct WlMessageHeader {
    /// Target object ID
    pub object_id: u32,
    /// Opcode (16 bits) + message size in bytes (16 bits)
    pub opcode_size: u32,
}

impl WlMessageHeader {
    pub fn opcode(&self) -> u16 {
        (self.opcode_size & 0xFFFF) as u16
    }
    pub fn size(&self) -> u16 {
        ((self.opcode_size >> 16) & 0xFFFF) as u16
    }
    pub fn new(object_id: u32, opcode: u16, size: u16) -> Self {
        Self {
            object_id,
            opcode_size: (opcode as u32) | ((size as u32) << 16),
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════
// WAYLAND GLOBAL INTERFACES
// ═══════════════════════════════════════════════════════════════════════

/// Interface names matching Wayland protocol spec
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WlInterface {
    WlDisplay,
    WlRegistry,
    WlCompositor,
    WlShm,
    WlShmPool,
    WlBuffer,
    WlSurface,
    WlCallback,
    WlSeat,
    WlPointer,
    WlKeyboard,
    WlTouch,
    WlOutput,
    WlDataDeviceManager,
    WlDataDevice,
    WlDataSource,
    WlDataOffer,
    XdgWmBase,
    XdgSurface,
    XdgToplevel,
    XdgPopup,
    XdgPositioner,
    ZwpLinuxDmabufV1,
}

impl WlInterface {
    pub fn name(&self) -> &'static str {
        match self {
            Self::WlDisplay => "wl_display",
            Self::WlRegistry => "wl_registry",
            Self::WlCompositor => "wl_compositor",
            Self::WlShm => "wl_shm",
            Self::WlShmPool => "wl_shm_pool",
            Self::WlBuffer => "wl_buffer",
            Self::WlSurface => "wl_surface",
            Self::WlCallback => "wl_callback",
            Self::WlSeat => "wl_seat",
            Self::WlPointer => "wl_pointer",
            Self::WlKeyboard => "wl_keyboard",
            Self::WlTouch => "wl_touch",
            Self::WlOutput => "wl_output",
            Self::WlDataDeviceManager => "wl_data_device_manager",
            Self::WlDataDevice => "wl_data_device",
            Self::WlDataSource => "wl_data_source",
            Self::WlDataOffer => "wl_data_offer",
            Self::XdgWmBase => "xdg_wm_base",
            Self::XdgSurface => "xdg_surface",
            Self::XdgToplevel => "xdg_toplevel",
            Self::XdgPopup => "xdg_popup",
            Self::XdgPositioner => "xdg_positioner",
            Self::ZwpLinuxDmabufV1 => "zwp_linux_dmabuf_v1",
        }
    }

    pub fn version(&self) -> u32 {
        match self {
            Self::WlCompositor => 5,
            Self::WlShm => 1,
            Self::WlSeat => 8,
            Self::WlOutput => 4,
            Self::WlDataDeviceManager => 3,
            Self::XdgWmBase => 5,
            Self::ZwpLinuxDmabufV1 => 4,
            _ => 1,
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════
// SHM BUFFER
// ═══════════════════════════════════════════════════════════════════════

/// Shared memory pool — a region of memory shared between client and compositor
#[derive(Debug)]
pub struct ShmPool {
    pub id: ObjectId,
    pub client: ClientId,
    /// Base address of the shared memory region
    pub base_addr: usize,
    /// Size of the pool in bytes
    pub size: usize,
}

/// A buffer backed by shared memory
#[derive(Debug, Clone)]
pub struct ShmBuffer {
    pub id: ObjectId,
    pub client: ClientId,
    pub pool_id: ObjectId,
    pub offset: usize,
    pub width: u32,
    pub height: u32,
    pub stride: u32,
    pub format: ShmFormat,
}

/// SHM pixel formats (matching wl_shm.format enum)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShmFormat {
    Argb8888 = 0,
    Xrgb8888 = 1,
    Rgb888 = 2,
    Bgr888 = 3,
    Abgr8888 = 4,
}

// ═══════════════════════════════════════════════════════════════════════
// SURFACE
// ═══════════════════════════════════════════════════════════════════════

/// A Wayland surface — the fundamental unit of content
#[derive(Debug)]
pub struct WlSurfaceState {
    pub id: ObjectId,
    pub client: ClientId,
    /// Attached buffer (pending)
    pub pending_buffer: Option<ObjectId>,
    /// Committed buffer
    pub committed_buffer: Option<ObjectId>,
    /// Surface position (set by compositor)
    pub x: i32,
    pub y: i32,
    /// Accumulated damage regions (buffer coordinates)
    pub damage: Vec<Rect>,
    /// Opaque region (for compositor optimizations)
    pub opaque_region: Option<Rect>,
    /// Input region (for hit testing)
    pub input_region: Option<Rect>,
    /// Transform (rotation)
    pub transform: WlTransform,
    /// Scale factor
    pub scale: i32,
    /// Frame callback (for VSync notification)
    pub frame_callback: Option<ObjectId>,
    /// Subsurfaces
    pub subsurfaces: Vec<ObjectId>,
    /// Mapped to a KnoxOS window
    pub window_id: Option<WindowId>,
}

/// Surface transform (wl_output.transform)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WlTransform {
    Normal = 0,
    Rotate90 = 1,
    Rotate180 = 2,
    Rotate270 = 3,
    Flipped = 4,
    FlippedRotate90 = 5,
    FlippedRotate180 = 6,
    FlippedRotate270 = 7,
}

// ═══════════════════════════════════════════════════════════════════════
// XDG SHELL — WINDOW MANAGEMENT
// ═══════════════════════════════════════════════════════════════════════

/// XDG toplevel window state
#[derive(Debug)]
pub struct XdgToplevelState {
    pub id: ObjectId,
    pub surface_id: ObjectId,
    pub client: ClientId,
    pub title: String,
    pub app_id: String,
    /// Requested min/max size from client
    pub min_width: u32,
    pub min_height: u32,
    pub max_width: u32,
    pub max_height: u32,
    /// Current states (maximized, fullscreen, activated, etc.)
    pub states: Vec<XdgToplevelStateFlag>,
    /// Pending configure serial
    pub pending_serial: Option<u32>,
}

/// XDG toplevel state flags (sent in configure events)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum XdgToplevelStateFlag {
    Maximized = 1,
    Fullscreen = 2,
    Resizing = 3,
    Activated = 4,
    TiledLeft = 5,
    TiledRight = 6,
    TiledTop = 7,
    TiledBottom = 8,
    Suspended = 9,
}

/// XDG popup state
#[derive(Debug)]
pub struct XdgPopupState {
    pub id: ObjectId,
    pub surface_id: ObjectId,
    pub parent_surface: ObjectId,
    pub client: ClientId,
    /// Popup position relative to parent
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    /// Grab (for menus that need to dismiss on outside click)
    pub grab: bool,
}

/// XDG positioner — configuration for popup placement
#[derive(Debug, Clone)]
pub struct XdgPositioner {
    pub id: ObjectId,
    pub width: u32,
    pub height: u32,
    pub anchor_rect: Rect,
    pub anchor: Anchor,
    pub gravity: Gravity,
    pub constraint_adjustment: u32,
    pub offset_x: i32,
    pub offset_y: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Anchor {
    None = 0,
    Top = 1,
    Bottom = 2,
    Left = 3,
    Right = 4,
    TopLeft = 5,
    BottomLeft = 6,
    TopRight = 7,
    BottomRight = 8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Gravity {
    None = 0,
    Top = 1,
    Bottom = 2,
    Left = 3,
    Right = 4,
    TopLeft = 5,
    BottomLeft = 6,
    TopRight = 7,
    BottomRight = 8,
}

// ═══════════════════════════════════════════════════════════════════════
// INPUT SEAT
// ═══════════════════════════════════════════════════════════════════════

/// Seat capabilities bitfield
pub const SEAT_CAP_POINTER: u32 = 1;
pub const SEAT_CAP_KEYBOARD: u32 = 2;
pub const SEAT_CAP_TOUCH: u32 = 4;

/// Keyboard key state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyState {
    Released = 0,
    Pressed = 1,
}

/// Pointer button state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ButtonState {
    Released = 0,
    Pressed = 1,
}

/// Pointer axis source
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AxisSource {
    Wheel = 0,
    Finger = 1,
    Continuous = 2,
    WheelTilt = 3,
}

// ═══════════════════════════════════════════════════════════════════════
// OUTPUT (MONITOR)
// ═══════════════════════════════════════════════════════════════════════

/// Output (monitor) description
#[derive(Debug, Clone)]
pub struct WlOutputInfo {
    pub id: ObjectId,
    pub name: String,
    pub description: String,
    pub x: i32,
    pub y: i32,
    pub physical_width_mm: i32,
    pub physical_height_mm: i32,
    pub subpixel: SubpixelLayout,
    pub make: String,
    pub model: String,
    pub transform: WlTransform,
    pub modes: Vec<OutputMode>,
    pub current_mode: usize,
    pub scale_factor: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubpixelLayout {
    Unknown = 0,
    None = 1,
    HorizontalRgb = 2,
    HorizontalBgr = 3,
    VerticalRgb = 4,
    VerticalBgr = 5,
}

#[derive(Debug, Clone)]
pub struct OutputMode {
    pub width: u32,
    pub height: u32,
    pub refresh_mhz: u32, // Refresh rate in millihertz (e.g., 60000 = 60Hz)
    pub preferred: bool,
    pub current: bool,
}

// ═══════════════════════════════════════════════════════════════════════
// DATA DEVICE (CLIPBOARD / DND)
// ═══════════════════════════════════════════════════════════════════════

/// Data offer — represents available clipboard/DnD content
#[derive(Debug)]
pub struct WlDataOffer {
    pub id: ObjectId,
    pub mime_types: Vec<String>,
    pub source_actions: u32,
    pub selected_action: u32,
}

/// Data source — a client offering data to the clipboard
#[derive(Debug)]
pub struct WlDataSource {
    pub id: ObjectId,
    pub client: ClientId,
    pub mime_types: Vec<String>,
    pub actions: u32,
}

/// DnD actions (bitfield)
pub const DND_ACTION_NONE: u32 = 0;
pub const DND_ACTION_COPY: u32 = 1;
pub const DND_ACTION_MOVE: u32 = 2;
pub const DND_ACTION_ASK: u32 = 4;

// ═══════════════════════════════════════════════════════════════════════
// COMPOSITOR STATE
// ═══════════════════════════════════════════════════════════════════════

struct CompositorState {
    /// All connected clients
    clients: BTreeMap<ClientId, ClientState>,
    /// All surfaces
    surfaces: BTreeMap<ObjectId, WlSurfaceState>,
    /// All SHM pools
    shm_pools: BTreeMap<ObjectId, ShmPool>,
    /// All SHM buffers
    shm_buffers: BTreeMap<ObjectId, ShmBuffer>,
    /// All XDG toplevels
    toplevels: BTreeMap<ObjectId, XdgToplevelState>,
    /// All XDG popups
    popups: BTreeMap<ObjectId, XdgPopupState>,
    /// All positioners
    positioners: BTreeMap<ObjectId, XdgPositioner>,
    /// Global outputs
    outputs: Vec<WlOutputInfo>,
    /// Next configure serial
    next_serial: u32,
    /// Compositor is running
    running: bool,
    /// Keyboard focus surface
    keyboard_focus: Option<ObjectId>,
    /// Pointer focus surface
    pointer_focus: Option<ObjectId>,
    /// Current pointer position (global)
    pointer_x: f64,
    pointer_y: f64,
}

struct ClientState {
    id: ClientId,
    /// Object ID → interface mapping
    objects: BTreeMap<ObjectId, WlInterface>,
    /// Next object ID for server-created objects
    next_id: ObjectId,
    /// PID of the client process
    pid: u32,
    /// Whether the client has been authenticated
    authenticated: bool,
}

impl CompositorState {
    fn new() -> Self {
        Self {
            clients: BTreeMap::new(),
            surfaces: BTreeMap::new(),
            shm_pools: BTreeMap::new(),
            shm_buffers: BTreeMap::new(),
            toplevels: BTreeMap::new(),
            popups: BTreeMap::new(),
            positioners: BTreeMap::new(),
            outputs: Vec::new(),
            next_serial: 1,
            running: false,
            keyboard_focus: None,
            pointer_focus: None,
            pointer_x: 0.0,
            pointer_y: 0.0,
        }
    }

    fn next_serial(&mut self) -> u32 {
        let s = self.next_serial;
        self.next_serial += 1;
        s
    }
}

lazy_static::lazy_static! {
    static ref COMPOSITOR: Mutex<CompositorState> = Mutex::new(CompositorState::new());
}

static COMPOSITOR_RUNNING: AtomicBool = AtomicBool::new(false);

// ═══════════════════════════════════════════════════════════════════════
// PUBLIC API
// ═══════════════════════════════════════════════════════════════════════

/// Initialize the Wayland compositor
pub fn init() {
    let mut state = COMPOSITOR.lock();
    // Register default output (primary display)
    let (sw, sh) = crate::gui::cached_screen_size();
    state.outputs.push(WlOutputInfo {
        id: 1,
        name: String::from("KnoxOS-1"),
        description: String::from("Primary Display"),
        x: 0,
        y: 0,
        physical_width_mm: 530, // ~24" at 1920x1080
        physical_height_mm: 300,
        subpixel: SubpixelLayout::HorizontalRgb,
        make: String::from("KnoxOS"),
        model: String::from("Virtual Display"),
        transform: WlTransform::Normal,
        modes: alloc::vec![OutputMode {
            width: sw as u32,
            height: sh as u32,
            refresh_mhz: 60000,
            preferred: true,
            current: true,
        }],
        current_mode: 0,
        scale_factor: 1,
    });
    state.running = true;
    COMPOSITOR_RUNNING.store(true, Ordering::Relaxed);
    crate::serial_println!("[KnoxOS] Wayland compositor initialized ({}x{})", sw, sh);
}

/// Register a new client connection
pub fn connect_client(pid: u32) -> ClientId {
    let id = NEXT_CLIENT_ID.fetch_add(1, Ordering::Relaxed);
    let mut state = COMPOSITOR.lock();
    state.clients.insert(
        id,
        ClientState {
            id,
            objects: BTreeMap::new(),
            next_id: 0xFF000000, // Server-allocated IDs start high
            pid,
            authenticated: true, // Internal clients are auto-authenticated
        },
    );
    crate::serial_println!("[Wayland] Client {} connected (PID {})", id, pid);
    id
}

/// Disconnect a client and clean up all resources
pub fn disconnect_client(client_id: ClientId) {
    let mut state = COMPOSITOR.lock();

    // Clean up surfaces owned by this client
    let surface_ids: Vec<ObjectId> = state
        .surfaces
        .iter()
        .filter(|(_, s)| s.client == client_id)
        .map(|(id, _)| *id)
        .collect();
    for sid in &surface_ids {
        state.surfaces.remove(sid);
    }

    // Clean up toplevels
    let toplevel_ids: Vec<ObjectId> = state
        .toplevels
        .iter()
        .filter(|(_, t)| t.client == client_id)
        .map(|(id, _)| *id)
        .collect();
    for tid in &toplevel_ids {
        state.toplevels.remove(tid);
    }

    // Clean up popups
    let popup_ids: Vec<ObjectId> = state
        .popups
        .iter()
        .filter(|(_, p)| p.client == client_id)
        .map(|(id, _)| *id)
        .collect();
    for pid in &popup_ids {
        state.popups.remove(pid);
    }

    // Clean up SHM
    let pool_ids: Vec<ObjectId> = state
        .shm_pools
        .iter()
        .filter(|(_, p)| p.client == client_id)
        .map(|(id, _)| *id)
        .collect();
    for pid in &pool_ids {
        state.shm_pools.remove(pid);
    }
    let buf_ids: Vec<ObjectId> = state
        .shm_buffers
        .iter()
        .filter(|(_, b)| b.client == client_id)
        .map(|(id, _)| *id)
        .collect();
    for bid in &buf_ids {
        state.shm_buffers.remove(bid);
    }

    // Reset focus if this client had it
    if let Some(focus) = state.keyboard_focus {
        if surface_ids.contains(&focus) {
            state.keyboard_focus = None;
        }
    }
    if let Some(focus) = state.pointer_focus {
        if surface_ids.contains(&focus) {
            state.pointer_focus = None;
        }
    }

    state.clients.remove(&client_id);
    crate::serial_println!("[Wayland] Client {} disconnected", client_id);
}

/// Create a surface for a client
pub fn create_surface(client_id: ClientId, surface_id: ObjectId) -> bool {
    let mut state = COMPOSITOR.lock();
    if state.surfaces.contains_key(&surface_id) {
        return false;
    }
    state.surfaces.insert(
        surface_id,
        WlSurfaceState {
            id: surface_id,
            client: client_id,
            pending_buffer: None,
            committed_buffer: None,
            x: 0,
            y: 0,
            damage: Vec::new(),
            opaque_region: None,
            input_region: None,
            transform: WlTransform::Normal,
            scale: 1,
            frame_callback: None,
            subsurfaces: Vec::new(),
            window_id: None,
        },
    );
    true
}

/// Attach a buffer to a surface (pending state)
pub fn surface_attach(surface_id: ObjectId, buffer_id: ObjectId, x: i32, y: i32) {
    let mut state = COMPOSITOR.lock();
    if let Some(surface) = state.surfaces.get_mut(&surface_id) {
        surface.pending_buffer = Some(buffer_id);
        surface.x += x;
        surface.y += y;
    }
}

/// Add damage to a surface (the region that was updated)
pub fn surface_damage(surface_id: ObjectId, x: i32, y: i32, width: u32, height: u32) {
    let mut state = COMPOSITOR.lock();
    if let Some(surface) = state.surfaces.get_mut(&surface_id) {
        surface.damage.push(Rect::new(x, y, width, height));
    }
}

/// Commit a surface — make pending state current
pub fn surface_commit(surface_id: ObjectId) {
    let mut state = COMPOSITOR.lock();
    if let Some(surface) = state.surfaces.get_mut(&surface_id) {
        // Move pending buffer to committed
        if let Some(buf) = surface.pending_buffer.take() {
            surface.committed_buffer = Some(buf);
        }

        // Process damage
        if !surface.damage.is_empty() {
            // Notify compositor to redraw this surface
            if let Some(wid) = surface.window_id {
                crate::gui::push_damage(Rect::new(
                    surface.x, surface.y, 1920, 1080, // Will be clamped
                ));
            }
            surface.damage.clear();
        }

        // Fire frame callback
        if let Some(_cb) = surface.frame_callback.take() {
            // Send done event with timestamp
            // In a real implementation, queue this for VSync
        }
    }
}

/// Request frame callback (VSync notification)
pub fn surface_frame(surface_id: ObjectId, callback_id: ObjectId) {
    let mut state = COMPOSITOR.lock();
    if let Some(surface) = state.surfaces.get_mut(&surface_id) {
        surface.frame_callback = Some(callback_id);
    }
}

/// Create an XDG toplevel (window)
pub fn create_toplevel(client_id: ClientId, toplevel_id: ObjectId, surface_id: ObjectId) -> bool {
    let mut state = COMPOSITOR.lock();
    if state.toplevels.contains_key(&toplevel_id) {
        return false;
    }
    state.toplevels.insert(
        toplevel_id,
        XdgToplevelState {
            id: toplevel_id,
            surface_id,
            client: client_id,
            title: String::new(),
            app_id: String::new(),
            min_width: 0,
            min_height: 0,
            max_width: 0,
            max_height: 0,
            states: alloc::vec![XdgToplevelStateFlag::Activated],
            pending_serial: None,
        },
    );
    true
}

/// Set toplevel title
pub fn set_toplevel_title(toplevel_id: ObjectId, title: &str) {
    let mut state = COMPOSITOR.lock();
    if let Some(toplevel) = state.toplevels.get_mut(&toplevel_id) {
        toplevel.title = String::from(title);
        let surface_id = toplevel.surface_id;
        // Update the KnoxOS window title if mapped
        if let Some(surface) = state.surfaces.get(&surface_id) {
            if let Some(_wid) = surface.window_id {
                // wm_core::set_window_title(wid, title);
            }
        }
    }
}

/// Set toplevel app ID
pub fn set_toplevel_app_id(toplevel_id: ObjectId, app_id: &str) {
    let mut state = COMPOSITOR.lock();
    if let Some(toplevel) = state.toplevels.get_mut(&toplevel_id) {
        toplevel.app_id = String::from(app_id);
    }
}

/// Create an XDG popup
pub fn create_popup(
    client_id: ClientId,
    popup_id: ObjectId,
    surface_id: ObjectId,
    parent_surface: ObjectId,
    positioner: &XdgPositioner,
) -> bool {
    let mut state = COMPOSITOR.lock();
    if state.popups.contains_key(&popup_id) {
        return false;
    }
    state.popups.insert(
        popup_id,
        XdgPopupState {
            id: popup_id,
            surface_id,
            parent_surface,
            client: client_id,
            x: positioner.offset_x,
            y: positioner.offset_y,
            width: positioner.width,
            height: positioner.height,
            grab: false,
        },
    );
    true
}

/// Send keyboard focus to a surface
pub fn set_keyboard_focus(surface_id: Option<ObjectId>) {
    let mut state = COMPOSITOR.lock();
    if let Some(old_focus) = state.keyboard_focus {
        // Send wl_keyboard.leave to old surface
        let _ = old_focus;
    }
    state.keyboard_focus = surface_id;
    if let Some(new_focus) = surface_id {
        // Send wl_keyboard.enter to new surface with current modifiers
        let _ = new_focus;
    }
}

/// Send pointer motion event
pub fn pointer_motion(surface_id: ObjectId, x: f64, y: f64) {
    let mut state = COMPOSITOR.lock();
    state.pointer_x = x;
    state.pointer_y = y;
    if state.pointer_focus != Some(surface_id) {
        // Send pointer.leave to old surface, pointer.enter to new
        state.pointer_focus = Some(surface_id);
    }
    // Send wl_pointer.motion event
}

/// Send pointer button event
pub fn pointer_button(button: u32, pressed: bool) {
    let state = COMPOSITOR.lock();
    let _button_state = if pressed {
        ButtonState::Pressed
    } else {
        ButtonState::Released
    };
    let _serial = state.next_serial;
    // Send wl_pointer.button event to focused surface
}

/// Send keyboard key event
pub fn keyboard_key(key: u32, pressed: bool) {
    let state = COMPOSITOR.lock();
    let _key_state = if pressed {
        KeyState::Pressed
    } else {
        KeyState::Released
    };
    let _serial = state.next_serial;
    // Send wl_keyboard.key event to focused surface
}

/// Get compositor info for debug/status
pub fn info() -> (usize, usize, usize, usize) {
    let state = COMPOSITOR.lock();
    (
        state.clients.len(),
        state.surfaces.len(),
        state.toplevels.len(),
        state.popups.len(),
    )
}

/// Check if compositor is running
pub fn is_running() -> bool {
    COMPOSITOR_RUNNING.load(Ordering::Relaxed)
}

/// Get output info
pub fn outputs() -> Vec<WlOutputInfo> {
    COMPOSITOR.lock().outputs.clone()
}

/// Add a new output (monitor connected)
pub fn add_output(info: WlOutputInfo) {
    let mut state = COMPOSITOR.lock();
    state.outputs.push(info);
    // Broadcast wl_registry.global to all clients
}

/// Remove an output (monitor disconnected)
pub fn remove_output(output_id: ObjectId) {
    let mut state = COMPOSITOR.lock();
    state.outputs.retain(|o| o.id != output_id);
    // Broadcast wl_registry.global_remove to all clients
}

/// Fire all pending frame callbacks with the current timestamp
pub fn fire_frame_callbacks() {
    let mut state = COMPOSITOR.lock();
    let timestamp_ms = crate::clock::uptime_seconds() * 1000;
    for surface in state.surfaces.values_mut() {
        if let Some(_cb) = surface.frame_callback.take() {
            // In a full implementation:
            // Send wl_callback.done(timestamp_ms) to the client
            // This tells the client it's safe to render the next frame
            let _ = timestamp_ms;
        }
    }
}
