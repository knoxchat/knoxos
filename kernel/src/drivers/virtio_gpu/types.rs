/// VirtIO GPU public types: scanouts, resources, contexts, cursor.
use alloc::string::String;
use alloc::vec::Vec;

use super::protocol::VirtioGpuFormat;

// ═══════════════════════════════════════════════════════════════════════
// DISPLAY / SCANOUT
// ═══════════════════════════════════════════════════════════════════════

/// Display rectangle
#[derive(Debug, Clone, Copy, Default)]
pub struct Rect {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

/// Display info for a single scanout head
#[derive(Debug, Clone)]
pub struct DisplayInfo {
    pub id: u32,
    pub enabled: bool,
    pub rect: Rect,
    pub flags: u32,
}

/// EDID data for a display
#[derive(Debug, Clone)]
pub struct EdidInfo {
    pub scanout_id: u32,
    pub edid_size: u32,
    pub edid_data: Vec<u8>,
}

// ═══════════════════════════════════════════════════════════════════════
// GPU RESOURCES
// ═══════════════════════════════════════════════════════════════════════

/// 2D resource descriptor
#[derive(Debug, Clone)]
pub struct Resource2D {
    pub resource_id: u32,
    pub format: VirtioGpuFormat,
    pub width: u32,
    pub height: u32,
    pub backing: Vec<u8>,
    pub attached: bool,
}

/// 3D resource descriptor
#[derive(Debug, Clone)]
pub struct Resource3D {
    pub resource_id: u32,
    pub target: u32, // PIPE_TEXTURE_*, PIPE_BUFFER
    pub format: u32,
    pub bind: u32,
    pub width: u32,
    pub height: u32,
    pub depth: u32,
    pub array_size: u32,
    pub last_level: u32,
    pub nr_samples: u32,
    pub flags: u32,
    pub backing: Vec<u8>,
}

/// Scanout binding
#[derive(Debug, Clone)]
pub struct Scanout {
    pub id: u32,
    pub resource_id: u32,
    pub rect: Rect,
    pub enabled: bool,
}

// ═══════════════════════════════════════════════════════════════════════
// 3D RENDERING CONTEXT
// ═══════════════════════════════════════════════════════════════════════

/// Virgl 3D rendering context
#[derive(Debug, Clone)]
pub struct RenderContext {
    pub ctx_id: u32,
    pub name: String,
    pub attached_resources: Vec<u32>,
}

/// Capability set info
#[derive(Debug, Clone)]
pub struct CapsetInfo {
    pub capset_id: u32,
    pub capset_max_version: u32,
    pub capset_max_size: u32,
}

// ═══════════════════════════════════════════════════════════════════════
// CURSOR
// ═══════════════════════════════════════════════════════════════════════

/// Hardware cursor state
#[derive(Debug, Clone)]
pub struct CursorState {
    pub scanout_id: u32,
    pub x: u32,
    pub y: u32,
    pub resource_id: u32,
    pub hot_x: u32,
    pub hot_y: u32,
    pub visible: bool,
}
