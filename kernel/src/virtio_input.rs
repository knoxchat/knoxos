#[cfg(target_arch = "x86_64")]
use crate::arch_compat::instructions::port::Port;
#[cfg(not(target_arch = "x86_64"))]
use crate::arch_compat::instructions::port::Port;
/// VirtIO Input Device Driver
///
/// Handles `virtio-mouse-pci` in QEMU. VirtIO input devices use an event-based
/// interface. Events arrive on the eventq (queue 0) as {type, code, value} tuples
/// matching Linux input_event format.
///
/// Strategy: accumulate REL/KEY events, then on SYN_REPORT encode them as synthetic
/// PS/2 packets and push into the existing mouse queue via `add_mouse_byte()`.
/// This reuses the entire PS/2 processing pipeline (acceleration, clicks, drag,
/// double-click, scroll) without duplicating any logic.
use alloc::alloc::{Layout, alloc_zeroed};
use alloc::vec;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, AtomicI32, AtomicU32, Ordering, fence};
use spin::Mutex;

use crate::serial_println;
use crate::virtio_net::{
    VIRTIO_STATUS_ACKNOWLEDGE, VIRTIO_STATUS_DRIVER, VIRTIO_STATUS_DRIVER_OK, scan_pci_bus,
};

// ── Linux input event types ─────────────────────────────────────────
const EV_SYN: u16 = 0x00;
const EV_KEY: u16 = 0x01;
const EV_REL: u16 = 0x02;
const EV_ABS: u16 = 0x03;

// Relative axes
const REL_X: u16 = 0x00;
const REL_Y: u16 = 0x01;
const REL_WHEEL_CODE: u16 = 0x08;

// Absolute axes (used by virtio-tablet-pci)
const ABS_X: u16 = 0x00;
const ABS_Y: u16 = 0x01;

// Button codes
const BTN_LEFT: u16 = 0x110;
const BTN_RIGHT: u16 = 0x111;
const BTN_MIDDLE: u16 = 0x112;

// ── VirtIO PCI legacy transport offsets ─────────────────────────────
const VIRTIO_PCI_QUEUE_ADDRESS: u16 = 8;
const VIRTIO_PCI_QUEUE_SIZE: u16 = 12;
const VIRTIO_PCI_QUEUE_SELECT: u16 = 14;
const VIRTIO_PCI_QUEUE_NOTIFY: u16 = 16;
const VIRTIO_PCI_DEVICE_STATUS: u16 = 18;

const VIRTIO_PCI_VENDOR: u16 = 0x1AF4;
const VRING_DESC_F_WRITE: u16 = 2;
const EVENT_QUEUE_SIZE: u16 = 64;

// PCI config space I/O ports
const PCI_CONFIG_ADDRESS: u16 = 0xCF8;
const PCI_CONFIG_DATA: u16 = 0xCFC;

// ── Virtqueue structures ────────────────────────────────────────────

#[repr(C)]
#[derive(Clone, Copy)]
struct VirtqDesc {
    addr: u64,
    len: u32,
    flags: u16,
    next: u16,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct VirtioInputEvent {
    event_type: u16,
    code: u16,
    value: u32,
}

struct VirtioInputDev {
    io_base: u16,
    queue_size: u16,
    /// Single contiguous page-aligned allocation for the entire virtqueue
    /// (descriptor table + available ring + used ring per VirtIO spec §2.6)
    vring_mem: *mut u8,
    vring_layout: Layout,
    /// Pointers into vring_mem at computed offsets
    desc_base: *mut VirtqDesc,
    avail_base: *mut u8,
    used_base: *mut u8,
    event_buffers: Vec<VirtioInputEvent>,
    avail_idx: u16,
    used_idx: u16,
    ready: bool,
}

// Safety: VirtioInputDev is only accessed behind INPUT_DEVICES Mutex
unsafe impl Send for VirtioInputDev {}

/// Round `val` up to the next multiple of `align` (must be power of 2)
fn align_up(val: usize, align: usize) -> usize {
    (val + align - 1) & !(align - 1)
}

impl VirtioInputDev {
    fn new(io_base: u16) -> Self {
        Self {
            io_base,
            queue_size: 0,
            vring_mem: core::ptr::null_mut(),
            vring_layout: unsafe { Layout::from_size_align_unchecked(4096, 4096) },
            desc_base: core::ptr::null_mut(),
            avail_base: core::ptr::null_mut(),
            used_base: core::ptr::null_mut(),
            event_buffers: Vec::new(),
            avail_idx: 0,
            used_idx: 0,
            ready: false,
        }
    }

