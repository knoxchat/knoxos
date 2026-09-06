//! Release Versioning & Signing — Build reproducible, signed releases
//!
//! Implements a release management pipeline:
//!   - Semantic versioning (MAJOR.MINOR.PATCH[-PRERELEASE])
//!   - Release metadata generation (build info, commit hash, timestamps)
//!   - Release artifact signing (SHA-256 HMAC, Ed25519-style)
//!   - Signature verification for update integrity
//!   - Release manifest (JSON-like) with checksums for all artifacts
//!   - Release channels (stable/beta/nightly)

use alloc::collections::BTreeMap;
use alloc::format;
use alloc::string::String;
use alloc::string::ToString;
use alloc::vec::Vec;
use spin::Mutex;

use crate::serial_println;

// ═══════════════════════════════════════════════════════════════════════
// SEMANTIC VERSION
// ═══════════════════════════════════════════════════════════════════════

/// Semantic version: MAJOR.MINOR.PATCH[-prerelease][+build]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SemVer {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
    pub prerelease: Option<String>,
    pub build_metadata: Option<String>,
}

impl SemVer {
    pub fn new(major: u32, minor: u32, patch: u32) -> Self {
        SemVer {
            major,
            minor,
            patch,
            prerelease: None,
            build_metadata: None,
        }
    }

    /// Parse "1.2.3" or "1.2.3-beta.1" or "1.2.3+build.42"
    pub fn parse(s: &str) -> Option<Self> {
        let (version_str, build_metadata) = if let Some(idx) = s.find('+') {
            (&s[..idx], Some(String::from(&s[idx + 1..])))
        } else {
            (s, None)
        };

        let (version_str, prerelease) = if let Some(idx) = version_str.find('-') {
            (
                &version_str[..idx],
                Some(String::from(&version_str[idx + 1..])),
            )
        } else {
            (version_str, None)
        };

        let parts: Vec<&str> = version_str.split('.').collect();
        if parts.len() != 3 {
            return None;
        }

        let major = parts[0].parse::<u32>().ok()?;
        let minor = parts[1].parse::<u32>().ok()?;
        let patch = parts[2].parse::<u32>().ok()?;

        Some(SemVer {
            major,
            minor,
            patch,
            prerelease,
            build_metadata,
        })
    }

    /// Compare versions (ignoring build metadata per SemVer spec)
    pub fn cmp_precedence(&self, other: &SemVer) -> core::cmp::Ordering {
        match self.major.cmp(&other.major) {
            core::cmp::Ordering::Equal => {}
            ord => return ord,
        }
        match self.minor.cmp(&other.minor) {
            core::cmp::Ordering::Equal => {}
            ord => return ord,
        }
        match self.patch.cmp(&other.patch) {
            core::cmp::Ordering::Equal => {}
            ord => return ord,
        }
        // A version with prerelease has lower precedence than the same version without
        match (&self.prerelease, &other.prerelease) {
            (None, None) => core::cmp::Ordering::Equal,
            (Some(_), None) => core::cmp::Ordering::Less,
            (None, Some(_)) => core::cmp::Ordering::Greater,
            (Some(a), Some(b)) => a.cmp(b),
        }
    }

    /// Bump major version
    pub fn bump_major(&self) -> SemVer {
        SemVer::new(self.major + 1, 0, 0)
    }

    /// Bump minor version
    pub fn bump_minor(&self) -> SemVer {
        SemVer::new(self.major, self.minor + 1, 0)
    }

    /// Bump patch version
    pub fn bump_patch(&self) -> SemVer {
        SemVer::new(self.major, self.minor, self.patch + 1)
    }
}

impl core::fmt::Display for SemVer {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)?;
        if let Some(ref pre) = self.prerelease {
            write!(f, "-{}", pre)?;
        }
        if let Some(ref build) = self.build_metadata {
            write!(f, "+{}", build)?;
        }
        Ok(())
    }
}

// ═══════════════════════════════════════════════════════════════════════
// RELEASE CHANNELS
// ═══════════════════════════════════════════════════════════════════════

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReleaseChannel {
    /// Stable release — fully tested
    Stable,
    /// Beta — feature-complete but may have bugs
    Beta,
    /// Nightly — bleeding edge, built from HEAD
    Nightly,
}

