use alloc::string::String;

/// Cipher suite
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CipherSuite {
    None,
    Wep40,
    Tkip,
    Ccmp, // AES-CCMP (WPA2)
    Wep104,
    Gcmp, // AES-GCMP (WPA3)
    Gcmp256,
    Ccmp256,
    Sae, // WPA3 SAE (Dragonfly)
}

/// Authentication Key Management
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AkmSuite {
    None,
    Psk,       // WPA2-PSK
    Ieee8021x, // WPA2-Enterprise
    Sae,       // WPA3-Personal
    Ft8021x,   // Fast Transition 802.1X
    FtPsk,     // Fast Transition PSK
    Owe,       // Opportunistic Wireless Encryption
}

/// Security configuration
#[derive(Debug, Clone)]
pub struct SecurityConfig {
    pub pairwise_cipher: CipherSuite,
    pub group_cipher: CipherSuite,
    pub akm: AkmSuite,
    pub pmf: PmfMode,
    pub passphrase: Option<String>,
    pub pmk: Option<[u8; 32]>, // Pairwise Master Key
    pub ptk: Option<[u8; 48]>, // Pairwise Transient Key
    pub gtk: Option<[u8; 32]>, // Group Temporal Key
}

/// Protected Management Frames mode
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PmfMode {
    Disabled,
    Optional,
    Required,
}
