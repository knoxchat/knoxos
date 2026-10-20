/// Ring 3 SHM present path: desktop windows, pixel copies, and PTY routing.
use alloc::collections::BTreeMap;
use alloc::vec;
use alloc::vec::Vec;
use spin::Mutex;

use crate::serial_println;

use super::compositor::COMPOSITOR;
use super::gates::{
    GATE_F_HEIGHT, GATE_F_WIDTH, GATE_F2_MARKER, GATE_F3_MAGIC, GATE_F3_MARKER, GATE_F4_MAGIC,
    GATE_F4_MARKER, GATE_F5_MARKER, GATE_F6_MAGIC, GATE_F6_MARKER, GATE_F7_MAGIC, GATE_F7_MARKER,
    GATE_TERM_MAGIC, GATE_TERM_MARKER, WAYLAND_SOCKET_PATH, is_known_shm_magic,
    paint_term_first_frame, shm_title_for_magic,
};
use super::shm::PixelFormat;

lazy_static::lazy_static! {
    /// Desktop window id → Wayland surface id for Ring 3 SHM clients.
    static ref WINDOW_SURFACES: Mutex<BTreeMap<u32, u32>> = Mutex::new(BTreeMap::new());
    /// Copied SHM pixels so the GUI can blit without locking the compositor.
    static ref WINDOW_PIXELS: Mutex<BTreeMap<u32, (Vec<u8>, u32, u32)>> = Mutex::new(BTreeMap::new());
    /// Ring 3 pid → chrome window id (live present updates this surface).
    static ref CLIENT_WINDOWS: Mutex<BTreeMap<u32, u32>> = Mutex::new(BTreeMap::new());
    /// Chrome window id → PTY index for Ring 3 Terminal.
    static ref WINDOW_PTY: Mutex<BTreeMap<u32, u32>> = Mutex::new(BTreeMap::new());
}

/// Present a Ring 3 SHM buffer as a compositor surface + desktop window.
///
/// `vaddr` is the client’s mmap’d 64×64 BGRA buffer. Pixels are copied into a
/// kernel SHM pool (F2) and a chrome window blits them. F3/F4 magics force
/// `WindowContentType::Empty` so the terminal and launcher are not in-kernel
/// app enums.
pub fn present_user_buffer(pid: u32, vaddr: u64) -> bool {
    let len = (GATE_F_WIDTH * GATE_F_HEIGHT * 4) as usize;
    let mut pixels = vec![0u8; len];
    crate::vmm::read_user_memory(pid, vaddr, &mut pixels);
    let term_painted = paint_term_first_frame(&mut pixels);

    let mut comp = COMPOSITOR.lock();
    let pool = comp.create_shm_pool(-1, len, pid);
    if !comp.write_to_pool(pool, 0, &pixels) {
        serial_println!("[WL] Gate F FAILED: SHM pool write");
        return false;
    }
    let Some(buf) = comp.create_shm_buffer(
        pool,
        0,
        GATE_F_WIDTH,
        GATE_F_HEIGHT,
        GATE_F_WIDTH * 4,
        PixelFormat::Bgra8888,
        pid,
    ) else {
        serial_println!("[WL] Gate F FAILED: SHM buffer");
        return false;
    };
    let sid = comp.create_surface(pid);
    comp.create_toplevel(sid);
    let title = shm_title_for_magic(&pixels);
    comp.toplevel_set_title(sid, title);
    if !comp.attach_shm_buffer(sid, buf, 0, 0) {
        serial_println!("[WL] Gate F FAILED: attach");
        return false;
    }
    comp.surface_damage(sid, 0, 0, GATE_F_WIDTH as i32, GATE_F_HEIGHT as i32);
    comp.surface_commit(sid);
    let roundtrip = comp.read_from_pool(pool, 0, 4);
    drop(comp);

    if let Some(&existing) = CLIENT_WINDOWS.lock().get(&pid) {
        WINDOW_PIXELS
            .lock()
            .insert(existing, (pixels.clone(), GATE_F_WIDTH, GATE_F_HEIGHT));
        crate::gui::request_redraw();
        return magic_ok_for(&pixels, &roundtrip, existing, sid, pid, title);
    }

    let mut win = crate::gui::window::Window::new(title, 120, 80, 280, 200);
    win.content_type = crate::gui::window::WindowContentType::Empty;
    let wid = win.id;
    crate::gui::window::WINDOW_MANAGER.lock().add_window(win);
    CLIENT_WINDOWS.lock().insert(pid, wid);
    WINDOW_SURFACES.lock().insert(wid, sid);
    WINDOW_PIXELS
        .lock()
        .insert(wid, (pixels.clone(), GATE_F_WIDTH, GATE_F_HEIGHT));
    if pixels.get(0..4) == Some(&GATE_TERM_MAGIC[..]) {
        if let Some(pty) = crate::pty::pty_for_pid(pid) {
            WINDOW_PTY.lock().insert(wid, pty);
        }
    }
    crate::gui::request_redraw();

    magic_ok_for(&pixels, &roundtrip, wid, sid, pid, title)
}

