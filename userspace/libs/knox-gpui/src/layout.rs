// SPDX-License-Identifier: MIT
//! Integer flex layout (GPUI `taffy` role, without the crate).
//!
//! Row/column, grow, justify, align, gap, padding, min/max, absolute children,
//! and scroll offset. Overflow clip is applied at paint time.

use alloc::vec;
use alloc::vec::Vec;

use crate::element::{AnyElement, Element};
use crate::geometry::{Bounds, Edges, Pixels, Point, Size};
use crate::style::{Align, Display, FlexDirection, Justify, Position, Style};

/// Layout fields a parent needs from a child without downcasting.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LayoutHints {
    pub display: Display,
    pub grow: u32,
    pub position: Position,
    pub left: Option<Pixels>,
    pub top: Option<Pixels>,
    pub right: Option<Pixels>,
    pub bottom: Option<Pixels>,
}

impl LayoutHints {
    pub fn from_style(style: &Style) -> Self {
        Self {
            display: style.display,
            grow: style.grow,
            position: style.position,
            left: style.left,
            top: style.top,
            right: style.right,
            bottom: style.bottom,
        }
    }
}

fn nonneg(p: Pixels) -> Pixels {
    Pixels(p.0.max(0))
}

/// Place `children` according to `style`. `origin` / `available` are the
/// parent's content proposal (margin is applied here).
pub fn flex_layout(
    style: &Style,
    children: &mut [AnyElement],
    origin: Point,
    available: Size,
) -> Bounds {
    if style.display == Display::None {
        return Bounds::new(origin, Size::ZERO);
    }

    let origin = Point {
        x: origin.x + style.margin.left,
        y: origin.y + style.margin.top,
    };
    let pad = style.padding;
    let inner_origin = Point {
        x: origin.x + pad.left,
        y: origin.y + pad.top,
    };

    let avail_w = nonneg(available.width - style.margin.horizontal());
    let avail_h = nonneg(available.height - style.margin.vertical());
    let tentative = style.clamp_size(Size {
        width: style.width.unwrap_or(avail_w),
        height: style.height.unwrap_or(avail_h),
    });
    let inner_avail = Size {
        width: nonneg(tentative.width - pad.horizontal()),
        height: nonneg(tentative.height - pad.vertical()),
    };

    let row = style.flex_direction == FlexDirection::Row;
    let mut flow: Vec<usize> = Vec::new();
    let mut abs: Vec<usize> = Vec::new();
    for (i, child) in children.iter().enumerate() {
        let hints = child.layout_hints();
        if hints.display == Display::None {
            continue;
        }
        if hints.position == Position::Absolute {
            abs.push(i);
        } else {
            flow.push(i);
        }
    }

    let n = flow.len();
    let gap = style.gap;
    let gap_total = if n > 1 {
        gap * (n as i32 - 1)
    } else {
        Pixels::ZERO
    };

    let cross_measure = if style.align == Align::Stretch {
        if row {
            inner_avail.height
        } else {
            inner_avail.width
        }
    } else {
        Pixels::ZERO
    };
    let measure_avail = if row {
        Size {
            width: Pixels::ZERO,
            height: cross_measure,
        }
    } else {
        Size {
            width: cross_measure,
            height: Pixels::ZERO,
        }
    };

    let mut measured: Vec<Size> = Vec::with_capacity(n);
    for &i in &flow {
        let b = children[i].request_layout(inner_origin, measure_avail);
        measured.push(b.size);
    }

    if style.wrap && n > 0 {
        return layout_wrapped(
            style,
            children,
            &flow,
            &measured,
            &abs,
            origin,
            inner_origin,
            inner_avail,
            avail_w,
            avail_h,
            pad,
        );
    }

    let packed_main = Pixels(
        measured
            .iter()
            .map(|s| if row { s.width.0 } else { s.height.0 })
            .sum::<i32>(),
    ) + gap_total;
    let content_cross = measured
        .iter()
        .map(|s| if row { s.height } else { s.width })
        .fold(Pixels::ZERO, |a, b| a.max(b));
    let (content_w, content_h) = if row {
        (packed_main, content_cross)
    } else {
        (content_cross, packed_main)
    };

    let auto_w = content_w + pad.horizontal();
    let auto_h = content_h + pad.vertical();
    let mut width = style.width.unwrap_or(auto_w.max(pad.horizontal()));
    let mut height = style.height.unwrap_or(auto_h.max(pad.vertical()));
    // Flex grow: the parent allocated `available` as the used item box.
    if style.grow > 0 {
        if style.width.is_none() {
            width = width.max(avail_w);
        }
        if style.height.is_none() {
            height = height.max(avail_h);
        }
    }
    let size = style.clamp_size(Size { width, height });
    let inner = Size {
        width: nonneg(size.width - pad.horizontal()),
        height: nonneg(size.height - pad.vertical()),
    };
    let inner_main = if row { inner.width } else { inner.height };
    let inner_cross = if row { inner.height } else { inner.width };

    let leftover = (inner_main.0 - packed_main.0).max(0);
    let grow_sum: u32 = flow.iter().map(|&i| children[i].layout_hints().grow).sum();
    let mut extras = vec![0i32; n];
    if grow_sum > 0 && leftover > 0 {
        let mut used = 0;
        for (k, &i) in flow.iter().enumerate() {
            let g = children[i].layout_hints().grow;
            if g == 0 {
                continue;
            }
            extras[k] = leftover * g as i32 / grow_sum as i32;
            used += extras[k];
        }
        let mut rem = leftover - used;
        for (k, &i) in flow.iter().enumerate() {
            if rem <= 0 {
                break;
            }
            if children[i].layout_hints().grow > 0 {
                extras[k] += 1;
                rem -= 1;
            }
        }
        for (k, child_size) in measured.iter_mut().enumerate() {
            if row {
                child_size.width = Pixels(child_size.width.0 + extras[k]);
            } else {
                child_size.height = Pixels(child_size.height.0 + extras[k]);
            }
        }
    }

    let justify_free = if grow_sum > 0 { 0 } else { leftover };
    let (mut main_pos, between) = match style.justify {
        Justify::Start => (0, 0),
        Justify::End => (justify_free, 0),
        Justify::Center => (justify_free / 2, 0),
        Justify::SpaceBetween if n > 1 => (0, justify_free / (n as i32 - 1)),
        Justify::SpaceBetween => (0, 0),
    };

    let scroll = style.scroll_offset;
    for (k, &i) in flow.iter().enumerate() {
        let child_size = measured[k];
        let child_cross = if row {
            child_size.height.0
        } else {
            child_size.width.0
        };
        let mut cross_pos = 0;
        let mut alloc_cross = child_cross;
        match style.align {
            Align::Start => {}
            Align::Center => cross_pos = (inner_cross.0 - child_cross) / 2,
            Align::End => cross_pos = inner_cross.0 - child_cross,
            Align::Stretch => alloc_cross = inner_cross.0,
        }

        let (x, y, avail) = if row {
            (
                inner_origin.x.0 + main_pos - scroll.x.0,
                inner_origin.y.0 + cross_pos - scroll.y.0,
                Size {
                    width: child_size.width,
                    height: Pixels(alloc_cross),
                },
            )
        } else {
            (
                inner_origin.x.0 + cross_pos - scroll.x.0,
                inner_origin.y.0 + main_pos - scroll.y.0,
                Size {
                    width: Pixels(alloc_cross),
                    height: child_size.height,
                },
            )
        };
        children[i].request_layout(Point::xy(x, y), avail);
        main_pos += if row {
            child_size.width.0
        } else {
            child_size.height.0
        };
        if k + 1 < n {
            main_pos += gap.0 + between;
        }
    }

    let self_bounds = Bounds::new(origin, size);
    for i in abs {
        layout_absolute(style, &mut children[i], self_bounds, inner_origin, inner);
    }
    self_bounds
}

