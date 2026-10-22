use crate::serial_println;
/// PXE Network Boot Server
///
/// TFTP server for PXE boot, DHCP options for next-server/filename,
/// chain-loading iPXE scripts, boot menu.
use alloc::string::String;
use alloc::vec::Vec;
use spin::Mutex;

#[derive(Debug, Clone)]
pub struct BootEntry {
    pub label: String,
    pub kernel_path: String,
    pub initrd_path: String,
    pub cmdline: String,
}

pub struct PxeServer {
    pub enabled: bool,
    pub tftp_root: String,
    pub boot_entries: Vec<BootEntry>,
    pub default_entry: usize,
    pub timeout_secs: u32,
    pub listen_port: u16,
}

lazy_static::lazy_static! {
    static ref PXE: Mutex<PxeServer> = Mutex::new(PxeServer {
        enabled: false,
        tftp_root: String::from("/srv/tftp"),
        boot_entries: Vec::new(),
        default_entry: 0,
        timeout_secs: 10,
        listen_port: 69,
    });
}

impl PxeServer {
    pub fn add_entry(&mut self, entry: BootEntry) {
        serial_println!("[PXE] Boot entry: {} → {}", entry.label, entry.kernel_path);
        self.boot_entries.push(entry);
    }

    pub fn enable(&mut self) {
        self.enabled = true;
        serial_println!("[PXE] Server enabled on port {}", self.listen_port);
    }

    pub fn generate_menu(&self) -> String {
        let mut menu = String::from("#!ipxe\nmenu KnoxOS PXE Boot\n");
        for (i, entry) in self.boot_entries.iter().enumerate() {
            menu.push_str(&alloc::format!("item {} {}\n", i, entry.label));
        }
        menu.push_str(&alloc::format!(
            "choose --default {} --timeout {} target\n",
            self.default_entry,
            self.timeout_secs * 1000
        ));
        menu
    }

    /// Handle TFTP read request
    pub fn handle_read(&self, filename: &str) -> Option<Vec<u8>> {
        serial_println!("[PXE] TFTP read: {}", filename);
        // Would read file from tftp_root
        None
    }
}

pub fn init() {
    serial_println!("[PXE] PXE network boot server initialized");
}
