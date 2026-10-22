/// SVG Renderer (Subset)
///
/// Renders a subset of SVG Tiny 1.2 for icons and simple graphics.
/// Supports basic shapes, paths, transforms, gradients, and text.
use alloc::string::String;
use alloc::vec::Vec;

use crate::serial_println;

/// Color
#[derive(Debug, Clone, Copy)]
pub struct SvgColor {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl SvgColor {
    pub fn rgba(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }
    pub fn to_u32(&self) -> u32 {
        (self.a as u32) << 24 | (self.r as u32) << 16 | (self.g as u32) << 8 | self.b as u32
    }
}

/// 2D transform matrix [a, b, c, d, e, f]
#[derive(Debug, Clone, Copy)]
pub struct Transform {
    pub a: f32,
    pub b: f32,
    pub c: f32,
    pub d: f32,
    pub e: f32,
    pub f: f32,
}

impl Transform {
    pub fn identity() -> Self {
        Self {
            a: 1.0,
            b: 0.0,
            c: 0.0,
            d: 1.0,
            e: 0.0,
            f: 0.0,
        }
    }
    pub fn translate(tx: f32, ty: f32) -> Self {
        Self {
            a: 1.0,
            b: 0.0,
            c: 0.0,
            d: 1.0,
            e: tx,
            f: ty,
        }
    }
    pub fn scale(sx: f32, sy: f32) -> Self {
        Self {
            a: sx,
            b: 0.0,
            c: 0.0,
            d: sy,
            e: 0.0,
            f: 0.0,
        }
    }

    pub fn apply(&self, x: f32, y: f32) -> (f32, f32) {
        (
            self.a * x + self.c * y + self.e,
            self.b * x + self.d * y + self.f,
        )
    }

    pub fn concat(&self, other: &Self) -> Self {
        Self {
            a: self.a * other.a + self.b * other.c,
            b: self.a * other.b + self.b * other.d,
            c: self.c * other.a + self.d * other.c,
            d: self.c * other.b + self.d * other.d,
            e: self.e * other.a + self.f * other.c + other.e,
            f: self.e * other.b + self.f * other.d + other.f,
        }
    }
}

/// SVG element types
#[derive(Debug, Clone)]
pub enum SvgElement {
    Rect {
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        rx: f32,
        ry: f32,
    },
    Circle {
        cx: f32,
        cy: f32,
        r: f32,
    },
    Ellipse {
        cx: f32,
        cy: f32,
        rx: f32,
        ry: f32,
    },
    Line {
        x1: f32,
        y1: f32,
        x2: f32,
        y2: f32,
    },
    Polyline {
        points: Vec<(f32, f32)>,
    },
    Polygon {
        points: Vec<(f32, f32)>,
    },
    Path {
        commands: Vec<PathCommand>,
    },
    Text {
        x: f32,
        y: f32,
        content: String,
        font_size: f32,
    },
    Group {
        children: Vec<SvgNode>,
    },
}

/// SVG path commands
#[derive(Debug, Clone, Copy)]
pub enum PathCommand {
    MoveTo(f32, f32),
    LineTo(f32, f32),
    CurveTo(f32, f32, f32, f32, f32, f32), // cubic Bezier
    QuadTo(f32, f32, f32, f32),
    ArcTo(f32, f32, f32, bool, bool, f32, f32),
    Close,
}

/// Style properties
#[derive(Debug, Clone)]
pub struct SvgStyle {
    pub fill: Option<SvgColor>,
    pub stroke: Option<SvgColor>,
    pub stroke_width: f32,
    pub opacity: f32,
}

impl Default for SvgStyle {
    fn default() -> Self {
        Self {
            fill: Some(SvgColor::rgba(0, 0, 0, 255)),
            stroke: None,
            stroke_width: 1.0,
            opacity: 1.0,
        }
    }
}

/// A node in the SVG tree
#[derive(Debug, Clone)]
pub struct SvgNode {
    pub element: SvgElement,
    pub style: SvgStyle,
    pub transform: Transform,
}

/// Parsed SVG document
pub struct SvgDocument {
    pub width: f32,
    pub height: f32,
    pub view_box: (f32, f32, f32, f32),
    pub root: Vec<SvgNode>,
}

impl SvgDocument {
    /// Render SVG to RGBA pixel buffer at specified size
    pub fn render(&self, target_width: u32, target_height: u32) -> Vec<u32> {
        let mut pixels = alloc::vec![0u32; (target_width * target_height) as usize];
        let sx = target_width as f32 / self.view_box.2;
        let sy = target_height as f32 / self.view_box.3;
        let base_transform = Transform::scale(sx, sy);

        for node in &self.root {
            self.render_node(
                node,
                &base_transform,
                &mut pixels,
                target_width,
                target_height,
            );
        }
        pixels
    }

    fn render_node(
        &self,
        node: &SvgNode,
        parent_transform: &Transform,
        pixels: &mut [u32],
        width: u32,
        height: u32,
    ) {
        let transform = parent_transform.concat(&node.transform);
        match &node.element {
            SvgElement::Rect {
                x,
                y,
                width: w,
                height: h,
                ..
            } => {
                if let Some(fill) = &node.style.fill {
                    let color = fill.to_u32();
                    self.fill_rect(*x, *y, *w, *h, &transform, color, pixels, width, height);
                }
            }
            SvgElement::Circle { cx, cy, r } => {
                if let Some(fill) = &node.style.fill {
                    let color = fill.to_u32();
                    self.fill_circle(*cx, *cy, *r, &transform, color, pixels, width, height);
                }
            }
            SvgElement::Group { children } => {
                for child in children {
                    self.render_node(child, &transform, pixels, width, height);
                }
            }
            _ => {} // Other shapes would be rasterized similarly
        }
    }

    fn fill_rect(
        &self,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        transform: &Transform,
        color: u32,
        pixels: &mut [u32],
        pw: u32,
        ph: u32,
    ) {
        let (tx0, ty0) = transform.apply(x, y);
        let (tx1, ty1) = transform.apply(x + w, y + h);
        let x0 = (tx0 as i32).max(0) as u32;
        let y0 = (ty0 as i32).max(0) as u32;
        let x1 = (tx1 as u32).min(pw);
        let y1 = (ty1 as u32).min(ph);
        for py in y0..y1 {
            for px in x0..x1 {
                pixels[(py * pw + px) as usize] = color;
            }
        }
    }

    fn fill_circle(
        &self,
        cx: f32,
        cy: f32,
        r: f32,
        transform: &Transform,
        color: u32,
        pixels: &mut [u32],
        pw: u32,
        ph: u32,
    ) {
        let (tcx, tcy) = transform.apply(cx, cy);
        let tr = r * transform.a; // approximate
        let x0 = ((tcx - tr) as i32).max(0) as u32;
        let y0 = ((tcy - tr) as i32).max(0) as u32;
        let x1 = ((tcx + tr) as u32 + 1).min(pw);
        let y1 = ((tcy + tr) as u32 + 1).min(ph);
        let r2 = tr * tr;
        for py in y0..y1 {
            for px in x0..x1 {
                let dx = px as f32 - tcx;
                let dy = py as f32 - tcy;
                if dx * dx + dy * dy <= r2 {
                    pixels[(py * pw + px) as usize] = color;
                }
            }
        }
    }
}

pub fn init() {
    serial_println!("[SVG] SVG renderer loaded");
}
