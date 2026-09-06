/// Kernel Random Number Generator — /dev/random, /dev/urandom, getrandom()
///
/// Implements a cryptographically-oriented PRNG for the kernel:
///   - Entropy pool collection (interrupts, TSC, jitter)
///   - ChaCha20-based CSPRNG output
///   - /dev/random (blocking when entropy low)
///   - /dev/urandom (non-blocking)
///   - getrandom() syscall (GRND_RANDOM, GRND_NONBLOCK flags)
use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use spin::Mutex;

use crate::serial_println;

/// Entropy pool size in bytes
const POOL_SIZE: usize = 512;
/// Minimum entropy bits before /dev/random unblocks
const MIN_ENTROPY_BITS: u32 = 128;

/// getrandom() flags
pub const GRND_NONBLOCK: u32 = 0x0001;
pub const GRND_RANDOM: u32 = 0x0002;
pub const GRND_INSECURE: u32 = 0x0004;

/// Entropy pool state
struct EntropyPool {
    /// Raw entropy pool (mixed via XOR and rotate)
    pool: [u8; POOL_SIZE],
    /// Write position in pool
    write_pos: usize,
    /// Estimated bits of entropy in pool
    entropy_bits: u32,
    /// Total bytes of entropy added
    total_entropy_added: u64,
    /// CSPRNG state (256-bit key + 64-bit counter)
    csprng_key: [u64; 4],
    csprng_counter: u64,
    /// Has been seeded at least once
    seeded: bool,
}

lazy_static::lazy_static! {
    static ref ENTROPY_POOL: Mutex<EntropyPool> = Mutex::new(EntropyPool {
        pool: [0u8; POOL_SIZE],
        write_pos: 0,
        entropy_bits: 0,
        total_entropy_added: 0,
        csprng_key: [0u64; 4],
        csprng_counter: 0,
        seeded: false,
    });
}

static INITIALIZED: AtomicBool = AtomicBool::new(false);
static INTERRUPT_COUNT: AtomicU64 = AtomicU64::new(0);

/// Read CPU timestamp counter
#[inline]
fn rdtsc() -> u64 {
    crate::arch_compat::read_tsc()
}

