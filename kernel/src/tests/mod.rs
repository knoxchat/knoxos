//! Kernel Unit Tests — Validates core subsystems
//!
//! Run with: `cargo test` (boots QEMU, runs tests, exits)
//!
//! Each test function exercises a specific kernel subsystem.
//!
//! Submodules stay in this declaration order so test-case discovery
//! matches the original suite (some tests share kernel globals).

#[cfg(test)]
use crate::serial_println;

#[rustfmt::skip]
mod vfs_tests;
#[rustfmt::skip]
mod allocator_tests;
#[rustfmt::skip]
mod image_tests;
#[rustfmt::skip]
mod firewall_tests;
#[rustfmt::skip]
mod password_tests;
#[rustfmt::skip]
mod scheduler_tests;
#[rustfmt::skip]
mod dns_tests;
#[rustfmt::skip]
mod theme_tests;
#[rustfmt::skip]
mod syscall_abi_tests;
#[rustfmt::skip]
mod midi_tests;
#[rustfmt::skip]
mod kpm_tests;
#[rustfmt::skip]
mod release_tests;
#[rustfmt::skip]
mod repo_server_tests;
#[rustfmt::skip]
mod bluetooth_tests;
#[rustfmt::skip]
mod tls_tests;
#[rustfmt::skip]
mod pdf_tests;
#[rustfmt::skip]
mod benchmark_tests;
#[rustfmt::skip]
mod window_tiling_tests;
#[rustfmt::skip]
mod socket_activation_tests;
#[rustfmt::skip]
mod thermal_tests;
#[rustfmt::skip]
mod media_player_tests;
#[rustfmt::skip]
mod sixel_tests;
#[rustfmt::skip]
mod ime_tests;
#[rustfmt::skip]
mod rtl_text_tests;
#[rustfmt::skip]
mod screen_record_tests;
#[rustfmt::skip]
mod vector_tests;
#[rustfmt::skip]
mod visual_test_tests;
#[rustfmt::skip]
mod pgo_tests;
#[rustfmt::skip]
mod service_manager_tests;
#[rustfmt::skip]
mod hwtest_tests;
#[rustfmt::skip]
mod gui_widget_tests;
#[rustfmt::skip]
mod fs_stress_tests;
#[rustfmt::skip]
mod network_conformance_tests;
#[rustfmt::skip]
mod security_tests;
#[rustfmt::skip]
mod accessibility_tests;
#[rustfmt::skip]
mod property_tests;
#[rustfmt::skip]
mod upgrade_tests;
#[rustfmt::skip]
mod context_layout_tests;
