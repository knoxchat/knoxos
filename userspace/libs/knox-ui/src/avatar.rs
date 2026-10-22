// SPDX-License-Identifier: MIT
//! Avatar and AvatarGroup (GPUI kit catalog, reimplemented).
//!
//! the Longbridge kit is a design reference only (Apache-2.0). This file is new
//! `no_std` code.

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

use knox_gpui::{Div, FontId, Hsla, IntoElement, Pixels, Styled, div, px, text};

use crate::icon::{Icon, IconName};
use crate::image::Image;
use crate::size::{ControlSize, Sizable};
use crate::theme::Theme;

/// User avatar: image, initials, or placeholder icon.
pub struct Avatar {
    theme: Theme,
    name: Option<String>,
    size: ControlSize,
    image: Option<Image>,
    placeholder: IconName,
    ml: Pixels,
}

impl Avatar {
    pub fn new() -> Self {
        Self {
            theme: Theme::default(),
            name: None,
            size: ControlSize::Medium,
            image: None,
            placeholder: IconName::User,
            ml: px(0),
        }
    }

    pub fn themed(mut self, theme: &Theme) -> Self {
        self.theme = *theme;
        self
    }

    pub fn name(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }

    pub fn src(mut self, image: Image) -> Self {
        self.image = Some(image);
        self
    }

    pub fn placeholder(mut self, icon: IconName) -> Self {
        self.placeholder = icon;
        self
    }

    pub fn ml(mut self, value: Pixels) -> Self {
        self.ml = value;
        self
    }

    pub fn initials(&self) -> String {
        initials(self.name.as_deref().unwrap_or(""))
    }
}

impl Default for Avatar {
    fn default() -> Self {
        Self::new()
    }
}

impl Sizable for Avatar {
    fn with_size(mut self, size: ControlSize) -> Self {
        self.size = size;
        self
    }
}

pub(crate) fn initials(name: &str) -> String {
    let mut parts = name.split_whitespace().filter(|s| !s.is_empty());
    match (parts.next(), parts.next()) {
        (Some(a), Some(b)) => {
            let mut s = String::new();
            if let Some(c) = a.chars().next() {
                s.push(c.to_ascii_uppercase());
            }
            if let Some(c) = b.chars().next() {
                s.push(c.to_ascii_uppercase());
            }
            s
        }
        (Some(a), None) => a.chars().take(2).map(|c| c.to_ascii_uppercase()).collect(),
        _ => String::new(),
    }
}

fn identity_color(key: &str, dark: bool) -> (Hsla, Hsla, Hsla) {
    let mut h = 0u32;
    for b in key.as_bytes() {
        h = h.wrapping_mul(16777619) ^ u32::from(*b);
    }
    let hue = (h % 12) as f32 / 12.0;
    if dark {
        (
            Hsla::new(hue, 0.42, 0.32, 1.0),
            Hsla::new(hue, 0.25, 0.88, 1.0),
            Hsla::new(hue, 0.40, 0.50, 1.0),
        )
    } else {
        (
            Hsla::new(hue, 0.40, 0.82, 1.0),
            Hsla::new(hue, 0.45, 0.28, 1.0),
            Hsla::new(hue, 0.35, 0.62, 1.0),
        )
    }
}

impl IntoElement for Avatar {
    type Element = Div;

    fn into_element(self) -> Div {
        let side = px(self.size.avatar());
        let short = self.initials();
        let dark = self.theme.mode.is_dark();
        let has_image = self.image.as_ref().map(Image::has_pixels).unwrap_or(false);
        let (bg, fg, border) = if !short.is_empty() && !has_image {
            identity_color(&short, dark)
        } else {
            (
                self.theme.colors.secondary,
                self.theme.colors.secondary_foreground,
                self.theme.colors.border,
            )
        };
        let font = match self.size {
            ControlSize::Small => 9u16,
            ControlSize::Medium => 11,
            ControlSize::Large => 14,
        };

        let inner = if has_image {
            let img = self
                .image
                .unwrap()
                .themed(&self.theme)
                .size(side, side)
                .rounded(self.theme.radius.full);
            div()
                .w(side)
                .h(side)
                .overflow_hidden()
                .rounded(self.theme.radius.full)
                .child(img)
        } else if !short.is_empty() {
            div()
                .w(side)
                .h(side)
                .flex()
                .items_center()
                .justify_center()
                .child(text(short).color(fg).font(FontId::UiBold).size(font))
        } else {
            div()
                .w(side)
                .h(side)
                .flex()
                .items_center()
                .justify_center()
                .child(
                    Icon::new(self.placeholder)
                        .themed(&self.theme)
                        .color(fg)
                        .with_size(self.size),
                )
        };

        div()
            .ml(self.ml)
            .w(side)
            .h(side)
            .flex()
            .items_center()
            .justify_center()
            .rounded(self.theme.radius.full)
            .overflow_hidden()
            .bg(bg)
            .border_1()
            .border_color(border)
            .child(inner)
    }
}

/// Overlapping row of avatars with an optional overflow count.
pub struct AvatarGroup {
    theme: Theme,
    avatars: Vec<Avatar>,
    size: ControlSize,
    limit: usize,
}

impl AvatarGroup {
    pub fn new() -> Self {
        Self {
            theme: Theme::default(),
            avatars: Vec::new(),
            size: ControlSize::Medium,
            limit: 3,
        }
    }

    pub fn themed(mut self, theme: &Theme) -> Self {
        self.theme = *theme;
        self
    }

    pub fn child(mut self, avatar: Avatar) -> Self {
        self.avatars.push(avatar);
        self
    }

    pub fn children(mut self, avatars: impl IntoIterator<Item = Avatar>) -> Self {
        self.avatars.extend(avatars);
        self
    }

    pub fn limit(mut self, limit: usize) -> Self {
        self.limit = limit.max(1);
        self
    }

    pub fn len(&self) -> usize {
        self.avatars.len()
    }
}

impl Default for AvatarGroup {
    fn default() -> Self {
        Self::new()
    }
}

impl Sizable for AvatarGroup {
    fn with_size(mut self, size: ControlSize) -> Self {
        self.size = size;
        self
    }
}

impl IntoElement for AvatarGroup {
    type Element = Div;

    fn into_element(self) -> Div {
        let extra = self.avatars.len().saturating_sub(self.limit);
        let overlap = px(-(self.size.avatar() * 3 / 10));
        let mut row = div().flex().items_center();
        for (i, mut avatar) in self.avatars.into_iter().take(self.limit).enumerate() {
            avatar = avatar.themed(&self.theme).with_size(self.size);
            if i > 0 {
                avatar = avatar.ml(overlap);
            }
            row = row.child(avatar);
        }
        if extra > 0 {
            row = row.child(
                Avatar::new()
                    .themed(&self.theme)
                    .with_size(self.size)
                    .name(format!("+{extra}"))
                    .ml(overlap),
            );
        }
        row
    }
}

#[cfg(test)]
mod tests {
    use super::{Avatar, AvatarGroup, initials};

    #[test]
    fn initials_from_name() {
        assert_eq!(initials("Jane Doe"), "JD");
        assert_eq!(initials("Ada"), "AD");
        assert_eq!(initials(""), "");
        assert_eq!(Avatar::new().name("Jane Doe").initials(), "JD");
    }

    #[test]
    fn group_limit_counts_overflow() {
        let g = AvatarGroup::new()
            .limit(2)
            .child(Avatar::new().name("A A"))
            .child(Avatar::new().name("B B"))
            .child(Avatar::new().name("C C"));
        assert_eq!(g.len(), 3);
    }
}
