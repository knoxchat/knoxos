// SPDX-License-Identifier: MIT
//! GPUI window model: bounds, scale, sprite atlas stub, Scene → backing store.
//!
//! `PlatformWindow::draw(&Scene)` is this type’s [`Window::draw`]: a software
//! raster into a packed BGRA [`RasterBuffer`]. The kernel compositor blits that
//! store; moving the window does not re-raster.

use alloc::string::String;

use crate::Bgra;
use crate::atlas::SpriteAtlas;
use crate::geometry::{Bounds, Point, Size, px};
use crate::input::WindowInput;
use crate::raster::RasterBuffer;
use crate::scene::Scene;

/// Client-side vs server-side decorations (D10.1 / D10.2).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ChromeKind {
    /// In-process views draw TitleBar / WindowBorder (D8.16).
    #[default]
    Csd,
    /// Ring 3 SHM keeps kernel bars (`wm_chrome.rs`) until the client draws CSD.
    Ssd,
}

/// Stable id for a window in [`crate::App`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct WindowId(u64);

impl WindowId {
    pub const fn from_u64(id: u64) -> Self {
        Self(id)
    }

    pub const fn as_u64(self) -> u64 {
        self.0
    }

    pub const fn as_u32(self) -> u32 {
        self.0 as u32
    }
}

/// One GPUI window: layout bounds, UI scale, atlas, dirty flag, backing store.
pub struct Window {
    id: WindowId,
    bounds: Bounds,
    /// 16.16 fixed-point, matching `gui::scale` (`65536` = 1.0×).
    scale_fp: u32,
    atlas: SpriteAtlas,
    store: RasterBuffer,
    dirty: bool,
    input: WindowInput,
    title: String,
    min_size: Size,
    chrome: ChromeKind,
}

impl Window {
    pub fn new(id: WindowId, bounds: Bounds) -> Self {
        let w = bounds.width().max(0) as usize;
        let h = bounds.height().max(0) as usize;
        Self {
            id,
            bounds,
            scale_fp: 65536,
            atlas: SpriteAtlas::new(),
            store: RasterBuffer::new(w.max(1), h.max(1)),
            dirty: true,
            input: WindowInput::new(),
            title: String::new(),
            min_size: Size::wh(1, 1),
            chrome: ChromeKind::Csd,
        }
    }

    pub fn title(&self) -> &str {
        &self.title
    }

    pub fn set_title(&mut self, title: impl Into<String>) {
        self.title = title.into();
    }

    pub fn min_size(&self) -> Size {
        self.min_size
    }

    pub fn set_min_size(&mut self, size: Size) {
        self.min_size = Size::wh(size.width.0.max(1), size.height.0.max(1));
        let b = self.bounds;
        self.set_bounds(b);
    }

    pub fn chrome(&self) -> ChromeKind {
        self.chrome
    }

    pub fn set_chrome(&mut self, chrome: ChromeKind) {
        self.chrome = chrome;
    }

    pub fn id(&self) -> WindowId {
        self.id
    }

    pub fn bounds(&self) -> Bounds {
        self.bounds
    }

    pub fn scale_fp(&self) -> u32 {
        self.scale_fp
    }

    pub fn sprite_atlas(&self) -> &SpriteAtlas {
        &self.atlas
    }

    pub fn store(&self) -> &RasterBuffer {
        &self.store
    }

    pub fn store_mut(&mut self) -> &mut RasterBuffer {
        &mut self.store
    }

    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    pub fn request_layout(&mut self) {
        self.dirty = true;
    }

    pub fn input(&self) -> &WindowInput {
        &self.input
    }

    pub fn input_mut(&mut self) -> &mut WindowInput {
        &mut self.input
    }

    pub fn atlas_mut(&mut self) -> &mut SpriteAtlas {
        &mut self.atlas
    }

    pub fn paint_parts_mut(&mut self) -> (&mut WindowInput, &mut SpriteAtlas) {
        (&mut self.input, &mut self.atlas)
    }

    pub fn set_scale_fp(&mut self, scale_fp: u32) {
        if self.scale_fp != scale_fp {
            self.scale_fp = scale_fp;
            self.dirty = true;
        }
    }

    /// Move without resizing. The backing store is unchanged (D2.6).
    pub fn set_origin(&mut self, origin: Point) {
        self.bounds.origin = origin;
    }