#[allow(clippy::too_many_arguments)]
fn layout_wrapped(
    style: &Style,
    children: &mut [AnyElement],
    flow: &[usize],
    measured: &[Size],
    abs: &[usize],
    origin: Point,
    inner_origin: Point,
    inner_avail: Size,
    avail_w: Pixels,
    avail_h: Pixels,
    pad: Edges,
) -> Bounds {
    let row = style.flex_direction == FlexDirection::Row;
    let gap = style.gap.0;
    let limit = if row {
        inner_avail.width.0
    } else {
        inner_avail.height.0
    };

    let mut lines: Vec<Vec<usize>> = Vec::new();
    let mut current: Vec<usize> = Vec::new();
    let mut used = 0;
    for (k, size) in measured.iter().enumerate() {
        let item = if row { size.width.0 } else { size.height.0 };
        let need = if current.is_empty() {
            item
        } else {
            used + gap + item
        };
        if !current.is_empty() && limit > 0 && need > limit {
            lines.push(core::mem::take(&mut current));
            used = 0;
        }
        if !current.is_empty() {
            used += gap;
        }
        used += item;
        current.push(k);
    }
    if !current.is_empty() {
        lines.push(current);
    }

    let mut line_mains = Vec::with_capacity(lines.len());
    let mut line_crosses = Vec::with_capacity(lines.len());
    for line in &lines {
        let mut main = 0;
        let mut cross = 0;
        for (j, &k) in line.iter().enumerate() {
            if j > 0 {
                main += gap;
            }
            if row {
                main += measured[k].width.0;
                cross = cross.max(measured[k].height.0);
            } else {
                main += measured[k].height.0;
                cross = cross.max(measured[k].width.0);
            }
        }
        line_mains.push(main);
        line_crosses.push(cross);
    }

    let content_main = line_mains.iter().copied().max().unwrap_or(0);
    let content_cross =
        line_crosses.iter().sum::<i32>() + gap * (lines.len().saturating_sub(1) as i32);
    let (content_w, content_h) = if row {
        (Pixels(content_main), Pixels(content_cross))
    } else {
        (Pixels(content_cross), Pixels(content_main))
    };

    let auto_w = content_w + pad.horizontal();
    let auto_h = content_h + pad.vertical();
    let mut width = style.width.unwrap_or(auto_w.max(pad.horizontal()));
    let mut height = style.height.unwrap_or(auto_h.max(pad.vertical()));
    if style.grow > 0 {
        if style.width.is_none() {
            width = width.max(avail_w);
        }
        if style.height.is_none() {
            height = height.max(avail_h);
        }
    }
    let size = style.clamp_size(Size { width, height });
    let inner = Size {
        width: nonneg(size.width - pad.horizontal()),
        height: nonneg(size.height - pad.vertical()),
    };
    let inner_main = if row { inner.width.0 } else { inner.height.0 };
    let scroll = style.scroll_offset;

    let mut cross_cursor = 0;
    for (line_i, line) in lines.iter().enumerate() {
        let packed = line_mains[line_i];
        let line_cross = line_crosses[line_i];
        let leftover = (inner_main - packed).max(0);
        let (mut main_pos, between) = match style.justify {
            Justify::Start => (0, 0),
            Justify::End => (leftover, 0),
            Justify::Center => (leftover / 2, 0),
            Justify::SpaceBetween if line.len() > 1 => (0, leftover / (line.len() as i32 - 1)),
            Justify::SpaceBetween => (0, 0),
        };

        for (j, &k) in line.iter().enumerate() {
            let child_size = measured[k];
            let child_cross = if row {
                child_size.height.0
            } else {
                child_size.width.0
            };
            let mut cross_pos = cross_cursor;
            let mut alloc_cross = child_cross;
            match style.align {
                Align::Start => {}
                Align::Center => cross_pos += (line_cross - child_cross) / 2,
                Align::End => cross_pos += line_cross - child_cross,
                Align::Stretch => alloc_cross = line_cross,
            }

            let (x, y, avail) = if row {
                (
                    inner_origin.x.0 + main_pos - scroll.x.0,
                    inner_origin.y.0 + cross_pos - scroll.y.0,
                    Size {
                        width: child_size.width,
                        height: Pixels(alloc_cross),
                    },
                )
            } else {
                (
                    inner_origin.x.0 + cross_pos - scroll.x.0,
                    inner_origin.y.0 + main_pos - scroll.y.0,
                    Size {
                        width: Pixels(alloc_cross),
                        height: child_size.height,
                    },
                )
            };
            children[flow[k]].request_layout(Point::xy(x, y), avail);
            main_pos += if row {
                child_size.width.0
            } else {
                child_size.height.0
            };
            if j + 1 < line.len() {
                main_pos += gap + between;
            }
        }
        cross_cursor += line_cross;
        if line_i + 1 < lines.len() {
            cross_cursor += gap;
        }
    }

    let self_bounds = Bounds::new(origin, size);
    for &i in abs {
        layout_absolute(style, &mut children[i], self_bounds, inner_origin, inner);
    }
    self_bounds
}

