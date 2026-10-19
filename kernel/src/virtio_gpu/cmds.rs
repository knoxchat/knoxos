/// Live controlq / cursorq command builders and desktop present path.
use core::sync::atomic::{AtomicBool, Ordering};

use crate::serial_println;

use super::gates::{GATE_F8_CURSOR, GATE_F8_SCANOUT, GATE_GPU_MARKER};
use super::protocol::{
    BOUNCE_RESP, CURSOR_BYTES, CURSOR_PX, CURSOR_RESOURCE, FB_RESOURCE, SCANOUT_BYTES, SCANOUT_H,
    SCANOUT_W, VIRTIO_GPU_FLAG_FENCE, VIRTIO_GPU_MAX_SCANOUTS, VirtioGpuCmd, VirtioGpuFormat,
    resp_ok, write_hdr,
};
use super::vq::{LIVE_GPU, LiveGpu};

static SCANOUT_LIVE: AtomicBool = AtomicBool::new(false);
static CURSOR_LIVE: AtomicBool = AtomicBool::new(false);
static DESKTOP_SCANOUT: AtomicBool = AtomicBool::new(false);

fn fill_pattern(phys: u64, bytes: usize, color: u32) {
    let virt = crate::vmm::phys_to_virt(phys);
    let px = color.to_le_bytes();
    // SAFETY: backing pages are exclusive DMA memory.
    unsafe {
        let p = virt as *mut u8;
        let mut off = 0;
        while off + 4 <= bytes {
            core::ptr::copy_nonoverlapping(px.as_ptr(), p.add(off), 4);
            off += 4;
        }
    }
}

fn gpu_cmd_create_2d(gpu: &mut LiveGpu, id: u32, w: u32, h: u32) -> bool {
    let fence = gpu.next_fence();
    let mut cmd = [0u8; 40];
    write_hdr(
        &mut cmd,
        VirtioGpuCmd::ResourceCreate2d as u32,
        VIRTIO_GPU_FLAG_FENCE,
        fence,
    );
    cmd[24..28].copy_from_slice(&id.to_le_bytes());
    cmd[28..32].copy_from_slice(&(VirtioGpuFormat::B8G8R8A8Unorm as u32).to_le_bytes());
    cmd[32..36].copy_from_slice(&w.to_le_bytes());
    cmd[36..40].copy_from_slice(&h.to_le_bytes());
    gpu.submit_ctrl(&cmd).map(resp_ok).unwrap_or(false)
}

fn gpu_cmd_attach(gpu: &mut LiveGpu, id: u32, phys: u64, len: u32) -> bool {
    let fence = gpu.next_fence();
    let mut cmd = [0u8; 48];
    write_hdr(
        &mut cmd,
        VirtioGpuCmd::ResourceAttachBacking as u32,
        VIRTIO_GPU_FLAG_FENCE,
        fence,
    );
    cmd[24..28].copy_from_slice(&id.to_le_bytes());
    cmd[28..32].copy_from_slice(&1u32.to_le_bytes());
    cmd[32..40].copy_from_slice(&phys.to_le_bytes());
    cmd[40..44].copy_from_slice(&len.to_le_bytes());
    cmd[44..48].copy_from_slice(&0u32.to_le_bytes());
    gpu.submit_ctrl(&cmd).map(resp_ok).unwrap_or(false)
}

fn gpu_cmd_set_scanout(gpu: &mut LiveGpu, w: u32, h: u32, resource: u32) -> bool {
    let fence = gpu.next_fence();
    let mut cmd = [0u8; 48];
    write_hdr(
        &mut cmd,
        VirtioGpuCmd::SetScanout as u32,
        VIRTIO_GPU_FLAG_FENCE,
        fence,
    );
    cmd[24..28].copy_from_slice(&0u32.to_le_bytes()); // x
    cmd[28..32].copy_from_slice(&0u32.to_le_bytes()); // y
    cmd[32..36].copy_from_slice(&w.to_le_bytes());
    cmd[36..40].copy_from_slice(&h.to_le_bytes());
    cmd[40..44].copy_from_slice(&0u32.to_le_bytes()); // scanout_id
    cmd[44..48].copy_from_slice(&resource.to_le_bytes());
    gpu.submit_ctrl(&cmd).map(resp_ok).unwrap_or(false)
}

