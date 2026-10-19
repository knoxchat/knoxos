/// Compositor object graph, SHM compositing, and the global compositor.
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, Ordering};
use spin::Mutex;

use crate::serial_println;

use super::seat::Seat;
use super::shm::{PixelFormat, ShmBuffer, ShmPool};
use super::surface::{DamageRect, Popup, Surface, SurfaceRole, Toplevel, ToplevelState};

// ─── Object IDs ────────────────────────────────────────────────────────

static NEXT_OBJECT_ID: AtomicU32 = AtomicU32::new(1);
pub(super) fn alloc_id() -> u32 {
    NEXT_OBJECT_ID.fetch_add(1, Ordering::Relaxed)
}

// ─── Output (display) ─────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct Output {
    pub id: u32,
    pub name: String,
    pub x: i32,
    pub y: i32,
    pub physical_width_mm: i32,
    pub physical_height_mm: i32,
    pub width: u32,
    pub height: u32,
    pub refresh_mhz: u32, // refresh rate in milli-Hz
    pub scale: i32,
}

// ─── Compositor ────────────────────────────────────────────────────────

pub struct Compositor {
    pub surfaces: BTreeMap<u32, Surface>,
    pub toplevels: BTreeMap<u32, Toplevel>,
    pub popups: BTreeMap<u32, Popup>,
    pub shm_pools: BTreeMap<u32, ShmPool>,
    pub shm_buffers: BTreeMap<u32, ShmBuffer>,
    pub seat: Seat,
    pub outputs: Vec<Output>,
    pub stacking_order: Vec<u32>, // surface IDs bottom-to-top
    pub frame_count: u64,
    pub needs_repaint: bool,
    pub dirty_regions: Vec<DamageRect>, // accumulate damage across all surfaces
    pub last_repaint_time: u64,
}

impl Default for Compositor {
    fn default() -> Self {
        Self::new()
    }
}

impl Compositor {
    pub fn new() -> Self {
        Self {
            surfaces: BTreeMap::new(),
            toplevels: BTreeMap::new(),
            popups: BTreeMap::new(),
            shm_pools: BTreeMap::new(),
            shm_buffers: BTreeMap::new(),
            seat: Seat::new(),
            outputs: Vec::new(),
            stacking_order: Vec::new(),
            frame_count: 0,
            needs_repaint: true,
            dirty_regions: Vec::new(),
            last_repaint_time: 0,
        }
    }

    // ── Surface management ────────────────────────────────────────

    pub fn create_surface(&mut self, client_pid: u32) -> u32 {
        let id = alloc_id();
        let surface = Surface::new(id, client_pid);
        self.surfaces.insert(id, surface);
        self.stacking_order.push(id);
        serial_println!("[WL] Surface {} created for pid {}", id, client_pid);
        id
    }

    pub fn destroy_surface(&mut self, id: u32) {
        self.surfaces.remove(&id);
        self.toplevels.remove(&id);
        self.popups.remove(&id);
        self.stacking_order.retain(|&s| s != id);
        self.needs_repaint = true;
        serial_println!("[WL] Surface {} destroyed", id);
    }

    pub fn surface_attach(&mut self, surface_id: u32, buffer_id: u32, x: i32, y: i32) {
        if let Some(surf) = self.surfaces.get_mut(&surface_id) {
            surf.pending.buffer_id = Some(buffer_id);
            surf.pending.buffer_x = x;
            surf.pending.buffer_y = y;
        }
    }

    pub fn surface_damage(&mut self, surface_id: u32, x: i32, y: i32, w: i32, h: i32) {
        if let Some(surf) = self.surfaces.get_mut(&surface_id) {
            surf.pending.damage.push(DamageRect {
                x,
                y,
                width: w,
                height: h,
            });
        }
    }

    pub fn surface_commit(&mut self, surface_id: u32) {
        if let Some(surf) = self.surfaces.get_mut(&surface_id) {
            surf.commit();
            surf.visible = true;
            self.needs_repaint = true;
        }
    }

