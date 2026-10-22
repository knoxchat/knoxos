pub(super) fn simple_hash(data: &[u8]) -> u64 {
    let mut hash: u64 = 5381;
    for &b in data {
        hash = hash.wrapping_mul(33).wrapping_add(b as u64);
    }
    hash
}

pub(super) fn rdtsc() -> u64 {
    #[cfg(target_arch = "x86_64")]
    return crate::arch_compat::read_tsc();
    #[cfg(not(target_arch = "x86_64"))]
    return 0;
}