    /// Resize reallocates the store and marks dirty. Origin-only changes
    /// should use [`set_origin`](Self::set_origin).
    ///
    /// D10.1: width/height are clamped to [`min_size`](Self::min_size) so a
    /// tile/snap cannot drop the raster store below the view's minimum.
    pub fn set_bounds(&mut self, mut bounds: Bounds) {
        let min_w = self.min_size.width.0.max(1);
        let min_h = self.min_size.height.0.max(1);
        if bounds.width() < min_w {
            bounds.size.width = px(min_w);
        }
        if bounds.height() < min_h {
            bounds.size.height = px(min_h);
        }
        let size_changed =
            bounds.width() != self.bounds.width() || bounds.height() != self.bounds.height();
        self.bounds = bounds;
        if size_changed {
            let w = bounds.width().max(0) as usize;
            let h = bounds.height().max(0) as usize;
            self.store = RasterBuffer::new(w.max(1), h.max(1));
            self.dirty = true;
        }
    }

    /// Raster `scene` into the backing store (D2.4). Store size follows bounds.
    pub fn draw(&mut self, scene: &Scene) {
        let w = self.bounds.width().max(0) as usize;
        let h = self.bounds.height().max(0) as usize;
        if self.store.width() != w.max(1) || self.store.height() != h.max(1) {
            self.store = RasterBuffer::new(w.max(1), h.max(1));
        }
        self.store.clear(Bgra::TRANSPARENT);
        self.store.draw_with_atlas(scene, Some(&self.atlas));
        self.dirty = false;
    }
}

#[cfg(test)]
mod tests {
    use super::{Window, WindowId};
    use crate::geometry::{Bounds, Hsla, Point, Size, px};
    use crate::scene::{Quad, Scene};

    /// D2.4 done-when: platform window raster fills its store.
    #[test]
    fn draw_fills_backing_store() {
        let mut win = Window::new(WindowId::from_u64(1), Bounds::from_xywh(0, 0, 8, 8));
        assert!(win.is_dirty());
        let mut scene = Scene::new();
        scene.push_quad(Quad::new(Bounds::from_xywh(0, 0, 8, 8), Hsla::RED));
        win.draw(&scene);
        assert!(!win.is_dirty());
        assert_eq!(win.store().pixel(0, 0), Some(Hsla::RED.to_bgra()));
        assert_eq!(win.store().pixel(7, 7), Some(Hsla::RED.to_bgra()));
        let _ = win.sprite_atlas();
        assert_eq!(win.scale_fp(), 65536);
    }

    /// D10.1 done-when (host): tiling below min-size keeps a raster store.
    #[test]
    fn tile_clamps_min_size_and_keeps_store() {
        let mut win = Window::new(WindowId::from_u64(2), Bounds::from_xywh(0, 0, 32, 24));
        win.set_title("Kit Gallery");
        win.set_chrome(super::ChromeKind::Csd);
        win.set_min_size(Size::wh(16, 12));
        let mut scene = Scene::new();
        scene.push_quad(Quad::new(Bounds::from_xywh(0, 0, 32, 24), Hsla::RED));
        win.draw(&scene);
        win.set_bounds(Bounds::from_xywh(8, 8, 4, 4));
        assert_eq!(win.bounds().width(), 16);
        assert_eq!(win.bounds().height(), 12);
        assert_eq!(win.title(), "Kit Gallery");
        assert_eq!(win.chrome(), super::ChromeKind::Csd);
        assert!(win.is_dirty());
        scene = Scene::new();
        scene.push_quad(Quad::new(Bounds::from_xywh(0, 0, 16, 12), Hsla::RED));
        win.draw(&scene);
        assert_eq!(win.store().pixel(0, 0), Some(Hsla::RED.to_bgra()));
        assert_eq!(win.store().width(), 16);
        assert_eq!(win.store().height(), 12);
    }

    /// D2.6 done-when (host): moving blits the same store without a repaint.
    #[test]
    fn move_does_not_repaint_store() {
        let mut win = Window::new(WindowId::from_u64(1), Bounds::from_xywh(0, 0, 8, 8));
        let mut scene = Scene::new();
        scene.push_quad(Quad::new(Bounds::from_xywh(0, 0, 8, 8), Hsla::BLUE));
        win.draw(&scene);
        let before = win.store().as_bytes().to_vec();
        win.set_origin(Point::xy(40, 60));
        assert!(!win.is_dirty());
        assert_eq!(win.store().as_bytes(), before.as_slice());
        assert_eq!(win.bounds().origin, Point::xy(40, 60));
        assert_eq!(win.bounds().size.width, px(8));
    }
}
