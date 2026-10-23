/// Desktop - Main desktop environment rendering
/// Draws the Aurora wallpaper, desktop icons, and coordinates all GUI elements
mod apps;
mod context_menu;
mod cursor;
mod files;
mod folder;
mod icons;
mod interaction;
mod types;
mod wallpaper;
mod zoom;

pub use apps::{
    open_application, open_browser_at, open_editor_at, open_explorer_at, open_session_defaults,
};
pub use context_menu::{
    close_context_menu, execute_action as execute_context_action, handle_context_menu_click,
    show_context_menu,
};
pub use cursor::{
    CURSOR_H, CURSOR_SAVE_H, CURSOR_SAVE_PAD, CURSOR_SAVE_W, CURSOR_W, CursorTheme, CursorType,
    cursor_colors, cursor_theme, draw_cursor, get_cursor_type, invalidate_cursor_bg,
    is_over_desktop_icon, last_cursor_pos, restore_cursor_background, save_cursor_background,
    set_cursor_theme, set_cursor_type, set_last_cursor_pos, update_cursor_for_position,
};
pub use files::accept_file_drop;
pub use folder::{
    empty_trash, move_to_trash, restore_from_trash, sync_desktop_folder, trash_count,
};
pub use interaction::{
    begin_rubber_band, cancel_icon_drag, finish_rubber_band, handle_click, handle_double_click,
    handle_icon_drag, handle_icon_drop, handle_rubber_band_drag, is_dragging_icon,
    is_rubber_band_active, press_icon,
};
pub use types::{
    CONTEXT_MENU, ContextAction, ContextMenuItem, DESKTOP, Desktop, DesktopContextMenu,
    DesktopIcon, IconType, RubberBand,
};
pub use wallpaper::invalidate_wallpaper_cache;

use alloc::boxed::Box;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, Ordering};
use spin::Mutex;

use super::FRAMEBUFFER;
use super::framebuffer::Rect;
use super::startmenu;
use super::taskbar;
use super::window;
use crate::serial_println;

use context_menu::{CONTEXT_MENU_WIDTH, draw_context_menu};
use icons::{draw_desktop_icons, draw_rubber_band};
use interaction::draw_snap_preview;
use wallpaper::{WALLPAPER_CACHE, draw_wallpaper};
use zoom::apply_zoom_lens;

/// Heap stack for the compositor. First GPUI pump lays out the kit gallery
/// plus shell windows; that depth overflows the bootloader stack and double
/// faults (RSP lands in unmapped memory, no guard page).
const COMPOSE_STACK_BYTES: usize = 512 * 1024;
static COMPOSE_STACK: Mutex<Option<Vec<u8>>> = Mutex::new(None);
static COMPOSE_DAMAGE: Mutex<Vec<Rect>> = Mutex::new(Vec::new());
static COMPOSE_JOB: Mutex<Option<Box<dyn FnOnce() + Send>>> = Mutex::new(None);
static COMPOSE_ACTIVE: AtomicBool = AtomicBool::new(false);
static COMPOSE_STACK_GATE: AtomicBool = AtomicBool::new(false);

/// Draw the complete desktop (called at startup and on redraws).
///
/// Accepts a list of damage rects describing which regions changed.
/// Only pixels inside the union of those rects are recomposed and presented
/// to the HW framebuffer. This is the key optimization that keeps the cursor
/// responsive even while windows are being dragged or resized:
///
///  - Mouse-only movement uses the fast `update_cursor_only()` path (no compositing)
///  - Window drag pushes two small damage rects (old position + new position)
///  - Click/scroll pushes damage for just the affected widget/area
///  - Full redraws (resolution change, first paint) push a screen-sized rect
pub fn draw_desktop_damaged(damage: &[Rect]) {
    if COMPOSE_ACTIVE.load(Ordering::SeqCst) {
        draw_desktop_damaged_inner(damage);
        return;
    }
    {
        let mut slot = COMPOSE_DAMAGE.lock();
        slot.clear();
        slot.extend_from_slice(damage);
    }
    COMPOSE_ACTIVE.store(true, Ordering::SeqCst);
    #[cfg(target_arch = "x86_64")]
    // SAFETY: GUI is single-threaded; the compose Vec is initialized once
    // and never moved; `compose_trampoline` returns to this asm.
    unsafe {
        call_on_compose_stack();
    }
    #[cfg(not(target_arch = "x86_64"))]
    compose_stack_entry();
    COMPOSE_ACTIVE.store(false, Ordering::SeqCst);
}