fn gpu_cmd_transfer(gpu: &mut LiveGpu, id: u32, w: u32, h: u32) -> bool {
    let fence = gpu.next_fence();
    let mut cmd = [0u8; 56];
    write_hdr(
        &mut cmd,
        VirtioGpuCmd::TransferToHost2d as u32,
        VIRTIO_GPU_FLAG_FENCE,
        fence,
    );
    cmd[24..28].copy_from_slice(&0u32.to_le_bytes());
    cmd[28..32].copy_from_slice(&0u32.to_le_bytes());
    cmd[32..36].copy_from_slice(&w.to_le_bytes());
    cmd[36..40].copy_from_slice(&h.to_le_bytes());
    cmd[40..48].copy_from_slice(&0u64.to_le_bytes());
    cmd[48..52].copy_from_slice(&id.to_le_bytes());
    cmd[52..56].copy_from_slice(&0u32.to_le_bytes());
    gpu.submit_ctrl(&cmd).map(resp_ok).unwrap_or(false)
}

fn gpu_cmd_flush(gpu: &mut LiveGpu, id: u32, w: u32, h: u32) -> bool {
    let fence = gpu.next_fence();
    let mut cmd = [0u8; 48];
    write_hdr(
        &mut cmd,
        VirtioGpuCmd::ResourceFlush as u32,
        VIRTIO_GPU_FLAG_FENCE,
        fence,
    );
    cmd[24..28].copy_from_slice(&0u32.to_le_bytes());
    cmd[28..32].copy_from_slice(&0u32.to_le_bytes());
    cmd[32..36].copy_from_slice(&w.to_le_bytes());
    cmd[36..40].copy_from_slice(&h.to_le_bytes());
    cmd[40..44].copy_from_slice(&id.to_le_bytes());
    cmd[44..48].copy_from_slice(&0u32.to_le_bytes());
    gpu.submit_ctrl(&cmd).map(resp_ok).unwrap_or(false)
}

fn gpu_cmd_get_display(gpu: &mut LiveGpu) -> Option<(u32, u32)> {
    let fence = gpu.next_fence();
    let mut cmd = [0u8; 24];
    write_hdr(
        &mut cmd,
        VirtioGpuCmd::GetDisplayInfo as u32,
        VIRTIO_GPU_FLAG_FENCE,
        fence,
    );
    let typ = gpu.submit_ctrl(&cmd)?;
    if typ != VirtioGpuCmd::RespOkDisplayInfo as u32 && !resp_ok(typ) {
        serial_println!("[VIRTIO-GPU] GET_DISPLAY_INFO resp={:#x}", typ);
        return None;
    }
    let bounce = gpu.bounce_virt();
    let mut w = 0u32;
    let mut h = 0u32;
    // SAFETY: response is 16 display entries after the 24-byte header.
    unsafe {
        let base = (bounce as *const u8).add(BOUNCE_RESP + 24);
        for i in 0..VIRTIO_GPU_MAX_SCANOUTS {
            let e = base.add(i * 24);
            let mut tmp = [0u8; 4];
            core::ptr::copy_nonoverlapping(e.add(8), tmp.as_mut_ptr(), 4);
            let width = u32::from_le_bytes(tmp);
            core::ptr::copy_nonoverlapping(e.add(12), tmp.as_mut_ptr(), 4);
            let height = u32::from_le_bytes(tmp);
            core::ptr::copy_nonoverlapping(e.add(16), tmp.as_mut_ptr(), 4);
            let enabled = u32::from_le_bytes(tmp);
            if enabled != 0 && width > 0 && height > 0 {
                w = width;
                h = height;
                break;
            }
            if w == 0 && width > 0 && height > 0 {
                w = width;
                h = height;
            }
        }
    }
    if w == 0 { None } else { Some((w, h)) }
}

