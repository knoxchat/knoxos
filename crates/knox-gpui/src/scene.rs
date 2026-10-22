// SPDX-License-Identifier: MIT
//! Scene primitives and paint-order recording for Knox GPUI.
//!
//! Type names follow Zed GPUI (`Quad`, `Shadow`, `Underline`, sprites, layers)
//! as a design reference (Apache-2.0). This file is new `no_std` code: one
//! insertion-order [`PaintOperation`] list for a software painter, not wgpu
//! instance buffers or per-kind batch vectors.

use alloc::vec::Vec;
use core::slice;

use crate::geometry::{Bounds, Corners, Edges, Hsla, Pixels, Point};

/// Clip region applied to a primitive. `None` means only the destination
/// buffer clips (no extra layer mask).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct ContentMask {
    pub bounds: Option<Bounds>,
}

impl ContentMask {
    /// No extra clip.
    pub const UNBOUNDED: Self = Self { bounds: None };

    pub const fn new(bounds: Bounds) -> Self {
        Self {
            bounds: Some(bounds),
        }
    }

    pub const fn is_unbounded(self) -> bool {
        self.bounds.is_none()
    }

    pub fn intersect(self, other: Self) -> Self {
        match (self.bounds, other.bounds) {
            (None, b) | (b, None) => Self { bounds: b },
            (Some(a), Some(b)) => Self {
                bounds: Some(a.intersect(b)),
            },
        }
    }

    /// Primitive bounds after this mask. Unbounded leaves `primitive` as-is.
    pub fn clip_bounds(self, primitive: Bounds) -> Bounds {
        match self.bounds {
            Some(mask) => mask.intersect(primitive),
            None => primitive,
        }
    }
}

impl From<Bounds> for ContentMask {
    fn from(bounds: Bounds) -> Self {
        Self::new(bounds)
    }
}

/// Source tile in a sprite atlas (the atlas itself is platform-owned, D2.4 / D5).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct AtlasTile {
    pub texture_id: u32,
    pub bounds: Bounds,
}

impl AtlasTile {
    pub const fn new(texture_id: u32, bounds: Bounds) -> Self {
        Self { texture_id, bounds }
    }
}

/// Quad border style (GPUI role; dashed is recorded now, rasterized in D1.4+).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum BorderStyle {
    #[default]
    Solid,
    Dashed,
}

/// Filled / stroked rounded rectangle.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Quad {
    pub bounds: Bounds,
    pub content_mask: ContentMask,
    pub background: Hsla,
    pub border_color: Hsla,
    pub border_widths: Edges,
    pub corner_radii: Corners,
    pub border_style: BorderStyle,
}

impl Quad {
    pub fn new(bounds: Bounds, background: Hsla) -> Self {
        Self {
            bounds,
            content_mask: ContentMask::UNBOUNDED,
            background,
            border_color: Hsla::TRANSPARENT,
            border_widths: Edges::ZERO,
            corner_radii: Corners::ZERO,
            border_style: BorderStyle::Solid,
        }
    }

    /// Uniform corner radius. Zero stays a sharp axis-aligned fill (D1.3).
    pub fn rounded(mut self, radius: Pixels) -> Self {
        self.corner_radii = Corners::all(radius);
        self
    }
}

/// Box shadow. Offset / blur / spread stay explicit for the CPU raster (D1.5);
/// GPUI’s wgpu instance bakes those into expanded `bounds`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Shadow {
    pub bounds: Bounds,
    pub content_mask: ContentMask,
    pub corner_radii: Corners,
    pub color: Hsla,
    pub offset: Point,
    pub blur_radius: Pixels,
    pub spread_radius: Pixels,
    pub inset: bool,
}

impl Shadow {
    pub fn new(bounds: Bounds, color: Hsla) -> Self {
        Self {
            bounds,
            content_mask: ContentMask::UNBOUNDED,
            corner_radii: Corners::ZERO,
            color,
            offset: Point::ZERO,
            blur_radius: Pixels::ZERO,
            spread_radius: Pixels::ZERO,
            inset: false,
        }
    }

