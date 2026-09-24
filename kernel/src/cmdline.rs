/// Kernel Command Line — Boot parameter parsing
///
/// Parses the kernel command line string passed by the bootloader,
/// providing key=value parameters and boolean flags to all subsystems.
///
/// Supported syntax:
///   - `key=value` — named parameter
///   - `flag` — boolean flag (present = true)
///   - `key="value with spaces"` — quoted values
///   - Parameters separated by spaces
///
/// Common parameters:
///   - `root=/dev/sda1` — root filesystem device
///   - `init=/bin/init` — init program path
///   - `console=ttyS0,115200` — serial console
///   - `quiet` — suppress boot messages
///   - `debug` — enable debug output
///   - `nosmp` — disable SMP
///   - `noapic` — disable APIC
///   - `mem=512M` — limit physical memory
///   - `loglevel=7` — kernel log level
///   - `panic=30` — reboot after panic (seconds)
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use spin::Mutex;

use crate::serial_println;

/// Parsed kernel command line
pub struct KernelCmdline {
    /// Raw command line string
    pub raw: String,
    /// Key-value parameters
    pub params: BTreeMap<String, String>,
    /// Boolean flags (parameters without =value)
    pub flags: Vec<String>,
}

impl KernelCmdline {
    pub fn new() -> Self {
        Self {
            raw: String::new(),
            params: BTreeMap::new(),
            flags: Vec::new(),
        }
    }

    /// Parse a command line string
    pub fn parse(cmdline: &str) -> Self {
        let mut result = Self {
            raw: String::from(cmdline),
            params: BTreeMap::new(),
            flags: Vec::new(),
        };

        let mut chars = cmdline.chars().peekable();
        while chars.peek().is_some() {
            // Skip whitespace
            while chars.peek() == Some(&' ') {
                chars.next();
            }

            if chars.peek().is_none() {
                break;
            }

            // Read token
            let mut token = String::new();
            let mut in_quotes = false;
            while let Some(&ch) = chars.peek() {
                if ch == '"' {
                    in_quotes = !in_quotes;
                    chars.next();
                } else if ch == ' ' && !in_quotes {
                    break;
                } else {
                    token.push(ch);
                    chars.next();
                }
            }

            if token.is_empty() {
                continue;
            }

            // Parse key=value or flag
            if let Some(eq_pos) = token.find('=') {
                let key = String::from(&token[..eq_pos]);
                let value = String::from(&token[eq_pos + 1..]);
                result.params.insert(key, value);
            } else {
                result.flags.push(token);
            }
        }

        result
    }

    /// Get a parameter value by key
    pub fn get(&self, key: &str) -> Option<&str> {
        self.params.get(key).map(|s| s.as_str())
    }

    /// Get a parameter as an integer
    pub fn get_int(&self, key: &str) -> Option<i64> {
        self.get(key).and_then(|v| {
            // Support suffixes: K, M, G
            let s = v.trim();
            if s.is_empty() {
                return None;
            }
            let (num_str, multiplier) = match s.as_bytes().last() {
                Some(b'K') | Some(b'k') => (&s[..s.len() - 1], 1024i64),
                Some(b'M') | Some(b'm') => (&s[..s.len() - 1], 1024 * 1024),
                Some(b'G') | Some(b'g') => (&s[..s.len() - 1], 1024 * 1024 * 1024),
                _ => (s, 1),
            };
            num_str.parse::<i64>().ok().map(|n| n * multiplier)
        })
    }

    /// Check if a flag is present
    pub fn has_flag(&self, flag: &str) -> bool {
        self.flags.iter().any(|f| f == flag)
    }

    /// Get the root device parameter
    pub fn root_device(&self) -> Option<&str> {
        self.get("root")
    }

    /// Get the init program path
    pub fn init_path(&self) -> &str {
        self.get("init").unwrap_or("/sbin/init")
    }

    /// Check if quiet mode is enabled
    pub fn is_quiet(&self) -> bool {
        self.has_flag("quiet")
    }