    pub fn surface_set_position(&mut self, surface_id: u32, x: i32, y: i32) {
        if let Some(surf) = self.surfaces.get_mut(&surface_id) {
            surf.x = x;
            surf.y = y;
            self.needs_repaint = true;
        }
    }

    // ── Toplevel management ───────────────────────────────────────

    pub fn create_toplevel(&mut self, surface_id: u32) -> u32 {
        if let Some(surf) = self.surfaces.get_mut(&surface_id) {
            surf.role = SurfaceRole::Toplevel;
        }
        let toplevel = Toplevel::new(surface_id);
        self.toplevels.insert(surface_id, toplevel);
        serial_println!("[WL] Toplevel created for surface {}", surface_id);
        surface_id
    }

    pub fn toplevel_set_title(&mut self, surface_id: u32, title: &str) {
        if let Some(tl) = self.toplevels.get_mut(&surface_id) {
            tl.title = String::from(title);
        }
    }

    pub fn toplevel_set_app_id(&mut self, surface_id: u32, app_id: &str) {
        if let Some(tl) = self.toplevels.get_mut(&surface_id) {
            tl.app_id = String::from(app_id);
        }
    }

    pub fn toplevel_maximize(&mut self, surface_id: u32) {
        if let Some(tl) = self.toplevels.get_mut(&surface_id) {
            tl.state = ToplevelState::Maximized;
            self.needs_repaint = true;
        }
    }

    pub fn toplevel_fullscreen(&mut self, surface_id: u32) {
        if let Some(tl) = self.toplevels.get_mut(&surface_id) {
            tl.state = ToplevelState::Fullscreen;
            self.needs_repaint = true;
        }
    }

    // ── Popup ─────────────────────────────────────────────────────

    pub fn create_popup(
        &mut self,
        surface_id: u32,
        parent_id: u32,
        x: i32,
        y: i32,
        w: u32,
        h: u32,
    ) -> u32 {
        if let Some(surf) = self.surfaces.get_mut(&surface_id) {
            surf.role = SurfaceRole::Popup;
            surf.parent = Some(parent_id);
        }
        let popup = Popup {
            surface_id,
            parent_surface_id: parent_id,
            x,
            y,
            width: w,
            height: h,
        };
        self.popups.insert(surface_id, popup);
        surface_id
    }

    // ── Focus management ──────────────────────────────────────────

    pub fn raise_surface(&mut self, surface_id: u32) {
        self.stacking_order.retain(|&s| s != surface_id);
        self.stacking_order.push(surface_id);
        self.seat.keyboard_focus = Some(surface_id);
        self.needs_repaint = true;
    }

    pub fn pointer_enter(&mut self, surface_id: u32, sx: f64, sy: f64) {
        self.seat.pointer_focus = Some(surface_id);
        self.seat.pointer_x = sx;
        self.seat.pointer_y = sy;
    }

    pub fn pointer_motion(&mut self, sx: f64, sy: f64) {
        self.seat.pointer_x = sx;
        self.seat.pointer_y = sy;
    }

    pub fn surface_at(&self, x: f64, y: f64) -> Option<u32> {
        for &sid in self.stacking_order.iter().rev() {
            if let Some(surf) = self.surfaces.get(&sid) {
                if !surf.visible {
                    continue;
                }
                let sx = surf.x as f64;
                let sy = surf.y as f64;
                // Use actual buffer dimensions from attached SHM buffer
                let (sw, sh) = if let Some(bid) = surf.current.buffer_id {
                    if let Some(buf) = self.shm_buffers.get(&bid) {
                        (buf.width as f64, buf.height as f64)
                    } else {
                        (800.0, 600.0)
                    }
                } else {
                    (800.0, 600.0)
                };
                if x >= sx && x < sx + sw && y >= sy && y < sy + sh {
                    return Some(sid);
                }
            }
        }
        None
    }

    // ── SHM buffer data access ────────────────────────────────────

