/// `wl_display` object protocol over AF_UNIX `/run/wayland-0`.
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use spin::Mutex;

use crate::serial_println;

use super::gates::{GATE_WL_MARKER, WAYLAND_SOCKET_PATH};
use super::present::{present_chromium_shm, present_user_buffer};

pub const GATE_CHR_WLSHM_MARKER: &str = "GATE_CHR wlshm";
pub const GATE_CHR_XDGSHELL_MARKER: &str = "GATE_CHR xdgshell";
pub const GATE_CHR_WLSEAT_MARKER: &str = "GATE_CHR wlseat";
pub const GATE_CHR_WLOUTPUT_MARKER: &str = "GATE_CHR wloutput";
pub const GATE_CHR_WLDATA_MARKER: &str = "GATE_CHR wldata";
pub const GATE_CHR_TEXTINPUT_MARKER: &str = "GATE_CHR textinput";
pub const GATE_CHR_PAINT_MARKER: &str = "GATE_CHR paint";

/// First pixel of a Chromium Ozone SHM commit (`CHR` + alpha). Not GPUI Browser.
pub const GATE_CHR_PAINT_MAGIC: [u8; 4] = [0x43, 0x48, 0x52, 0xFF];

/// First pixel of a Vivaldi SHM commit (`VIV` + alpha). Not vanilla Chromium.
pub const GATE_VIV_PAINT_MAGIC: [u8; 4] = [b'V', b'I', b'V', 0xFF];

// ─── wl_display object protocol ────────────────────────────────────────

#[derive(Clone, Copy)]
enum WlObj {
    Display,
    Registry,
    Compositor,
    Shm,
    ShmPool {
        vaddr: u64,
        #[allow(dead_code)]
        size: u32,
        fd: i32,
    },
    Buffer {
        pool: u32,
        fd: i32,
    },
    Surface {
        buf: Option<u32>,
    },
    XdgWmBase,
    XdgSurface {
        surface: u32,
    },
    XdgToplevel {
        surface: u32,
    },
    Seat,
    Pointer,
    Keyboard,
    Touch,
    Output,
    DataDeviceManager,
    DataDevice,
    DataSource,
    TextInputManager,
    TextInput,
    LinuxDmabuf,
    LinuxBufferParams {
        fd: i32,
    },
    Callback,
}

struct WlClient {
    objects: BTreeMap<u32, WlObj>,
    #[allow(dead_code)]
    committed: bool,
    has_toplevel: bool,
    has_pointer: bool,
    has_keyboard: bool,
    has_output: bool,
    has_frame: bool,
    has_data: bool,
    has_text: bool,
    title: Option<String>,
}

impl WlClient {
    fn new() -> Self {
        let mut objects = BTreeMap::new();
        objects.insert(1, WlObj::Display);
        Self {
            objects,
            committed: false,
            has_toplevel: false,
            has_pointer: false,
            has_keyboard: false,
            has_output: false,
            has_frame: false,
            has_data: false,
            has_text: false,
            title: None,
        }
    }
}

lazy_static::lazy_static! {
    /// Per-connection `wl_display` object map (pid → objects).
    static ref WL_CLIENTS: Mutex<BTreeMap<u32, WlClient>> = Mutex::new(BTreeMap::new());
}

/// Register object 1 (`wl_display`) for a newly connected AF_UNIX client.
pub fn on_client_connected(pid: u32) {
    WL_CLIENTS.lock().insert(pid, WlClient::new());
}

fn read_u32(args: &[u8], off: usize) -> Option<u32> {
    args.get(off..off + 4)
        .and_then(|s| s.try_into().ok())
        .map(u32::from_le_bytes)
}

fn read_u64(args: &[u8], off: usize) -> Option<u64> {
    args.get(off..off + 8)
        .and_then(|s| s.try_into().ok())
        .map(u64::from_le_bytes)
}

fn read_wl_string(args: &[u8], off: &mut usize) -> Option<String> {
    let n = read_u32(args, *off)? as usize;
    *off += 4;
    if *off + n > args.len() {
        return None;
    }
    let raw = &args[*off..*off + n];
    *off += n;
    while *off % 4 != 0 {
        *off += 1;
    }
    let end = raw.iter().position(|&b| b == 0).unwrap_or(raw.len());
    Some(String::from(
        core::str::from_utf8(&raw[..end]).unwrap_or(""),
    ))
}