    /// Uniform corner radius.
    pub fn rounded(mut self, radius: Pixels) -> Self {
        self.corner_radii = Corners::all(radius);
        self
    }

    pub fn blur(mut self, radius: Pixels) -> Self {
        self.blur_radius = radius;
        self
    }

    pub fn offset(mut self, offset: Point) -> Self {
        self.offset = offset;
        self
    }

    pub fn spread(mut self, radius: Pixels) -> Self {
        self.spread_radius = radius;
        self
    }

    pub fn inset(mut self) -> Self {
        self.inset = true;
        self
    }

    /// Unblurred geometry after offset and spread.
    pub fn shape_bounds(self) -> Bounds {
        if self.inset {
            self.bounds.inset(self.spread_radius).translate(self.offset)
        } else {
            self.bounds
                .dilate(self.spread_radius)
                .translate(self.offset)
        }
    }

    /// Corner radii of [`shape_bounds`](Self::shape_bounds) after spread.
    pub fn shape_radii(self) -> Corners {
        let add = if self.inset {
            -self.spread_radius.get()
        } else {
            self.spread_radius.get()
        };
        let adj = |c: Pixels| Pixels::new((c.get() + add).max(0));
        Corners {
            top_left: adj(self.corner_radii.top_left),
            top_right: adj(self.corner_radii.top_right),
            bottom_right: adj(self.corner_radii.bottom_right),
            bottom_left: adj(self.corner_radii.bottom_left),
        }
        .clamp_to(self.shape_bounds().size)
    }

    /// AABB of pixels this shadow can cover, including the blur halo.
    pub fn paint_bounds(self) -> Bounds {
        if self.inset {
            return self.bounds;
        }
        let halo = Pixels::new(self.blur_radius.get().max(1));
        self.shape_bounds().dilate(halo)
    }
}

/// Text underline (optionally wavy).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Underline {
    pub bounds: Bounds,
    pub content_mask: ContentMask,
    pub color: Hsla,
    pub thickness: Pixels,
    pub wavy: bool,
}

impl Underline {
    pub fn new(bounds: Bounds, color: Hsla, thickness: Pixels) -> Self {
        Self {
            bounds,
            content_mask: ContentMask::UNBOUNDED,
            color,
            thickness,
            wavy: false,
        }
    }
}

/// Tinted atlas tile (glyphs).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MonochromeSprite {
    pub bounds: Bounds,
    pub content_mask: ContentMask,
    pub color: Hsla,
    pub tile: AtlasTile,
}

impl MonochromeSprite {
    pub fn new(bounds: Bounds, tile: AtlasTile, color: Hsla) -> Self {
        Self {
            bounds,
            content_mask: ContentMask::UNBOUNDED,
            color,
            tile,
        }
    }
}

/// Color atlas tile (icons / images).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PolychromeSprite {
    pub bounds: Bounds,
    pub content_mask: ContentMask,
    pub corner_radii: Corners,
    pub tile: AtlasTile,
    pub grayscale: bool,
    pub opacity: f32,
}

impl PolychromeSprite {
    pub fn new(bounds: Bounds, tile: AtlasTile) -> Self {
        Self {
            bounds,
            content_mask: ContentMask::UNBOUNDED,
            corner_radii: Corners::ZERO,
            tile,
            grayscale: false,
            opacity: 1.0,
        }
    }
}

/// One recorded command, in the order a software painter must walk it.
#[derive(Clone, Debug, PartialEq)]
pub enum PaintOperation {
    Shadow(Shadow),
    Quad(Quad),
    Underline(Underline),
    MonochromeSprite(MonochromeSprite),
    PolychromeSprite(PolychromeSprite),
    StartLayer { clip: Bounds },
    EndLayer,
}

impl PaintOperation {
    pub fn as_quad(&self) -> Option<&Quad> {
        match self {
            Self::Quad(quad) => Some(quad),
            _ => None,
        }
    }

    pub fn as_shadow(&self) -> Option<&Shadow> {
        match self {
            Self::Shadow(shadow) => Some(shadow),
            _ => None,
        }
    }

