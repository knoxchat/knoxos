/// Gate F8 / Gate GPU serial markers for the live VirtIO-GPU path.

pub const GATE_F8_SCANOUT: &str = "GATE_F8 virtio-gpu scanout";
pub const GATE_F8_CURSOR: &str = "GATE_F8 hw cursor";
pub const GATE_GPU_MARKER: &str = "GATE_GPU desktop scanout";
pub const GATE_GPUI_GPU_SCANOUT: &str = "GATE_GPUI gpu scanout";
/// DRM/KMS primary + cursor planes on the VGA-class VirtIO GPU (`-vga virtio`).
pub const GATE_GPU_STACK: &str = "GATE_GPU modern stack";
/// Hardware cursor plane is the pointing device (arrow uploaded, not hidden).
pub const GATE_GPU_POINTER: &str = "GATE_GPU pointer plane";