pub(super) fn gpu_update_cursor_cmd(
    gpu: &mut LiveGpu,
    x: u32,
    y: u32,
    resource: u32,
    hot_x: u32,
    hot_y: u32,
) -> bool {
    let mut cmd = [0u8; 56];
    write_hdr(&mut cmd, VirtioGpuCmd::UpdateCursor as u32, 0, 0);
    cmd[24..28].copy_from_slice(&0u32.to_le_bytes());
    cmd[28..32].copy_from_slice(&x.to_le_bytes());
    cmd[32..36].copy_from_slice(&y.to_le_bytes());
    cmd[36..40].copy_from_slice(&0u32.to_le_bytes());
    cmd[40..44].copy_from_slice(&resource.to_le_bytes());
    cmd[44..48].copy_from_slice(&hot_x.to_le_bytes());
    cmd[48..52].copy_from_slice(&hot_y.to_le_bytes());
    cmd[52..56].copy_from_slice(&0u32.to_le_bytes());
    gpu.submit_cursor_cmd(&cmd)
}

fn gpu_move_cursor_cmd(gpu: &mut LiveGpu, x: u32, y: u32) -> bool {
    let mut cmd = [0u8; 56];
    write_hdr(&mut cmd, VirtioGpuCmd::MoveCursor as u32, 0, 0);
    cmd[24..28].copy_from_slice(&0u32.to_le_bytes());
    cmd[28..32].copy_from_slice(&x.to_le_bytes());
    cmd[32..36].copy_from_slice(&y.to_le_bytes());
    gpu.submit_cursor_cmd(&cmd)
}

pub(super) fn scanout_self_test(gpu: &mut LiveGpu) -> bool {
    fill_pattern(gpu.scanout_phys, SCANOUT_BYTES, 0xFF20_C997);
    fill_pattern(gpu.cursor_phys, CURSOR_BYTES, 0xFFFF_FFFF);

    let display = gpu_cmd_get_display(gpu);
    if let Some((w, h)) = display {
        serial_println!("[VIRTIO-GPU] display {}x{}", w, h);
    } else {
        serial_println!("[VIRTIO-GPU] GET_DISPLAY_INFO had no mode; continuing");
    }

    if !gpu_cmd_create_2d(gpu, FB_RESOURCE, SCANOUT_W, SCANOUT_H) {
        serial_println!("[VIRTIO-GPU] RESOURCE_CREATE_2D scanout failed");
        return false;
    }
    if !gpu_cmd_attach(gpu, FB_RESOURCE, gpu.scanout_phys, SCANOUT_BYTES as u32) {
        serial_println!("[VIRTIO-GPU] ATTACH_BACKING scanout failed");
        return false;
    }
    if !gpu_cmd_transfer(gpu, FB_RESOURCE, SCANOUT_W, SCANOUT_H) {
        serial_println!("[VIRTIO-GPU] TRANSFER_TO_HOST_2D failed");
        return false;
    }
    if !gpu_cmd_set_scanout(gpu, SCANOUT_W, SCANOUT_H, FB_RESOURCE) {
        serial_println!("[VIRTIO-GPU] SET_SCANOUT failed");
        return false;
    }
    if !gpu_cmd_flush(gpu, FB_RESOURCE, SCANOUT_W, SCANOUT_H) {
        serial_println!("[VIRTIO-GPU] RESOURCE_FLUSH failed");
        return false;
    }
    SCANOUT_LIVE.store(true, Ordering::SeqCst);
    serial_println!("[VIRTIO-GPU] {}", GATE_F8_SCANOUT);

    if !gpu_cmd_create_2d(gpu, CURSOR_RESOURCE, CURSOR_PX, CURSOR_PX) {
        serial_println!("[VIRTIO-GPU] cursor CREATE_2D failed");
        return true;
    }
    if !gpu_cmd_attach(gpu, CURSOR_RESOURCE, gpu.cursor_phys, CURSOR_BYTES as u32) {
        serial_println!("[VIRTIO-GPU] cursor ATTACH_BACKING failed");
        return true;
    }
    let _ = gpu_cmd_transfer(gpu, CURSOR_RESOURCE, CURSOR_PX, CURSOR_PX);
    if gpu_update_cursor_cmd(gpu, 16, 16, CURSOR_RESOURCE, 0, 0) {
        CURSOR_LIVE.store(true, Ordering::SeqCst);
        serial_println!("[VIRTIO-GPU] {}", GATE_F8_CURSOR);
        // Hide the empty 64×64 overlay. The visible pointer is the software
        // arrow on the VGA framebuffer (`-vga std` / cocoa show-cursor=off).
        let _ = gpu_update_cursor_cmd(gpu, 0, 0, 0, 0, 0);
    } else {
        serial_println!("[VIRTIO-GPU] UPDATE_CURSOR failed");
    }
    true
}

