/// `wl_display` object protocol over AF_UNIX `/run/wayland-0`.
use alloc::collections::BTreeMap;
use spin::Mutex;

use crate::serial_println;

use super::gates::{GATE_WL_MARKER, WAYLAND_SOCKET_PATH};
use super::present::present_user_buffer;

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
    },
    Buffer {
        pool: u32,
    },
    Surface {
        buf: Option<u32>,
    },
}

struct WlClient {
    objects: BTreeMap<u32, WlObj>,
    #[allow(dead_code)]
    committed: bool,
}

impl WlClient {
    fn new() -> Self {
        let mut objects = BTreeMap::new();
        objects.insert(1, WlObj::Display);
        Self {
            objects,
            committed: false,
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

fn skip_wl_string(args: &[u8], off: &mut usize) -> bool {
    let Some(n) = read_u32(args, *off) else {
        return false;
    };
    *off += 4;
    let n = n as usize;
    if *off + n > args.len() {
        return false;
    }
    *off += n;
    while *off % 4 != 0 {
        *off += 1;
    }
    true
}

/// Parse concatenated Wayland messages and present on `wl_surface.commit`.
pub fn dispatch_client_bytes(pid: u32, buf: &[u8]) -> bool {
    {
        let mut clients = WL_CLIENTS.lock();
        clients.entry(pid).or_insert_with(WlClient::new);
    }
    let mut i = 0;
    let mut present_vaddr: Option<u64> = None;
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
                // wl_display.get_registry(new_id)
                if let Some(new_id) = read_u32(args, 0) {
                    client.objects.insert(new_id, WlObj::Registry);
                }
            }
            (Some(WlObj::Registry), 0) => {
                // wl_registry.bind(name, interface, version, new_id)
                let name = read_u32(args, 0).unwrap_or(0);
                let mut off = 4;
                let _ = skip_wl_string(args, &mut off);
                off += 4; // version
                if let Some(new_id) = read_u32(args, off) {
                    let obj = if name == 1 {
                        WlObj::Compositor
                    } else {
                        WlObj::Shm
                    };
                    client.objects.insert(new_id, obj);
                }
            }
            (Some(WlObj::Compositor), 0) => {
                if let Some(new_id) = read_u32(args, 0) {
                    client.objects.insert(new_id, WlObj::Surface { buf: None });
                }
            }
            (Some(WlObj::Shm), 0) => {
                // KnoxOS: create_pool(new_id, size, vaddr)
                if let (Some(new_id), Some(sz), Some(vaddr)) =
                    (read_u32(args, 0), read_u32(args, 4), read_u64(args, 8))
                {
                    client
                        .objects
                        .insert(new_id, WlObj::ShmPool { vaddr, size: sz });
                }
            }
            (Some(WlObj::ShmPool { .. }), 0) => {
                if let Some(new_id) = read_u32(args, 0) {
                    client
                        .objects
                        .insert(new_id, WlObj::Buffer { pool: object_id });
                }
            }
            (Some(WlObj::Surface { .. }), 1) => {
                let buf_id = read_u32(args, 0).unwrap_or(0);
                client
                    .objects
                    .insert(object_id, WlObj::Surface { buf: Some(buf_id) });
            }
            (Some(WlObj::Surface { buf }), 6) => {
                if let Some(buf_id) = buf {
                    if let Some(WlObj::Buffer { pool }) = client.objects.get(&buf_id).copied() {
                        if let Some(WlObj::ShmPool { vaddr, .. }) =
                            client.objects.get(&pool).copied()
                        {
                            present_vaddr = Some(vaddr);
                            client.committed = true;
                        }
                    }
                }
            }
            _ => {}
        }
        i += size;
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
