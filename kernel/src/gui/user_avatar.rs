use crate::serial_println;
/// User Avatar & Account Switcher
///
/// User profile picture display, fast user switch in system tray,
/// session management for multiple logged-in users.
use alloc::string::String;
use alloc::vec::Vec;
use spin::Mutex;

#[derive(Debug, Clone)]
pub struct UserAccount {
    pub uid: u32,
    pub username: String,
    pub display_name: String,
    pub avatar_path: Option<String>,
    pub logged_in: bool,
    pub session_id: Option<u32>,
}

pub struct AccountSwitcher {
    pub accounts: Vec<UserAccount>,
    pub active_uid: u32,
}

lazy_static::lazy_static! {
    static ref SWITCHER: Mutex<AccountSwitcher> = Mutex::new(AccountSwitcher {
        accounts: Vec::new(),
        active_uid: 0,
    });
}

impl AccountSwitcher {
    pub fn register_user(&mut self, account: UserAccount) {
        serial_println!(
            "[ACCOUNT] Registered: {} (uid={})",
            account.display_name,
            account.uid
        );
        self.accounts.push(account);
    }

    pub fn switch_to(&mut self, uid: u32) -> bool {
        if let Some(account) = self.accounts.iter().find(|a| a.uid == uid) {
            serial_println!("[ACCOUNT] Switching to: {}", account.display_name);
            self.active_uid = uid;
            true
        } else {
            false
        }
    }

    pub fn active_user(&self) -> Option<&UserAccount> {
        self.accounts.iter().find(|a| a.uid == self.active_uid)
    }

    pub fn set_avatar(&mut self, uid: u32, path: &str) {
        if let Some(a) = self.accounts.iter_mut().find(|a| a.uid == uid) {
            a.avatar_path = Some(String::from(path));
            serial_println!("[ACCOUNT] Avatar set for {}: {}", a.username, path);
        }
    }

    pub fn logged_in_users(&self) -> Vec<&UserAccount> {
        self.accounts.iter().filter(|a| a.logged_in).collect()
    }
}

pub fn init() {
    serial_println!("[ACCOUNT] User avatar / account switcher initialized");
}