fn layout_absolute(
    parent: &Style,
    child: &mut AnyElement,
    parent_bounds: Bounds,
    inner_origin: Point,
    inner: Size,
) {
    let hints = child.layout_hints();
    let measured = child.request_layout(inner_origin, inner);
    let mut x = inner_origin.x;
    let mut y = inner_origin.y;
    if let Some(left) = hints.left {
        x = inner_origin.x + left;
    } else if let Some(right) = hints.right {
        x = parent_bounds.origin.x + parent_bounds.size.width
            - parent.padding.right
            - right
            - measured.size.width;
    }
    if let Some(top) = hints.top {
        y = inner_origin.y + top;
    } else if let Some(bottom) = hints.bottom {
        y = parent_bounds.origin.y + parent_bounds.size.height
            - parent.padding.bottom
            - bottom
            - measured.size.height;
    }
    child.request_layout(Point { x, y }, measured.size);
}

#[cfg(test)]
mod tests {
    use crate::div;
    use crate::element::paint_element;
    use crate::geometry::{Bounds, Hsla, Point, Size, px};
    use crate::raster::RasterBuffer;
    use crate::scene::Scene;
    use crate::style::Styled;

    /// D4.1 done-when: host tests cover row, column, grow-split.
    #[test]
    fn row_column_and_grow_split() {
        let mut row = div()
            .flex()
            .gap_2()
            .child(div().w(px(10)).h(px(10)).bg(Hsla::RED))
            .child(div().w(px(10)).h(px(10)).bg(Hsla::BLUE));
        let mut scene = Scene::new();
        let bounds = paint_element(&mut row, Point::ZERO, Size::wh(40, 10), &mut scene);
        assert_eq!(bounds, Bounds::from_xywh(0, 0, 28, 10));

        let mut col = div()
            .flex_col()
            .gap_2()
            .child(div().w(px(8)).h(px(8)).bg(Hsla::RED))
            .child(div().w(px(8)).h(px(8)).bg(Hsla::BLUE));
        let mut scene = Scene::new();
        paint_element(&mut col, Point::ZERO, Size::wh(8, 40), &mut scene);
        let mut buf = RasterBuffer::new(8, 24);
        buf.draw(&scene);
        assert_eq!(buf.pixel(0, 0), Some(Hsla::RED.to_bgra()));
        assert_eq!(buf.pixel(0, 7), Some(Hsla::RED.to_bgra()));
        assert_eq!(buf.pixel(0, 8), Some(crate::Bgra::TRANSPARENT));
        assert_eq!(buf.pixel(0, 16), Some(Hsla::BLUE.to_bgra()));

        let mut split = div()
            .flex()
            .w(px(20))
            .h(px(8))
            .child(div().h(px(8)).bg(Hsla::RED).grow())
            .child(div().h(px(8)).bg(Hsla::BLUE).grow());
        let mut scene = Scene::new();
        paint_element(&mut split, Point::ZERO, Size::wh(20, 8), &mut scene);
        let mut buf = RasterBuffer::new(20, 8);
        buf.draw(&scene);
        assert_eq!(buf.pixel(0, 0), Some(Hsla::RED.to_bgra()));
        assert_eq!(buf.pixel(9, 0), Some(Hsla::RED.to_bgra()));
        assert_eq!(buf.pixel(10, 0), Some(Hsla::BLUE.to_bgra()));
        assert_eq!(buf.pixel(19, 0), Some(Hsla::BLUE.to_bgra()));
    }

