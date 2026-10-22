//! QEMU fw_cfg named-file reader (I/O ports 0x510 / 0x511).
//!
//! Used to pass host timezone (`opt/knoxos.tz`) into the guest at boot.

use alloc::string::String;
use alloc::vec::Vec;

const FW_CFG_PORT_SEL: u16 = 0x510;
const FW_CFG_PORT_DATA: u16 = 0x511;
const FW_CFG_SIGNATURE: u16 = 0x0000;
const FW_CFG_FILE_DIR: u16 = 0x0019;

/// Read a named fw_cfg file (for example `opt/knoxos.tz`) as UTF-8 text.
pub fn read_file(name: &str) -> Option<String> {
    #[cfg(target_arch = "x86_64")]
    {
        read_file_x86(name)
    }
    #[cfg(not(target_arch = "x86_64"))]
    {
        let _ = name;
        None
    }
}

#[cfg(target_arch = "x86_64")]
fn read_file_x86(name: &str) -> Option<String> {
    use x86_64::instructions::port::Port;

    // SAFETY: exclusive kernel access to the architecture-defined fw_cfg ports.
    unsafe {
        let mut sel: Port<u16> = Port::new(FW_CFG_PORT_SEL);
        let mut data: Port<u8> = Port::new(FW_CFG_PORT_DATA);

        sel.write(FW_CFG_SIGNATURE);
        let mut sig = [0u8; 4];
        for b in &mut sig {
            *b = data.read();
        }
        if &sig != b"QEMU" {
            return None;
        }

        sel.write(FW_CFG_FILE_DIR);
        let mut count_bytes = [0u8; 4];
        for b in &mut count_bytes {
            *b = data.read();
        }
        let count = u32::from_be_bytes(count_bytes) as usize;
        if count == 0 || count > 512 {
            return None;
        }

        let mut select = 0u16;
        let mut size = 0u32;
        let mut found = false;
        for _ in 0..count {
            let mut size_b = [0u8; 4];
            for b in &mut size_b {
                *b = data.read();
            }
            let mut sel_b = [0u8; 2];
            for b in &mut sel_b {
                *b = data.read();
            }
            let _res0 = data.read();
            let _res1 = data.read();
            let mut fname = [0u8; 56];
            for b in &mut fname {
                *b = data.read();
            }
            let end = fname.iter().position(|&c| c == 0).unwrap_or(fname.len());
            if core::str::from_utf8(&fname[..end]).ok() == Some(name) {
                size = u32::from_be_bytes(size_b);
                select = u16::from_be_bytes(sel_b);
                found = true;
                break;
            }
        }
        if !found || size == 0 || size > 256 {
            return None;
        }

        sel.write(select);
        let mut buf = Vec::with_capacity(size as usize);
        for _ in 0..size {
            buf.push(data.read());
        }
        while buf.last() == Some(&0) || buf.last() == Some(&b'\n') || buf.last() == Some(&b'\r') {
            buf.pop();
        }
        if buf.is_empty() {
            return None;
        }
        String::from_utf8(buf).ok()
    }
}
