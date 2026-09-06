// binfmt.rs — Binary format handlers
// Supports ELF, scripts (#!), and extensible format registration

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use lazy_static::lazy_static;
use spin::Mutex;

/// Binary format types
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BinFmtType {
    Elf,    // ELF binaries
    Script, // #! scripts
    Misc,   // Miscellaneous (binfmt_misc)
}

/// Binary format magic bytes
#[derive(Debug, Clone)]
pub struct BinFmtMagic {
    pub offset: usize,
    pub magic: Vec<u8>,
    pub mask: Vec<u8>,
}

/// A registered binary format handler
#[derive(Debug, Clone)]
pub struct BinFmtHandler {
    pub name: String,
    pub fmt_type: BinFmtType,
    pub magic: Option<BinFmtMagic>,
    pub extension: Option<String>,
    pub interpreter: Option<String>,
    pub flags: u32,
    pub enabled: bool,
}

/// Flags for binfmt_misc registration
pub const BINFMT_PRESERVE_ARGV0: u32 = 0x01;
pub const BINFMT_OPEN_BINARY: u32 = 0x02;
pub const BINFMT_CREDENTIALS: u32 = 0x04;
pub const BINFMT_FIX_BINARY: u32 = 0x08;

/// Result of binary format detection
#[derive(Debug, Clone)]
pub struct BinFmtResult {
    pub handler_name: String,
    pub fmt_type: BinFmtType,
    pub interpreter: Option<String>,
    pub interpreter_args: Vec<String>,
    pub binary_path: String,
    pub flags: u32,
}

/// Script interpreter info from #! line
#[derive(Debug, Clone)]
pub struct ScriptInfo {
    pub interpreter: String,
    pub arg: Option<String>,
}

lazy_static! {
    static ref HANDLERS: Mutex<BinFmtTable> = Mutex::new(BinFmtTable::new());
}

struct BinFmtTable {
    handlers: BTreeMap<String, BinFmtHandler>,
    misc_handlers: Vec<BinFmtHandler>,
}

impl BinFmtTable {
    fn new() -> Self {
        let mut table = BinFmtTable {
            handlers: BTreeMap::new(),
            misc_handlers: Vec::new(),
        };
        table.register_defaults();
        table
    }

    fn register_defaults(&mut self) {
        // ELF handler
        self.handlers.insert(
            String::from("elf"),
            BinFmtHandler {
                name: String::from("elf"),
                fmt_type: BinFmtType::Elf,
                magic: Some(BinFmtMagic {
                    offset: 0,
                    magic: alloc::vec![0x7f, b'E', b'L', b'F'],
                    mask: alloc::vec![0xff, 0xff, 0xff, 0xff],
                }),
                extension: None,
                interpreter: None, // ELF interpreter is in the binary itself (PT_INTERP)
                flags: 0,
                enabled: true,
            },
        );

        // Script handler
        self.handlers.insert(
            String::from("script"),
            BinFmtHandler {
                name: String::from("script"),
                fmt_type: BinFmtType::Script,
                magic: Some(BinFmtMagic {
                    offset: 0,
                    magic: alloc::vec![b'#', b'!'],
                    mask: alloc::vec![0xff, 0xff],
                }),
                extension: None,
                interpreter: None, // Interpreter comes from #! line
                flags: 0,
                enabled: true,
            },
        );
    }
}

/// Detect binary format from file header
pub fn detect_format(data: &[u8], path: &str) -> Option<BinFmtResult> {
    let table = HANDLERS.lock();

    // Check ELF magic
    if data.len() >= 4 && data[0] == 0x7f && data[1] == b'E' && data[2] == b'L' && data[3] == b'F' {
        return Some(BinFmtResult {
            handler_name: String::from("elf"),
            fmt_type: BinFmtType::Elf,
            interpreter: extract_elf_interp(data),
            interpreter_args: Vec::new(),
            binary_path: String::from(path),
            flags: 0,
        });
    }

    // Check script #!
    if data.len() >= 2 && data[0] == b'#' && data[1] == b'!' {
        if let Some(script_info) = parse_shebang(data) {
            let mut args = Vec::new();
            if let Some(arg) = &script_info.arg {
                args.push(arg.clone());
            }
            return Some(BinFmtResult {
                handler_name: String::from("script"),
                fmt_type: BinFmtType::Script,
                interpreter: Some(script_info.interpreter.clone()),
                interpreter_args: args,
                binary_path: String::from(path),
                flags: 0,
            });
        }
    }

    // Check binfmt_misc handlers
    for handler in &table.misc_handlers {
        if !handler.enabled {
            continue;
        }

        // Check magic
        if let Some(magic) = &handler.magic {
            if check_magic(data, magic) {
                return Some(BinFmtResult {
                    handler_name: handler.name.clone(),
                    fmt_type: BinFmtType::Misc,
                    interpreter: handler.interpreter.clone(),
                    interpreter_args: Vec::new(),
                    binary_path: String::from(path),
                    flags: handler.flags,
                });
            }
        }

        // Check extension
        if let Some(ext) = &handler.extension {
            if path.ends_with(ext.as_str()) {
                return Some(BinFmtResult {
                    handler_name: handler.name.clone(),
                    fmt_type: BinFmtType::Misc,
                    interpreter: handler.interpreter.clone(),
                    interpreter_args: Vec::new(),
                    binary_path: String::from(path),
                    flags: handler.flags,
                });
            }
        }
    }

    None
}