    pub fn bounds(&self) -> Option<Bounds> {
        match self {
            Self::Shadow(p) => Some(p.bounds),
            Self::Quad(p) => Some(p.bounds),
            Self::Underline(p) => Some(p.bounds),
            Self::MonochromeSprite(p) => Some(p.bounds),
            Self::PolychromeSprite(p) => Some(p.bounds),
            Self::StartLayer { clip } => Some(*clip),
            Self::EndLayer => None,
        }
    }
}

/// Display list. Push primitives; iterate [`paint_order`](Self::paint_order)
/// for the software raster (D1.3+).
#[derive(Clone, Debug, Default)]
pub struct Scene {
    ops: Vec<PaintOperation>,
    clip_stack: Vec<Bounds>,
}

impl Scene {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn clear(&mut self) {
        self.ops.clear();
        self.clip_stack.clear();
    }

    pub fn is_empty(&self) -> bool {
        self.ops.is_empty()
    }

    pub fn len(&self) -> usize {
        self.ops.len()
    }

    pub fn layer_depth(&self) -> usize {
        self.clip_stack.len()
    }

    /// Insertion order. Not GPUI’s wgpu `batches()` (which groups by primitive
    /// kind and would pull every shadow in front of every quad).
    pub fn paint_order(&self) -> slice::Iter<'_, PaintOperation> {
        self.ops.iter()
    }

    /// Union of primitive bounds (D13.3). Empty scene returns `None`.
    pub fn dirty_bounds(&self) -> Option<Bounds> {
        let mut acc: Option<Bounds> = None;
        for op in &self.ops {
            let Some(b) = op.bounds() else {
                continue;
            };
            acc = Some(match acc {
                Some(a) => a.union(b),
                None => b,
            });
        }
        acc
    }

    pub fn as_slice(&self) -> &[PaintOperation] {
        &self.ops
    }

    /// Begin a clip layer. Nested layers intersect with the parent clip.
    pub fn start_layer(&mut self, clip: Bounds) {
        let clip = match self.clip_stack.last() {
            Some(&parent) => parent.intersect(clip),
            None => clip,
        };
        self.clip_stack.push(clip);
        self.ops.push(PaintOperation::StartLayer { clip });
    }

    /// End the current clip layer. Unmatched pops are ignored.
    pub fn end_layer(&mut self) {
        if self.clip_stack.pop().is_some() {
            self.ops.push(PaintOperation::EndLayer);
        }
    }

    pub fn push_quad(&mut self, mut quad: Quad) {
        quad.content_mask = self.stamp_mask(quad.content_mask);
        self.ops.push(PaintOperation::Quad(quad));
    }

    pub fn push_shadow(&mut self, mut shadow: Shadow) {
        shadow.content_mask = self.stamp_mask(shadow.content_mask);
        self.ops.push(PaintOperation::Shadow(shadow));
    }

    pub fn push_underline(&mut self, mut underline: Underline) {
        underline.content_mask = self.stamp_mask(underline.content_mask);
        self.ops.push(PaintOperation::Underline(underline));
    }

    pub fn push_mono_sprite(&mut self, mut sprite: MonochromeSprite) {
        sprite.content_mask = self.stamp_mask(sprite.content_mask);
        self.ops.push(PaintOperation::MonochromeSprite(sprite));
    }

    pub fn push_polychrome_sprite(&mut self, mut sprite: PolychromeSprite) {
        sprite.content_mask = self.stamp_mask(sprite.content_mask);
        self.ops.push(PaintOperation::PolychromeSprite(sprite));
    }

    fn stamp_mask(&self, mask: ContentMask) -> ContentMask {
        match self.clip_stack.last() {
            Some(&clip) => mask.intersect(ContentMask::new(clip)),
            None => mask,
        }
    }
}

impl<'a> IntoIterator for &'a Scene {
    type Item = &'a PaintOperation;
    type IntoIter = slice::Iter<'a, PaintOperation>;

    fn into_iter(self) -> Self::IntoIter {
        self.paint_order()
    }
}

