// SPDX-License-Identifier: MIT
//! Kernel clock for [`knox_gpui::PlatformDispatcher`].

/// PIT / kernel tick count. Host tests use a fake clock instead.
pub fn now_ticks() -> u64 {
    crate::interrupts::get_ticks()
}
