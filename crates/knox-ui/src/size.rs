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

    pub fn font(self) -> u16 {
        match self {
            Self::Small => 12,
            Self::Medium => 13,
            Self::Large => 14,
        }
    }

    pub fn indicator(self) -> i32 {
        match self {
            Self::Small => 14,
            Self::Medium => 16,
            Self::Large => 18,
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

    /// Kit icon box (14 / 16 / 24).
    pub fn icon(self) -> i32 {
        match self {
            Self::Small => 14,
            Self::Medium => 16,
            Self::Large => 24,
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
