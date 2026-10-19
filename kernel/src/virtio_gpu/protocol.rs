/// VirtIO GPU command protocol: opcodes, formats, feature bits, headers.

// ═══════════════════════════════════════════════════════════════════════
// VIRTIO GPU CONSTANTS
// ═══════════════════════════════════════════════════════════════════════

/// VirtIO GPU command types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum VirtioGpuCmd {
    // 2D commands
    GetDisplayInfo = 0x0100,
    ResourceCreate2d = 0x0101,
    ResourceUnref = 0x0102,
    SetScanout = 0x0103,
    ResourceFlush = 0x0104,
    TransferToHost2d = 0x0105,
    ResourceAttachBacking = 0x0106,
    ResourceDetachBacking = 0x0107,
    GetCapsetInfo = 0x0108,
    GetCapset = 0x0109,
    GetEdid = 0x010A,

    // Cursor commands
    UpdateCursor = 0x0300,
    MoveCursor = 0x0301,

    // 3D commands
    CtxCreate = 0x0200,
    CtxDestroy = 0x0201,
    CtxAttachResource = 0x0202,
    CtxDetachResource = 0x0203,
    ResourceCreate3d = 0x0204,
    TransferToHost3d = 0x0205,
    TransferFromHost3d = 0x0206,
    Submit3d = 0x0207,

    // Responses
    RespOkNodata = 0x1100,
    RespOkDisplayInfo = 0x1101,
    RespOkCapsetInfo = 0x1102,
    RespOkCapset = 0x1103,
    RespOkEdid = 0x1104,
    RespErrUnspec = 0x1200,
    RespErrOutOfMemory = 0x1201,
    RespErrInvalidScanoutId = 0x1202,
    RespErrInvalidResourceId = 0x1203,
    RespErrInvalidContextId = 0x1204,
    RespErrInvalidParameter = 0x1205,
}

/// Pixel formats
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum VirtioGpuFormat {
    B8G8R8A8Unorm = 1,
    B8G8R8X8Unorm = 2,
    A8R8G8B8Unorm = 3,
    X8R8G8B8Unorm = 4,
    R8G8B8A8Unorm = 67,
    X8B8G8R8Unorm = 68,
    A8B8G8R8Unorm = 121,
    R8G8B8X8Unorm = 134,
}

/// Feature flags
pub const VIRTIO_GPU_F_VIRGL: u32 = 1;
pub const VIRTIO_GPU_F_EDID: u32 = 2;
pub const VIRTIO_GPU_F_RESOURCE_UUID: u32 = 4;
pub const VIRTIO_GPU_F_RESOURCE_BLOB: u32 = 8;
pub const VIRTIO_GPU_F_CONTEXT_INIT: u32 = 16;

/// Maximum scanouts / display heads
pub const VIRTIO_GPU_MAX_SCANOUTS: usize = 16;

/// Fence bit in `virtio_gpu_ctrl_hdr.flags`.
pub(super) const VIRTIO_GPU_FLAG_FENCE: u32 = 1;
pub(super) const GPU_RESP_OK_MIN: u32 = 0x1100;
pub(super) const GPU_RESP_OK_MAX: u32 = 0x11FF;

pub(super) const SCANOUT_W: u32 = 64;
pub(super) const SCANOUT_H: u32 = 64;
pub(super) const SCANOUT_BYTES: usize = (SCANOUT_W * SCANOUT_H * 4) as usize;
pub(super) const CURSOR_PX: u32 = 64;
pub(super) const CURSOR_BYTES: usize = (CURSOR_PX * CURSOR_PX * 4) as usize;
pub(super) const BOUNCE_CMD: usize = 0;
pub(super) const BOUNCE_RESP: usize = 512;
pub(super) const BOUNCE_RESP_LEN: u32 = 512;
pub(super) const QUEUE_CAP: u16 = 16;

pub(super) const FB_RESOURCE: u32 = 1;
pub(super) const CURSOR_RESOURCE: u32 = 2;

pub(super) fn write_hdr(buf: &mut [u8], typ: u32, flags: u32, fence: u64) {
    buf[0..4].copy_from_slice(&typ.to_le_bytes());
    buf[4..8].copy_from_slice(&flags.to_le_bytes());
    buf[8..16].copy_from_slice(&fence.to_le_bytes());
    buf[16..20].copy_from_slice(&0u32.to_le_bytes());
    buf[20] = 0;
    buf[21] = 0;
    buf[22] = 0;
    buf[23] = 0;
}

pub(super) fn resp_ok(typ: u32) -> bool {
    (GPU_RESP_OK_MIN..=GPU_RESP_OK_MAX).contains(&typ)
}
