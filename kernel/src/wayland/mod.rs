/// Wayland — Real Wayland-style compositor with SHM buffer exchange
///
/// Provides server-side compositor primitives for KnoxOS GUI:
///   - Surface management (shared-memory buffers with real kernel memory)
///   - Compositor — damage-driven redraw with framebuffer compositing
///   - Shell surfaces — toplevel, popup, fullscreen
///   - Seat — pointer/keyboard input routing
///   - Output — display enumeration
///   - SHM buffer exchange — real pixel data transfer to framebuffer
///
/// This provides kernel-side Wayland abstractions with real buffer
/// management and compositing to the hardware framebuffer.
///
/// Module layout:
///   shm         — pixel formats, SHM pools and buffers
///   surface     — surfaces, damage, xdg-style toplevel/popup
///   seat        — pointer/keyboard seat
///   compositor  — object graph, compositing, global compositor
///   api         — public wrappers around the compositor
///   gates       — Gate F / Gate WL / Gate TERM markers and self-test
///   present     — Ring 3 SHM present, chrome windows, PTY routing
///   protocol    — `wl_display` object protocol on `/run/wayland-0`
mod api;
mod compositor;
mod gates;
mod present;
mod protocol;
mod seat;
mod shm;
mod surface;

pub use api::{
    attach_shm_buffer, buffer_set_busy, commit_surface, compose_to_buffer, compose_to_framebuffer,
    create_shm_buffer, create_shm_pool, create_surface, destroy_shm_buffer, destroy_shm_pool,
    destroy_surface, get_buffer_data, get_buffer_pixels, get_dirty_regions, make_toplevel,
    needs_repaint, pool_count, raise, read_from_pool, record_damage, release_buffer, set_title,
    surface_count, write_to_pool,
};
pub use compositor::{COMPOSITOR, Compositor, Output};
pub use gates::{
    GATE_F_HEIGHT, GATE_F_WIDTH, GATE_F1_MARKER, GATE_F2_MARKER, GATE_F3_MARKER, GATE_F4_MARKER,
    GATE_F5_MARKER, GATE_F6_MARKER, GATE_F7_MARKER, GATE_GPUI_BUTTON_H, GATE_GPUI_BUTTON_W,
    GATE_GPUI_CALC_H, GATE_GPUI_CALC_W, GATE_GPUI_RING3_CALC, GATE_TERM_IO_MARKER,
    GATE_TERM_MARKER, GATE_TERM_PAINT_MARKER, GATE_WL_MARKER, WAYLAND_SOCKET_PATH,
    shm_pool_self_test,
};
pub use present::{
    dismiss_gate_demo_windows, forget_window, present_unix_buffer, present_user_buffer,
    pty_key_to_window, set_window_shm_pixels, window_pty, window_shm_pixels,
};
pub use protocol::{dispatch_client_bytes, on_client_connected};
pub use seat::{PointerAxis, PointerButton, Seat};
pub use shm::{PixelFormat, ShmBuffer, ShmPool};
pub use surface::{DamageRect, Popup, Surface, SurfaceRole, SurfaceState, Toplevel, ToplevelState};

use alloc::string::String;
use core::sync::atomic::{AtomicBool, Ordering};

use crate::serial_println;

use compositor::alloc_id;

static WAYLAND_ENABLED: AtomicBool = AtomicBool::new(false);

pub fn is_enabled() -> bool {
    WAYLAND_ENABLED.load(Ordering::Relaxed)
}

/// Initialize the Wayland compositor layer with real SHM buffer exchange
pub fn init() {
    let mut comp = compositor::COMPOSITOR.lock();

    // Register primary output (from framebuffer)
    let (out_w, out_h) = crate::gui::screen_size();
    comp.outputs.push(compositor::Output {
        id: alloc_id(),
        name: String::from("HDMI-A-1"),
        x: 0,
        y: 0,
        physical_width_mm: 527,
        physical_height_mm: 296,
        width: out_w as u32,
        height: out_h as u32,
        refresh_mhz: 60000,
        scale: 1,
    });

    drop(comp);

    WAYLAND_ENABLED.store(true, Ordering::Relaxed);
    crate::vfs::ensure_directory("/run");
    match crate::net::listen_unix_path(WAYLAND_SOCKET_PATH) {
        Ok(id) => serial_println!("[WL] listening on {} (socket {})", WAYLAND_SOCKET_PATH, id),
        Err(e) => serial_println!("[WL] listen {} failed: {}", WAYLAND_SOCKET_PATH, e),
    }
    serial_println!(
        "[WL] Wayland compositor initialized: {}x{} output, real SHM buffer exchange, damage-driven compositing",
        out_w,
        out_h
    );
}