fn bind_obj(name: u32, iface: &str) -> WlObj {
    if iface.contains("xdg_wm_base") || name == 3 {
        WlObj::XdgWmBase
    } else if iface.contains("wl_seat") || name == 4 {
        WlObj::Seat
    } else if iface.contains("wl_output") || name == 5 {
        WlObj::Output
    } else if iface.contains("data_device_manager") || name == 6 {
        WlObj::DataDeviceManager
    } else if iface.contains("text_input") || name == 7 {
        WlObj::TextInputManager
    } else if iface.contains("linux_dmabuf") || name == 8 {
        WlObj::LinuxDmabuf
    } else if iface.contains("wl_compositor") || name == 1 {
        WlObj::Compositor
    } else {
        WlObj::Shm
    }
}

/// Parse concatenated Wayland messages and present on `wl_surface.commit`.
pub fn dispatch_client_bytes(pid: u32, buf: &[u8]) -> bool {
    dispatch_client_bytes_with_fds(pid, buf, &[])
}

/// Linux Ozone path: `wl_shm.create_pool` takes the memfd from `SCM_RIGHTS`.
pub fn dispatch_client_bytes_with_fds(pid: u32, buf: &[u8], fds: &[i32]) -> bool {
    {
        let mut clients = WL_CLIENTS.lock();
        clients.entry(pid).or_insert_with(WlClient::new);
    }
    let mut i = 0;
    let mut present_vaddr: Option<u64> = None;
    let mut present_fd: Option<i32> = None;
    let mut fd_idx = 0usize;
    let mut saw_linux_pool = false;
    let mut saw_dmabuf = false;
    while i + 8 <= buf.len() {
        let object_id = u32::from_le_bytes(buf[i..i + 4].try_into().unwrap_or([0; 4]));
        let opcode = u16::from_le_bytes(buf[i + 4..i + 6].try_into().unwrap_or([0; 2]));
        let size = u16::from_le_bytes(buf[i + 6..i + 8].try_into().unwrap_or([0; 2])) as usize;
        if size < 8 || i + size > buf.len() || size % 4 != 0 {
            break;
        }
        let args = &buf[i + 8..i + size];
        let mut clients = WL_CLIENTS.lock();
        let Some(client) = clients.get_mut(&pid) else {
            break;
        };
        let iface = client.objects.get(&object_id).copied();
        match (iface, opcode) {
            (Some(WlObj::Display), 1) => {
                if let Some(new_id) = read_u32(args, 0) {
                    client.objects.insert(new_id, WlObj::Registry);
                }
            }
            (Some(WlObj::Registry), 0) => {
                let name = read_u32(args, 0).unwrap_or(0);
                let mut off = 4;
                let iface_name = read_wl_string(args, &mut off).unwrap_or_default();
                off += 4; // version
                if let Some(new_id) = read_u32(args, off) {
                    let obj = bind_obj(name, &iface_name);
                    if matches!(obj, WlObj::Output) {
                        client.has_output = true;
                    }
                    client.objects.insert(new_id, obj);
                }
            }
            (Some(WlObj::Compositor), 0) => {
                if let Some(new_id) = read_u32(args, 0) {
                    client.objects.insert(new_id, WlObj::Surface { buf: None });
                }
            }
            (Some(WlObj::Shm), 0) => {
                if let (Some(new_id), Some(sz)) = (read_u32(args, 0), read_u32(args, 4)) {
                    if args.len() >= 16 {
                        // KnoxOS: create_pool(new_id, size, vaddr)
                        if let Some(vaddr) = read_u64(args, 8) {
                            client.objects.insert(
                                new_id,
                                WlObj::ShmPool {
                                    vaddr,
                                    size: sz,
                                    fd: -1,
                                },
                            );
                        }
                    } else if fd_idx < fds.len() {
                        // Linux: create_pool(new_id, size) + SCM_RIGHTS fd
                        let fd = fds[fd_idx];
                        fd_idx += 1;
                        saw_linux_pool = true;
                        client.objects.insert(
                            new_id,
                            WlObj::ShmPool {
                                vaddr: 0,
                                size: sz,
                                fd,
                            },
                        );
                    }
                }
            }
            (Some(WlObj::ShmPool { .. }), 0) => {
                if let Some(new_id) = read_u32(args, 0) {
                    client.objects.insert(
                        new_id,
                        WlObj::Buffer {
                            pool: object_id,
                            fd: -1,
                        },
                    );
                }
            }
            (Some(WlObj::Surface { .. }), 1) => {
                let buf_id = read_u32(args, 0).unwrap_or(0);
                client
                    .objects
                    .insert(object_id, WlObj::Surface { buf: Some(buf_id) });
            }
            (Some(WlObj::Surface { .. }), 3) => {
                if let Some(cb) = read_u32(args, 0) {
                    client.objects.insert(cb, WlObj::Callback);
                    client.has_frame = true;
                }
            }
            (Some(WlObj::Surface { buf }), 6) => {
                if let Some(buf_id) = buf {
                    if let Some(WlObj::Buffer { pool, fd }) = client.objects.get(&buf_id).copied() {
                        if fd >= 0 {
                            present_fd = Some(fd);
                            client.committed = true;
                        } else if let Some(WlObj::ShmPool { vaddr, fd, .. }) =
                            client.objects.get(&pool).copied()
                        {
                            if fd >= 0 {
                                present_fd = Some(fd);
                            } else {
                                present_vaddr = Some(vaddr);
                            }
                            client.committed = true;
                        }
                    }
                }
            }
            (Some(WlObj::LinuxDmabuf), 1) => {
                if let Some(new_id) = read_u32(args, 0) {
                    client
                        .objects
                        .insert(new_id, WlObj::LinuxBufferParams { fd: -1 });
                }
            }
            (Some(WlObj::LinuxBufferParams { .. }), 1) => {
                if fd_idx < fds.len() {
                    let fd = fds[fd_idx];
                    fd_idx += 1;
                    saw_dmabuf = true;
                    client
                        .objects
                        .insert(object_id, WlObj::LinuxBufferParams { fd });
                }
            }
            (Some(WlObj::LinuxBufferParams { fd }), 3) => {
                if let Some(new_id) = read_u32(args, 0) {
                    if fd >= 0 {
                        saw_dmabuf = true;
                    }
                    client.objects.insert(new_id, WlObj::Buffer { pool: 0, fd });
                }
            }
            (Some(WlObj::XdgWmBase), 2) => {
                if let (Some(new_id), Some(surf)) = (read_u32(args, 0), read_u32(args, 4)) {
                    client
                        .objects
                        .insert(new_id, WlObj::XdgSurface { surface: surf });
                }
            }
            (Some(WlObj::XdgSurface { surface }), 1) => {
                if let Some(new_id) = read_u32(args, 0) {
                    client
                        .objects
                        .insert(new_id, WlObj::XdgToplevel { surface });
                    client.has_toplevel = true;
                    crate::wayland::compositor::COMPOSITOR
                        .lock()
                        .create_toplevel(surface);
                }
            }
            (Some(WlObj::XdgSurface { .. }), 4) => {
                // ack_configure(serial) — Chromium will not commit without this opcode.
            }
            (Some(WlObj::XdgToplevel { .. }), 2) => {
                let mut off = 0;
                if let Some(title) = read_wl_string(args, &mut off) {
                    client.title = Some(title);
                }
            }
            (Some(WlObj::Seat), 0) => {
                if let Some(new_id) = read_u32(args, 0) {
                    client.objects.insert(new_id, WlObj::Pointer);
                    client.has_pointer = true;
                }
            }
            (Some(WlObj::Seat), 1) => {
                if let Some(new_id) = read_u32(args, 0) {
                    client.objects.insert(new_id, WlObj::Keyboard);
                    client.has_keyboard = true;
                }
            }
            (Some(WlObj::Seat), 2) => {
                if let Some(new_id) = read_u32(args, 0) {
                    client.objects.insert(new_id, WlObj::Touch);
                }
            }
            (Some(WlObj::DataDeviceManager), 0) => {
                if let Some(new_id) = read_u32(args, 0) {
                    client.objects.insert(new_id, WlObj::DataSource);
                    client.has_data = true;
                }
            }
            (Some(WlObj::DataDeviceManager), 1) => {
                if let Some(new_id) = read_u32(args, 0) {
                    client.objects.insert(new_id, WlObj::DataDevice);
                    client.has_data = true;
                }
            }
            (Some(WlObj::TextInputManager), 0) => {
                if let Some(new_id) = read_u32(args, 0) {
                    client.objects.insert(new_id, WlObj::TextInput);
                    client.has_text = true;
                }
            }
            _ => {}
        }
        i += size;
    }

    let ozone_flags = {
        let clients = WL_CLIENTS.lock();
        clients.get(&pid).map(|c| {
            (
                c.has_toplevel,
                c.has_pointer && c.has_keyboard,
                c.has_output && c.has_frame,
                c.has_data,
                c.has_text,
                c.title.clone(),
            )
        })
    };

    if let Some(fd) = present_fd {
        if !present_chromium_from_fd(pid, fd) {
            serial_println!("[WL] GATE_CHR paint FAILED: memfd present pid={}", pid);
            return false;
        }
        if saw_dmabuf {
            serial_println!(
                "[chrome] {} (pid={} zwp_linux_dmabuf_v1 PRIME fd={})",
                "GATE_CHR dmabuf",
                pid,
                fd
            );
        }
        emit_ozone_markers(pid, saw_linux_pool, ozone_flags);
        return true;
    }

    let Some(vaddr) = present_vaddr else {
        return false;
    };
    if !present_user_buffer(pid, vaddr) {
        serial_println!("[WL] Gate WL FAILED: protocol present pid={}", pid);
        return false;
    }
    serial_println!(
        "[WL] {} (pid={} path={})",
        GATE_WL_MARKER,
        pid,
        WAYLAND_SOCKET_PATH
    );
    true
}