    /// Check if debug mode is enabled
    pub fn is_debug(&self) -> bool {
        self.has_flag("debug")
    }

    /// Get log level (0-7, default 4)
    pub fn log_level(&self) -> u8 {
        self.get_int("loglevel").unwrap_or(4) as u8
    }

    /// Get console parameter
    pub fn console(&self) -> Option<&str> {
        self.get("console")
    }

    /// Check if SMP is disabled
    pub fn nosmp(&self) -> bool {
        self.has_flag("nosmp")
    }

    /// Check if APIC is disabled
    pub fn noapic(&self) -> bool {
        self.has_flag("noapic")
    }

    /// Get panic timeout in seconds (0 = no auto-reboot)
    pub fn panic_timeout(&self) -> u32 {
        self.get_int("panic").unwrap_or(0) as u32
    }

    /// Get memory limit in bytes (None = use all available)
    pub fn mem_limit(&self) -> Option<u64> {
        self.get_int("mem").map(|v| v as u64)
    }
}

lazy_static::lazy_static! {
    static ref CMDLINE: Mutex<KernelCmdline> = Mutex::new(KernelCmdline::new());
}

/// Parse and store the kernel command line
pub fn parse(cmdline: &str) {
    let parsed = KernelCmdline::parse(cmdline);
    serial_println!("[KnoxOS] Kernel cmdline: \"{}\"", cmdline);
    serial_println!(
        "[KnoxOS] Parsed {} params, {} flags",
        parsed.params.len(),
        parsed.flags.len()
    );
    for (k, v) in &parsed.params {
        serial_println!("[KnoxOS]   {}={}", k, v);
    }
    for f in &parsed.flags {
        serial_println!("[KnoxOS]   flag: {}", f);
    }
    *CMDLINE.lock() = parsed;
}

/// Read the firmware/QEMU fw_cfg kernel command line when present.
pub fn from_firmware() -> Option<String> {
    #[cfg(target_arch = "x86_64")]
    {
        read_fw_cfg_cmdline()
    }
    #[cfg(not(target_arch = "x86_64"))]
    {
        None
    }
}

#[cfg(target_arch = "x86_64")]
fn read_fw_cfg_cmdline() -> Option<String> {
    use x86_64::instructions::port::Port;
    const FW_CFG_PORT_SEL: u16 = 0x510;
    const FW_CFG_PORT_DATA: u16 = 0x511;
    const FW_CFG_SIGNATURE: u16 = 0x0000;
    const FW_CFG_CMDLINE_SIZE: u16 = 0x0014;
    const FW_CFG_CMDLINE_DATA: u16 = 0x0015;

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

        sel.write(FW_CFG_CMDLINE_SIZE);
        let mut size_bytes = [0u8; 4];
        for b in &mut size_bytes {
            *b = data.read();
        }
        let size = u32::from_le_bytes(size_bytes) as usize;
        if size == 0 || size > 4096 {
            return None;
        }

        sel.write(FW_CFG_CMDLINE_DATA);
        let mut buf = Vec::with_capacity(size);
        for _ in 0..size {
            buf.push(data.read());
        }
        while buf.last() == Some(&0) {
            buf.pop();
        }
        if buf.is_empty() {
            return None;
        }
        String::from_utf8(buf).ok()
    }
}

/// Get a parameter value
pub fn get(key: &str) -> Option<String> {
    CMDLINE.lock().get(key).map(String::from)
}

/// Check if a flag is present
pub fn has_flag(flag: &str) -> bool {
    CMDLINE.lock().has_flag(flag)
}

/// Check if quiet mode
pub fn is_quiet() -> bool {
    CMDLINE.lock().is_quiet()
}

/// Check if debug mode
pub fn is_debug() -> bool {
    CMDLINE.lock().is_debug()
}

/// Get log level
pub fn log_level() -> u8 {
    CMDLINE.lock().log_level()
}

/// Initialize with default empty command line
pub fn init() {
    serial_println!("[KnoxOS] Kernel command line parser initialized");
}
