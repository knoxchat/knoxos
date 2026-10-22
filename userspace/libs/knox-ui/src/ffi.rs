// SPDX-License-Identifier: MIT
//! C ABI for the Knox GPUI rasterizer (D15.2).
//!
//! Ring 3 has no Rust GUI runtime yet. Callers pass a packed BGRA SHM
//! pointer; this crate stays `no_std`. Kernel Wayland present uses the
//! same helpers at window size (not 64×64).

use knox_gpui::{
    HostTextSystem, IntoElement, PaintEnv, Point, RasterBuffer, Scene, Size, SpriteAtlas,
    WindowInput, paint_element_with_input, px,
};

use crate::button::{Button, ButtonVariants};
use crate::calculator::Calculator;
use crate::theme::Theme;

/// Packed BGRA bytes needed for `width × height`.
pub fn shm_len(width: u32, height: u32) -> usize {
    (width as usize)
        .saturating_mul(height as usize)
        .saturating_mul(4)
}

fn paint_to_shm(mut el: knox_gpui::Div, width: u32, height: u32, dest: &mut [u8]) -> bool {
    let need = shm_len(width, height);
    if width == 0 || height == 0 || dest.len() < need {
        return false;
    }
    let mut atlas = SpriteAtlas::new();
    let sys = HostTextSystem;
    let mut env = PaintEnv {
        atlas: &mut atlas,
        text: Some(&sys),
    };
    let mut scene = Scene::new();
    let mut input = WindowInput::new();
    let size = Size::wh(width as i32, height as i32);
    paint_element_with_input(&mut el, Point::ZERO, size, &mut scene, &mut input, &mut env);
    let mut buf = RasterBuffer::new(width as usize, height as usize);
    buf.draw_with_atlas(&scene, Some(&atlas));
    dest[..need].copy_from_slice(buf.as_bytes());
    true
}

/// Raster a primary Button into packed BGRA (window-sized SHM).
pub fn raster_button_to_shm(width: u32, height: u32, dest: &mut [u8]) -> bool {
    let theme = Theme::aurora_dark();
    let el = Button::new("ring3-btn")
        .themed(&theme)
        .primary()
        .label("Knox GPUI")
        .min_w(px((width as i32 - 32).max(48)))
        .into_element();
    paint_to_shm(el, width, height, dest)
}

/// Raster a Calculator keypad into packed BGRA (window-sized SHM).
pub fn raster_calculator_to_shm(width: u32, height: u32, dest: &mut [u8]) -> bool {
    raster_calculator_state_to_shm(width, height, "0", "", dest)
}

/// Raster a Calculator with a live display/history string.
pub fn raster_calculator_state_to_shm(
    width: u32,
    height: u32,
    display: &str,
    history: &str,
    dest: &mut [u8],
) -> bool {
    let theme = Theme::aurora_dark();
    let el = Calculator::new()
        .themed(&theme)
        .size(px(width as i32), px(height as i32))
        .display(display)
        .history(history)
        .into_element();
    paint_to_shm(el, width, height, dest)
}

/// D15.2 C ABI: raster a Button into `dest`.
///
/// Returns `0` on success, `-1` on a null pointer or undersized buffer.
///
/// # Safety
/// `dest` must be writable for `dest_len` bytes (or null, which fails).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn knox_ui_raster_button(
    dest: *mut u8,
    dest_len: usize,
    width: u32,
    height: u32,
) -> i32 {
    if dest.is_null() {
        return -1;
    }
    // SAFETY: caller guarantees `dest` is writable for `dest_len` bytes.
    let slice = unsafe { core::slice::from_raw_parts_mut(dest, dest_len) };
    if raster_button_to_shm(width, height, slice) {
        0
    } else {
        -1
    }
}

/// D15.2 C ABI: raster a Calculator into `dest`.
///
/// # Safety
/// `dest` must be writable for `dest_len` bytes (or null, which fails).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn knox_ui_raster_calculator(
    dest: *mut u8,
    dest_len: usize,
    width: u32,
    height: u32,
) -> i32 {
    if dest.is_null() {
        return -1;
    }
    // SAFETY: caller guarantees `dest` is writable for `dest_len` bytes.
    let slice = unsafe { core::slice::from_raw_parts_mut(dest, dest_len) };
    if raster_calculator_to_shm(width, height, slice) {
        0
    } else {
        -1
    }
}

#[cfg(test)]
mod tests {
    use super::{
        knox_ui_raster_button, knox_ui_raster_calculator, raster_button_to_shm,
        raster_calculator_state_to_shm, shm_len,
    };

    #[test]
    fn button_shm_is_window_sized_not_64() {
        const W: u32 = 280;
        const H: u32 = 200;
        let mut shm = alloc::vec![0u8; shm_len(W, H)];
        assert!(raster_button_to_shm(W, H, &mut shm));
        assert_ne!(W, 64);
        assert!(shm.iter().any(|b| *b != 0));
    }

    #[test]
    fn c_abi_button_and_calc() {
        const W: u32 = 320;
        const H: u32 = 240;
        let mut shm = alloc::vec![0u8; shm_len(W, H)];
        let rc = unsafe { knox_ui_raster_button(shm.as_mut_ptr(), shm.len(), W, H) };
        assert_eq!(rc, 0);
        let rc = unsafe { knox_ui_raster_calculator(shm.as_mut_ptr(), shm.len(), W, H) };
        assert_eq!(rc, 0);
        let rc = unsafe { knox_ui_raster_button(core::ptr::null_mut(), 0, W, H) };
        assert_eq!(rc, -1);
    }

    #[test]
    fn calc_display_change_alters_shm() {
        const W: u32 = 320;
        const H: u32 = 440;
        let mut a = alloc::vec![0u8; shm_len(W, H)];
        let mut b = alloc::vec![0u8; shm_len(W, H)];
        assert!(raster_calculator_state_to_shm(W, H, "0", "", &mut a));
        assert!(raster_calculator_state_to_shm(W, H, "7", "", &mut b));
        assert_ne!(a, b);
    }
}