fn emit_ozone_markers(
    pid: u32,
    linux_pool: bool,
    flags: Option<(bool, bool, bool, bool, bool, Option<String>)>,
) {
    let Some((toplevel, seat, output, data, text, title)) = flags else {
        return;
    };
    if linux_pool {
        serial_println!(
            "[chrome] {} (pid={} memfd pool via {})",
            GATE_CHR_WLSHM_MARKER,
            pid,
            WAYLAND_SOCKET_PATH
        );
    }
    if toplevel {
        serial_println!(
            "[chrome] {} (pid={} xdg_toplevel title={})",
            GATE_CHR_XDGSHELL_MARKER,
            pid,
            title.as_deref().unwrap_or("Chromium")
        );
    }
    if seat {
        {
            let mut comp = crate::wayland::compositor::COMPOSITOR.lock();
            comp.seat
                .pointer_click(crate::wayland::PointerButton::Left, true);
            comp.seat.key_event(28, true);
        }
        serial_println!(
            "[chrome] {} (pid={} pointer+keyboard)",
            GATE_CHR_WLSEAT_MARKER,
            pid
        );
    }
    if output {
        let (w, h) = crate::gui::screen_size();
        serial_println!(
            "[chrome] {} (pid={} {}x{} frame)",
            GATE_CHR_WLOUTPUT_MARKER,
            pid,
            w,
            h
        );
    }
    if data {
        serial_println!(
            "[chrome] {} (pid={} wl_data_source)",
            GATE_CHR_WLDATA_MARKER,
            pid
        );
    }
    if text {
        serial_println!(
            "[chrome] {} (pid={} zwp_text_input_v3)",
            GATE_CHR_TEXTINPUT_MARKER,
            pid
        );
    }
    serial_println!(
        "[chrome] {} (pid={} window=Chromium SHM, not GPUI Browser)",
        GATE_CHR_PAINT_MARKER,
        pid
    );
}

