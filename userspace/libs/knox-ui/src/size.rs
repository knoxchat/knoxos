// SPDX-License-Identifier: MIT
//! Density / disable / select traits (GPUI kit `Sizable`, `Disableable`,
//! `Selectable` roles). Design reference only (Apache-2.0); this file is new
//! `no_std` code.

/// Kit `Size::{Small, Medium, Large}` density. Named [`ControlSize`] so it
/// does not collide with `knox_gpui::Size` geometry.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ControlSize {
    XSmall,
    Small,
    #[default]
    Medium,
    Large,
}

impl ControlSize {
    pub fn height(self) -> i32 {
        match self {
            Self::XSmall => 20,
            Self::Small => 24,
            Self::Medium => 32,
            Self::Large => 40,
        }
    }

    pub fn pad_x(self, compact: bool) -> i32 {
        let base = match self {
            Self::XSmall => 6,
            Self::Small => 8,
            Self::Medium => 12,
            Self::Large => 16,
        };
        if compact { (base / 2).max(4) } else { base }
    }

    /// Matches [`crate::theme::TypeTokens`] caption / body / ui / title.
    pub fn font(self) -> u16 {
        match self {
            Self::XSmall => 14,
            Self::Small => 15,
            Self::Medium => 16,
            Self::Large => 18,
        }
    }

    pub fn indicator(self) -> i32 {
        match self {
            Self::XSmall => 10,
            Self::Small => 12,
            Self::Medium => 16,
            Self::Large => 20,
        }
    }

    /// Kit avatar diameter (20 / 24 / 32 / 48).
    pub fn avatar(self) -> i32 {
        match self {
            Self::XSmall => 20,
            Self::Small => 24,
            Self::Medium => 32,
            Self::Large => 48,
        }
    }

    /// Lucide box paired with [`Self::font`].
    pub fn icon(self) -> i32 {
        match self {
            Self::XSmall => 14,
            Self::Small => 16,
            Self::Medium => 18,
            Self::Large => 22,
        }
    }

    pub fn switch_track(self) -> (i32, i32, i32) {
        match self {
            Self::XSmall => (24, 14, 10),
            Self::Small => (28, 16, 12),
            Self::Medium => (36, 20, 16),
            Self::Large => (44, 24, 20),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::XSmall => "XSmall",
            Self::Small => "Small",
            Self::Medium => "Medium",
            Self::Large => "Large",
        }
    }
}

pub trait Sizable: Sized {
    fn with_size(self, size: ControlSize) -> Self;

    fn xsmall(self) -> Self {
        self.with_size(ControlSize::XSmall)
    }

    fn small(self) -> Self {
        self.with_size(ControlSize::Small)
    }

    fn large(self) -> Self {
        self.with_size(ControlSize::Large)
    }
}

pub trait Disableable: Sized {
    fn disabled(self, disabled: bool) -> Self;
}

pub trait Selectable: Sized {
    fn selected(self, selected: bool) -> Self;
}

#[cfg(test)]
mod tests {
    use super::ControlSize;
    use crate::theme::TypeTokens;

    #[test]
    fn control_scale_pairs_type_and_lucide() {
        assert_eq!(ControlSize::Small.font(), TypeTokens::BODY);
        assert_eq!(ControlSize::Medium.font(), TypeTokens::UI);
        assert_eq!(ControlSize::Large.font(), TypeTokens::TITLE);
        assert_eq!(ControlSize::Small.icon(), TypeTokens::ICON_CAPTION);
        assert_eq!(ControlSize::Medium.icon(), TypeTokens::ICON_UI);
        assert_eq!(ControlSize::Large.icon(), TypeTokens::ICON_TITLE);
        assert_eq!(ControlSize::XSmall.height(), 20);
        assert_eq!(ControlSize::Small.height(), 24);
        assert_eq!(ControlSize::Medium.height(), 32);
        assert_eq!(ControlSize::Large.height(), 40);
        assert_eq!(ControlSize::XSmall.font(), TypeTokens::CAPTION);
    }
}
