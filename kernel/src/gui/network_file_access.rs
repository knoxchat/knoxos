use crate::serial_println;
/// Network File Access (NFS/SMB sidebar)
///
/// Browse network shares in file manager sidebar, mount NFS/SMB
/// locations, cached credentials, bookmarks.
use alloc::string::String;
use alloc::vec::Vec;
use spin::Mutex;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum NetProtocol {
    Nfs,
    Smb,
    Sftp,
    Ftp,
    WebDav,
}

#[derive(Debug, Clone)]
pub struct NetworkLocation {
    pub protocol: NetProtocol,
    pub host: String,
    pub share: String,
    pub mount_point: Option<String>,
    pub bookmark: bool,
    pub display_name: String,
}

pub struct NetworkFileBrowser {
    pub locations: Vec<NetworkLocation>,
    pub discovered: Vec<NetworkLocation>,
    pub browsing: bool,
}

lazy_static::lazy_static! {
    static ref BROWSER: Mutex<NetworkFileBrowser> = Mutex::new(NetworkFileBrowser {
        locations: Vec::new(),
        discovered: Vec::new(),
        browsing: false,
    });
}

impl NetworkFileBrowser {
    pub fn add_bookmark(&mut self, loc: NetworkLocation) {
        serial_println!(
            "[NET_FILES] Bookmark: {:?}://{}/{}",
            loc.protocol,
            loc.host,
            loc.share
        );
        self.locations.push(loc);
    }

    pub fn discover(&mut self) {
        serial_println!("[NET_FILES] Discovering network shares...");
        self.browsing = true;
        // Would use mDNS/DNS-SD for SMB and NFSv4 service discovery
    }

    pub fn mount(&mut self, idx: usize) -> bool {
        if idx >= self.locations.len() {
            return false;
        }
        let loc = &mut self.locations[idx];
        let mount = alloc::format!("/mnt/network/{}", loc.display_name);
        serial_println!(
            "[NET_FILES] Mounting {:?}://{}/{} → {}",
            loc.protocol,
            loc.host,
            loc.share,
            mount
        );
        loc.mount_point = Some(mount);
        true
    }

    pub fn unmount(&mut self, idx: usize) {
        if idx < self.locations.len() {
            self.locations[idx].mount_point = None;
        }
    }

    pub fn bookmarks(&self) -> Vec<&NetworkLocation> {
        self.locations.iter().filter(|l| l.bookmark).collect()
    }
}

pub fn init() {
    serial_println!("[NET_FILES] Network file browser initialized");
}
