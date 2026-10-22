// SPDX-License-Identifier: MIT
//! Raster a Scene into a caller-owned BGRA buffer (Ring 3 SHM / host `Vec`).
//!
//! D15.1: this path never touches a kernel framebuffer. The same packed
//! layout is what `/run/wayland-0` clients present at window size (D15.2).

use crate::raster::RasterBuffer;
use crate::scene::Scene;

/// Copy `scene` into `dest` as packed BGRA (`width * height * 4` bytes).
///
/// Returns `false` if `dest` is too small or the size is zero.
pub fn raster_scene_to_shm(scene: &Scene, width: u32, height: u32, dest: &mut [u8]) -> bool {
    let w = width as usize;
    let h = height as usize;
    if w == 0 || h == 0 {
        return false;
    }
    let need = w.saturating_mul(h).saturating_mul(4);
    if dest.len() < need {
        return false;
    }
    let mut buf = RasterBuffer::new(w, h);
    buf.draw(scene);
    dest[..need].copy_from_slice(buf.as_bytes());
    true
}

#[cfg(test)]
mod tests {
    use super::raster_scene_to_shm;
    use crate::geometry::{Bounds, Hsla};
    use crate::scene::{Quad, Scene};

    /// D15.1 done-when: Scene rasters into a fake SHM Vec, not a framebuffer.
    #[test]
    fn raster_into_fake_shm_is_window_sized() {
        const W: u32 = 280;
        const H: u32 = 200;
        let mut scene = Scene::new();
        scene.push_quad(Quad::new(
            Bounds::from_xywh(0, 0, W as i32, H as i32),
            Hsla::RED,
        ));
        let mut shm = alloc::vec![0u8; (W * H * 4) as usize];
        assert!(raster_scene_to_shm(&scene, W, H, &mut shm));
        assert_ne!(W, 64, "must not be the Gate F 64×64 KAT size");
        let red = Hsla::RED.to_bgra().to_bytes();
        assert_eq!(&shm[0..4], &red);
        let last = ((W * H - 1) * 4) as usize;
        assert_eq!(&shm[last..last + 4], &red);
        assert!(!raster_scene_to_shm(&scene, W, H, &mut shm[..4]));
    }
}
