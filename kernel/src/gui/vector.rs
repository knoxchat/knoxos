use crate::gui::framebuffer::{FrameBuffer, Pixel};
/// Vector Graphics — Triangle rasterization, Bezier curves, SVG rendering
///
/// Provides vector graphics primitives for the GUI:
///   - Triangle rasterization (filled, with edge AA)
///   - Quadratic and cubic Bezier curves
///   - SVG path rendering (move, line, curve, close)
///   - Polygon fill (scanline algorithm)
///   - Arc rendering
///   - Used by icon rendering, charts, and custom UI shapes
use alloc::vec::Vec;

// ═══════════════════════════════════════════════════════════════════════
// TRIANGLE RASTERIZATION
// ═══════════════════════════════════════════════════════════════════════

/// Point in 2D space (sub-pixel precision)
#[derive(Debug, Clone, Copy)]
pub struct Point {
    pub x: f32,
    pub y: f32,
}

impl Point {
    pub fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }
}

/// Rasterize a filled triangle with anti-aliased edges
pub fn fill_triangle(fb: &mut FrameBuffer, p0: Point, p1: Point, p2: Point, color: Pixel) {
    // Bounding box
    let min_x = p0.x.min(p1.x).min(p2.x).max(0.0) as i32;
    let min_y = p0.y.min(p1.y).min(p2.y).max(0.0) as i32;
    let max_x = (p0.x.max(p1.x).max(p2.x) as i32 + 1).min(fb.width as i32 - 1);
    let max_y = (p0.y.max(p1.y).max(p2.y) as i32 + 1).min(fb.height as i32 - 1);

    // 2× area of triangle (for barycentric coords)
    let area = edge_function(p0, p1, p2);
    if area.abs() < 0.001 {
        return; // Degenerate triangle
    }
    let inv_area = 1.0 / area;

    for y in min_y..=max_y {
        for x in min_x..=max_x {
            let p = Point::new(x as f32 + 0.5, y as f32 + 0.5);

            let w0 = edge_function(p1, p2, p) * inv_area;
            let w1 = edge_function(p2, p0, p) * inv_area;
            let w2 = edge_function(p0, p1, p) * inv_area;

            if w0 >= -0.01 && w1 >= -0.01 && w2 >= -0.01 {
                // Compute edge AA: alpha based on distance to nearest edge
                let edge_dist = w0.min(w1).min(w2);
                let alpha = if edge_dist < 0.0 {
                    0
                } else if edge_dist < 1.0 {
                    (edge_dist * 255.0) as u8
                } else {
                    255
                };

                if alpha > 0 {
                    let blended = Pixel::new(
                        color.r,
                        color.g,
                        color.b,
                        ((color.a as u16 * alpha as u16 + 128) >> 8) as u8,
                    );
                    fb.set_pixel(x as usize, y as usize, blended);
                }
            }
        }
    }
}

/// Edge function for barycentric coordinates
fn edge_function(a: Point, b: Point, c: Point) -> f32 {
    (c.x - a.x) * (b.y - a.y) - (c.y - a.y) * (b.x - a.x)
}