fn magic_ok_for(
    pixels: &[u8],
    roundtrip: &Option<Vec<u8>>,
    wid: u32,
    sid: u32,
    pid: u32,
    title: &str,
) -> bool {
    let head = pixels.get(0..4);
    let magic_ok = match head {
        Some(p) if is_known_shm_magic(p) => roundtrip.as_deref() == Some(p),
        _ => false,
    };
    if magic_ok {
        serial_println!(
            "[WL] {} (pid={} surface={} window={} title={})",
            GATE_F2_MARKER,
            pid,
            sid,
            wid,
            title
        );
        if pixels[0..4] == GATE_F3_MAGIC {
            serial_println!(
                "[WL] {} (window={} content=Empty, not WindowContentType::Terminal)",
                GATE_F3_MARKER,
                wid
            );
        }
        if pixels[0..4] == GATE_F4_MAGIC {
            serial_println!(
                "[WL] {} (window={} content=Empty SHM, not launcher stub)",
                GATE_F4_MARKER,
                wid
            );
        }
        if pixels[0..4] == GATE_F6_MAGIC {
            serial_println!(
                "[WL] {} (window={} content=Empty SHM, not WindowContentType::Calculator)",
                GATE_F6_MARKER,
                wid
            );
        }
        if pixels[0..4] == GATE_F7_MAGIC {
            serial_println!(
                "[WL] {} (window={} content=Empty SHM, not WindowContentType::FileExplorer)",
                GATE_F7_MARKER,
                wid
            );
        }
        if pixels[0..4] == GATE_TERM_MAGIC {
            serial_println!(
                "[WL] {} (window={} content=Empty SHM, openpty live, not WindowContentType::Terminal)",
                GATE_TERM_MARKER,
                wid
            );
        }
    } else {
        serial_println!(
            "[WL] Gate F2 FAILED: client={:?} shm={:?}",
            pixels.get(0..4),
            roundtrip.as_deref()
        );
    }
    magic_ok
}

/// Present via the compositor's AF_UNIX listen socket (Gate F5).
pub fn present_unix_buffer(pid: u32, vaddr: u64) -> bool {
    if !present_user_buffer(pid, vaddr) {
        serial_println!(
            "[WL] Gate F5 FAILED: unix present pid={} vaddr={:#x}",
            pid,
            vaddr
        );
        return false;
    }
    serial_println!(
        "[WL] {} (pid={} path={})",
        GATE_F5_MARKER,
        pid,
        WAYLAND_SOCKET_PATH
    );
    true
}

/// SHM pixels for a desktop window that was created from a Ring 3 present.
pub fn window_shm_pixels(window_id: u32) -> Option<(Vec<u8>, u32, u32)> {
    WINDOW_PIXELS.lock().get(&window_id).cloned()
}

/// Close leftover Ring 3 SHM demo windows after boot gates so the desktop
/// is not left with a scaled `"OK"` Terminal stub.
pub fn dismiss_gate_demo_windows() {
    let wids: Vec<u32> = CLIENT_WINDOWS.lock().values().copied().collect();
    if wids.is_empty() {
        return;
    }
    let n = wids.len();
    for wid in wids {
        crate::gui::window::WINDOW_MANAGER.lock().close_window(wid);
        crate::gui::taskbar::remove_entry(wid);
    }
    serial_println!("[WL] dismissed {} gate demo window(s)", n);
}

/// Drop compositor mappings when a chrome window is closed.
pub fn forget_window(window_id: u32) {
    WINDOW_SURFACES.lock().remove(&window_id);
    WINDOW_PIXELS.lock().remove(&window_id);
    WINDOW_PTY.lock().remove(&window_id);
    CLIENT_WINDOWS.lock().retain(|_, wid| *wid != window_id);
}

/// PTY index attached to a Ring 3 Terminal SHM window.
pub fn window_pty(window_id: u32) -> Option<u32> {
    WINDOW_PTY.lock().get(&window_id).copied()
}

/// Keyboard → PTY master for an Empty+SHM Terminal window.
pub fn pty_key_to_window(window_id: u32, bytes: &[u8]) {
    let Some(pty) = window_pty(window_id) else {
        return;
    };
    let _ = crate::pty::write_master(pty, bytes);
    let mut out = [0u8; 64];
    let n = crate::pty::read_master(pty, &mut out).unwrap_or(0);
    if n == 0 {
        return;
    }
    let mut pixels_guard = WINDOW_PIXELS.lock();
    if let Some((pixels, w, h)) = pixels_guard.get_mut(&window_id) {
        // Paint echoed bytes as a 1-pixel-high activity strip so SHM updates.
        let stride = (*w as usize) * 4;
        let row = ((*h as usize) / 2).saturating_sub(1);
        for (i, b) in out[..n].iter().enumerate() {
            let x = i.min(*w as usize - 1);
            let off = row * stride + x * 4;
            if off + 3 < pixels.len() {
                pixels[off] = *b;
                pixels[off + 1] = 0xC0;
                pixels[off + 2] = 0x20;
                pixels[off + 3] = 0xFF;
            }
        }
    }
}