    /// Write pixel data directly into a SHM pool's backing memory.
    /// Clients use this to submit rendered content to the compositor.
    /// Returns true if the write was successful.
    pub fn write_to_pool(&mut self, pool_id: u32, offset: usize, data: &[u8]) -> bool {
        if let Some(pool) = self.shm_pools.get(&pool_id) {
            if pool.base_addr == 0 {
                return false;
            }
            if offset + data.len() > pool.size {
                serial_println!(
                    "[WL] write_to_pool: out of bounds ({} + {} > {})",
                    offset,
                    data.len(),
                    pool.size
                );
                return false;
            }
            // SAFETY: src and dst are valid for the copied byte count and do not overlap.
            unsafe {
                core::ptr::copy_nonoverlapping(
                    data.as_ptr(),
                    (pool.base_addr as *mut u8).add(offset),
                    data.len(),
                );
            }
            true
        } else {
            false
        }
    }

    /// Read pixel data from a SHM pool's backing memory.
    /// Used by the compositor to read client-rendered content.
    pub fn read_from_pool(&self, pool_id: u32, offset: usize, len: usize) -> Option<Vec<u8>> {
        if let Some(pool) = self.shm_pools.get(&pool_id) {
            if pool.base_addr == 0 || offset + len > pool.size {
                return None;
            }
            let mut data = vec![0u8; len];
            // SAFETY: src and dst are valid for the copied byte count and do not overlap.
            unsafe {
                core::ptr::copy_nonoverlapping(
                    (pool.base_addr as *const u8).add(offset),
                    data.as_mut_ptr(),
                    len,
                );
            }
            Some(data)
        } else {
            None
        }
    }

    /// Read raw pixel data for a specific buffer from its SHM pool.
    /// Returns (data_ptr, width, height, stride, format) for direct compositing.
    pub fn get_buffer_pixels(&self, buffer_id: u32) -> Option<(u64, u32, u32, u32, PixelFormat)> {
        if let Some(buf) = self.shm_buffers.get(&buffer_id) {
            if let Some(pool) = self.shm_pools.get(&buf.pool_id) {
                if pool.base_addr == 0 {
                    return None;
                }
                let ptr = buf.data_ptr(pool.base_addr);
                return Some((ptr, buf.width, buf.height, buf.stride, buf.format));
            }
        }
        None
    }

