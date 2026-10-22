/// Embedded Font Data for KnoxOS
///
/// TTF bytes compiled into the kernel via `include_bytes!()`.
/// Source of truth for UI: `userspace/share/fonts/inter/`
/// (also installed at boot to `/usr/share/fonts/truetype/inter/`).
///
///   - UI: Inter (https://rsms.me/inter/) — SIL Open Font License 1.1
///   - Mono: Hack (https://sourcefoundry.org/hack/) — MIT
///
/// The six Inter faces are the same Latin subset used by the `os` project
/// (`os/kernel/knox-kernel/ui/fonts/`). `fontdue` rasterizes them.

/// Inter Light (300)
pub static INTER_LIGHT: &[u8] =
    include_bytes!("../../../userspace/share/fonts/inter/Inter-Light.ttf");

/// Inter Regular (400)
pub static INTER_REGULAR: &[u8] =
    include_bytes!("../../../userspace/share/fonts/inter/Inter-Regular.ttf");

/// Inter Medium (500)
pub static INTER_MEDIUM: &[u8] =
    include_bytes!("../../../userspace/share/fonts/inter/Inter-Medium.ttf");

/// Inter SemiBold (600)
pub static INTER_SEMIBOLD: &[u8] =
    include_bytes!("../../../userspace/share/fonts/inter/Inter-SemiBold.ttf");

/// Inter Bold (700)
pub static INTER_BOLD: &[u8] =
    include_bytes!("../../../userspace/share/fonts/inter/Inter-Bold.ttf");

/// Inter ExtraBold (800)
pub static INTER_EXTRABOLD: &[u8] =
    include_bytes!("../../../userspace/share/fonts/inter/Inter-ExtraBold.ttf");

/// UI Regular (alias used by the truetype registry / GSUB path)
pub static FONT_UI_DATA: &[u8] = INTER_REGULAR;

/// UI Bold
pub static FONT_UI_BOLD_DATA: &[u8] = INTER_BOLD;

/// Monospace — Hack Regular
pub static FONT_MONO_DATA: &[u8] = include_bytes!("../../fonts/Hack-Regular.ttf");

/// Inter OFL text shipped next to the faces.
pub static FONT_UI_LICENSE: &[u8] = include_bytes!("../../../userspace/share/fonts/inter/OFL.txt");

/// VFS directory matching FHS truetype layout.
pub const SYSTEM_FONT_DIR: &str = "/usr/share/fonts/truetype/inter";

pub const HACK_FONT_DIR: &str = "/usr/share/fonts/hack";

/// Install the embedded faces + OFL into the RAM VFS so fontconfig and
/// userspace see real files, not only the kernel registry.
pub fn install_system_fonts() {
    crate::vfs::ensure_directory(SYSTEM_FONT_DIR);
    crate::vfs::ensure_directory(HACK_FONT_DIR);
    let faces = [
        (
            "/usr/share/fonts/truetype/inter/Inter-Light.ttf",
            INTER_LIGHT,
        ),
        (
            "/usr/share/fonts/truetype/inter/Inter-Regular.ttf",
            INTER_REGULAR,
        ),
        (
            "/usr/share/fonts/truetype/inter/Inter-Medium.ttf",
            INTER_MEDIUM,
        ),
        (
            "/usr/share/fonts/truetype/inter/Inter-SemiBold.ttf",
            INTER_SEMIBOLD,
        ),
        ("/usr/share/fonts/truetype/inter/Inter-Bold.ttf", INTER_BOLD),
        (
            "/usr/share/fonts/truetype/inter/Inter-ExtraBold.ttf",
            INTER_EXTRABOLD,
        ),
        ("/usr/share/fonts/truetype/inter/OFL.txt", FONT_UI_LICENSE),
        ("/usr/share/fonts/hack/Hack-Regular.ttf", FONT_MONO_DATA),
    ];
    for (path, data) in faces {
        let _ = crate::vfs::write_file_dispatch(path, data);
    }
}