/// Run GPUI hit-testing / layout on the compositor heap stack.
///
/// Pointer dispatch (`set_absolute_mouse` → `proof_tree`) is as deep as the
/// first desktop paint. Doing it on the bootloader stack double-faults:
/// RSP walks into the physical-memory window (`0x18000000…`) with no guard.
pub fn run_on_compose_stack<F: FnOnce() + Send + 'static>(f: F) {
    if COMPOSE_ACTIVE.load(Ordering::SeqCst) {
        f();
        return;
    }
    *COMPOSE_JOB.lock() = Some(Box::new(f));
    COMPOSE_ACTIVE.store(true, Ordering::SeqCst);
    #[cfg(target_arch = "x86_64")]
    // SAFETY: same contract as `draw_desktop_damaged`; job runs and returns.
    unsafe {
        call_on_compose_stack();
    }
    #[cfg(not(target_arch = "x86_64"))]
    compose_stack_entry();
    COMPOSE_ACTIVE.store(false, Ordering::SeqCst);
}

fn compose_stack_top() -> u64 {
    let mut g = COMPOSE_STACK.lock();
    let stack = g.get_or_insert_with(|| alloc::vec![0u8; COMPOSE_STACK_BYTES]);
    let top = stack.as_ptr() as u64 + stack.len() as u64;
    drop(g);
    top & !0xF
}

extern "C" fn compose_stack_entry() {
    let job = COMPOSE_JOB.lock().take();
    if let Some(job) = job {
        job();
        return;
    }
    compose_trampoline();
}

extern "C" fn compose_trampoline() {
    #[cfg(target_arch = "x86_64")]
    if !COMPOSE_STACK_GATE.swap(true, Ordering::SeqCst) {
        let rsp: u64;
        // SAFETY: reads RSP only.
        unsafe {
            core::arch::asm!("mov {}, rsp", out(reg) rsp, options(nomem, nostack));
        }
        serial_println!("[desktop] trampoline rsp={:#x}", rsp);
    }
    let damage = COMPOSE_DAMAGE.lock().clone();
    draw_desktop_damaged_inner(&damage);
}

#[cfg(target_arch = "x86_64")]
unsafe fn call_on_compose_stack() {
    let top = compose_stack_top();
    // SAFETY: r12 holds the compose top and rbx the old RSP (both callee-saved
    // across the C call). `{top}` is copied into r12 *before* rbx is reused so
    // LLVM cannot allocate `{top}` to rbx and turn the switch into a no-op.
    // `compose_stack_entry` returns to this asm before RSP is restored.
    unsafe {
        core::arch::asm!(
            "push rbx",
            "push r12",
            "mov r12, {top}",
            "mov rbx, rsp",
            "mov rsp, r12",
            "call {entry}",
            "mov rsp, rbx",
            "pop r12",
            "pop rbx",
            top = in(reg) top,
            entry = sym compose_stack_entry,
            clobber_abi("C"),
            options(nostack),
        );
    }
}