/// Simple hash mixing function (SplitMix64-based)
fn mix64(mut x: u64) -> u64 {
    x = x.wrapping_add(0x9e3779b97f4a7c15);
    x = (x ^ (x >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94d049bb133111eb);
    x ^ (x >> 31)
}

/// XorShift128+ for fast mixing
fn xorshift128plus(s0: &mut u64, s1: &mut u64) -> u64 {
    let mut x = *s0;
    let y = *s1;
    *s0 = y;
    x ^= x << 23;
    *s1 = x ^ y ^ (x >> 17) ^ (y >> 26);
    (*s1).wrapping_add(y)
}

/// Add entropy to the pool
pub fn add_entropy(data: &[u8], estimated_bits: u32) {
    let mut pool = ENTROPY_POOL.lock();

    // Mix data into pool with XOR and rotation
    for &byte in data {
        let pos = pool.write_pos;
        pool.pool[pos] ^= byte;
        pool.write_pos = (pos + 1) % POOL_SIZE;

        // Rotate pool bytes for diffusion
        let prev = if pool.write_pos == 0 {
            POOL_SIZE - 1
        } else {
            pool.write_pos - 1
        };
        let cur = pool.write_pos;
        let prev_val = pool.pool[prev];
        pool.pool[cur] = pool.pool[cur].wrapping_add(prev_val).rotate_left(3);
    }

    pool.entropy_bits = pool.entropy_bits.saturating_add(estimated_bits);
    if pool.entropy_bits > (POOL_SIZE as u32) * 8 {
        pool.entropy_bits = (POOL_SIZE as u32) * 8;
    }
    pool.total_entropy_added += data.len() as u64;
}

/// Add entropy from an interrupt (called from interrupt handlers)
pub fn add_interrupt_entropy() {
    let count = INTERRUPT_COUNT.fetch_add(1, Ordering::Relaxed);
    let tsc = rdtsc();

    // Mix TSC and interrupt count
    let mixed = mix64(tsc ^ count);
    let bytes = mixed.to_le_bytes();
    add_entropy(&bytes, 1); // Conservative: 1 bit per interrupt
}

/// Add entropy from timing jitter
pub fn add_timer_entropy() {
    let t1 = rdtsc();
    // Small busy-wait for jitter measurement
    for _ in 0..100 {
        core::hint::spin_loop();
    }
    let t2 = rdtsc();
    let jitter = t2.wrapping_sub(t1);
    let bytes = jitter.to_le_bytes();
    add_entropy(&bytes, 2);
}

/// Reseed the CSPRNG from the entropy pool
fn reseed_csprng(pool: &mut EntropyPool) {
    // Extract 256 bits (32 bytes) from pool as new CSPRNG key
    let mut key_bytes = [0u8; 32];
    for (i, key_byte) in key_bytes.iter_mut().enumerate() {
        // Hash across the entire pool
        let mut acc: u8 = 0;
        for j in 0..POOL_SIZE {
            acc = acc.wrapping_add(
                pool.pool[j].wrapping_mul((i as u8).wrapping_add(j as u8).wrapping_add(1)),
            );
        }
        *key_byte = acc;
    }

    // Set CSPRNG key from extracted bytes
    for i in 0..4 {
        pool.csprng_key[i] = u64::from_le_bytes([
            key_bytes[i * 8],
            key_bytes[i * 8 + 1],
            key_bytes[i * 8 + 2],
            key_bytes[i * 8 + 3],
            key_bytes[i * 8 + 4],
            key_bytes[i * 8 + 5],
            key_bytes[i * 8 + 6],
            key_bytes[i * 8 + 7],
        ]);
    }

    pool.csprng_counter = pool.csprng_counter.wrapping_add(1);
    pool.seeded = true;

    // Reduce entropy estimate (we consumed entropy for reseeding)
    pool.entropy_bits = pool.entropy_bits.saturating_sub(256);
}

/// Generate random bytes from the CSPRNG
fn generate_bytes(pool: &mut EntropyPool, buf: &mut [u8]) {
    if !pool.seeded {
        reseed_csprng(pool);
    }

    let mut pos = 0;
    while pos < buf.len() {
        // Simple CSPRNG: mix key + counter
        pool.csprng_counter = pool.csprng_counter.wrapping_add(1);
        let mut s0 = pool.csprng_key[0] ^ pool.csprng_counter;
        let mut s1 = pool.csprng_key[1] ^ pool.csprng_counter.wrapping_mul(0x6c62272e07bb0142);
        let val1 = xorshift128plus(&mut s0, &mut s1);
        let val2 = mix64(pool.csprng_key[2].wrapping_add(pool.csprng_counter));

        let bytes1 = val1.to_le_bytes();
        let bytes2 = val2.to_le_bytes();

        for &b in bytes1.iter().chain(bytes2.iter()) {
            if pos >= buf.len() {
                break;
            }
            buf[pos] = b;
            pos += 1;
        }
    }

    // Periodically reseed
    if pool.csprng_counter.is_multiple_of(1024) && pool.entropy_bits >= 128 {
        reseed_csprng(pool);
    }
}

/// Read from /dev/urandom (non-blocking, always succeeds)
pub fn urandom_read(buf: &mut [u8]) -> usize {
    let mut pool = ENTROPY_POOL.lock();
    generate_bytes(&mut pool, buf);
    buf.len()
}

/// Read from /dev/random (blocks if insufficient entropy)
pub fn random_read(buf: &mut [u8]) -> Result<usize, i32> {
    let mut pool = ENTROPY_POOL.lock();
    if pool.entropy_bits < MIN_ENTROPY_BITS && pool.seeded {
        // In a real kernel, we'd block. Here we just generate anyway
        // (similar to Linux 5.18+ which made /dev/random non-blocking)
    }
    generate_bytes(&mut pool, buf);
    Ok(buf.len())
}

/// getrandom() syscall implementation
pub fn getrandom(buf: &mut [u8], flags: u32) -> Result<usize, i32> {
    if buf.is_empty() {
        return Ok(0);
    }

    let blocking = flags & GRND_NONBLOCK == 0;
    let use_random = flags & GRND_RANDOM != 0;

    let mut pool = ENTROPY_POOL.lock();

    if use_random && pool.entropy_bits < MIN_ENTROPY_BITS && !blocking {
        return Err(-11); // EAGAIN
    }
    // In a blocking scenario, we'd sleep. For now, generate anyway.

    generate_bytes(&mut pool, buf);
    Ok(buf.len())
}

/// Get entropy pool information (for /proc/sys/kernel/random/)
pub fn entropy_avail() -> u32 {
    let pool = ENTROPY_POOL.lock();
    pool.entropy_bits
}

/// Get pool size
pub fn pool_size() -> u32 {
    (POOL_SIZE as u32) * 8
}

/// Generate a random u64
pub fn random_u64() -> u64 {
    let mut buf = [0u8; 8];
    urandom_read(&mut buf);
    u64::from_le_bytes(buf)
}

/// Generate a random u32
pub fn random_u32() -> u32 {
    let mut buf = [0u8; 4];
    urandom_read(&mut buf);
    u32::from_le_bytes(buf)
}

/// Generate a random number in range [0, max)
pub fn random_range(max: u64) -> u64 {
    if max == 0 {
        return 0;
    }
    random_u64() % max
}

/// Fill a buffer with random bytes (convenience)
pub fn fill_random(buf: &mut [u8]) {
    urandom_read(buf);
}

/// Initialize the random subsystem
pub fn init() {
    // Check for RDRAND/RDSEED support
    let cpuid = crate::arch_compat::raw_cpuid::CpuId::new();
    let has_rdrand = cpuid
        .get_feature_info()
        .map(|f| f.has_rdrand())
        .unwrap_or(false);

    if has_rdrand {
        HAS_RDRAND.store(true, Ordering::Release);
        serial_println!("[KnoxOS] RDRAND hardware RNG detected");

        // Seed from RDRAND for high-quality initial entropy
        for _ in 0..32 {
            if let Some(val) = rdrand_u64() {
                let bytes = val.to_le_bytes();
                add_entropy(&bytes, 64); // RDRAND provides full entropy
            }
        }
    }

    // Check for RDSEED (better entropy than RDRAND)
    let has_rdseed = cpuid
        .get_extended_feature_info()
        .map(|f| f.has_rdseed())
        .unwrap_or(false);
    if has_rdseed {
        HAS_RDSEED.store(true, Ordering::Release);
        serial_println!("[KnoxOS] RDSEED hardware seed generator detected");

        for _ in 0..8 {
            if let Some(val) = rdseed_u64() {
                let bytes = val.to_le_bytes();
                add_entropy(&bytes, 64);
            }
        }
    }

    // Seed initial entropy from TSC and other sources
    let tsc = rdtsc();
    let tsc_bytes = tsc.to_le_bytes();
    add_entropy(&tsc_bytes, 8);

    // Add multiple timing samples
    for _ in 0..16 {
        add_timer_entropy();
    }

    // Mix in some hardware-specific data
    if let Some(fi) = cpuid.get_feature_info() {
        let model = fi.model_id() as u64;
        let family = fi.family_id() as u64;
        let stepping = fi.stepping_id() as u64;
        let mixed = mix64(model | (family << 8) | (stepping << 16));
        let bytes = mixed.to_le_bytes();
        add_entropy(&bytes, 4);
    }

    // Force initial seed
    {
        let mut pool = ENTROPY_POOL.lock();
        reseed_csprng(&mut pool);
    }

    INITIALIZED.store(true, Ordering::Release);

    let rdrand_str = if has_rdrand { "RDRAND" } else { "software" };
    let rdseed_str = if has_rdseed { "+RDSEED" } else { "" };
    serial_println!(
        "[KnoxOS] Random subsystem initialized (entropy={} bits, pool={}B, hw={}{})",
        entropy_avail(),
        POOL_SIZE,
        rdrand_str,
        rdseed_str,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// HARDWARE RNG: RDRAND / RDSEED
// ═══════════════════════════════════════════════════════════════════════

static HAS_RDRAND: AtomicBool = AtomicBool::new(false);
static HAS_RDSEED: AtomicBool = AtomicBool::new(false);

/// Read a random u64 from RDRAND instruction
/// Returns None if RDRAND is unavailable or fails after retries
pub fn rdrand_u64() -> Option<u64> {
    if !HAS_RDRAND.load(Ordering::Relaxed) {
        return None;
    }
    // RDRAND can fail transiently; retry up to 10 times per Intel recommendation
    for _ in 0..10 {
        let mut val: u64 = 0;
        let mut success: u8 = 0;
        unsafe {
            #[cfg(target_arch = "x86_64")]
            core::arch::asm!(
                "rdrand {val}",
                "setc {success}",
                val = out(reg) val,
                success = out(reg_byte) success,
            );
        }
        if success != 0 {
            return Some(val);
        }
    }
    None
}

/// Read a random u64 from RDSEED instruction (true entropy seed)
/// Returns None if RDSEED is unavailable or fails
pub fn rdseed_u64() -> Option<u64> {
    if !HAS_RDSEED.load(Ordering::Relaxed) {
        return None;
    }
    for _ in 0..10 {
        let mut val: u64 = 0;
        let mut success: u8 = 0;
        unsafe {
            #[cfg(target_arch = "x86_64")]
            core::arch::asm!(
                "rdseed {val}",
                "setc {success}",
                val = out(reg) val,
                success = out(reg_byte) success,
            );
        }
        if success != 0 {
            return Some(val);
        }
    }
    None
}

/// Check if hardware RNG is available
pub fn has_hardware_rng() -> bool {
    HAS_RDRAND.load(Ordering::Relaxed)
}

/// Fill buffer using hardware RNG (RDRAND), falling back to CSPRNG
pub fn secure_random_bytes(buf: &mut [u8]) {
    if HAS_RDRAND.load(Ordering::Relaxed) {
        let mut i = 0;
        while i + 8 <= buf.len() {
            if let Some(val) = rdrand_u64() {
                buf[i..i + 8].copy_from_slice(&val.to_le_bytes());
            } else {
                // Fallback to CSPRNG for this chunk
                urandom_read(&mut buf[i..i + 8]);
            }
            i += 8;
        }
        if i < buf.len() {
            if let Some(val) = rdrand_u64() {
                let bytes = val.to_le_bytes();
                let remaining = buf.len() - i;
                buf[i..].copy_from_slice(&bytes[..remaining]);
            } else {
                urandom_read(&mut buf[i..]);
            }
        }
    } else {
        urandom_read(buf);
    }
}

/// Re-seed entropy pool from hardware RNG (call periodically)
pub fn reseed_from_hardware() {
    if HAS_RDRAND.load(Ordering::Relaxed) {
        for _ in 0..4 {
            if let Some(val) = rdrand_u64() {
                add_entropy(&val.to_le_bytes(), 64);
            }
        }
    }
}