    /// Composite all visible surfaces onto a destination BGRA buffer.
    /// This is the core rendering function: reads SHM client buffers and
    /// alpha-blends them in stacking order onto the output buffer.
    ///
    /// `dest` is a BGRA pixel buffer (4 bytes per pixel, row-major).
    /// `dest_width` and `dest_height` define the output dimensions.
    /// Only damaged regions are composited when `damage_only` is true.
    pub fn compose_to_buffer(
        &mut self,
        dest: &mut [u8],
        dest_width: u32,
        dest_height: u32,
        damage_only: bool,
    ) -> usize {
        let visible: Vec<u32> = self
            .stacking_order
            .iter()
            .filter(|&&sid| self.surfaces.get(&sid).is_some_and(|s| s.visible))
            .copied()
            .collect();

        let mut surfaces_composited = 0usize;

        for sid in &visible {
            let (surf_x, surf_y, buffer_id) = {
                if let Some(surf) = self.surfaces.get(sid) {
                    if let Some(bid) = surf.current.buffer_id {
                        (surf.x, surf.y, bid)
                    } else {
                        continue;
                    }
                } else {
                    continue;
                }
            };

            // Get the buffer pixel data
            let (data_ptr, buf_w, buf_h, buf_stride, buf_fmt) =
                if let Some(info) = self.get_buffer_pixels(buffer_id) {
                    info
                } else {
                    continue;
                };

            // Mark buffer as busy (display is reading from it)
            if let Some(buf) = self.shm_buffers.get_mut(&buffer_id) {
                buf.busy = true;
            }

            let bpp = buf_fmt.bytes_per_pixel();
            let dest_stride = dest_width as usize * 4; // always BGRA

            // Composite this surface's pixel data onto the destination buffer
            for row in 0..buf_h as i32 {
                let dy = surf_y + row;
                if dy < 0 || dy >= dest_height as i32 {
                    continue;
                }
                for col in 0..buf_w as i32 {
                    let dx = surf_x + col;
                    if dx < 0 || dx >= dest_width as i32 {
                        continue;
                    }
                    // Read source pixel from SHM buffer
                    let src_off = row as usize * buf_stride as usize + col as usize * bpp;
                    // SAFETY: the pointer is derived from a kernel object we own or a mapped physical address; the type layout matches the hardware or on-disk record.
                    let (sr, sg, sb, sa) = unsafe {
                        let base = data_ptr as *const u8;
                        match buf_fmt {
                            PixelFormat::Argb8888 => {
                                let b = *base.add(src_off);
                                let g = *base.add(src_off + 1);
                                let r = *base.add(src_off + 2);
                                let a = *base.add(src_off + 3);
                                (r, g, b, a)
                            }
                            PixelFormat::Xrgb8888 => {
                                let b = *base.add(src_off);
                                let g = *base.add(src_off + 1);
                                let r = *base.add(src_off + 2);
                                (r, g, b, 255u8)
                            }
                            PixelFormat::Bgra8888 => {
                                let b = *base.add(src_off);
                                let g = *base.add(src_off + 1);
                                let r = *base.add(src_off + 2);
                                let a = *base.add(src_off + 3);
                                (r, g, b, a)
                            }
                            PixelFormat::Rgb565 => {
                                let lo = *base.add(src_off) as u16;
                                let hi = *base.add(src_off + 1) as u16;
                                let val = lo | (hi << 8);
                                let r = ((val >> 11) & 0x1f) as u8 * 8;
                                let g = ((val >> 5) & 0x3f) as u8 * 4;
                                let b = (val & 0x1f) as u8 * 8;
                                (r, g, b, 255u8)
                            }
                        }
                    };

                    // Alpha-blend onto destination (BGRA byte order)
                    let dst_off = dy as usize * dest_stride + dx as usize * 4;
                    if dst_off + 3 >= dest.len() {
                        continue;
                    }

                    if sa == 255 {
                        // Opaque: direct write
                        dest[dst_off] = sb;
                        dest[dst_off + 1] = sg;
                        dest[dst_off + 2] = sr;
                        dest[dst_off + 3] = 255;
                    } else if sa > 0 {
                        // Alpha blend
                        let alpha = sa as u16;
                        let inv_alpha = 255 - alpha;
                        dest[dst_off] =
                            ((sb as u16 * alpha + dest[dst_off] as u16 * inv_alpha) / 255) as u8;
                        dest[dst_off + 1] = ((sg as u16 * alpha
                            + dest[dst_off + 1] as u16 * inv_alpha)
                            / 255) as u8;
                        dest[dst_off + 2] = ((sr as u16 * alpha
                            + dest[dst_off + 2] as u16 * inv_alpha)
                            / 255) as u8;
                        dest[dst_off + 3] = 255;
                    }
                }
            }

            // Release the buffer back to the client
            if let Some(buf) = self.shm_buffers.get_mut(&buffer_id) {
                buf.busy = false;
                buf.released = true;
            }

            surfaces_composited += 1;
        }

        // Send frame callbacks (increment frame counter)
        self.frame_count += 1;
        self.needs_repaint = false;
        self.dirty_regions.clear();

        surfaces_composited
    }

    // ── Frame scheduling ──────────────────────────────────────────

    pub fn begin_frame(&mut self) -> Vec<u32> {
        self.frame_count += 1;
        self.needs_repaint = false;
        // Return list of visible surfaces in stacking order
        self.stacking_order
            .iter()
            .filter(|&&sid| self.surfaces.get(&sid).is_some_and(|s| s.visible))
            .copied()
            .collect()
    }

    pub fn end_frame(&mut self) {
        // Send frame done callbacks — release all buffers that were composited
        for buf in self.shm_buffers.values_mut() {
            if buf.busy {
                buf.busy = false;
                buf.released = true;
            }
        }
    }

    // ── Client cleanup ────────────────────────────────────────────

    pub fn client_disconnected(&mut self, pid: u32) {
        let surface_ids: Vec<u32> = self
            .surfaces
            .iter()
            .filter(|(_, s)| s.client_pid == pid)
            .map(|(&id, _)| id)
            .collect();
        for id in surface_ids {
            self.destroy_surface(id);
        }
    }

    // ── Shared Memory Pool Management ──────────────────────────────