/// True once controlq SET_SCANOUT + FLUSH completed against the host.
pub fn scanout_live() -> bool {
    SCANOUT_LIVE.load(Ordering::Relaxed)
}

/// True once cursorq UPDATE_CURSOR completed against the host.
pub fn cursor_live() -> bool {
    CURSOR_LIVE.load(Ordering::Relaxed)
}

/// Copy a region of the software desktop framebuffer into the live VirtIO
/// scanout resource and flush it to the host (Gate GPU).
pub fn present_desktop_fb(
    src: &[u8],
    width: usize,
    height: usize,
    pitch: usize,
    x: i32,
    y: i32,
    w: u32,
    h: u32,
) {
    if !SCANOUT_LIVE.load(Ordering::Relaxed) {
        return;
    }
    let x0 = x.max(0) as usize;
    let y0 = y.max(0) as usize;
    let x1 = ((x + w as i32).max(0) as usize).min(width);
    let y1 = ((y + h as i32).max(0) as usize).min(height);
    if x0 >= x1 || y0 >= y1 || src.len() < 4 {
        return;
    }
    let rw = (x1 - x0).max(1);
    let rh = (y1 - y0).max(1);
    let mut gpu_lock = LIVE_GPU.lock();
    let Some(ref mut gpu) = *gpu_lock else {
        return;
    };
    if !gpu.ready {
        return;
    }
    let virt = crate::vmm::phys_to_virt(gpu.scanout_phys);
    // SAFETY: scanout backing is exclusive DMA memory of SCANOUT_BYTES.
    unsafe {
        let dst = virt as *mut u8;
        for sy in 0..SCANOUT_H as usize {
            let src_y = y0 + sy * rh / SCANOUT_H as usize;
            if src_y >= height {
                break;
            }
            for sx in 0..SCANOUT_W as usize {
                let src_x = x0 + sx * rw / SCANOUT_W as usize;
                if src_x >= width {
                    break;
                }
                let so = src_y * pitch + src_x * 4;
                let doff = (sy * SCANOUT_W as usize + sx) * 4;
                if so + 4 <= src.len() && doff + 4 <= SCANOUT_BYTES {
                    core::ptr::copy_nonoverlapping(src.as_ptr().add(so), dst.add(doff), 4);
                }
            }
        }
    }
    let _ = gpu_cmd_transfer(gpu, FB_RESOURCE, SCANOUT_W, SCANOUT_H);
    let _ = gpu_cmd_flush(gpu, FB_RESOURCE, SCANOUT_W, SCANOUT_H);
    if !DESKTOP_SCANOUT.swap(true, Ordering::SeqCst) {
        serial_println!("[VIRTIO-GPU] {}", GATE_GPU_MARKER);
    }
}

/// Move the host cursor plane. No-op until Gate F8 cursor is live.
pub fn hw_move_cursor(x: i32, y: i32) {
    if !CURSOR_LIVE.load(Ordering::Relaxed) {
        return;
    }
    let x = x.max(0) as u32;
    let y = y.max(0) as u32;
    if let Some(ref mut gpu) = *LIVE_GPU.lock() {
        if gpu.ready {
            let _ = gpu_move_cursor_cmd(gpu, x, y);
        }
    }
}