/// Parse shebang line (#!)
fn parse_shebang(data: &[u8]) -> Option<ScriptInfo> {
    // Find end of first line
    let mut end = 2; // skip #!
    while end < data.len() && end < 256 && data[end] != b'\n' {
        end += 1;
    }

    let line = core::str::from_utf8(&data[2..end]).ok()?;
    let line = line.trim();

    if line.is_empty() {
        return None;
    }

    // Split into interpreter and optional argument
    let mut parts = line.splitn(2, ' ');
    let interpreter = parts.next()?.trim();
    let arg = parts.next().map(|s| String::from(s.trim()));

    if interpreter.is_empty() {
        return None;
    }

    Some(ScriptInfo {
        interpreter: String::from(interpreter),
        arg,
    })
}

/// Extract ELF interpreter path from PT_INTERP segment
fn extract_elf_interp(data: &[u8]) -> Option<String> {
    if data.len() < 64 {
        return None;
    }

    // Check ELF64
    if data[4] != 2 {
        return None; // Not 64-bit
    }

    // Read program header offset (e_phoff at offset 32)
    let phoff = u64::from_le_bytes([
        data[32], data[33], data[34], data[35], data[36], data[37], data[38], data[39],
    ]) as usize;

    // Read program header entry size (e_phentsize at offset 54)
    let phentsize = u16::from_le_bytes([data[54], data[55]]) as usize;

    // Read program header count (e_phnum at offset 56)
    let phnum = u16::from_le_bytes([data[56], data[57]]) as usize;

    // PT_INTERP = 3
    for i in 0..phnum {
        let ph_start = phoff + i * phentsize;
        if ph_start + 56 > data.len() {
            break;
        }

        let p_type = u32::from_le_bytes([
            data[ph_start],
            data[ph_start + 1],
            data[ph_start + 2],
            data[ph_start + 3],
        ]);

        if p_type == 3 {
            // PT_INTERP
            let p_offset = u64::from_le_bytes([
                data[ph_start + 8],
                data[ph_start + 9],
                data[ph_start + 10],
                data[ph_start + 11],
                data[ph_start + 12],
                data[ph_start + 13],
                data[ph_start + 14],
                data[ph_start + 15],
            ]) as usize;

            let p_filesz = u64::from_le_bytes([
                data[ph_start + 32],
                data[ph_start + 33],
                data[ph_start + 34],
                data[ph_start + 35],
                data[ph_start + 36],
                data[ph_start + 37],
                data[ph_start + 38],
                data[ph_start + 39],
            ]) as usize;

            if p_offset + p_filesz <= data.len() && p_filesz > 0 {
                // Read null-terminated string
                let end = p_offset + p_filesz;
                let mut str_end = p_offset;
                while str_end < end && data[str_end] != 0 {
                    str_end += 1;
                }
                return core::str::from_utf8(&data[p_offset..str_end])
                    .ok()
                    .map(String::from);
            }
        }
    }

    None
}

/// Check magic bytes with mask
fn check_magic(data: &[u8], magic: &BinFmtMagic) -> bool {
    if data.len() < magic.offset + magic.magic.len() {
        return false;
    }

    for i in 0..magic.magic.len() {
        let mask = if i < magic.mask.len() {
            magic.mask[i]
        } else {
            0xff
        };
        if (data[magic.offset + i] & mask) != (magic.magic[i] & mask) {
            return false;
        }
    }

    true
}

/// Register a binfmt_misc handler
pub fn register_misc(
    name: &str,
    magic: Option<BinFmtMagic>,
    extension: Option<&str>,
    interpreter: &str,
    flags: u32,
) -> Result<(), &'static str> {
    let mut table = HANDLERS.lock();

    // Check for duplicate
    for h in &table.misc_handlers {
        if h.name == name {
            return Err("Handler already registered");
        }
    }

    table.misc_handlers.push(BinFmtHandler {
        name: String::from(name),
        fmt_type: BinFmtType::Misc,
        magic,
        extension: extension.map(String::from),
        interpreter: Some(String::from(interpreter)),
        flags,
        enabled: true,
    });

    Ok(())
}

/// Unregister a binfmt_misc handler
pub fn unregister_misc(name: &str) -> Result<(), &'static str> {
    let mut table = HANDLERS.lock();
    let len_before = table.misc_handlers.len();
    table.misc_handlers.retain(|h| h.name != name);
    if table.misc_handlers.len() == len_before {
        Err("Handler not found")
    } else {
        Ok(())
    }
}

/// Enable/disable a binfmt_misc handler
pub fn set_misc_enabled(name: &str, enabled: bool) -> Result<(), &'static str> {
    let mut table = HANDLERS.lock();
    for h in &mut table.misc_handlers {
        if h.name == name {
            h.enabled = enabled;
            return Ok(());
        }
    }
    Err("Handler not found")
}

/// List all registered handlers
pub fn list_handlers() -> Vec<String> {
    let table = HANDLERS.lock();
    let mut names: Vec<String> = table.handlers.keys().cloned().collect();
    for h in &table.misc_handlers {
        names.push(h.name.clone());
    }
    names
}

/// Generate /proc/sys/fs/binfmt_misc/status
pub fn proc_binfmt_misc_status() -> String {
    let table = HANDLERS.lock();
    let mut output = String::from("enabled\n");
    for h in &table.misc_handlers {
        output.push_str(&alloc::format!(
            "{}: {} interp={} flags={:#x}\n",
            h.name,
            if h.enabled { "enabled" } else { "disabled" },
            h.interpreter.as_deref().unwrap_or("none"),
            h.flags,
        ));
    }
    output
}

/// Initialize binfmt subsystem
pub fn init() {
    // Default handlers (ELF + script) are registered in BinFmtTable::new()
    crate::serial_println!("  binfmt subsystem initialized (ELF, script #!, binfmt_misc)");
}