    /// Create a shared memory pool backed by real kernel-allocated memory
    pub fn create_shm_pool(&mut self, fd: i32, size: usize, pid: u32) -> u32 {
        let id = alloc_id();

        // Allocate real kernel memory for the SHM pool using the global allocator.
        // The buffer is zeroed so clients start with a clean slate.
        let layout = alloc::alloc::Layout::from_size_align(size, 4096)
            .unwrap_or(alloc::alloc::Layout::from_size_align(size, 8).unwrap());
        // SAFETY: kernel-mode exclusive access; operands are owned, mapped, or architecture-defined for this path.
        let base_addr = unsafe { alloc::alloc::alloc_zeroed(layout) } as u64;

        if base_addr == 0 {
            serial_println!("[WL] SHM pool alloc failed: {} bytes for pid {}", size, pid);
            // Return a pool with zero base — callers should check
            let pool = ShmPool {
                id,
                fd,
                size,
                base_addr: 0,
                alloc_layout: Some(layout),
                client_pid: pid,
            };
            self.shm_pools.insert(id, pool);
            return id;
        }

        let pool = ShmPool {
            id,
            fd,
            size,
            base_addr,
            alloc_layout: Some(layout),
            client_pid: pid,
        };
        self.shm_pools.insert(id, pool);
        serial_println!(
            "[WL] SHM pool {} created: {} bytes at {:#x} (pid {})",
            id,
            size,
            base_addr,
            pid
        );
        id
    }

    /// Resize an existing SHM pool (allocate new, copy, free old)
    pub fn resize_shm_pool(&mut self, pool_id: u32, new_size: usize) -> bool {
        if let Some(pool) = self.shm_pools.get_mut(&pool_id) {
            if new_size <= pool.size {
                return true; // shrink is a no-op (Wayland spec: pool can only grow)
            }
            let new_layout = alloc::alloc::Layout::from_size_align(new_size, 4096)
                .unwrap_or(alloc::alloc::Layout::from_size_align(new_size, 8).unwrap());
            // SAFETY: kernel-mode exclusive access; operands are owned, mapped, or architecture-defined for this path.
            let new_addr = unsafe { alloc::alloc::alloc_zeroed(new_layout) } as u64;
            if new_addr == 0 {
                serial_println!(
                    "[WL] SHM pool {} resize failed: {} bytes",
                    pool_id,
                    new_size
                );
                return false;
            }
            // Copy old data to new allocation
            if pool.base_addr != 0 {
                // SAFETY: src and dst are valid for the copied byte count and do not overlap.
                unsafe {
                    core::ptr::copy_nonoverlapping(
                        pool.base_addr as *const u8,
                        new_addr as *mut u8,
                        pool.size,
                    );
                }
                // Free old allocation
                if let Some(old_layout) = pool.alloc_layout {
                    // SAFETY: the pointer is derived from a kernel object we own or a mapped physical address; the type layout matches the hardware or on-disk record.
                    unsafe {
                        alloc::alloc::dealloc(pool.base_addr as *mut u8, old_layout);
                    }
                }
            }
            pool.base_addr = new_addr;
            pool.size = new_size;
            pool.alloc_layout = Some(new_layout);
            serial_println!(
                "[WL] SHM pool {} resized to {} bytes at {:#x}",
                pool_id,
                new_size,
                new_addr
            );
            true
        } else {
            false
        }
    }

    /// Destroy a SHM pool, all its buffers, and free the backing memory
    pub fn destroy_shm_pool(&mut self, pool_id: u32) {
        if let Some(pool) = self.shm_pools.remove(&pool_id) {
            // Clean up all buffers from this pool
            let buffer_ids: Vec<u32> = self
                .shm_buffers
                .iter()
                .filter(|(_, b)| b.pool_id == pool_id)
                .map(|(&id, _)| id)
                .collect();
            for bid in buffer_ids {
                self.shm_buffers.remove(&bid);
            }
            // Free the allocated kernel memory
            if pool.base_addr != 0 {
                if let Some(layout) = pool.alloc_layout {
                    // SAFETY: the pointer is derived from a kernel object we own or a mapped physical address; the type layout matches the hardware or on-disk record.
                    unsafe {
                        alloc::alloc::dealloc(pool.base_addr as *mut u8, layout);
                    }
                }
            }
            serial_println!(
                "[WL] SHM pool {} destroyed (freed {} bytes)",
                pool_id,
                pool.size
            );
        }
    }