    fn avail_idx_ptr(&self) -> *mut u16 {
        unsafe { (self.avail_base as *mut u16).add(1) }
    }
    fn avail_ring_entry(&self, i: u16) -> *mut u16 {
        unsafe { (self.avail_base as *mut u16).add(2 + i as usize) }
    }
    fn used_idx_ptr(&self) -> *const u16 {
        unsafe { (self.used_base as *const u16).add(1) }
    }
    fn used_ring_id(&self, i: u16) -> u32 {
        unsafe {
            let base = (self.used_base as *const u16).add(2) as *const u32;
            core::ptr::read_volatile(base.add(i as usize * 2))
        }
    }

    /// Initialize the VirtIO input device (legacy transport)
    fn init_legacy(&mut self) -> bool {
        unsafe {
            let base = self.io_base;

            // Reset → Acknowledge → Driver
            let mut status_port: Port<u8> = Port::new(base + VIRTIO_PCI_DEVICE_STATUS);
            status_port.write(0);
            status_port.write(VIRTIO_STATUS_ACKNOWLEDGE);
            status_port.write(VIRTIO_STATUS_ACKNOWLEDGE | VIRTIO_STATUS_DRIVER);

            // Negotiate features (none needed for basic input)
            let mut feat_port: Port<u32> = Port::new(base);
            let _dev_features = feat_port.read();
            let mut guest_feat: Port<u32> = Port::new(base + 4);
            guest_feat.write(0);

            // Setup eventq (queue 0)
            let mut queue_sel: Port<u16> = Port::new(base + VIRTIO_PCI_QUEUE_SELECT);
            queue_sel.write(0);

            let mut qs_port: Port<u16> = Port::new(base + VIRTIO_PCI_QUEUE_SIZE);
            let max_qs = qs_port.read();
            if max_qs == 0 {
                serial_println!("[VIRTIO-INPUT] No event queue available");
                return false;
            }

            self.queue_size = max_qs.min(EVENT_QUEUE_SIZE);
            let qsz = self.queue_size as usize;

            // Compute virtqueue layout (VirtIO spec §2.6)
            // Layout: Descriptor Table | Available Ring | pad-to-4096 | Used Ring
            let desc_size = qsz * 16;
            let avail_size = 6 + 2 * qsz;
            let used_offset = align_up(desc_size + avail_size, 4096);
            let used_size = 6 + 8 * qsz;
            let total_size = used_offset + used_size;

            // Allocate a single contiguous page-aligned buffer for the virtqueue
            let layout =
                Layout::from_size_align(total_size, 4096).expect("[VIRTIO-INPUT] Invalid layout");
            let mem = alloc_zeroed(layout);
            if mem.is_null() {
                serial_println!("[VIRTIO-INPUT] Failed to allocate virtqueue memory");
                return false;
            }
            self.vring_mem = mem;
            self.vring_layout = layout;
            self.desc_base = mem as *mut VirtqDesc;
            self.avail_base = mem.add(desc_size);
            self.used_base = mem.add(used_offset);

            serial_println!(
                "[VIRTIO-INPUT] Virtqueue: base={:#x} qsz={} avail_off={} used_off={}",
                mem as u64,
                qsz,
                desc_size,
                used_offset
            );

            // Allocate per-descriptor event buffers
            self.event_buffers = vec![
                VirtioInputEvent {
                    event_type: 0,
                    code: 0,
                    value: 0
                };
                qsz
            ];

            // Fill all descriptors as WRITE (device→driver) pointing to event buffers
            for i in 0..qsz {
                let buf_addr = &self.event_buffers[i] as *const VirtioInputEvent as u64;
                let desc = &mut *self.desc_base.add(i);
                desc.addr = buf_addr;
                desc.len = core::mem::size_of::<VirtioInputEvent>() as u32;
                desc.flags = VRING_DESC_F_WRITE;
                desc.next = 0;
            }

            // Add all descriptors to the available ring
            for i in 0..qsz {
                core::ptr::write_volatile(self.avail_ring_entry(i as u16), i as u16);
            }
            self.avail_idx = qsz as u16;
            // flags = 0
            core::ptr::write_volatile(self.avail_base as *mut u16, 0u16);
            fence(Ordering::Release);
            core::ptr::write_volatile(self.avail_idx_ptr(), self.avail_idx);

            // Tell device the physical page frame number of the contiguous virtqueue
            let pfn = mem as u64 / 4096;
            let mut qa_port: Port<u32> = Port::new(base + VIRTIO_PCI_QUEUE_ADDRESS);
            qa_port.write(pfn as u32);

            // Mark device ready
            status_port
                .write(VIRTIO_STATUS_ACKNOWLEDGE | VIRTIO_STATUS_DRIVER | VIRTIO_STATUS_DRIVER_OK);

            self.used_idx = 0;
            self.ready = true;
            true
        }
    }

