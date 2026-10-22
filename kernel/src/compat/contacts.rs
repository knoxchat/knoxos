/// Contacts Manager — CardDAV sync + local contact storage
///
/// Provides contact management with vCard parsing, search, groups,
/// and CardDAV server synchronization.
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU64, Ordering};
use spin::Mutex;

use crate::serial_println;

static NEXT_CONTACT_ID: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, Clone)]
pub struct Contact {
    pub id: u64,
    pub first_name: String,
    pub last_name: String,
    pub display_name: String,
    pub emails: Vec<EmailEntry>,
    pub phones: Vec<PhoneEntry>,
    pub addresses: Vec<AddressEntry>,
    pub organization: Option<String>,
    pub title: Option<String>,
    pub notes: Option<String>,
    pub photo: Option<Vec<u8>>,
    pub groups: Vec<String>,
    pub uid: String, // vCard UID
}

#[derive(Debug, Clone)]
pub struct EmailEntry {
    pub label: String, // home, work, other
    pub address: String,
}

#[derive(Debug, Clone)]
pub struct PhoneEntry {
    pub label: String, // mobile, home, work
    pub number: String,
}

#[derive(Debug, Clone)]
pub struct AddressEntry {
    pub label: String,
    pub street: String,
    pub city: String,
    pub state: String,
    pub postal_code: String,
    pub country: String,
}

impl Contact {
    pub fn new(first_name: &str, last_name: &str) -> Self {
        let id = NEXT_CONTACT_ID.fetch_add(1, Ordering::Relaxed);
        let display = if last_name.is_empty() {
            String::from(first_name)
        } else {
            alloc::format!("{} {}", first_name, last_name)
        };
        Self {
            id,
            first_name: String::from(first_name),
            last_name: String::from(last_name),
            display_name: display,
            emails: Vec::new(),
            phones: Vec::new(),
            addresses: Vec::new(),
            organization: None,
            title: None,
            notes: None,
            photo: None,
            groups: Vec::new(),
            uid: alloc::format!("contact-{}", id),
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════
// vCARD PARSER
// ═══════════════════════════════════════════════════════════════════════

/// Parse a vCard 3.0/4.0 string into a Contact
pub fn parse_vcard(vcard: &str) -> Option<Contact> {
    let mut contact = Contact::new("", "");
    let mut in_card = false;

    for line in vcard.lines() {
        let line = line.trim();
        if line == "BEGIN:VCARD" {
            in_card = true;
            continue;
        }
        if line == "END:VCARD" {
            break;
        }
        if !in_card {
            continue;
        }

        if let Some(val) = line.strip_prefix("FN:") {
            contact.display_name = String::from(val);
        } else if let Some(val) = line.strip_prefix("N:") {
            let parts: Vec<&str> = val.split(';').collect();
            if parts.len() >= 2 {
                contact.last_name = String::from(parts[0]);
                contact.first_name = String::from(parts[1]);
            }
        } else if line.starts_with("EMAIL") {
            if let Some(pos) = line.find(':') {
                let label = if line.contains("WORK") {
                    "work"
                } else {
                    "home"
                };
                contact.emails.push(EmailEntry {
                    label: String::from(label),
                    address: String::from(&line[pos + 1..]),
                });
            }
        } else if line.starts_with("TEL") {
            if let Some(pos) = line.find(':') {
                let label = if line.contains("CELL") {
                    "mobile"
                } else if line.contains("WORK") {
                    "work"
                } else {
                    "home"
                };
                contact.phones.push(PhoneEntry {
                    label: String::from(label),
                    number: String::from(&line[pos + 1..]),
                });
            }
        } else if let Some(val) = line.strip_prefix("ORG:") {
            contact.organization = Some(String::from(val));
        } else if let Some(val) = line.strip_prefix("TITLE:") {
            contact.title = Some(String::from(val));
        } else if let Some(val) = line.strip_prefix("NOTE:") {
            contact.notes = Some(String::from(val));
        } else if let Some(val) = line.strip_prefix("UID:") {
            contact.uid = String::from(val);
        }
    }

    if contact.display_name.is_empty() && contact.first_name.is_empty() {
        return None;
    }
    Some(contact)
}

/// Export a Contact to vCard 3.0 format
pub fn to_vcard(contact: &Contact) -> String {
    let mut lines = Vec::new();
    lines.push(String::from("BEGIN:VCARD"));
    lines.push(String::from("VERSION:3.0"));
    lines.push(alloc::format!("FN:{}", contact.display_name));
    lines.push(alloc::format!(
        "N:{};{};;;",
        contact.last_name,
        contact.first_name
    ));
    for e in &contact.emails {
        lines.push(alloc::format!(
            "EMAIL;TYPE={}:{}",
            e.label.to_uppercase(),
            e.address
        ));
    }
    for p in &contact.phones {
        lines.push(alloc::format!(
            "TEL;TYPE={}:{}",
            p.label.to_uppercase(),
            p.number
        ));
    }
    if let Some(ref org) = contact.organization {
        lines.push(alloc::format!("ORG:{}", org));
    }
    if let Some(ref title) = contact.title {
        lines.push(alloc::format!("TITLE:{}", title));
    }
    lines.push(alloc::format!("UID:{}", contact.uid));
    lines.push(String::from("END:VCARD"));
    lines.join("\r\n")
}

// ═══════════════════════════════════════════════════════════════════════
// CONTACT STORE
// ═══════════════════════════════════════════════════════════════════════

lazy_static::lazy_static! {
    static ref CONTACTS: Mutex<BTreeMap<u64, Contact>> = Mutex::new(BTreeMap::new());
}

pub fn add(contact: Contact) -> u64 {
    let id = contact.id;
    serial_println!("[contacts] Added: {}", contact.display_name);
    CONTACTS.lock().insert(id, contact);
    id
}

pub fn get(id: u64) -> Option<Contact> {
    CONTACTS.lock().get(&id).cloned()
}

pub fn delete(id: u64) -> bool {
    CONTACTS.lock().remove(&id).is_some()
}

pub fn search(query: &str) -> Vec<Contact> {
    let q = query.to_lowercase();
    let contacts = CONTACTS.lock();
    contacts
        .values()
        .filter(|c| {
            c.display_name.to_lowercase().contains(&q)
                || c.emails
                    .iter()
                    .any(|e| e.address.to_lowercase().contains(&q))
                || c.phones.iter().any(|p| p.number.contains(&q))
        })
        .cloned()
        .collect()
}

pub fn list_all() -> Vec<(u64, String)> {
    CONTACTS
        .lock()
        .iter()
        .map(|(id, c)| (*id, c.display_name.clone()))
        .collect()
}

/// CardDAV sync — push/pull contacts with a CardDAV server
pub fn carddav_sync(server_url: &str) -> Result<usize, &'static str> {
    serial_println!("[contacts] CardDAV sync with {}", server_url);
    // In a full implementation this would do HTTP PROPFIND/GET/PUT
    Ok(0)
}

pub fn init() {
    serial_println!("[contacts] Contacts manager (CardDAV) initialized");
}
