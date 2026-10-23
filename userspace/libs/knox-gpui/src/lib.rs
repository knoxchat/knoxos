// SPDX-License-Identifier: MIT
//! Knox GPUI — `no_std` + `alloc` UI core for KnoxOS.
//!
//! Zed GPUI and the Longbridge kit are design references (Apache-2.0).
//! Those trees are not shipped (`desktopDE-impl.md` D14.7). This crate must
//! not depend on them or on `std`.
#![no_std]

extern crate alloc;

mod aa;
pub mod app;
pub mod atlas;
pub mod div;
pub mod element;
pub mod entity;
pub mod executor;
pub mod field;
pub mod geometry;
pub mod input;
pub mod keymap;
pub mod layout;
pub mod raster;
pub mod scene;
pub mod shm;
pub mod style;
pub mod text;
pub mod window;

pub use app::App;
pub use atlas::SpriteAtlas;
pub use div::{Div, div};
pub use element::{
    AnyElement, Element, IntoElement, PaintEnv, dispatch_ime_commit, dispatch_key_down,
    dispatch_mouse_down, dispatch_mouse_down_button, dispatch_mouse_move, dispatch_mouse_up,
    dispatch_scroll, paint_element, paint_element_with_input, prepare_element,
};
pub use entity::{Entity, EntityId, EntityMap};
pub use executor::PlatformDispatcher;
pub use field::{TextInput, text_input};
pub use geometry::{
    Bgra, Bounds, Corners, Edges, Hsla, Pixels, Point, Size, bounds, hsla, point, px, size,
};
pub use input::{
    ClickEvent, CursorStyle, FocusHandle, Hitbox, HitboxId, Key, KeyDownEvent, Keystroke,
    Modifiers, MouseButton, MouseEvent, ScrollEvent, WindowInput,
};
pub use keymap::{ActionName, Binding, Keymap};
pub use layout::{LayoutHints, flex_layout};
pub use raster::{RasterBuffer, opaque_fill_uses_simd};
pub use scene::{
    AtlasTile, BorderStyle, ContentMask, MonochromeSprite, PaintOperation, PolychromeSprite, Quad,
    Scene, Shadow, Underline,
};
pub use shm::raster_scene_to_shm;
pub use style::{
    Align, Display, ElementState, FlexDirection, FluentBuilder, Justify, Overflow, Position, Style,
    StyleRefinement, Styled, ring_color,
};
pub use text::{
    FontId, GlyphId, GlyphRaster, HostTextSystem, PlatformTextSystem, Text, glyph_layout_advance,
    text,
};
pub use window::{ChromeKind, Window, WindowId};

/// Crate version; referenced by the kernel so the path dep stays live (D0.1).
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Alloc probe used by host tests and as a kernel link tether.
pub fn alloc_probe() -> alloc::vec::Vec<u8> {
    alloc::vec::Vec::new()
}

#[cfg(test)]
mod tests {
    use super::alloc_probe;
    use alloc::vec;

    #[test]
    fn alloc_vec_len() {
        let v = vec![1u8, 2, 3];
        assert_eq!(v.len(), 3);
        assert_eq!(alloc_probe().len(), 0);
    }

    #[test]
    fn version_is_set() {
        assert!(!super::VERSION.is_empty());
    }
}
