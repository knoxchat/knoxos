// SPDX-License-Identifier: MIT
//! Tailwind-like style and [`Styled`] builders (GPUI / GPUI kit names).
//!
//! Spacing scale: `px_N` / `gap_N` use 4px steps (`px_3` = 12, `gap_2` = 8).

use crate::geometry::{Corners, Edges, Hsla, Pixels, Point, Size, px};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Display {
    #[default]
    Flex,
    None,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum FlexDirection {
    #[default]
    Row,
    Column,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Justify {
    #[default]
    Start,
    Center,
    End,
    SpaceBetween,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Align {
    Start,
    Center,
    End,
    #[default]
    Stretch,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Position {
    #[default]
    Relative,
    Absolute,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Overflow {
    #[default]
    Visible,
    Hidden,
    Scroll,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Style {
    pub display: Display,
    pub flex_direction: FlexDirection,
    pub justify: Justify,
    pub align: Align,
    pub position: Position,
    pub overflow: Overflow,
    pub grow: u32,
    pub width: Option<Pixels>,
    pub height: Option<Pixels>,
    pub min_width: Option<Pixels>,
    pub max_width: Option<Pixels>,
    pub min_height: Option<Pixels>,
    pub max_height: Option<Pixels>,
    pub left: Option<Pixels>,
    pub top: Option<Pixels>,
    pub right: Option<Pixels>,
    pub bottom: Option<Pixels>,
    pub padding: Edges,
    pub margin: Edges,
    pub gap: Pixels,
    pub scroll_offset: Point,
    pub background: Option<Hsla>,
    pub border_widths: Edges,
    pub border_color: Hsla,
    pub corner_radii: Corners,
    pub opacity: f32,
}

impl Default for Style {
    fn default() -> Self {
        Self {
            display: Display::Flex,
            flex_direction: FlexDirection::Row,
            justify: Justify::Start,
            align: Align::Stretch,
            position: Position::Relative,
            overflow: Overflow::Visible,
            grow: 0,
            width: None,
            height: None,
            min_width: None,
            max_width: None,
            min_height: None,
            max_height: None,
            left: None,
            top: None,
            right: None,
            bottom: None,
            padding: Edges::ZERO,
            margin: Edges::ZERO,
            gap: Pixels::ZERO,
            scroll_offset: Point::ZERO,
            background: None,
            border_widths: Edges::ZERO,
            border_color: Hsla::TRANSPARENT,
            corner_radii: Corners::ZERO,
            opacity: 1.0,
        }
    }
}

impl Style {
    pub fn clamp_size(&self, mut size: Size) -> Size {
        if let Some(min) = self.min_width {
            size.width = size.width.max(min);
        }
        if let Some(max) = self.max_width {
            size.width = size.width.min(max);
        }
        if let Some(min) = self.min_height {
            size.height = size.height.max(min);
        }
        if let Some(max) = self.max_height {
            size.height = size.height.min(max);
        }
        size
    }
}

/// Partial overlay for pointer pseudo-states (`:hover` / `:active` / `:focus`).
///
/// Only visual fields: layout (size, padding, gap) stays on the base [`Style`].
/// Apply order is hover → focus → active (GPUI / kit contract).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct StyleRefinement {
    pub background: Option<Hsla>,
    pub border_color: Option<Hsla>,
    pub corner_radii: Option<Corners>,
    pub opacity: Option<f32>,
}

impl StyleRefinement {
    pub const EMPTY: Self = Self {
        background: None,
        border_color: None,
        corner_radii: None,
        opacity: None,
    };

    pub fn is_empty(self) -> bool {
        self.background.is_none()
            && self.border_color.is_none()
            && self.corner_radii.is_none()
            && self.opacity.is_none()
    }

    pub fn bg(mut self, color: Hsla) -> Self {
        self.background = Some(color);
        self
    }

    pub fn rounded(mut self, radius: Pixels) -> Self {
        self.corner_radii = Some(Corners::all(radius));
        self
    }

    pub fn opacity(mut self, opacity: f32) -> Self {
        self.opacity = Some(opacity);
        self
    }

    pub fn border_color(mut self, color: Hsla) -> Self {
        self.border_color = Some(color);
        self
    }

    /// Overlay this refinement onto `style`.
    pub fn apply(self, style: &mut Style) {
        if let Some(background) = self.background {
            style.background = Some(background);
        }
        if let Some(border_color) = self.border_color {
            style.border_color = border_color;
        }
        if let Some(corner_radii) = self.corner_radii {
            style.corner_radii = corner_radii;
        }
        if let Some(opacity) = self.opacity {
            style.opacity = opacity;
        }
    }
}

/// Default focus-ring color (Aurora coral `ring` token).
pub fn ring_color() -> Hsla {
    Hsla::from_rgb(232, 121, 100)
}

/// Pointer / focus bits used when resolving [`StyleRefinement`]s.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ElementState {
    pub hovered: bool,
    pub active: bool,
    pub focused: bool,
}

impl Style {
    /// Base style plus hover → focus → active overlays.
    pub fn resolved(
        &self,
        hover: &StyleRefinement,
        focus: &StyleRefinement,
        active: &StyleRefinement,
        state: ElementState,
    ) -> Style {
        let mut style = *self;
        if state.hovered {
            hover.apply(&mut style);
        }
        if state.focused {
            focus.apply(&mut style);
        }
        if state.active {
            active.apply(&mut style);
        }
        style
    }
}

/// Fluent style builders. Names match GPUI kit where cheap (`px_3`, `flex`,
/// `gap_2`, `bg`, `rounded`).
pub trait Styled: Sized {
    fn style(&self) -> &Style;
    fn style_mut(&mut self) -> &mut Style;

    fn flex(mut self) -> Self {
        self.style_mut().display = Display::Flex;
        self.style_mut().flex_direction = FlexDirection::Row;
        self
    }

    fn flex_col(mut self) -> Self {
        self.style_mut().display = Display::Flex;
        self.style_mut().flex_direction = FlexDirection::Column;
        self
    }

    fn px_3(mut self) -> Self {
        self.style_mut().padding = Edges::all(px(12));
        self
    }

    fn gap_2(mut self) -> Self {
        self.style_mut().gap = px(8);
        self
    }

    fn bg(mut self, color: Hsla) -> Self {
        self.style_mut().background = Some(color);
        self
    }

    fn rounded(mut self, radius: Pixels) -> Self {
        self.style_mut().corner_radii = Corners::all(radius);
        self
    }

    fn w(mut self, width: Pixels) -> Self {
        self.style_mut().width = Some(width);
        self
    }

    fn h(mut self, height: Pixels) -> Self {
        self.style_mut().height = Some(height);
        self
    }

    fn size(mut self, size: Size) -> Self {
        self.style_mut().width = Some(size.width);
        self.style_mut().height = Some(size.height);
        self
    }

    fn p(mut self, padding: Pixels) -> Self {
        self.style_mut().padding = Edges::all(padding);
        self
    }

    fn m(mut self, margin: Pixels) -> Self {
        self.style_mut().margin = Edges::all(margin);
        self
    }

    fn opacity(mut self, opacity: f32) -> Self {
        self.style_mut().opacity = opacity;
        self
    }

    fn grow(mut self) -> Self {
        self.style_mut().grow = 1;
        self
    }

    fn justify_center(mut self) -> Self {
        self.style_mut().justify = Justify::Center;
        self
    }

    fn justify_end(mut self) -> Self {
        self.style_mut().justify = Justify::End;
        self
    }

    fn justify_between(mut self) -> Self {
        self.style_mut().justify = Justify::SpaceBetween;
        self
    }

    fn items_center(mut self) -> Self {
        self.style_mut().align = Align::Center;
        self
    }

    fn items_end(mut self) -> Self {
        self.style_mut().align = Align::End;
        self
    }

    fn items_start(mut self) -> Self {
        self.style_mut().align = Align::Start;
        self
    }

    fn min_w(mut self, width: Pixels) -> Self {
        self.style_mut().min_width = Some(width);
        self
    }

    fn max_w(mut self, width: Pixels) -> Self {
        self.style_mut().max_width = Some(width);
        self
    }

    fn min_h(mut self, height: Pixels) -> Self {
        self.style_mut().min_height = Some(height);
        self
    }

    fn max_h(mut self, height: Pixels) -> Self {
        self.style_mut().max_height = Some(height);
        self
    }

    fn absolute(mut self) -> Self {
        self.style_mut().position = Position::Absolute;
        self
    }

    fn overlay(self) -> Self {
        self.absolute()
    }

    fn left(mut self, value: Pixels) -> Self {
        self.style_mut().left = Some(value);
        self
    }

    fn top(mut self, value: Pixels) -> Self {
        self.style_mut().top = Some(value);
        self
    }

    fn right(mut self, value: Pixels) -> Self {
        self.style_mut().right = Some(value);
        self
    }

    fn bottom(mut self, value: Pixels) -> Self {
        self.style_mut().bottom = Some(value);
        self
    }

    fn overflow_hidden(mut self) -> Self {
        self.style_mut().overflow = Overflow::Hidden;
        self
    }

    fn scroll_y(mut self, offset: Pixels) -> Self {
        self.style_mut().scroll_offset.y = offset;
        self
    }

    fn gap(mut self, gap: Pixels) -> Self {
        self.style_mut().gap = gap;
        self
    }

    fn gap_1(mut self) -> Self {
        self.style_mut().gap = px(4);
        self
    }

    fn pad_x(mut self, pad: Pixels) -> Self {
        self.style_mut().padding.left = pad;
        self.style_mut().padding.right = pad;
        self
    }

    fn pad_y(mut self, pad: Pixels) -> Self {
        self.style_mut().padding.top = pad;
        self.style_mut().padding.bottom = pad;
        self
    }

    fn border_1(mut self) -> Self {
        self.style_mut().border_widths = Edges::all(px(1));
        self
    }

    fn border_color(mut self, color: Hsla) -> Self {
        self.style_mut().border_color = color;
        self
    }

    fn rounded_corners(mut self, corners: Corners) -> Self {
        self.style_mut().corner_radii = corners;
        self
    }

    fn relative(mut self) -> Self {
        self.style_mut().position = Position::Relative;
        self
    }

    fn ml(mut self, value: Pixels) -> Self {
        self.style_mut().margin.left = value;
        self
    }

    fn mr(mut self, value: Pixels) -> Self {
        self.style_mut().margin.right = value;
        self
    }

    fn mt(mut self, value: Pixels) -> Self {
        self.style_mut().margin.top = value;
        self
    }

    fn mb(mut self, value: Pixels) -> Self {
        self.style_mut().margin.bottom = value;
        self
    }
}

/// Kit `FluentBuilder` role: conditional method chains (`when`).
pub trait FluentBuilder: Sized {
    fn when(self, cond: bool, f: impl FnOnce(Self) -> Self) -> Self {
        if cond { f(self) } else { self }
    }
}

impl<T> FluentBuilder for T {}

#[cfg(test)]
mod tests {
    use super::{Display, Style, Styled};
    use crate::Div;
    use crate::geometry::{Edges, Hsla, px};

    /// D3.2 done-when: host test style sets padding and bg.
    #[test]
    fn style_sets_padding_and_bg() {
        let d = Div::new().px_3().bg(Hsla::RED);
        assert_eq!(d.style().padding, Edges::all(px(12)));
        assert_eq!(d.style().background, Some(Hsla::RED));
        assert_eq!(d.style().display, Display::Flex);
        let _ = Style::default();
    }
}
