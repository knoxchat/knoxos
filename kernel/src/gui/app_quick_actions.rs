/// Application Quick Actions (Jump Lists)
///
/// Provides right-click context actions for taskbar app icons:
/// recent documents, common tasks, pinned items.
use alloc::string::String;
use alloc::vec::Vec;
use spin::Mutex;

/// Type of quick action
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ActionType {
    RecentDocument,
    PinnedItem,
    Task,
    Separator,
}

/// A quick action entry
#[derive(Debug, Clone)]
pub struct QuickAction {
    pub action_type: ActionType,
    pub label: String,
    pub icon: String,
    pub command: String,
    pub pinned: bool,
}

/// Per-application jump list
#[derive(Debug, Clone)]
pub struct JumpList {
    pub app_id: String,
    pub actions: Vec<QuickAction>,
}

lazy_static::lazy_static! {
    static ref JUMP_LISTS: Mutex<Vec<JumpList>> = Mutex::new(Vec::new());
}

impl JumpList {
    pub fn new(app_id: &str) -> Self {
        Self {
            app_id: String::from(app_id),
            actions: Vec::new(),
        }
    }

    pub fn add_task(&mut self, label: &str, icon: &str, command: &str) {
        self.actions.push(QuickAction {
            action_type: ActionType::Task,
            label: String::from(label),
            icon: String::from(icon),
            command: String::from(command),
            pinned: false,
        });
    }

    pub fn add_recent(&mut self, label: &str, path: &str) {
        // Keep max 10 recent items
        let recent_count = self
            .actions
            .iter()
            .filter(|a| a.action_type == ActionType::RecentDocument)
            .count();
        if recent_count >= 10 {
            if let Some(pos) = self
                .actions
                .iter()
                .position(|a| a.action_type == ActionType::RecentDocument && !a.pinned)
            {
                self.actions.remove(pos);
            }
        }
        self.actions.push(QuickAction {
            action_type: ActionType::RecentDocument,
            label: String::from(label),
            icon: String::from("document"),
            command: String::from(path),
            pinned: false,
        });
    }

    pub fn pin_item(&mut self, label: &str) {
        if let Some(action) = self.actions.iter_mut().find(|a| a.label == label) {
            action.pinned = true;
            action.action_type = ActionType::PinnedItem;
        }
    }

    pub fn unpin_item(&mut self, label: &str) {
        if let Some(action) = self
            .actions
            .iter_mut()
            .find(|a| a.label == label && a.pinned)
        {
            action.pinned = false;
            action.action_type = ActionType::RecentDocument;
        }
    }
}

/// Register or update a jump list for an app
pub fn set_jump_list(list: JumpList) {
    let mut lists = JUMP_LISTS.lock();
    if let Some(existing) = lists.iter_mut().find(|l| l.app_id == list.app_id) {
        *existing = list;
    } else {
        lists.push(list);
    }
}

/// Get jump list for an app
pub fn get_jump_list(app_id: &str) -> Option<JumpList> {
    JUMP_LISTS
        .lock()
        .iter()
        .find(|l| l.app_id == app_id)
        .cloned()
}

pub fn init() {
    crate::serial_println!("[JUMPLIST] Application quick actions loaded");
}