    #[test]
    fn min_width_clamps_auto_child() {
        let mut el = div().h(px(8)).min_w(px(12)).bg(Hsla::RED);
        let mut scene = Scene::new();
        let bounds = paint_element(&mut el, Point::ZERO, Size::wh(4, 8), &mut scene);
        assert_eq!(bounds, Bounds::from_xywh(0, 0, 12, 8));
    }

    /// D4.2 done-when: overlay is not in flex flow.
    #[test]
    fn absolute_child_not_in_flex_flow() {
        let mut root = div()
            .w(px(20))
            .h(px(20))
            .bg(Hsla::BLUE)
            .child(div().w(px(8)).h(px(8)).bg(Hsla::RED))
            .child(
                div()
                    .absolute()
                    .left(px(10))
                    .top(px(10))
                    .w(px(8))
                    .h(px(8))
                    .bg(Hsla::GREEN),
            );
        let mut scene = Scene::new();
        paint_element(&mut root, Point::ZERO, Size::wh(20, 20), &mut scene);
        let mut buf = RasterBuffer::new(20, 20);
        buf.draw(&scene);
        assert_eq!(buf.pixel(0, 0), Some(Hsla::RED.to_bgra()));
        assert_eq!(buf.pixel(10, 10), Some(Hsla::GREEN.to_bgra()));
        assert_eq!(buf.pixel(19, 19), Some(Hsla::BLUE.to_bgra()));
        // In-flow sibling was not pushed right by the overlay.
        assert_eq!(buf.pixel(8, 0), Some(Hsla::BLUE.to_bgra()));
    }

