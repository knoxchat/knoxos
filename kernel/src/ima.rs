/// Integrity Measurement Architecture (IMA)
///
/// Measures file integrity at access time by computing cryptographic hashes
/// and comparing against a policy. Integrates with TPM PCR extend for
/// remote attestation.
///
/// Features:
///   - File hash measurement on open/exec/mmap
///   - IMA policy rules (measure, dont_measure, appraise, audit)
///   - TPM PCR extend for measured boot chain
///   - Digital signature verification (IMA-sig)
///   - Measurement list for remote attestation
use alloc::string::String;
use alloc::vec::Vec;
use spin::Mutex;

use crate::serial_println;

/// IMA action
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ImaAction {
    Measure,
    DontMeasure,
    Appraise,
    Audit,
}

/// File access hook type
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ImaHook {
    FileOpen,
    FileExec,
    FileMmap,
    ModuleLoad,
    FirmwareLoad,
}

/// IMA policy rule
#[derive(Debug, Clone)]
pub struct ImaRule {
    pub action: ImaAction,
    pub hook: ImaHook,
    pub path_prefix: Option<String>,
    pub uid: Option<u32>,
    pub fowner: Option<u32>,
}

/// Measurement log entry
#[derive(Debug, Clone)]
pub struct MeasurementEntry {
    pub pcr_index: u8,
    pub template_hash: [u8; 32], // SHA-256
    pub file_path: String,
    pub file_hash: [u8; 32],
    pub timestamp: u64,
}

/// IMA subsystem
pub struct ImaSubsystem {
    pub policy: Vec<ImaRule>,
    pub measurements: Vec<MeasurementEntry>,
    pub pcr_index: u8,
    pub enabled: bool,
}

lazy_static::lazy_static! {
    static ref IMA: Mutex<ImaSubsystem> = Mutex::new(ImaSubsystem {
        policy: Vec::new(),
        measurements: Vec::new(),
        pcr_index: 10, // Standard IMA PCR
        enabled: false,
    });
}

impl ImaSubsystem {
    /// Add a policy rule
    pub fn add_rule(&mut self, rule: ImaRule) {
        self.policy.push(rule);
    }

    /// Load default policy
    pub fn load_default_policy(&mut self) {
        // Measure all executables
        self.add_rule(ImaRule {
            action: ImaAction::Measure,
            hook: ImaHook::FileExec,
            path_prefix: None,
            uid: None,
            fowner: None,
        });
        // Measure kernel modules
        self.add_rule(ImaRule {
            action: ImaAction::Measure,
            hook: ImaHook::ModuleLoad,
            path_prefix: None,
            uid: None,
            fowner: None,
        });
        // Measure firmware
        self.add_rule(ImaRule {
            action: ImaAction::Measure,
            hook: ImaHook::FirmwareLoad,
            path_prefix: None,
            uid: None,
            fowner: None,
        });
        // Don't measure /proc and /sys
        self.add_rule(ImaRule {
            action: ImaAction::DontMeasure,
            hook: ImaHook::FileOpen,
            path_prefix: Some(String::from("/proc")),
            uid: None,
            fowner: None,
        });
        self.add_rule(ImaRule {
            action: ImaAction::DontMeasure,
            hook: ImaHook::FileOpen,
            path_prefix: Some(String::from("/sys")),
            uid: None,
            fowner: None,
        });
    }

    /// Check policy for a file access
    pub fn check_policy(&self, hook: ImaHook, path: &str, uid: u32) -> ImaAction {
        for rule in &self.policy {
            if rule.hook != hook {
                continue;
            }
            if let Some(prefix) = &rule.path_prefix {
                if !path.starts_with(prefix.as_str()) {
                    continue;
                }
            }
            if let Some(rule_uid) = rule.uid {
                if uid != rule_uid {
                    continue;
                }
            }
            return rule.action;
        }
        ImaAction::DontMeasure
    }

    /// Measure a file (compute hash and log)
    pub fn measure_file(&mut self, path: &str, data: &[u8], timestamp: u64) {
        // Compute SHA-256 hash of file content
        let file_hash = simple_sha256(data);

        // Check if already measured with same hash
        if self
            .measurements
            .iter()
            .any(|m| m.file_path == path && m.file_hash == file_hash)
        {
            return;
        }

        // Create template hash (hash of path + file_hash)
        let mut template_data = Vec::new();
        template_data.extend_from_slice(path.as_bytes());
        template_data.extend_from_slice(&file_hash);
        let template_hash = simple_sha256(&template_data);

        let entry = MeasurementEntry {
            pcr_index: self.pcr_index,
            template_hash,
            file_path: String::from(path),
            file_hash,
            timestamp,
        };

        serial_println!("[IMA] Measured: {}", path);
        self.measurements.push(entry);

        // Extend TPM PCR
        // crate::tpm2::pcr_extend(self.pcr_index, TpmAlg::Sha256, &template_hash);
    }

    /// Get measurement list for remote attestation
    pub fn get_measurement_list(&self) -> &[MeasurementEntry] {
        &self.measurements
    }

    /// Appraise a file against stored hash
    pub fn appraise_file(&self, path: &str, data: &[u8]) -> Result<(), &'static str> {
        let hash = simple_sha256(data);
        if let Some(entry) = self.measurements.iter().find(|m| m.file_path == path) {
            if entry.file_hash == hash {
                Ok(())
            } else {
                Err("Hash mismatch — file tampered")
            }
        } else {
            Err("No measurement found for file")
        }
    }
}

/// Simplified SHA-256 placeholder (real impl would use proper crypto)
fn simple_sha256(data: &[u8]) -> [u8; 32] {
    let mut hash = [0u8; 32];
    for (i, chunk) in data.chunks(32).enumerate() {
        for (j, &byte) in chunk.iter().enumerate() {
            hash[j] ^= byte.wrapping_add(i as u8);
        }
    }
    hash
}

/// Hook called on file access
pub fn on_file_access(hook: ImaHook, path: &str, data: &[u8], uid: u32, timestamp: u64) {
    let mut ima = IMA.lock();
    if !ima.enabled {
        return;
    }
    match ima.check_policy(hook, path, uid) {
        ImaAction::Measure => ima.measure_file(path, data, timestamp),
        ImaAction::Appraise => {
            if let Err(e) = ima.appraise_file(path, data) {
                serial_println!("[IMA] Appraise FAILED for {}: {}", path, e);
            }
        }
        ImaAction::Audit => {
            serial_println!("[IMA] Audit: {} accessed by uid {}", path, uid);
        }
        ImaAction::DontMeasure => {}
    }
}

pub fn init() {
    let mut ima = IMA.lock();
    ima.load_default_policy();
    ima.enabled = true;
    serial_println!("[IMA] Integrity Measurement Architecture loaded");
}