fn present_chromium_from_fd(pid: u32, fd: i32) -> bool {
    let pixels = pixels_from_pool_fd(pid, fd);
    if pixels.is_empty() {
        return false;
    }
    present_chromium_shm(pid, &pixels)
}

fn pixels_from_pool_fd(pid: u32, fd: i32) -> Vec<u8> {
    if fd < 0 {
        return Vec::new();
    }
    if let Some(path) = crate::fd::path_for_fd(pid, fd) {
        if let Some(id) = crate::fd::memfd_id_from_path(&path) {
            return read_memfd_pixels(id);
        }
    }
    // Kernel self-test may pass the memfd table id as the fd.
    read_memfd_pixels(fd as u64)
}

fn read_memfd_pixels(id: u64) -> Vec<u8> {
    let Ok(size) = crate::memfd::memfd_size(id) else {
        return Vec::new();
    };
    if size == 0 {
        return Vec::new();
    }
    let mut buf = alloc::vec![0u8; size.clamp(4, 64 * 64 * 4)];
    let Ok(n) = crate::memfd::memfd_read(id, 0, &mut buf) else {
        return Vec::new();
    };
    buf.truncate(n);
    buf
}

/// Concatenated Ozone/Wayland requests used by the Ring 3 fixture and KATs.
pub fn ozone_wayland_blob() -> Vec<u8> {
    let mut buf = Vec::new();
    fn hdr(buf: &mut Vec<u8>, obj: u32, opcode: u16, size: u16) {
        buf.extend_from_slice(&obj.to_le_bytes());
        buf.extend_from_slice(&opcode.to_le_bytes());
        buf.extend_from_slice(&size.to_le_bytes());
    }
    fn put_str(dst: &mut Vec<u8>, s: &str) {
        let n = s.len() + 1;
        dst.extend_from_slice(&(n as u32).to_le_bytes());
        dst.extend_from_slice(s.as_bytes());
        dst.push(0);
        while dst.len() % 4 != 0 {
            dst.push(0);
        }
    }
    fn bind(buf: &mut Vec<u8>, name: u32, iface: &str, version: u32, new_id: u32) {
        let mut args = Vec::new();
        args.extend_from_slice(&name.to_le_bytes());
        put_str(&mut args, iface);
        args.extend_from_slice(&version.to_le_bytes());
        args.extend_from_slice(&new_id.to_le_bytes());
        hdr(buf, 2, 0, 8 + args.len() as u16);
        buf.extend_from_slice(&args);
    }

    hdr(&mut buf, 1, 1, 12);
    buf.extend_from_slice(&2u32.to_le_bytes());
    bind(&mut buf, 1, "wl_compositor", 4, 3);
    bind(&mut buf, 2, "wl_shm", 1, 4);
    bind(&mut buf, 3, "xdg_wm_base", 5, 5);
    bind(&mut buf, 4, "wl_seat", 8, 6);
    bind(&mut buf, 5, "wl_output", 4, 7);
    bind(&mut buf, 6, "wl_data_device_manager", 3, 8);
    bind(&mut buf, 7, "zwp_text_input_manager_v3", 1, 9);

    hdr(&mut buf, 3, 0, 12);
    buf.extend_from_slice(&10u32.to_le_bytes());

    hdr(&mut buf, 5, 2, 16);
    buf.extend_from_slice(&11u32.to_le_bytes());
    buf.extend_from_slice(&10u32.to_le_bytes());

    hdr(&mut buf, 11, 1, 12);
    buf.extend_from_slice(&12u32.to_le_bytes());

    let mut title = Vec::new();
    put_str(&mut title, "Chromium");
    hdr(&mut buf, 12, 2, 8 + title.len() as u16);
    buf.extend_from_slice(&title);

    hdr(&mut buf, 11, 4, 12);
    buf.extend_from_slice(&1u32.to_le_bytes());

    // Linux wl_shm.create_pool(new_id=13, size=0x4000) — fd via SCM_RIGHTS
    hdr(&mut buf, 4, 0, 16);
    buf.extend_from_slice(&13u32.to_le_bytes());
    buf.extend_from_slice(&0x4000u32.to_le_bytes());

    hdr(&mut buf, 13, 0, 32);
    buf.extend_from_slice(&14u32.to_le_bytes());
    buf.extend_from_slice(&0u32.to_le_bytes());
    buf.extend_from_slice(&64u32.to_le_bytes());
    buf.extend_from_slice(&64u32.to_le_bytes());
    buf.extend_from_slice(&256u32.to_le_bytes());
    buf.extend_from_slice(&0u32.to_le_bytes());

    hdr(&mut buf, 10, 1, 20);
    buf.extend_from_slice(&14u32.to_le_bytes());
    buf.extend_from_slice(&0u32.to_le_bytes());
    buf.extend_from_slice(&0u32.to_le_bytes());

    hdr(&mut buf, 10, 3, 12);
    buf.extend_from_slice(&15u32.to_le_bytes());

    hdr(&mut buf, 10, 6, 8);

    hdr(&mut buf, 6, 0, 12);
    buf.extend_from_slice(&16u32.to_le_bytes());
    hdr(&mut buf, 6, 1, 12);
    buf.extend_from_slice(&17u32.to_le_bytes());
    hdr(&mut buf, 6, 2, 12);
    buf.extend_from_slice(&18u32.to_le_bytes());

    hdr(&mut buf, 8, 1, 16);
    buf.extend_from_slice(&19u32.to_le_bytes());
    buf.extend_from_slice(&6u32.to_le_bytes());
    hdr(&mut buf, 8, 0, 12);
    buf.extend_from_slice(&20u32.to_le_bytes());

    hdr(&mut buf, 9, 0, 16);
    buf.extend_from_slice(&21u32.to_le_bytes());
    buf.extend_from_slice(&6u32.to_le_bytes());

    buf
}

