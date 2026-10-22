// SPDX-License-Identifier: MIT
//! Density / disable / select traits (GPUI kit `Sizable`, `Disableable`,
//! `Selectable` roles). Design reference only (Apache-2.0); this file is new
//! `no_std` code.

/// Kit `Size::{Small, Medium, Large}` density. Named [`ControlSize`] so it
/// does not collide with `knox_gpui::Size` geometry.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ControlSize {
    Small,
    #[default]
    Medium,
    Large,
}

impl ControlSize {
    pub fn height(self) -> i32 {
        match self {
            Self::Small => 24,
            Self::Medium => 32,
            Self::Large => 40,
        }
    }

    pub fn pad_x(self, compact: bool) -> i32 {
        let base = match self {
            Self::Small => 8,
            Self::Medium => 12,
            Self::Large => 16,
        };
        if compact { (base / 2).max(4) } else { base }
    }

    /// Matches [`crate::theme::TypeTokens`] body / ui / title.
    pub fn font(self) -> u16 {
        match self {
            Self::Small => 15,
            Self::Medium => 16,
            Self::Large => 18,
        }
    }

    pub fn indicator(self) -> i32 {
        match self {
            Self::Small => 12,
            Self::Medium => 16,
            Self::Large => 20,
        }
    }

    /// Kit avatar diameter (24 / 32 / 48).
    pub fn avatar(self) -> i32 {
        match self {
            Self::Small => 24,
            Self::Medium => 32,
            Self::Large => 48,
        }
    }

    /// Lucide box paired with [`Self::font`] (14 / 16 / 24).
    pub fn icon(self) -> i32 {
        match self {
            Self::Small => 16,
            Self::Medium => 18,
            Self::Large => 22,
        }
    }

    pub fn switch_track(self) -> (i32, i32, i32) {
        match self {
            Self::Small => (28, 16, 12),
            Self::Medium => (36, 20, 16),
            Self::Large => (44, 24, 20),
        }
    }
}

pub trait Sizable: Sized {
    fn with_size(self, size: ControlSize) -> Self;

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
        assert_eq!(ControlSize::Small.height(), 24);
        assert_eq!(ControlSize::Medium.height(), 32);
        assert_eq!(ControlSize::Large.height(), 40);
    }
}