    /// D4.3 done-when: host test clips a tall child.
    #[test]
    fn overflow_hidden_clips_tall_child() {
        let mut root = div()
            .w(px(8))
            .h(px(8))
            .overflow_hidden()
            .child(div().w(px(16)).h(px(16)).bg(Hsla::RED));
        let mut scene = Scene::new();
        paint_element(&mut root, Point::ZERO, Size::wh(8, 8), &mut scene);
        let mut buf = RasterBuffer::new(16, 16);
        buf.draw(&scene);
        assert_eq!(buf.pixel(0, 0), Some(Hsla::RED.to_bgra()));
        assert_eq!(buf.pixel(7, 7), Some(Hsla::RED.to_bgra()));
        assert_eq!(buf.pixel(8, 0), Some(crate::Bgra::TRANSPARENT));
        assert_eq!(buf.pixel(0, 8), Some(crate::Bgra::TRANSPARENT));
    }

    #[test]
    fn scroll_offset_reveals_below_fold() {
        let mut root = div()
            .flex_col()
            .w(px(8))
            .h(px(8))
            .overflow_hidden()
            .scroll_y(px(8))
            .child(div().w(px(8)).h(px(8)).bg(Hsla::RED))
            .child(div().w(px(8)).h(px(8)).bg(Hsla::BLUE));
        let mut scene = Scene::new();
        paint_element(&mut root, Point::ZERO, Size::wh(8, 8), &mut scene);
        let mut buf = RasterBuffer::new(8, 8);
        buf.draw(&scene);
        assert_eq!(buf.pixel(0, 0), Some(Hsla::BLUE.to_bgra()));
        assert_eq!(buf.pixel(7, 7), Some(Hsla::BLUE.to_bgra()));
    }

    #[test]
    fn flex_wrap_breaks_to_second_row() {
        let mut root = div()
            .flex()
            .flex_wrap()
            .w(px(28))
            .gap_2()
            .child(div().w(px(10)).h(px(10)).bg(Hsla::RED))
            .child(div().w(px(10)).h(px(10)).bg(Hsla::BLUE))
            .child(div().w(px(10)).h(px(10)).bg(Hsla::GREEN));
        let mut scene = Scene::new();
        let bounds = paint_element(&mut root, Point::ZERO, Size::wh(28, 40), &mut scene);
        assert_eq!(bounds, Bounds::from_xywh(0, 0, 28, 28));
        let mut buf = RasterBuffer::new(28, 28);
        buf.draw(&scene);
        assert_eq!(buf.pixel(0, 0), Some(Hsla::RED.to_bgra()));
        assert_eq!(buf.pixel(18, 0), Some(Hsla::BLUE.to_bgra()));
        assert_eq!(buf.pixel(0, 18), Some(Hsla::GREEN.to_bgra()));
        assert_eq!(buf.pixel(18, 18), Some(crate::Bgra::TRANSPARENT));
    }

    #[test]
    fn justify_center_and_items_end() {
        let mut root = div()
            .flex()
            .w(px(30))
            .h(px(16))
            .justify_center()
            .items_end()
            .child(div().w(px(10)).h(px(8)).bg(Hsla::RED));
        let mut scene = Scene::new();
        paint_element(&mut root, Point::ZERO, Size::wh(30, 16), &mut scene);
        let mut buf = RasterBuffer::new(30, 16);
        buf.draw(&scene);
        assert_eq!(buf.pixel(10, 8), Some(Hsla::RED.to_bgra()));
        assert_eq!(buf.pixel(19, 15), Some(Hsla::RED.to_bgra()));
        assert_eq!(buf.pixel(0, 0), Some(crate::Bgra::TRANSPARENT));
        assert_eq!(buf.pixel(10, 0), Some(crate::Bgra::TRANSPARENT));
    }
}