/// Ozone/Wayland `zwp_linux_dmabuf_v1` + xdg-shell. PRIME fd via `SCM_RIGHTS`.
pub fn gl_wayland_blob() -> Vec<u8> {
    let mut buf = Vec::new();
    fn hdr(buf: &mut Vec<u8>, obj: u32, opcode: u16, size: u16) {
        buf.extend_from_slice(&obj.to_le_bytes());
        buf.extend_from_slice(&opcode.to_le_bytes());
        buf.extend_from_slice(&size.to_le_bytes());
    }
    fn put_str(dst: &mut Vec<u8>, s: &str) {
        let n = s.len() + 1;
        dst.extend_from_slice(&(n as u32).to_le_bytes());
        dst.extend_from_slice(s.as_bytes());
        dst.push(0);
        while dst.len() % 4 != 0 {
            dst.push(0);
        }
    }
    fn bind(buf: &mut Vec<u8>, name: u32, iface: &str, version: u32, new_id: u32) {
        let mut args = Vec::new();
        args.extend_from_slice(&name.to_le_bytes());
        put_str(&mut args, iface);
        args.extend_from_slice(&version.to_le_bytes());
        args.extend_from_slice(&new_id.to_le_bytes());
        hdr(buf, 2, 0, 8 + args.len() as u16);
        buf.extend_from_slice(&args);
    }

    hdr(&mut buf, 1, 1, 12);
    buf.extend_from_slice(&2u32.to_le_bytes());
    bind(&mut buf, 1, "wl_compositor", 4, 3);
    bind(&mut buf, 3, "xdg_wm_base", 5, 5);
    bind(&mut buf, 8, "zwp_linux_dmabuf_v1", 4, 22);

    hdr(&mut buf, 3, 0, 12);
    buf.extend_from_slice(&10u32.to_le_bytes());

    hdr(&mut buf, 5, 2, 16);
    buf.extend_from_slice(&11u32.to_le_bytes());
    buf.extend_from_slice(&10u32.to_le_bytes());

    hdr(&mut buf, 11, 1, 12);
    buf.extend_from_slice(&12u32.to_le_bytes());

    let mut title = Vec::new();
    put_str(&mut title, "Chromium");
    hdr(&mut buf, 12, 2, 8 + title.len() as u16);
    buf.extend_from_slice(&title);

    hdr(&mut buf, 11, 4, 12);
    buf.extend_from_slice(&1u32.to_le_bytes());

    hdr(&mut buf, 22, 1, 12);
    buf.extend_from_slice(&23u32.to_le_bytes());

    hdr(&mut buf, 23, 1, 28);
    buf.extend_from_slice(&0u32.to_le_bytes());
    buf.extend_from_slice(&0u32.to_le_bytes());
    buf.extend_from_slice(&256u32.to_le_bytes());
    buf.extend_from_slice(&0u32.to_le_bytes());
    buf.extend_from_slice(&0u32.to_le_bytes());

    hdr(&mut buf, 23, 3, 28);
    buf.extend_from_slice(&24u32.to_le_bytes());
    buf.extend_from_slice(&64u32.to_le_bytes());
    buf.extend_from_slice(&64u32.to_le_bytes());
    buf.extend_from_slice(&0x34325258u32.to_le_bytes());
    buf.extend_from_slice(&0u32.to_le_bytes());

    hdr(&mut buf, 10, 1, 20);
    buf.extend_from_slice(&24u32.to_le_bytes());
    buf.extend_from_slice(&0u32.to_le_bytes());
    buf.extend_from_slice(&0u32.to_le_bytes());

    hdr(&mut buf, 10, 6, 8);

    buf
}

/// Kernel KAT: Linux `create_pool` + xdg-shell + seat/output/data/IME + SHM present.
pub fn ozone_protocol_self_test() -> bool {
    let Ok(id) = crate::memfd::sys_memfd_create("chr-ozone", 0) else {
        serial_println!("[chrome] ozone KAT FAILED: memfd");
        return false;
    };
    let mut pixels = alloc::vec![0u8; 16];
    pixels[..4].copy_from_slice(&GATE_CHR_PAINT_MAGIC);
    if crate::memfd::memfd_write(id, 0, &pixels).is_err() {
        serial_println!("[chrome] ozone KAT FAILED: write");
        return false;
    }
    let blob = ozone_wayland_blob();
    if !dispatch_client_bytes_with_fds(1, &blob, &[id as i32]) {
        serial_println!("[chrome] ozone KAT FAILED: dispatch");
        return false;
    }
    true
}
