use alloc::vec::Vec;

use super::state::InstallerState;
use super::types::DiskInfo;

// ─── Disk Probing ────────────────────────────────────────────────────

pub(super) fn probe_disks(st: &mut InstallerState) {
    st.disk_count = 0;

    // Try to detect AHCI/NVMe disks
    let detected = detect_disks();
    for d in &detected {
        if st.disk_count < st.disks.len() {
            st.disks[st.disk_count] = d.clone();
            st.disk_count += 1;
        }
    }

    // Fallback virtual disk for live mode
    if st.disk_count == 0 {
        let mut d = DiskInfo::empty();
        let name = b"/dev/vda";
        let model = b"VirtIO Block Device";
        d.name[..name.len()].copy_from_slice(name);
        d.name_len = name.len();
        d.model[..model.len()].copy_from_slice(model);
        d.model_len = model.len();
        d.size_mb = 32768;
        st.disks[0] = d;
        st.disk_count = 1;
    }
}

fn detect_disks() -> Vec<DiskInfo> {
    let mut out = Vec::new();
    let mut d = DiskInfo::empty();
    let name = b"/dev/nvme0n1";
    let model = b"NVMe SSD";
    d.name[..name.len()].copy_from_slice(name);
    d.name_len = name.len();
    d.model[..model.len()].copy_from_slice(model);
    d.model_len = model.len();
    d.size_mb = 0;
    out.push(d);
    out
}