impl ReleaseChannel {
    pub fn as_str(&self) -> &str {
        match self {
            ReleaseChannel::Stable => "stable",
            ReleaseChannel::Beta => "beta",
            ReleaseChannel::Nightly => "nightly",
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════
// RELEASE ARTIFACT
// ═══════════════════════════════════════════════════════════════════════

/// A single artifact in a release (e.g., BIOS image, UEFI image, ISO)
#[derive(Debug, Clone)]
pub struct ReleaseArtifact {
    pub name: String,
    pub filename: String,
    pub size: u64,
    pub sha256: [u8; 32],
    pub artifact_type: ArtifactType,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArtifactType {
    BiosImage,
    UefiImage,
    IsoImage,
    KernelBinary,
    SourceTarball,
    Checksum,
    Signature,
}

impl ArtifactType {
    pub fn as_str(&self) -> &str {
        match self {
            ArtifactType::BiosImage => "bios-image",
            ArtifactType::UefiImage => "uefi-image",
            ArtifactType::IsoImage => "iso-image",
            ArtifactType::KernelBinary => "kernel-binary",
            ArtifactType::SourceTarball => "source-tarball",
            ArtifactType::Checksum => "checksum",
            ArtifactType::Signature => "signature",
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════
// RELEASE MANIFEST
// ═══════════════════════════════════════════════════════════════════════

/// Complete release manifest
#[derive(Debug, Clone)]
pub struct ReleaseManifest {
    pub version: SemVer,
    pub channel: ReleaseChannel,
    pub timestamp: u64,
    pub commit_hash: String,
    pub artifacts: Vec<ReleaseArtifact>,
    pub release_notes: String,
    pub min_upgrade_version: Option<SemVer>,
}

// ═══════════════════════════════════════════════════════════════════════
// SIGNING
// ═══════════════════════════════════════════════════════════════════════

/// Signing key pair (simplified — in production, use Ed25519)
#[derive(Debug, Clone)]
pub struct SigningKeyPair {
    pub key_id: [u8; 8],
    pub public_key: [u8; 32],
    /// Private key — NEVER expose outside signing operations
    private_key: [u8; 32],
}

/// Release signature
#[derive(Debug, Clone)]
pub struct ReleaseSignature {
    pub key_id: [u8; 8],
    pub algorithm: String,
    pub signature: Vec<u8>,
    pub signed_hash: [u8; 32],
}

/// SHA-256 implementation for signing
fn sha256_hash(data: &[u8]) -> [u8; 32] {
    let mut h: [u32; 8] = [
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
        0x5be0cd19,
    ];

    let k: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4,
        0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe,
        0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f,
        0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
        0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc,
        0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
        0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116,
        0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
        0xc67178f2,
    ];

    let bit_len = (data.len() as u64) * 8;
    let mut padded = Vec::from(data);
    padded.push(0x80);
    while (padded.len() % 64) != 56 {
        padded.push(0x00);
    }
    padded.extend_from_slice(&bit_len.to_be_bytes());

    for chunk in padded.chunks(64) {
        let mut w = [0u32; 64];
        for i in 0..16 {
            w[i] = u32::from_be_bytes([
                chunk[i * 4],
                chunk[i * 4 + 1],
                chunk[i * 4 + 2],
                chunk[i * 4 + 3],
            ]);
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16]
                .wrapping_add(s0)
                .wrapping_add(w[i - 7])
                .wrapping_add(s1);
        }

        let (mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut hh) =
            (h[0], h[1], h[2], h[3], h[4], h[5], h[6], h[7]);

        for i in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ ((!e) & g);
            let temp1 = hh
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(k[i])
                .wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let temp2 = s0.wrapping_add(maj);

            hh = g;
            g = f;
            f = e;
            e = d.wrapping_add(temp1);
            d = c;
            c = b;
            b = a;
            a = temp1.wrapping_add(temp2);
        }

        h[0] = h[0].wrapping_add(a);
        h[1] = h[1].wrapping_add(b);
        h[2] = h[2].wrapping_add(c);
        h[3] = h[3].wrapping_add(d);
        h[4] = h[4].wrapping_add(e);
        h[5] = h[5].wrapping_add(f);
        h[6] = h[6].wrapping_add(g);
        h[7] = h[7].wrapping_add(hh);
    }

    let mut result = [0u8; 32];
    for (i, val) in h.iter().enumerate() {
        result[i * 4..i * 4 + 4].copy_from_slice(&val.to_be_bytes());
    }
    result
}

/// HMAC-SHA256 for signing
fn hmac_sha256(key: &[u8], data: &[u8]) -> [u8; 32] {
    let block_size = 64;

    // If key is longer than block size, hash it first
    let key_block = if key.len() > block_size {
        let h = sha256_hash(key);
        let mut kb = [0u8; 64];
        kb[..32].copy_from_slice(&h);
        kb
    } else {
        let mut kb = [0u8; 64];
        kb[..key.len()].copy_from_slice(key);
        kb
    };

    // Inner pad
    let mut i_key_pad = [0x36u8; 64];
    for i in 0..64 {
        i_key_pad[i] ^= key_block[i];
    }

    // Outer pad
    let mut o_key_pad = [0x5cu8; 64];
    for i in 0..64 {
        o_key_pad[i] ^= key_block[i];
    }

    // inner = H(i_key_pad || data)
    let mut inner_data = Vec::with_capacity(64 + data.len());
    inner_data.extend_from_slice(&i_key_pad);
    inner_data.extend_from_slice(data);
    let inner_hash = sha256_hash(&inner_data);

    // outer = H(o_key_pad || inner)
    let mut outer_data = Vec::with_capacity(64 + 32);
    outer_data.extend_from_slice(&o_key_pad);
    outer_data.extend_from_slice(&inner_hash);
    sha256_hash(&outer_data)
}

fn hex_encode(data: &[u8]) -> String {
    let mut s = String::new();
    for byte in data {
        s.push_str(&format!("{:02x}", byte));
    }
    s
}

/// Generate a signing key pair from a seed
pub fn generate_key_pair(seed: &[u8; 32]) -> SigningKeyPair {
    let private_key = *seed;
    let public_key = sha256_hash(seed); // Simplified: real Ed25519 uses curve multiplication

    let mut key_id = [0u8; 8];
    key_id.copy_from_slice(&public_key[..8]);

    SigningKeyPair {
        key_id,
        public_key,
        private_key,
    }
}

/// Sign a release artifact
pub fn sign_artifact(data: &[u8], key: &SigningKeyPair) -> ReleaseSignature {
    let data_hash = sha256_hash(data);
    let signature = hmac_sha256(&key.private_key, &data_hash);

    ReleaseSignature {
        key_id: key.key_id,
        algorithm: String::from("HMAC-SHA256"),
        signature: signature.to_vec(),
        signed_hash: data_hash,
    }
}

/// Verify a release signature
pub fn verify_signature(data: &[u8], sig: &ReleaseSignature, key: &SigningKeyPair) -> bool {
    let data_hash = sha256_hash(data);

    // Verify the hash matches
    if data_hash != sig.signed_hash {
        serial_println!("[release] Hash mismatch during verification");
        return false;
    }

    // Verify the HMAC
    let expected_sig = hmac_sha256(&key.private_key, &data_hash);

    // Constant-time comparison
    let mut diff = 0u8;
    if sig.signature.len() != expected_sig.len() {
        return false;
    }
    for (a, b) in sig.signature.iter().zip(expected_sig.iter()) {
        diff |= a ^ b;
    }
    diff == 0
}

// ═══════════════════════════════════════════════════════════════════════
// RELEASE MANIFEST GENERATION
// ═══════════════════════════════════════════════════════════════════════

/// Generate a release manifest for the current build
pub fn create_release(
    version: SemVer,
    channel: ReleaseChannel,
    release_notes: &str,
) -> ReleaseManifest {
    let timestamp = crate::rtc::read_rtc().to_unix_timestamp() as u64;

    // Compute SHA-256 for known build artifacts
    let mut artifacts = Vec::new();

    // BIOS image
    if let Some(bios_data) = crate::vfs::read_file_dispatch("/boot/knoxos-bios.img") {
        artifacts.push(ReleaseArtifact {
            name: String::from("KnoxOS BIOS Boot Image"),
            filename: String::from("knoxos-bios.img"),
            size: bios_data.len() as u64,
            sha256: sha256_hash(&bios_data),
            artifact_type: ArtifactType::BiosImage,
        });
    }

    // UEFI image
    if let Some(uefi_data) = crate::vfs::read_file_dispatch("/boot/knoxos-uefi.img") {
        artifacts.push(ReleaseArtifact {
            name: String::from("KnoxOS UEFI Boot Image"),
            filename: String::from("knoxos-uefi.img"),
            size: uefi_data.len() as u64,
            sha256: sha256_hash(&uefi_data),
            artifact_type: ArtifactType::UefiImage,
        });
    }

    // Kernel binary
    if let Some(kernel_data) = crate::vfs::read_file_dispatch("/boot/knoxos-kernel") {
        artifacts.push(ReleaseArtifact {
            name: String::from("KnoxOS Kernel Binary"),
            filename: String::from("knoxos-kernel"),
            size: kernel_data.len() as u64,
            sha256: sha256_hash(&kernel_data),
            artifact_type: ArtifactType::KernelBinary,
        });
    }

    ReleaseManifest {
        version,
        channel,
        timestamp,
        commit_hash: String::from("HEAD"),
        artifacts,
        release_notes: String::from(release_notes),
        min_upgrade_version: None,
    }
}

/// Serialize a release manifest to text format
pub fn serialize_manifest(manifest: &ReleaseManifest) -> String {
    let mut out = String::new();
    out.push_str("# KnoxOS Release Manifest\n\n");
    out.push_str(&format!("version: {}\n", manifest.version));
    out.push_str(&format!("channel: {}\n", manifest.channel.as_str()));
    out.push_str(&format!("timestamp: {}\n", manifest.timestamp));
    out.push_str(&format!("commit: {}\n", manifest.commit_hash));

    if let Some(ref min_ver) = manifest.min_upgrade_version {
        out.push_str(&format!("min-upgrade: {}\n", min_ver));
    }

    out.push_str(&format!("\nrelease-notes: |\n"));
    for line in manifest.release_notes.lines() {
        out.push_str(&format!("  {}\n", line));
    }

    out.push_str(&format!("\nartifacts:\n"));
    for art in &manifest.artifacts {
        out.push_str(&format!("  - name: {}\n", art.name));
        out.push_str(&format!("    filename: {}\n", art.filename));
        out.push_str(&format!("    size: {}\n", art.size));
        out.push_str(&format!("    sha256: {}\n", hex_encode(&art.sha256)));
        out.push_str(&format!("    type: {}\n", art.artifact_type.as_str()));
    }

    out
}

/// Sign a release manifest and write to VFS
pub fn sign_and_publish_release(
    manifest: &ReleaseManifest,
    key: &SigningKeyPair,
) -> Result<(), &'static str> {
    let manifest_text = serialize_manifest(manifest);
    let manifest_bytes = manifest_text.as_bytes();

    // Sign the manifest
    let signature = sign_artifact(manifest_bytes, key);

    // Write manifest
    let manifest_path = format!("/var/releases/{}/MANIFEST", manifest.version);
    let release_dir = format!("/var/releases/{}", manifest.version);
    crate::vfs::ensure_directory(&release_dir);
    crate::vfs::write_file_dispatch(&manifest_path, manifest_bytes);

    // Write signature
    let sig_path = format!("{}/MANIFEST.sig", release_dir);
    let mut sig_data = Vec::new();
    sig_data.extend_from_slice(&signature.key_id);
    sig_data.push(0x01); // Algorithm ID: HMAC-SHA256
    sig_data.extend_from_slice(&signature.signature);
    crate::vfs::write_file_dispatch(&sig_path, &sig_data);

    // Write SHA256SUMS
    let mut checksums = String::new();
    for art in &manifest.artifacts {
        checksums.push_str(&format!("{}  {}\n", hex_encode(&art.sha256), art.filename));
    }
    let sums_path = format!("{}/SHA256SUMS", release_dir);
    crate::vfs::write_file_dispatch(&sums_path, checksums.as_bytes());

    serial_println!(
        "[release] Published signed release v{} ({} artifacts) to {}",
        manifest.version.to_string(),
        manifest.artifacts.len(),
        release_dir
    );

    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════
// RELEASE HISTORY
// ═══════════════════════════════════════════════════════════════════════

lazy_static::lazy_static! {
    static ref RELEASES: Mutex<Vec<ReleaseManifest>> = Mutex::new(Vec::new());
    static ref SIGNING_KEY: Mutex<Option<SigningKeyPair>> = Mutex::new(None);
}

/// Register a release in the history
pub fn register_release(manifest: ReleaseManifest) {
    RELEASES.lock().push(manifest);
}

/// Get the latest release for a channel
pub fn latest_release(channel: ReleaseChannel) -> Option<SemVer> {
    let releases = RELEASES.lock();
    releases
        .iter()
        .filter(|r| r.channel == channel)
        .max_by(|a, b| a.version.cmp_precedence(&b.version))
        .map(|r| r.version.clone())
}

/// Get release count
pub fn release_count() -> usize {
    RELEASES.lock().len()
}

// ═══════════════════════════════════════════════════════════════════════
// INITIALIZATION
// ═══════════════════════════════════════════════════════════════════════

/// Initialize the release signing subsystem
pub fn init() {
    // Generate a default signing key from a deterministic seed
    // In production, this would be loaded from secure storage
    let seed: [u8; 32] = [
        0x4b, 0x6e, 0x6f, 0x78, 0x4f, 0x53, 0x52, 0x65, 0x6c, 0x65, 0x61, 0x73, 0x65, 0x4b, 0x65,
        0x79, 0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d,
        0x0e, 0x0f,
    ];
    let key = generate_key_pair(&seed);
    *SIGNING_KEY.lock() = Some(key);

    // Ensure release directory exists
    crate::vfs::ensure_directory("/var/releases");

    // Register current version
    let current = ReleaseManifest {
        version: SemVer::new(0, 2, 1),
        channel: ReleaseChannel::Stable,
        timestamp: crate::rtc::read_rtc().to_unix_timestamp() as u64,
        commit_hash: String::from("HEAD"),
        artifacts: Vec::new(),
        release_notes: String::from(
            "KnoxOS v0.2.1 — Package management, MIDI, A2DP, performance tracking",
        ),
        min_upgrade_version: Some(SemVer::new(0, 1, 0)),
    };
    register_release(current);

    serial_println!("[KnoxOS] Release signing subsystem initialized (v0.2.1)");
}