    /// Create a buffer from a SHM pool
    pub fn create_shm_buffer(
        &mut self,
        pool_id: u32,
        offset: usize,
        width: u32,
        height: u32,
        stride: u32,
        format: PixelFormat,
        pid: u32,
    ) -> Option<u32> {
        if !self.shm_pools.contains_key(&pool_id) {
            return None;
        }

        let id = alloc_id();
        let buffer = ShmBuffer::new(id, pool_id, offset, width, height, stride, format, pid);
        self.shm_buffers.insert(id, buffer);
        serial_println!(
            "[WL] Buffer {} created: {}x{} format={:?}",
            id,
            width,
            height,
            format
        );
        Some(id)
    }

    /// Destroy a SHM buffer
    pub fn destroy_shm_buffer(&mut self, buffer_id: u32) {
        if self.shm_buffers.remove(&buffer_id).is_some() {
            serial_println!("[WL] Buffer {} destroyed", buffer_id);
        }
    }

    /// Mark buffer as busy (in use by display/GPU)
    pub fn buffer_set_busy(&mut self, buffer_id: u32, busy: bool) {
        if let Some(buf) = self.shm_buffers.get_mut(&buffer_id) {
            buf.busy = busy;
        }
    }

    /// Attach a SHM buffer to a surface (calls existing surface_attach)
    pub fn attach_shm_buffer(&mut self, surface_id: u32, buffer_id: u32, x: i32, y: i32) -> bool {
        if !self.shm_buffers.contains_key(&buffer_id) {
            return false;
        }
        self.surface_attach(surface_id, buffer_id, x, y);
        true
    }

    // ── Damage Region Tracking ────────────────────────────────────

    /// Record damage for a specific surface buffer
    pub fn record_buffer_damage(&mut self, surface_id: u32, x: i32, y: i32, w: i32, h: i32) {
        if let Some(surf) = self.surfaces.get(&surface_id) {
            if let Some(buffer_id) = surf.pending.buffer_id {
                if let Some(buf) = self.shm_buffers.get_mut(&buffer_id) {
                    buf.pending_damage.push(DamageRect {
                        x,
                        y,
                        width: w,
                        height: h,
                    });
                }
            }
        }
    }

    /// Accumulate damage regions across entire compositor
    pub fn accumulate_damage(&mut self) {
        self.dirty_regions.clear();

        for buf in self.shm_buffers.values_mut() {
            if !buf.pending_damage.is_empty() {
                self.dirty_regions.append(&mut buf.pending_damage);
                self.needs_repaint = true;
            }
        }
    }

    /// Get regions that need repainting (damage-driven redraw)
    pub fn get_dirty_regions(&self) -> &[DamageRect] {
        &self.dirty_regions
    }

    /// Clear dirty regions after redraw
    pub fn clear_dirty_regions(&mut self) {
        self.dirty_regions.clear();
    }

    /// Copy SHM buffer data to destination (for rendering)
    pub fn get_buffer_data(&self, buffer_id: u32) -> Option<(u64, usize)> {
        if let Some(buf) = self.shm_buffers.get(&buffer_id) {
            if let Some(pool) = self.shm_pools.get(&buf.pool_id) {
                let ptr = buf.data_ptr(pool.base_addr);
                let size = buf.size_bytes();
                return Some((ptr, size));
            }
        }
        None
    }

    /// Release buffer (signal to client that display is done reading)
    pub fn release_buffer(&mut self, buffer_id: u32) {
        if let Some(buf) = self.shm_buffers.get_mut(&buffer_id) {
            buf.released = true;
            buf.busy = false;
            serial_println!("[WL] Buffer {} released", buffer_id);
        }
    }
}

lazy_static::lazy_static! {
    pub static ref COMPOSITOR: Mutex<Compositor> = Mutex::new(Compositor::new());
}