#[cfg(test)]
mod tests {
    use super::{
        AtlasTile, ContentMask, MonochromeSprite, PaintOperation, PolychromeSprite, Quad, Scene,
        Shadow, Underline,
    };
    use crate::geometry::{Bounds, Hsla, Point, px};

    fn tile() -> AtlasTile {
        AtlasTile::new(1, Bounds::from_xywh(0, 0, 8, 8))
    }

    fn red_quad(x: i32, y: i32) -> Quad {
        Quad::new(Bounds::from_xywh(x, y, 10, 10), Hsla::RED)
    }

    fn blue_quad(x: i32, y: i32) -> Quad {
        Quad::new(Bounds::from_xywh(x, y, 6, 6), Hsla::BLUE)
    }

    /// D1.2 done-when: two recorded quads walk in insertion / paint order.
    #[test]
    fn two_quads_iterate_in_paint_order() {
        let mut scene = Scene::new();
        scene.push_quad(red_quad(0, 0));
        scene.push_quad(blue_quad(2, 2));

        let ops: alloc::vec::Vec<_> = scene.paint_order().collect();
        assert_eq!(ops.len(), 2);

        let back = ops[0].as_quad().expect("first op is a quad");
        let front = ops[1].as_quad().expect("second op is a quad");
        assert_eq!(back.background, Hsla::RED);
        assert_eq!(back.bounds, Bounds::from_xywh(0, 0, 10, 10));
        assert_eq!(front.background, Hsla::BLUE);
        assert_eq!(front.bounds, Bounds::from_xywh(2, 2, 6, 6));

        // Reverse of insertion must not satisfy the golden.
        assert_ne!(back.background, front.background);
        assert_ne!(ops[0], ops[1]);
    }

    #[test]
    fn paint_order_is_insertion_not_kind_batch() {
        let mut scene = Scene::new();
        let bounds = Bounds::from_xywh(0, 0, 8, 8);
        scene.push_quad(Quad::new(bounds, Hsla::RED));
        scene.push_shadow(Shadow::new(bounds, Hsla::BLACK));
        scene.push_underline(Underline::new(bounds, Hsla::WHITE, px(1)));
        scene.push_mono_sprite(MonochromeSprite::new(bounds, tile(), Hsla::GREEN));
        scene.push_polychrome_sprite(PolychromeSprite::new(bounds, tile()));
        scene.push_quad(Quad::new(bounds, Hsla::BLUE));

        let expected = [
            PaintOperation::Quad(Quad::new(bounds, Hsla::RED)),
            PaintOperation::Shadow(Shadow::new(bounds, Hsla::BLACK)),
            PaintOperation::Underline(Underline::new(bounds, Hsla::WHITE, px(1))),
            PaintOperation::MonochromeSprite(MonochromeSprite::new(bounds, tile(), Hsla::GREEN)),
            PaintOperation::PolychromeSprite(PolychromeSprite::new(bounds, tile())),
            PaintOperation::Quad(Quad::new(bounds, Hsla::BLUE)),
        ];
        assert_eq!(scene.as_slice(), expected.as_slice());
        // A wgpu-style batch walk would emit Shadow before the first Quad.
    }

    #[test]
    fn dirty_bounds_is_not_fullscreen() {
        let mut scene = Scene::new();
        scene.push_quad(red_quad(10, 12));
        let dirty = scene.dirty_bounds().expect("quad");
        assert!(dirty.width() < 1920);
        assert!(dirty.height() < 1080);
        assert_eq!(dirty, Bounds::from_xywh(10, 12, 10, 10));
    }

    #[test]
    fn layers_wrap_children_in_paint_order() {
        let mut scene = Scene::new();
        let clip = Bounds::from_xywh(4, 4, 16, 16);
        scene.push_quad(red_quad(0, 0));
        scene.start_layer(clip);
        scene.push_quad(blue_quad(8, 8));
        scene.push_shadow(Shadow::new(Bounds::from_xywh(8, 8, 6, 6), Hsla::BLACK));
        scene.end_layer();
        scene.push_quad(Quad::new(Bounds::from_xywh(20, 20, 4, 4), Hsla::GREEN));

        let ops = scene.as_slice();
        assert_eq!(ops.len(), 6);
        assert!(matches!(ops[0], PaintOperation::Quad(q) if q.background == Hsla::RED));
        assert_eq!(ops[1], PaintOperation::StartLayer { clip });
        assert!(matches!(ops[2], PaintOperation::Quad(q) if q.background == Hsla::BLUE));
        assert!(matches!(ops[3], PaintOperation::Shadow(_)));
        assert_eq!(ops[4], PaintOperation::EndLayer);
        assert!(matches!(ops[5], PaintOperation::Quad(q) if q.background == Hsla::GREEN));
        assert_eq!(scene.layer_depth(), 0);
    }

