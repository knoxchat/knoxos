// ─── Secure random ─────────────────────────────────────────────────────

/// Generate random bytes using RDRAND
pub fn random_bytes(buf: &mut [u8]) {
    let mut i = 0;
    while i + 8 <= buf.len() {
        let mut val: u64 = 0;
        // SAFETY: inline assembly uses the architecture instruction and listed clobbers; no unexpected memory is accessed.
        unsafe {
            #[cfg(target_arch = "x86_64")]
            core::arch::asm!(
                "rdrand {val}",
                val = out(reg) val,
            );
        }
        buf[i..i + 8].copy_from_slice(&val.to_le_bytes());
        i += 8;
    }
    if i < buf.len() {
        let mut val: u64 = 0;
        // SAFETY: inline assembly uses the architecture instruction and listed clobbers; no unexpected memory is accessed.
        unsafe {
            #[cfg(target_arch = "x86_64")]
            core::arch::asm!(
                "rdrand {val}",
                val = out(reg) val,
            );
        }
        let bytes = val.to_le_bytes();
        let remaining = buf.len() - i;
        buf[i..i + remaining].copy_from_slice(&bytes[..remaining]);
    }
}
