/// Guided Installer — Full graphical OS installation wizard
///
/// Provides a step-by-step installation flow:
///   1. Welcome / language selection
///   2. Disk selection & partitioning
///   3. Filesystem formatting (ext4 / btrfs)
///   4. Bootloader installation (GRUB / systemd-boot)
///   5. User account creation
///   6. Timezone & locale selection
///   7. Installation progress & completion
///
/// Design: Nebula Depth glassmorphism theme with centered wizard card,
/// step indicator sidebar, and animated progress during installation.
mod api;
mod disks;
mod draw;
mod engine;
mod input;
mod nav;
mod render;
mod state;
mod steps;
mod types;

pub use api::{back_step, launch_installer, next_step, pick, snapshot};
pub use input::{
    handle_arrow_down, handle_arrow_up, handle_backspace, handle_char, handle_click, handle_tab,
};
pub use render::render;
pub use state::{close_installer, is_active};
pub use types::{BootloaderType, DiskInfo, FsType, InstallSnap, PartitionScheme, STEP_NAMES};
