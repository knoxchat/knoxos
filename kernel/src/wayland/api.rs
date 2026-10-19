/// Thin public wrappers around the global compositor.
use alloc::vec::Vec;

use crate::serial_println;

use super::compositor::COMPOSITOR;
use super::shm::PixelFormat;
use super::surface::DamageRect;

// ─── Public API ────────────────────────────────────────────────────────

pub fn create_surface(pid: u32) -> u32 {
    COMPOSITOR.lock().create_surface(pid)
}

pub fn destroy_surface(id: u32) {
    COMPOSITOR.lock().destroy_surface(id);
}

pub fn commit_surface(id: u32) {
    COMPOSITOR.lock().surface_commit(id);
}

pub fn set_title(surface_id: u32, title: &str) {
    COMPOSITOR.lock().toplevel_set_title(surface_id, title);
}

pub fn make_toplevel(surface_id: u32) {
    COMPOSITOR.lock().create_toplevel(surface_id);
}

pub fn raise(surface_id: u32) {
    COMPOSITOR.lock().raise_surface(surface_id);
}

// ─── Shared Memory Buffer Public API ────────────────────────────────

pub fn create_shm_pool(fd: i32, size: usize, pid: u32) -> u32 {
    COMPOSITOR.lock().create_shm_pool(fd, size, pid)
}

pub fn destroy_shm_pool(pool_id: u32) {
    COMPOSITOR.lock().destroy_shm_pool(pool_id);
}

pub fn create_shm_buffer(
    pool_id: u32,
    offset: usize,
    width: u32,
    height: u32,
    stride: u32,
    format: PixelFormat,
    pid: u32,
) -> Option<u32> {
    COMPOSITOR
        .lock()
        .create_shm_buffer(pool_id, offset, width, height, stride, format, pid)
}

pub fn destroy_shm_buffer(buffer_id: u32) {
    COMPOSITOR.lock().destroy_shm_buffer(buffer_id);
}

pub fn attach_shm_buffer(surface_id: u32, buffer_id: u32, x: i32, y: i32) -> bool {
    COMPOSITOR
        .lock()
        .attach_shm_buffer(surface_id, buffer_id, x, y)
}

pub fn record_damage(surface_id: u32, x: i32, y: i32, w: i32, h: i32) {
    COMPOSITOR
        .lock()
        .record_buffer_damage(surface_id, x, y, w, h);
}

pub fn get_dirty_regions() -> Vec<DamageRect> {
    COMPOSITOR.lock().get_dirty_regions().to_vec()
}

pub fn buffer_set_busy(buffer_id: u32, busy: bool) {
    COMPOSITOR.lock().buffer_set_busy(buffer_id, busy);
}

pub fn release_buffer(buffer_id: u32) {
    COMPOSITOR.lock().release_buffer(buffer_id);
}

pub fn get_buffer_data(buffer_id: u32) -> Option<(u64, usize)> {
    COMPOSITOR.lock().get_buffer_data(buffer_id)
}

/// Write pixel data into a SHM pool's backing memory
pub fn write_to_pool(pool_id: u32, offset: usize, data: &[u8]) -> bool {
    COMPOSITOR.lock().write_to_pool(pool_id, offset, data)
}

/// Read pixel data from a SHM pool
pub fn read_from_pool(pool_id: u32, offset: usize, len: usize) -> Option<Vec<u8>> {
    COMPOSITOR.lock().read_from_pool(pool_id, offset, len)
}

/// Get buffer pixel info for compositing: (data_ptr, width, height, stride, format)
pub fn get_buffer_pixels(buffer_id: u32) -> Option<(u64, u32, u32, u32, PixelFormat)> {
    COMPOSITOR.lock().get_buffer_pixels(buffer_id)
}

/// Composite all visible Wayland surfaces onto a BGRA destination buffer.
/// Returns the number of surfaces composited.
pub fn compose_to_buffer(
    dest: &mut [u8],
    dest_width: u32,
    dest_height: u32,
    damage_only: bool,
) -> usize {
    COMPOSITOR
        .lock()
        .compose_to_buffer(dest, dest_width, dest_height, damage_only)
}

/// Composite Wayland surfaces directly onto the GUI framebuffer.
/// This is the main integration point between Wayland clients and the display.
pub fn compose_to_framebuffer() -> usize {
    let mut fb_lock = crate::gui::FRAMEBUFFER.lock();
    if let Some(ref mut fb) = *fb_lock {
        let w = fb.width as u32;
        let h = fb.height as u32;

        // Composite directly into the framebuffer's back buffer
        let count = {
            let buf = &mut fb.buffer;
            COMPOSITOR.lock().compose_to_buffer(buf, w, h, false)
        };

        if count > 0 {
            // Present the composited frame to hardware
            fb.present();
            serial_println!(
                "[WL] Composited {} surfaces to framebuffer ({}x{})",
                count,
                w,
                h
            );
        }
        count
    } else {
        0
    }
}

/// Check if the compositor has pending repaints
pub fn needs_repaint() -> bool {
    COMPOSITOR.lock().needs_repaint
}

/// Get the number of active surfaces
pub fn surface_count() -> usize {
    COMPOSITOR.lock().surfaces.len()
}

/// Get the number of active SHM pools
pub fn pool_count() -> usize {
    COMPOSITOR.lock().shm_pools.len()
}
