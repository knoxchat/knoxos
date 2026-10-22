// SPDX-License-Identifier: MIT
//! KnoxOS `PlatformWindow`: scale from `gui::scale`, `draw(&Scene)` → store.

use knox_gpui::{Scene, Window};

/// Apply KnoxOS UI scale and raster `scene` into the window backing store (D2.4).
pub fn draw_scene(window: &mut Window, scene: &Scene) {
    window.set_scale_fp(crate::gui::scale::scale_fp());
    window.draw(scene);
}