fn draw_desktop_damaged_inner(damage: &[Rect]) {
    // Read mouse position BEFORE locking FRAMEBUFFER to avoid deadlock.
    let (mx, my) = {
        let mouse = super::input::MOUSE.lock();
        (mouse.x, mouse.y)
    };
    if let Some(ref mut fb) = *FRAMEBUFFER.lock() {
        super::FRAME_COUNTER.fetch_add(1, core::sync::atomic::Ordering::Relaxed);

        let screen_w = fb.width as u32;
        let screen_h = fb.height as u32;
        let screen_rect = Rect::new(0, 0, screen_w, screen_h);

        // ── 0. Restore old cursor pixels FIRST ──────────────────────
        // The cursor was drawn into the back buffer on the previous frame.
        // We MUST erase it before compositing, otherwise the old cursor
        // pixels get baked into the wallpaper cache reads and leave ghosts.
        // Hardware cursor lives on a GPU plane — skip the software save/restore.
        let hw_pointer = super::hw_cursor::is_enabled();
        let (old_cx, old_cy) = last_cursor_pos();
        let old_cursor_rect = if hw_pointer {
            Rect::new(0, 0, 0, 0)
        } else {
            Rect::new(
                old_cx - CURSOR_SAVE_PAD,
                old_cy - CURSOR_SAVE_PAD,
                CURSOR_SAVE_W as u32,
                CURSOR_SAVE_H as u32,
            )
        };
        if !hw_pointer {
            restore_cursor_background(fb);
        }

        // New cursor rect for the position we'll draw at this frame
        let new_cursor_rect = if hw_pointer {
            Rect::new(0, 0, 0, 0)
        } else {
            Rect::new(
                mx - CURSOR_SAVE_PAD,
                my - CURSOR_SAVE_PAD,
                CURSOR_SAVE_W as u32,
                CURSOR_SAVE_H as u32,
            )
        };

        // Compute the union bounding box of all damage rects, clamped to screen.
        // Software cursor: include old + new save areas. Hardware cursor plane
        // does not touch the composed FB.
        let base_damage = if damage.is_empty() {
            screen_rect
        } else {
            let mut u = damage[0];
            for r in &damage[1..] {
                u = super::rect_union(u, *r);
            }
            u
        };
        let damage_union = clamp_rect_to_screen(
            if hw_pointer {
                base_damage
            } else {
                super::rect_union(
                    super::rect_union(base_damage, old_cursor_rect),
                    new_cursor_rect,
                )
            },
            screen_w,
            screen_h,
        );

        // Check if this is a full-screen redraw (damage covers ≥90% of screen)
        let damage_area = damage_union.width as u64 * damage_union.height as u64;
        let screen_area = screen_w as u64 * screen_h as u64;
        let is_full = damage_area * 100 / screen_area.max(1) >= 90;

        // Update taskbar/dock hover state and auto-hide animation
        taskbar::update_hover(mx, my, screen_w, screen_h);
        taskbar::update_auto_hide(my, screen_h);
        if startmenu::is_visible() && !super::gpui_platform::launcher_is_live() {
            startmenu::update_hover(mx, my);
        }

        // 1. Restore wallpaper in damaged region only
        {
            let mut cache = WALLPAPER_CACHE.lock();
            if cache.is_none() {
                // First time: render full wallpaper and cache it.
                // The cursor has already been erased above, so this is clean.
                draw_wallpaper(fb);
                *cache = Some(fb.buffer.clone());
            } else if let Some(ref cached) = *cache {
                if is_full {
                    // Full screen — single memcpy (fastest)
                    let len = fb.buffer.len().min(cached.len());
                    // SAFETY: src and dst are valid for the copied byte count and do not overlap.
                    unsafe {
                        core::ptr::copy_nonoverlapping(
                            cached.as_ptr(),
                            fb.buffer.as_mut_ptr(),
                            len,
                        );
                    }
                } else {
                    // Partial — only copy rows within the damage union
                    let bpp = fb.bytes_per_pixel;
                    let pitch = fb.pitch;
                    let x0 = damage_union.x.max(0) as usize;
                    let y0 = damage_union.y.max(0) as usize;
                    let x1 = ((damage_union.x + damage_union.width as i32) as usize).min(fb.width);
                    let y1 =
                        ((damage_union.y + damage_union.height as i32) as usize).min(fb.height);
                    let row_bytes = (x1 - x0) * bpp;
                    for row in y0..y1 {
                        let off = row * pitch + x0 * bpp;
                        if off + row_bytes <= cached.len() && off + row_bytes <= fb.buffer.len() {
                            // SAFETY: src and dst are valid for the copied byte count and do not overlap.
                            unsafe {
                                core::ptr::copy_nonoverlapping(
                                    cached.as_ptr().add(off),
                                    fb.buffer.as_mut_ptr().add(off),
                                    row_bytes,
                                );
                            }
                        }
                    }
                }
            }
        }

        // 2-8: Compose all layers (icons, taskbar, windows, overlays)
        //
        // For partial redraws, push a clip so drawing outside the damage
        // region is skipped. Additionally, skip entire layers that don't
        // overlap the damage region at all — avoids function call overhead,
        // lock acquisitions, and loop iterations for unaffected layers.
        if !is_full {
            fb.push_clip(damage_union);
        }

        // D2.5: Knox GPUI pump — proof card is a window store blit (D1.7 scene).
        super::gpui_platform::pump(fb, screen_w, screen_h, damage_union, is_full);

        // Desktop widgets — rendered on wallpaper surface before icons/windows
        {
            let widget_area = super::desktop_widgets::bounding_rect(screen_w);
            if is_full || damage_union.intersects(&widget_area) {
                super::desktop_widgets::draw(fb);
            }
        }

        // Desktop icons — GPUI layer (D9.7) blits in pump. Compositor still
        // paints the in-flight drag preview (D9.9).
        {
            let icon_area = Rect::new(0, 0, 200, screen_h);
            if is_full || damage_union.intersects(&icon_area) {
                draw_desktop_icons(fb);
            }
        }

        // Rubber band — GPUI when live (D9.7).
        if !super::gpui_platform::desktop_is_live() {
            draw_rubber_band(fb);
        }

        // Taskbar — only if damage overlaps the taskbar strip at screen bottom
        {
            let taskbar_y = screen_h as i32 - super::scale::taskbar_height() as i32;
            let taskbar_area = Rect::new(0, taskbar_y, screen_w, super::scale::taskbar_height());
            if !super::gpui_platform::panel_is_live()
                && (is_full || damage_union.intersects(&taskbar_area))
            {
                taskbar::draw_taskbar(fb);
            }
        }

        // Windows — draw_all now does per-window clip intersection checks internally
        window::WINDOW_MANAGER.lock().draw_all(fb);

        // Snap preview — only while dragging
        draw_snap_preview(fb, mx, my);

        // Alt+Tab / overview — GPUI overlays (D9.6); old painters are fallback.
        if super::gpui_platform::switcher_is_live() {
            super::gpui_platform::present_switcher(fb, damage_union, is_full);
        } else {
            if super::alt_tab::is_visible() {
                super::alt_tab::draw(fb);
            }
            if super::expose::is_visible() {
                super::expose::draw(fb);
            }
        }

        // Overlays — only if damage overlaps their approximate regions or if visible
        if super::gpui_platform::launcher_is_live() {
            if startmenu::is_visible() {
                super::gpui_platform::present_launcher(fb, damage_union, is_full);
            }
        } else if is_full || startmenu::is_visible() {
            startmenu::draw_start_menu(fb);
        }
        if super::gpui_platform::desktop_is_live() {
            if super::gpui_platform::desktop_menu_open() {
                super::gpui_platform::present_desktop_menu(fb, damage_union, is_full);
            }
        } else {
            let cm = CONTEXT_MENU.lock();
            if cm.visible {
                let cm_rect = Rect::new(cm.x, cm.y, CONTEXT_MENU_WIDTH, 200);
                if is_full || damage_union.intersects(&cm_rect) {
                    drop(cm);
                    draw_context_menu(fb);
                } else {
                    drop(cm);
                }
            }
        }
        if super::gpui_platform::osd_is_live() {
            super::gpui_platform::present_osd(fb, damage_union, is_full);
        }
        if super::gpui_platform::tray_menu_open() {
            super::gpui_platform::present_tray_menu(fb, damage_union, is_full);
        }
        if is_full || super::taskbar::is_context_menu_open() {
            super::taskbar::draw_context_menu(fb);
        }
        if is_full || super::taskbar::is_preview_visible() {
            super::taskbar::draw_window_preview(fb);
        }
        if super::gpui_platform::notify_is_live() {
            super::gpui_platform::present_notify(fb, damage_union, is_full);
        }

        // File picker / screenshot / installer Dialog overlays (D11.10).
        if super::gpui_platform::dialogs_is_live() {
            if super::file_picker::is_visible() {
                super::gpui_platform::present_picker(fb, damage_union, is_full);
            }
            if super::screenshot::is_dialog_open() {
                super::gpui_platform::present_shot(fb, damage_union, is_full);
            }
            if super::installer::is_active() {
                super::gpui_platform::present_install(fb, damage_union, is_full);
            }
        } else if is_full || super::file_picker::is_visible() {
            super::file_picker::draw(fb);
        } else if super::installer::is_active() {
            super::installer::render(fb);
        }

        // D12.2 lock overlay (desktop remains underneath).
        if super::lock_screen::is_locked() {
            if super::gpui_platform::session_is_live() {
                super::gpui_platform::present_lock(fb, damage_union, is_full);
            } else {
                super::lock_screen::draw_lock_screen(fb);
            }
        }

        // Keyboard shortcuts overlay (D16.2 GPUI Dialog).
        if super::shortcuts_overlay::is_visible() {
            if super::gpui_platform::is_ready() {
                super::gpui_platform::present_shortcuts(fb, damage_union, is_full);
            } else if is_full || super::shortcuts_overlay::is_visible() {
                super::shortcuts_overlay::draw(fb);
            }
        }

        // Tick system sounds (turns off speaker after tone duration)
        super::sounds::tick();

        if !is_full {
            fb.pop_clip();
        }

        // 9. Cursor — hardware plane when Gate F8 is live; otherwise software blit.
        update_cursor_for_position(mx, my);
        if super::hw_cursor::is_enabled() {
            super::hw_cursor::set_position(mx, my);
        } else {
            save_cursor_background(fb, mx, my);
            draw_cursor(fb, mx, my);
        }
        set_last_cursor_pos(mx, my);

        // 9.5 Screen magnification (17.9) — Zoom lens around cursor
        if super::accessibility::is_zoomed() {
            apply_zoom_lens(fb, mx, my);
        }

        // 9.6 Night light — warm color temperature shift (blue light filter)
        if super::night_light::is_active() {
            super::night_light::apply(fb, damage_union);
        }

        // 10. Present the damaged region (which now includes old + new cursor area)
        // to HW framebuffer.
        fb.present_rect(
            damage_union.x,
            damage_union.y,
            damage_union.width,
            damage_union.height,
        );
        super::input::end_frame();
    }
}