    #[test]
    fn nested_layers_intersect_clip() {
        let mut scene = Scene::new();
        scene.start_layer(Bounds::from_xywh(0, 0, 20, 20));
        scene.start_layer(Bounds::from_xywh(10, 10, 20, 20));
        scene.push_quad(red_quad(0, 0));
        scene.end_layer();
        scene.end_layer();

        assert!(matches!(
            scene.as_slice()[0],
            PaintOperation::StartLayer { clip } if clip == Bounds::from_xywh(0, 0, 20, 20)
        ));
        assert!(matches!(
            scene.as_slice()[1],
            PaintOperation::StartLayer { clip } if clip == Bounds::from_xywh(10, 10, 10, 10)
        ));
        let quad = scene.as_slice()[2].as_quad().unwrap();
        assert_eq!(
            quad.content_mask,
            ContentMask::new(Bounds::from_xywh(10, 10, 10, 10))
        );
        assert_eq!(scene.as_slice()[3], PaintOperation::EndLayer);
        assert_eq!(scene.as_slice()[4], PaintOperation::EndLayer);
    }

    #[test]
    fn layer_stamps_content_mask_on_primitives() {
        let mut scene = Scene::new();
        let clip = Bounds::from_xywh(2, 2, 8, 8);
        scene.start_layer(clip);
        scene.push_quad(red_quad(0, 0));
        scene.push_underline(Underline::new(
            Bounds::from_xywh(0, 8, 10, 2),
            Hsla::WHITE,
            px(1),
        ));
        scene.end_layer();

        let mask = ContentMask::new(clip);
        assert_eq!(scene.as_slice()[1].as_quad().unwrap().content_mask, mask);
        match &scene.as_slice()[2] {
            PaintOperation::Underline(u) => assert_eq!(u.content_mask, mask),
            other => panic!("expected underline, got {other:?}"),
        }
    }

    #[test]
    fn unmatched_end_layer_is_ignored() {
        let mut scene = Scene::new();
        scene.end_layer();
        assert!(scene.is_empty());
        scene.start_layer(Bounds::from_xywh(0, 0, 1, 1));
        scene.end_layer();
        scene.end_layer();
        assert_eq!(scene.len(), 2);
        assert_eq!(scene.layer_depth(), 0);
    }

    #[test]
    fn clear_drops_ops_and_open_layers() {
        let mut scene = Scene::new();
        scene.start_layer(Bounds::from_xywh(0, 0, 4, 4));
        scene.push_quad(red_quad(0, 0));
        scene.push_shadow(Shadow {
            offset: Point::xy(1, 2),
            blur_radius: px(3),
            spread_radius: px(1),
            ..Shadow::new(Bounds::from_xywh(0, 0, 10, 10), Hsla::BLACK)
        });
        assert!(!scene.is_empty());
        assert_eq!(scene.layer_depth(), 1);
        scene.clear();
        assert!(scene.is_empty());
        assert_eq!(scene.len(), 0);
        assert_eq!(scene.layer_depth(), 0);
        assert_eq!(scene.paint_order().count(), 0);
    }

    #[test]
    fn into_iter_matches_paint_order() {
        let mut scene = Scene::new();
        scene.push_quad(red_quad(0, 0));
        scene.push_quad(blue_quad(1, 1));
        let via_iter: alloc::vec::Vec<_> = (&scene).into_iter().collect();
        let via_order: alloc::vec::Vec<_> = scene.paint_order().collect();
        assert_eq!(via_iter, via_order);
    }
}