/// Rasterize a triangle with per-vertex colors (Gouraud shading)
pub fn fill_triangle_gradient(
    fb: &mut FrameBuffer,
    p0: Point,
    c0: Pixel,
    p1: Point,
    c1: Pixel,
    p2: Point,
    c2: Pixel,
) {
    let min_x = p0.x.min(p1.x).min(p2.x).max(0.0) as i32;
    let min_y = p0.y.min(p1.y).min(p2.y).max(0.0) as i32;
    let max_x = (p0.x.max(p1.x).max(p2.x) as i32 + 1).min(fb.width as i32 - 1);
    let max_y = (p0.y.max(p1.y).max(p2.y) as i32 + 1).min(fb.height as i32 - 1);

    let area = edge_function(p0, p1, p2);
    if area.abs() < 0.001 {
        return;
    }
    let inv_area = 1.0 / area;

    for y in min_y..=max_y {
        for x in min_x..=max_x {
            let p = Point::new(x as f32 + 0.5, y as f32 + 0.5);

            let w0 = edge_function(p1, p2, p) * inv_area;
            let w1 = edge_function(p2, p0, p) * inv_area;
            let w2 = 1.0 - w0 - w1;

            if w0 >= -0.01 && w1 >= -0.01 && w2 >= -0.01 {
                let r = (c0.r as f32 * w0 + c1.r as f32 * w1 + c2.r as f32 * w2).clamp(0.0, 255.0)
                    as u8;
                let g = (c0.g as f32 * w0 + c1.g as f32 * w1 + c2.g as f32 * w2).clamp(0.0, 255.0)
                    as u8;
                let b = (c0.b as f32 * w0 + c1.b as f32 * w1 + c2.b as f32 * w2).clamp(0.0, 255.0)
                    as u8;
                let a = (c0.a as f32 * w0 + c1.a as f32 * w1 + c2.a as f32 * w2).clamp(0.0, 255.0)
                    as u8;
                fb.set_pixel(x as usize, y as usize, Pixel::new(r, g, b, a));
            }
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════
// BEZIER CURVES
// ═══════════════════════════════════════════════════════════════════════

/// Evaluate a quadratic Bezier curve at parameter t
pub fn quadratic_bezier(p0: Point, p1: Point, p2: Point, t: f32) -> Point {
    let inv_t = 1.0 - t;
    Point {
        x: inv_t * inv_t * p0.x + 2.0 * inv_t * t * p1.x + t * t * p2.x,
        y: inv_t * inv_t * p0.y + 2.0 * inv_t * t * p1.y + t * t * p2.y,
    }
}

/// Evaluate a cubic Bezier curve at parameter t
pub fn cubic_bezier(p0: Point, p1: Point, p2: Point, p3: Point, t: f32) -> Point {
    let inv_t = 1.0 - t;
    let inv_t2 = inv_t * inv_t;
    let inv_t3 = inv_t2 * inv_t;
    let t2 = t * t;
    let t3 = t2 * t;
    Point {
        x: inv_t3 * p0.x + 3.0 * inv_t2 * t * p1.x + 3.0 * inv_t * t2 * p2.x + t3 * p3.x,
        y: inv_t3 * p0.y + 3.0 * inv_t2 * t * p1.y + 3.0 * inv_t * t2 * p2.y + t3 * p3.y,
    }
}

/// Draw a quadratic Bezier curve
pub fn draw_quadratic_bezier(
    fb: &mut FrameBuffer,
    p0: Point,
    p1: Point,
    p2: Point,
    color: Pixel,
    segments: usize,
) {
    let seg = segments.max(4);
    let mut prev = p0;
    for i in 1..=seg {
        let t = i as f32 / seg as f32;
        let curr = quadratic_bezier(p0, p1, p2, t);
        draw_line_aa(fb, prev, curr, color);
        prev = curr;
    }
}

/// Draw a cubic Bezier curve
pub fn draw_cubic_bezier(
    fb: &mut FrameBuffer,
    p0: Point,
    p1: Point,
    p2: Point,
    p3: Point,
    color: Pixel,
    segments: usize,
) {
    let seg = segments.max(4);
    let mut prev = p0;
    for i in 1..=seg {
        let t = i as f32 / seg as f32;
        let curr = cubic_bezier(p0, p1, p2, p3, t);
        draw_line_aa(fb, prev, curr, color);
        prev = curr;
    }
}

/// Draw an anti-aliased line between two points (Xiaolin Wu's algorithm)
fn draw_line_aa(fb: &mut FrameBuffer, p0: Point, p1: Point, color: Pixel) {
    let mut x0 = p0.x;
    let mut y0 = p0.y;
    let mut x1 = p1.x;
    let mut y1 = p1.y;

    let steep = (y1 - y0).abs() > (x1 - x0).abs();
    if steep {
        core::mem::swap(&mut x0, &mut y0);
        core::mem::swap(&mut x1, &mut y1);
    }
    if x0 > x1 {
        core::mem::swap(&mut x0, &mut x1);
        core::mem::swap(&mut y0, &mut y1);
    }

    let dx = x1 - x0;
    let dy = y1 - y0;
    let gradient = if dx.abs() < 0.001 { 1.0 } else { dy / dx };

    // First endpoint
    let xend = libm::roundf(x0);
    let yend = y0 + gradient * (xend - x0);
    let xpxl1 = xend as i32;
    let ypxl1 = libm::floorf(yend) as i32;

    // Second endpoint
    let xend2 = libm::roundf(x1);
    let xpxl2 = xend2 as i32;

    let mut intery = yend + gradient;

    for x in (xpxl1 + 1)..xpxl2 {
        let y = libm::floorf(intery) as i32;
        let fpart = intery - libm::floorf(intery);
        let alpha1 = ((1.0 - fpart) * color.a as f32) as u8;
        let alpha2 = (fpart * color.a as f32) as u8;

        if steep {
            fb.set_pixel(
                y as usize,
                x as usize,
                Pixel::new(color.r, color.g, color.b, alpha1),
            );
            fb.set_pixel(
                (y + 1) as usize,
                x as usize,
                Pixel::new(color.r, color.g, color.b, alpha2),
            );
        } else {
            fb.set_pixel(
                x as usize,
                y as usize,
                Pixel::new(color.r, color.g, color.b, alpha1),
            );
            fb.set_pixel(
                x as usize,
                (y + 1) as usize,
                Pixel::new(color.r, color.g, color.b, alpha2),
            );
        }

        intery += gradient;
    }
}

// ═══════════════════════════════════════════════════════════════════════
// POLYGON FILL (Scanline Algorithm)
// ═══════════════════════════════════════════════════════════════════════

/// Fill a polygon defined by a list of vertices
pub fn fill_polygon(fb: &mut FrameBuffer, vertices: &[Point], color: Pixel) {
    if vertices.len() < 3 {
        return;
    }

    // Find bounding box
    let mut min_y = f32::MAX;
    let mut max_y = f32::MIN;
    for v in vertices {
        if v.y < min_y {
            min_y = v.y;
        }
        if v.y > max_y {
            max_y = v.y;
        }
    }

    let start_y = min_y.max(0.0) as i32;
    let end_y = (max_y as i32 + 1).min(fb.height as i32 - 1);

    // Scanline fill
    for y in start_y..=end_y {
        let scan_y = y as f32 + 0.5;
        let mut intersections = Vec::new();

        let n = vertices.len();
        for i in 0..n {
            let j = (i + 1) % n;
            let (mut y0, mut y1) = (vertices[i].y, vertices[j].y);
            let (mut x0, mut x1) = (vertices[i].x, vertices[j].x);

            if y0 > y1 {
                core::mem::swap(&mut y0, &mut y1);
                core::mem::swap(&mut x0, &mut x1);
            }

            if scan_y >= y0 && scan_y < y1 {
                let t = (scan_y - y0) / (y1 - y0);
                let x = x0 + t * (x1 - x0);
                intersections.push(x);
            }
        }

        intersections.sort_by(|a, b| a.partial_cmp(b).unwrap_or(core::cmp::Ordering::Equal));

        // Fill between pairs of intersections
        for pair in intersections.chunks(2) {
            if pair.len() == 2 {
                let start_x = pair[0].max(0.0) as i32;
                let end_x = (pair[1] as i32).min(fb.width as i32 - 1);
                for x in start_x..=end_x {
                    fb.set_pixel(x as usize, y as usize, color);
                }
            }
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════
// SVG PATH RENDERING (simplified subset)
// ═══════════════════════════════════════════════════════════════════════

/// SVG path command
#[derive(Debug, Clone)]
pub enum PathCommand {
    MoveTo(Point),
    LineTo(Point),
    QuadTo(Point, Point),         // control, end
    CubicTo(Point, Point, Point), // c1, c2, end
    ArcTo {
        rx: f32,
        ry: f32,
        rotation: f32,
        large_arc: bool,
        sweep: bool,
        end: Point,
    },
    Close,
}

/// Render an SVG path as a filled shape
pub fn fill_path(fb: &mut FrameBuffer, commands: &[PathCommand], color: Pixel) {
    // Convert path to polygon vertices
    let vertices = path_to_polygon(commands, 16);
    if vertices.len() >= 3 {
        fill_polygon(fb, &vertices, color);
    }
}

/// Render an SVG path as a stroked outline
pub fn stroke_path(fb: &mut FrameBuffer, commands: &[PathCommand], color: Pixel, _width: f32) {
    let vertices = path_to_polygon(commands, 16);
    if vertices.len() < 2 {
        return;
    }
    for i in 0..vertices.len() - 1 {
        draw_line_aa(fb, vertices[i], vertices[i + 1], color);
    }
}

/// Convert path commands to a polygon (flattened to line segments)
fn path_to_polygon(commands: &[PathCommand], curve_segments: usize) -> Vec<Point> {
    let mut vertices = Vec::new();
    let mut current = Point::new(0.0, 0.0);
    let mut start = Point::new(0.0, 0.0);

    for cmd in commands {
        match cmd {
            PathCommand::MoveTo(p) => {
                current = *p;
                start = *p;
                vertices.push(*p);
            }
            PathCommand::LineTo(p) => {
                vertices.push(*p);
                current = *p;
            }
            PathCommand::QuadTo(ctrl, end) => {
                for i in 1..=curve_segments {
                    let t = i as f32 / curve_segments as f32;
                    let p = quadratic_bezier(current, *ctrl, *end, t);
                    vertices.push(p);
                }
                current = *end;
            }
            PathCommand::CubicTo(c1, c2, end) => {
                for i in 1..=curve_segments {
                    let t = i as f32 / curve_segments as f32;
                    let p = cubic_bezier(current, *c1, *c2, *end, t);
                    vertices.push(p);
                }
                current = *end;
            }
            PathCommand::ArcTo { rx, ry, end, .. } => {
                // Simplified: approximate arc as line to endpoint
                vertices.push(*end);
                current = *end;
            }
            PathCommand::Close => {
                if !vertices.is_empty() {
                    vertices.push(start);
                    current = start;
                }
            }
        }
    }

    vertices
}

/// Draw a circle arc between two angles
pub fn draw_arc(
    fb: &mut FrameBuffer,
    cx: f32,
    cy: f32,
    radius: f32,
    start_angle: f32,
    end_angle: f32,
    color: Pixel,
    segments: usize,
) {
    let seg = segments.max(4);
    let angle_step = (end_angle - start_angle) / seg as f32;
    let mut prev = Point::new(
        cx + radius * libm::cosf(start_angle),
        cy + radius * libm::sinf(start_angle),
    );

    for i in 1..=seg {
        let angle = start_angle + angle_step * i as f32;
        let curr = Point::new(
            cx + radius * libm::cosf(angle),
            cy + radius * libm::sinf(angle),
        );
        draw_line_aa(fb, prev, curr, color);
        prev = curr;
    }
}