/// Legacy full-screen redraw (calls damage-based path with full screen rect).
pub fn draw_desktop() {
    let damage = super::take_damage();
    if damage.is_empty() {
        // No damage rects queued — full screen
        let (sw, sh) = super::cached_screen_size();
        draw_desktop_damaged(&[Rect::new(0, 0, sw as u32, sh as u32)]);
    } else {
        draw_desktop_damaged(&damage);
    }
}

/// Clamp a rect to screen bounds.
fn clamp_rect_to_screen(r: Rect, sw: u32, sh: u32) -> Rect {
    let x0 = r.x.max(0).min(sw as i32);
    let y0 = r.y.max(0).min(sh as i32);
    let x1 = (r.x + r.width as i32).max(0).min(sw as i32);
    let y1 = (r.y + r.height as i32).max(0).min(sh as i32);
    Rect::new(x0, y0, (x1 - x0).max(0) as u32, (y1 - y0).max(0) as u32)
}

/// Timer tick handler - update clock and request redraw
pub fn on_timer_tick(ticks: u64) {
    // Capture old clock text for change detection
    let old_clock = taskbar::get_clock_text();
    taskbar::update_clock(ticks);
    let new_clock = taskbar::get_clock_text();

    // Tick notification auto-dismiss; GPUI toasts/center damage their overlay.
    let mut notifs = super::notifications::NOTIFICATIONS.lock();
    let before = notifs.has_visible_toasts() || notifs.panel_open;
    notifs.tick();
    let after = notifs.has_visible_toasts() || notifs.panel_open;
    drop(notifs);
    if before || after {
        super::gpui_platform::notify_damage();
    }

    // Clock lives in the desktop widget card — not the dock or the whole FB.
    if old_clock != new_clock {
        super::request_desktop_widget_redraw();
    }
}
