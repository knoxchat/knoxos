use alloc::string::String;

pub(super) fn generate_guid() -> u64 {
    static NEXT: core::sync::atomic::AtomicU64 =
        core::sync::atomic::AtomicU64::new(0x1234_5678_9ABC_DEF0);
    let val = NEXT.fetch_add(1, core::sync::atomic::Ordering::Relaxed);
    // Mix with TSC for uniqueness
    #[cfg(target_arch = "x86_64")]
    {
        let mut tsc: u64 = 0;
        unsafe {
            #[cfg(target_arch = "x86_64")]
            core::arch::asm!("rdtsc", out("eax") _, out("edx") _, options(nostack));
        }
        let _ = tsc;
        val.wrapping_mul(6364136223846793005).wrapping_add(1)
    }
    #[cfg(not(target_arch = "x86_64"))]
    {
        val.wrapping_mul(6364136223846793005).wrapping_add(1)
    }
}

pub(super) fn format_size(bytes: u64) -> String {
    if bytes >= 1024 * 1024 * 1024 * 1024 {
        alloc::format!("{:.1}T", bytes as f64 / (1024.0 * 1024.0 * 1024.0 * 1024.0))
    } else if bytes >= 1024 * 1024 * 1024 {
        alloc::format!("{:.1}G", bytes as f64 / (1024.0 * 1024.0 * 1024.0))
    } else if bytes >= 1024 * 1024 {
        alloc::format!("{:.1}M", bytes as f64 / (1024.0 * 1024.0))
    } else if bytes >= 1024 {
        alloc::format!("{:.1}K", bytes as f64 / 1024.0)
    } else {
        alloc::format!("{}B", bytes)
    }
}

pub(super) fn parse_size(s: &str) -> Option<u64> {
    let s = s.trim();
    if s.ends_with('G') || s.ends_with('g') {
        s[..s.len() - 1]
            .parse::<u64>()
            .ok()
            .map(|v| v * 1024 * 1024 * 1024)
    } else if s.ends_with('M') || s.ends_with('m') {
        s[..s.len() - 1]
            .parse::<u64>()
            .ok()
            .map(|v| v * 1024 * 1024)
    } else if s.ends_with('K') || s.ends_with('k') {
        s[..s.len() - 1].parse::<u64>().ok().map(|v| v * 1024)
    } else if s.ends_with('T') || s.ends_with('t') {
        s[..s.len() - 1]
            .parse::<u64>()
            .ok()
            .map(|v| v * 1024 * 1024 * 1024 * 1024)
    } else {
        s.parse::<u64>().ok()
    }
}
