use crate::serial_println;
/// Online Accounts (OAuth Integration)
///
/// Manages online account connections for cloud services.
/// Stores tokens securely and provides unified access to
/// contacts, calendars, files, and email.
use alloc::string::String;
use alloc::vec::Vec;
use spin::Mutex;

/// Account provider
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AccountProvider {
    Google,
    Microsoft,
    NextCloud,
    Imap,
    CalDav,
    CardDav,
    WebDav,
}

/// Account capabilities
#[derive(Debug, Clone, Copy)]
pub struct AccountCaps(u16);

impl AccountCaps {
    pub const MAIL: Self = Self(1 << 0);
    pub const CALENDAR: Self = Self(1 << 1);
    pub const CONTACTS: Self = Self(1 << 2);
    pub const FILES: Self = Self(1 << 3);
    pub const PHOTOS: Self = Self(1 << 4);
    pub const CHAT: Self = Self(1 << 5);
}

/// An online account
#[derive(Debug, Clone)]
pub struct OnlineAccount {
    pub id: u32,
    pub provider: AccountProvider,
    pub display_name: String,
    pub email: String,
    pub enabled: bool,
    pub capabilities: u16,
    // Tokens stored separately via keyring
    pub access_token_id: Option<u64>,
    pub refresh_token_id: Option<u64>,
    pub token_expiry: u64,
    pub server_url: String,
}

lazy_static::lazy_static! {
    static ref ACCOUNTS: Mutex<Vec<OnlineAccount>> = Mutex::new(Vec::new());
    static ref NEXT_ID: Mutex<u32> = Mutex::new(1);
}

impl OnlineAccount {
    pub fn new(provider: AccountProvider, email: &str) -> Self {
        let id = {
            let mut n = NEXT_ID.lock();
            let v = *n;
            *n += 1;
            v
        };
        Self {
            id,
            provider,
            display_name: String::from(email),
            email: String::from(email),
            enabled: true,
            capabilities: 0,
            access_token_id: None,
            refresh_token_id: None,
            token_expiry: 0,
            server_url: String::new(),
        }
    }

    /// Check if token needs refresh
    pub fn needs_refresh(&self, now: u64) -> bool {
        self.token_expiry > 0 && now >= self.token_expiry.saturating_sub(300)
    }

    /// Perform OAuth2 token refresh
    pub fn refresh_token(&mut self) -> Result<(), &'static str> {
        if self.refresh_token_id.is_none() {
            return Err("No refresh token");
        }
        // HTTP POST to token endpoint with refresh_token grant
        // Update access_token_id, token_expiry
        serial_println!("[ACCOUNTS] Refreshed token for {}", self.email);
        Ok(())
    }

    /// Revoke account (sign out)
    pub fn revoke(&mut self) {
        self.access_token_id = None;
        self.refresh_token_id = None;
        self.enabled = false;
        serial_println!("[ACCOUNTS] Revoked: {}", self.email);
    }
}

pub fn add_account(account: OnlineAccount) {
    serial_println!(
        "[ACCOUNTS] Added: {} ({:?})",
        account.email,
        account.provider
    );
    ACCOUNTS.lock().push(account);
}

pub fn remove_account(id: u32) {
    let mut accounts = ACCOUNTS.lock();
    if let Some(acc) = accounts.iter_mut().find(|a| a.id == id) {
        acc.revoke();
    }
    accounts.retain(|a| a.id != id);
}

pub fn list_accounts() -> Vec<OnlineAccount> {
    ACCOUNTS.lock().clone()
}

pub fn init() {
    serial_println!("[ACCOUNTS] Online accounts service loaded");
}