    /// Poll for completed events from the device
    fn poll_events(&mut self) {
        if !self.ready {
            return;
        }

        let qsz = self.queue_size;

        loop {
            let device_used_idx = unsafe { core::ptr::read_volatile(self.used_idx_ptr()) };
            if device_used_idx == self.used_idx {
                break;
            }

            let ring_idx = self.used_idx % qsz;
            let desc_id = self.used_ring_id(ring_idx);

            if (desc_id as usize) < self.event_buffers.len() {
                let evt = self.event_buffers[desc_id as usize];
                dispatch_input_event(evt);

                // Re-arm: put descriptor back in available ring
                let avail_slot = self.avail_idx % qsz;
                unsafe {
                    core::ptr::write_volatile(self.avail_ring_entry(avail_slot), desc_id as u16);
                    self.avail_idx = self.avail_idx.wrapping_add(1);
                    fence(Ordering::Release);
                    core::ptr::write_volatile(self.avail_idx_ptr(), self.avail_idx);
                }

                // Kick the device
                unsafe {
                    let mut notify: Port<u16> = Port::new(self.io_base + VIRTIO_PCI_QUEUE_NOTIFY);
                    notify.write(0);
                }
            }

            self.used_idx = self.used_idx.wrapping_add(1);
        }
    }
}

// ── Global state ────────────────────────────────────────────────────

static VIRTIO_INPUT_ACTIVE: AtomicBool = AtomicBool::new(false);

lazy_static::lazy_static! {
    static ref INPUT_DEVICES: Mutex<Vec<VirtioInputDev>> = Mutex::new(Vec::new());
}

// ── Accumulated event state (between SYN frames) ────────────────────

static REL_DX: AtomicI32 = AtomicI32::new(0);
static REL_DY: AtomicI32 = AtomicI32::new(0);
static ACCUM_WHEEL: AtomicI32 = AtomicI32::new(0);
static BTN_STATE: AtomicU32 = AtomicU32::new(0);

/// Absolute X coordinate from EV_ABS event (-1 = no event)
static ABS_CURSOR_X: AtomicI32 = AtomicI32::new(-1);
/// Absolute Y coordinate from EV_ABS event (-1 = no event)
static ABS_CURSOR_Y: AtomicI32 = AtomicI32::new(-1);
/// Whether we received any EV_ABS event this frame
static HAS_ABS_EVENT: AtomicBool = AtomicBool::new(false);

const BTN_LEFT_BIT: u32 = 1;
const BTN_RIGHT_BIT: u32 = 2;
const BTN_MIDDLE_BIT: u32 = 4;

/// Dispatch a single VirtIO input event, accumulating state.
fn dispatch_input_event(evt: VirtioInputEvent) {
    match evt.event_type {
        EV_ABS => match evt.code {
            ABS_X => {
                ABS_CURSOR_X.store(evt.value as i32, Ordering::Relaxed);
                HAS_ABS_EVENT.store(true, Ordering::Relaxed);
            }
            ABS_Y => {
                ABS_CURSOR_Y.store(evt.value as i32, Ordering::Relaxed);
                HAS_ABS_EVENT.store(true, Ordering::Relaxed);
            }
            _ => {}
        },
        EV_REL => match evt.code {
            REL_X => {
                REL_DX.fetch_add(evt.value as i32, Ordering::Relaxed);
            }
            REL_Y => {
                REL_DY.fetch_add(evt.value as i32, Ordering::Relaxed);
            }
            REL_WHEEL_CODE => {
                ACCUM_WHEEL.fetch_add(evt.value as i32, Ordering::Relaxed);
            }
            _ => {}
        },
        EV_KEY => {
            let pressed = evt.value != 0;
            match evt.code {
                BTN_LEFT => {
                    if pressed {
                        BTN_STATE.fetch_or(BTN_LEFT_BIT, Ordering::Relaxed);
                    } else {
                        BTN_STATE.fetch_and(!BTN_LEFT_BIT, Ordering::Relaxed);
                    }
                }
                BTN_RIGHT => {
                    if pressed {
                        BTN_STATE.fetch_or(BTN_RIGHT_BIT, Ordering::Relaxed);
                    } else {
                        BTN_STATE.fetch_and(!BTN_RIGHT_BIT, Ordering::Relaxed);
                    }
                }
                BTN_MIDDLE => {
                    if pressed {
                        BTN_STATE.fetch_or(BTN_MIDDLE_BIT, Ordering::Relaxed);
                    } else {
                        BTN_STATE.fetch_and(!BTN_MIDDLE_BIT, Ordering::Relaxed);
                    }
                }
                _ => {}
            }
        }
        EV_SYN => {
            // SYN_REPORT — commit accumulated state.
            // If we received absolute coordinates (from virtio-tablet-pci),
            // use those directly. Otherwise fall back to relative deltas
            // injected as PS/2 packets.
            if HAS_ABS_EVENT.load(Ordering::Relaxed) {
                // Absolute mode — scale from 0-32767 to screen resolution
                let abs_x = ABS_CURSOR_X.swap(-1, Ordering::Relaxed);
                let abs_y = ABS_CURSOR_Y.swap(-1, Ordering::Relaxed);
                HAS_ABS_EVENT.store(false, Ordering::Relaxed);

                let wheel = ACCUM_WHEEL.swap(0, Ordering::Relaxed);
                let btns = BTN_STATE.load(Ordering::Relaxed);
                // Discard any relative deltas accumulated alongside absolute events
                REL_DX.store(0, Ordering::Relaxed);
                REL_DY.store(0, Ordering::Relaxed);

                let (sw, sh) = crate::gui::cached_screen_size();
                let x = if abs_x >= 0 {
                    ((abs_x as i64 * sw as i64) / 32768).min(sw as i64 - 1) as i32
                } else {
                    0
                };
                let y = if abs_y >= 0 {
                    ((abs_y as i64 * sh as i64) / 32768).min(sh as i64 - 1) as i32
                } else {
                    0
                };

                let left = btns & BTN_LEFT_BIT != 0;
                let right = btns & BTN_RIGHT_BIT != 0;
                let middle = btns & BTN_MIDDLE_BIT != 0;

                crate::gui::input::set_absolute_mouse(
                    x,
                    y,
                    left,
                    right,
                    middle,
                    wheel.clamp(-127, 127) as i8,
                );
            } else {
                // Relative mode — existing PS/2 injection
                let dx = REL_DX.swap(0, Ordering::Relaxed);
                let dy = REL_DY.swap(0, Ordering::Relaxed);
                let wheel = ACCUM_WHEEL.swap(0, Ordering::Relaxed);
                let btns = BTN_STATE.load(Ordering::Relaxed);

                inject_ps2_packet(dx, dy, btns as u8, wheel.clamp(-127, 127) as i8);
            }
        }
        _ => {}
    }
}

/// Encode a mouse event as a PS/2 4-byte (IntelliMouse) packet and push
/// each byte into the mouse queue for processing by `drain_mouse_queue()`.
///
/// PS/2 packet format:
///   byte0: Y_ovf | X_ovf | Y_sign | X_sign | 1 | Mid | Right | Left
///   byte1: X movement (low 8 bits, sign in byte0 bit4)
///   byte2: Y movement (low 8 bits, sign in byte0 bit5) — PS/2 Y is inverted
///   byte3: scroll wheel (signed, IntelliMouse)
fn inject_ps2_packet(dx: i32, dy: i32, buttons: u8, scroll: i8) {
    // Clamp to PS/2 9-bit signed range
    let dx = dx.clamp(-255, 255);
    // Negate Y: VirtIO positive-down → PS/2 positive-up
    let dy_ps2 = (-dy).clamp(-255, 255);

    let mut byte0: u8 = 0x08; // bit 3 always set (PS/2 protocol)
    byte0 |= buttons & 0x07; // bits 0-2: left, right, middle
    if dx < 0 {
        byte0 |= 0x10;
    } // X sign
    if dy_ps2 < 0 {
        byte0 |= 0x20;
    } // Y sign

    crate::gui::input::add_mouse_byte(byte0);
    crate::gui::input::add_mouse_byte((dx & 0xFF) as u8);
    crate::gui::input::add_mouse_byte((dy_ps2 & 0xFF) as u8);
    crate::gui::input::add_mouse_byte(scroll as u8);
}

// ── PCI config helpers (self-contained, no dependency on virtio_net internals) ──

unsafe fn pci_config_read32(bus: u8, device: u8, func: u8, offset: u8) -> u32 {
    let address = 0x8000_0000u32
        | ((bus as u32) << 16)
        | ((device as u32) << 11)
        | ((func as u32) << 8)
        | ((offset as u32) & 0xFC);
    let mut addr_port: Port<u32> = Port::new(PCI_CONFIG_ADDRESS);
    let mut data_port: Port<u32> = Port::new(PCI_CONFIG_DATA);
    addr_port.write(address);
    data_port.read()
}

unsafe fn pci_config_read16(bus: u8, device: u8, func: u8, offset: u8) -> u16 {
    let val32 = pci_config_read32(bus, device, func, offset & 0xFC);
    ((val32 >> ((offset & 2) * 8)) & 0xFFFF) as u16
}

// ── Public API ──────────────────────────────────────────────────────

/// Initialize VirtIO input devices.
/// Scans PCI for virtio-input devices (subsystem 18), sets up virtqueues,
/// and enables IntelliMouse mode so 4-byte PS/2 packets with scroll are accepted.
pub fn init() {
    serial_println!("[VIRTIO-INPUT] Scanning for VirtIO input devices...");

    let pci_devices = scan_pci_bus();
    let mut count = 0u32;

    serial_println!(
        "[VIRTIO-INPUT] PCI scan found {} device(s) total",
        pci_devices.len()
    );

    for dev in &pci_devices {
        if dev.vendor_id != VIRTIO_PCI_VENDOR {
            continue;
        }
        // VirtIO legacy: device ID 0x1000..0x103F → subtract 0xFFF to get subsystem
        // VirtIO input has subsystem ID 18 → legacy device_id = 0x1012
        // But also verify via PCI subsystem ID register
        let subsystem = unsafe { pci_config_read16(dev.bus, dev.device, dev.function, 0x2E) };
        serial_println!(
            "[VIRTIO-INPUT] VirtIO device {:02x}:{:02x}.{} devid={:#x} subsys={} bar0={:#x}",
            dev.bus,
            dev.device,
            dev.function,
            dev.device_id,
            subsystem,
            dev.bar0
        );
        if subsystem != 18 {
            continue;
        }

        // Legacy VirtIO uses an I/O port BAR (bit 0 set). Modern-only
        // devices expose memory BARs — our legacy driver cannot handle those.
        if dev.bar0 & 1 == 0 {
            serial_println!(
                "[VIRTIO-INPUT] Device {:02x}:{:02x}.{} uses memory BAR (modern transport) — skipping",
                dev.bus,
                dev.device,
                dev.function
            );
            continue;
        }

        let io_base = (dev.bar0 & 0xFFFC) as u16;
        serial_println!(
            "[VIRTIO-INPUT] Found device at {:02x}:{:02x}.{} io_base={:#x} irq={}",
            dev.bus,
            dev.device,
            dev.function,
            io_base,
            dev.irq
        );

        // Enable I/O + Memory + Bus Master via PCI command register
        unsafe {
            let cmd_addr = 0x8000_0000u32
                | ((dev.bus as u32) << 16)
                | ((dev.device as u32) << 11)
                | ((dev.function as u32) << 8)
                | 0x04;
            let mut addr_port: Port<u32> = Port::new(PCI_CONFIG_ADDRESS);
            let mut data_port: Port<u32> = Port::new(PCI_CONFIG_DATA);
            addr_port.write(cmd_addr);
            let cmd = data_port.read();
            addr_port.write(cmd_addr);
            data_port.write(cmd | 0x07);
        }

        let mut input_dev = VirtioInputDev::new(io_base);
        if input_dev.init_legacy() {
            serial_println!("[VIRTIO-INPUT] Device {} initialized", count);

            // Initial kick to make device aware of available buffers
            unsafe {
                let mut notify: Port<u16> = Port::new(io_base + VIRTIO_PCI_QUEUE_NOTIFY);
                notify.write(0);
            }

            INPUT_DEVICES.lock().push(input_dev);
            count += 1;
        }
    }

    if count > 0 {
        // Enable IntelliMouse (4-byte packets) so scroll wheel data is processed
        crate::gui::input::MOUSE.lock().intellimouse = true;
        VIRTIO_INPUT_ACTIVE.store(true, Ordering::SeqCst);
        serial_println!("[VIRTIO-INPUT] {} input device(s) active", count);
    } else {
        serial_println!("[VIRTIO-INPUT] No VirtIO input devices found (PS/2 fallback)");
    }
}

/// Poll all VirtIO input devices for new events.
/// Called from the GUI redraw loop for low-latency input.
pub fn poll() {
    if !VIRTIO_INPUT_ACTIVE.load(Ordering::Relaxed) {
        return;
    }
    let mut devs = INPUT_DEVICES.lock();
    for dev in devs.iter_mut() {
        dev.poll_events();
    }
}

/// Returns true if any VirtIO input device is active.
pub fn is_active() -> bool {
    VIRTIO_INPUT_ACTIVE.load(Ordering::Relaxed)
}
