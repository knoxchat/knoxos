/// Notification System — toast data and notification center state.
///
/// D9.4: paint and pointer handling live in `gpui_platform/notify.rs`. This
/// module keeps the queue the rest of the kernel (and D-Bus) feeds.
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use spin::Mutex;

// ═══════════════════════════════════════════════════════════════════════════
// PER-APP NOTIFICATION PREFERENCES
// ═══════════════════════════════════════════════════════════════════════════

/// Per-application notification preferences
#[derive(Debug, Clone)]
pub struct AppNotifPrefs {
    /// Whether notifications from this app are muted entirely
    pub muted: bool,
    /// Whether to show toasts (banner-style popups)
    pub show_toasts: bool,
    /// Whether to play a sound (future: when audio is implemented)
    pub play_sound: bool,
    /// Override urgency: None = use notification's own urgency
    pub urgency_override: Option<NotificationUrgency>,
    /// Whether this app can send Critical notifications
    pub allow_critical: bool,
}

impl Default for AppNotifPrefs {
    fn default() -> Self {
        Self {
            muted: false,
            show_toasts: true,
            play_sound: true,
            urgency_override: None,
            allow_critical: true,
        }
    }
}

lazy_static::lazy_static! {
    /// Per-app notification preferences: app_name → prefs
    pub static ref APP_NOTIF_PREFS: Mutex<BTreeMap<String, AppNotifPrefs>> =
        Mutex::new(BTreeMap::new());
}

/// Get (or create default) preferences for an app
pub fn get_app_prefs(app_name: &str) -> AppNotifPrefs {
    let prefs = APP_NOTIF_PREFS.lock();
    prefs.get(app_name).cloned().unwrap_or_default()
}

/// Set preferences for an app
pub fn set_app_prefs(app_name: &str, prefs: AppNotifPrefs) {
    APP_NOTIF_PREFS.lock().insert(String::from(app_name), prefs);
}

/// Mute notifications from a specific app
pub fn mute_app(app_name: &str) {
    let mut prefs_map = APP_NOTIF_PREFS.lock();
    let entry = prefs_map.entry(String::from(app_name)).or_default();
    entry.muted = true;
}

/// Unmute notifications from a specific app
pub fn unmute_app(app_name: &str) {
    let mut prefs_map = APP_NOTIF_PREFS.lock();
    let entry = prefs_map.entry(String::from(app_name)).or_default();
    entry.muted = false;
}

/// Disable toasts for an app (still appears in notification center)
pub fn disable_toasts(app_name: &str) {
    let mut prefs_map = APP_NOTIF_PREFS.lock();
    let entry = prefs_map.entry(String::from(app_name)).or_default();
    entry.show_toasts = false;
}

/// Get list of all apps that have sent notifications (for settings UI)
pub fn known_app_names() -> Vec<String> {
    let nc = NOTIFICATIONS.lock();
    let mut names: Vec<String> = nc
        .notifications
        .iter()
        .map(|n| n.app_name.clone())
        .collect();
    names.sort();
    names.dedup();
    names
}

/// Notification urgency level
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotificationUrgency {
    Low,
    Normal,
    Critical,
}

/// Notification icon type (determines the accent color and small icon)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotificationIcon {
    Info,
    Success,
    Warning,
    Error,
    System,
    App,
    Network,
    Battery,
    Volume,
    Update,
}

/// Action button on a notification
#[derive(Clone)]
pub struct NotificationAction {
    pub label: String,
    pub action_id: String,
    /// If true, style as primary (filled cyan); else secondary (outlined)
    pub primary: bool,
}

/// A single notification
#[derive(Clone)]
pub struct Notification {
    pub id: u32,
    pub title: String,
    pub body: String,
    pub icon: NotificationIcon,
    pub urgency: NotificationUrgency,
    /// Tick when this notification was created
    pub created_tick: u64,
    /// Duration in ticks before auto-dismiss (0 = manual dismiss only)
    pub duration_ticks: u64,
    /// Whether this notification's toast has been dismissed (no longer a toast)
    pub dismissed: bool,
    /// Whether the user has seen this in the notification center
    pub read: bool,
    /// Current slide-in animation progress (0-255, 255 = fully visible)
    pub anim_progress: u8,
    /// Whether this is sliding out (dismissing)
    pub sliding_out: bool,
    /// App name that sent the notification
    pub app_name: String,
    /// Whether to show a progress bar
    pub has_progress: bool,
    /// Progress value (0-100)
    pub progress: u8,
    /// Action buttons (max 3)
    pub actions: Vec<NotificationAction>,
}

const TOAST_MAX_VISIBLE: usize = 5;
/// Auto-dismiss after ~5 seconds (90 ticks at 18.2Hz)
const DEFAULT_DURATION: u64 = 90;
/// Critical notifications stay for ~15 seconds
const CRITICAL_DURATION: u64 = 270;
/// Maximum number of notifications kept in history
const HISTORY_MAX: usize = 200;

/// Notification center state
pub struct NotificationCenter {
    /// All notifications (active + history)
    pub notifications: Vec<Notification>,
    /// Next notification ID
    next_id: u32,
    /// Whether the notification center panel is open
    pub panel_open: bool,
    /// Unread count badge
    pub unread_count: u32,
    /// "Do not disturb" mode
    pub dnd_mode: bool,
    /// Notification center scroll offset
    pub scroll_offset: usize,
    /// Group notifications by app in the panel
    pub group_by_app: bool,
    /// Collapsed app groups (app names whose groups are collapsed)
    pub collapsed_groups: Vec<String>,
}

lazy_static::lazy_static! {
    pub static ref NOTIFICATIONS: Mutex<NotificationCenter> = Mutex::new(NotificationCenter {
        notifications: Vec::new(),
        next_id: 1,
        panel_open: false,
        unread_count: 0,
        dnd_mode: false,
        scroll_offset: 0,
        group_by_app: false,
        collapsed_groups: Vec::new(),
    });
}

impl NotificationCenter {
    /// Push a new notification (respects per-app preferences)
    pub fn push(
        &mut self,
        title: &str,
        body: &str,
        icon: NotificationIcon,
        urgency: NotificationUrgency,
    ) -> u32 {
        self.push_from_app("System", title, body, icon, urgency)
    }

    /// Push a notification from a named app (checks per-app prefs)
    pub fn push_from_app(
        &mut self,
        app_name: &str,
        title: &str,
        body: &str,
        icon: NotificationIcon,
        urgency: NotificationUrgency,
    ) -> u32 {
        // Check per-app preferences
        let prefs = get_app_prefs(app_name);
        if prefs.muted {
            // Still assign an ID but mark as dismissed immediately
            let id = self.next_id;
            self.next_id += 1;
            return id;
        }

        // Apply urgency override or restrict critical
        let effective_urgency = if let Some(ovr) = prefs.urgency_override {
            ovr
        } else if urgency == NotificationUrgency::Critical && !prefs.allow_critical {
            NotificationUrgency::Normal
        } else {
            urgency
        };

        let id = self.next_id;
        self.next_id += 1;

        let duration = match effective_urgency {
            NotificationUrgency::Critical => CRITICAL_DURATION,
            NotificationUrgency::Normal => DEFAULT_DURATION,
            NotificationUrgency::Low => DEFAULT_DURATION / 2,
        };

        let tick = crate::interrupts::get_ticks();

        // If toasts are disabled, notification goes straight to center (dismissed from toast)
        let start_dismissed = !prefs.show_toasts;

        self.notifications.push(Notification {
            id,
            title: String::from(title),
            body: String::from(body),
            icon,
            urgency: effective_urgency,
            created_tick: tick,
            duration_ticks: if start_dismissed { 0 } else { duration },
            dismissed: start_dismissed,
            read: start_dismissed,
            anim_progress: if start_dismissed { 255 } else { 0 },
            sliding_out: false,
            app_name: String::from(app_name),
            has_progress: false,
            progress: 0,
            actions: Vec::new(),
        });

        if !self.dnd_mode {
            self.unread_count += 1;
            // Play notification sound
            super::sounds::notification();
        }

        id
    }

    /// Push a notification with progress bar
    pub fn push_with_progress(
        &mut self,
        title: &str,
        body: &str,
        icon: NotificationIcon,
        progress: u8,
    ) -> u32 {
        let id = self.push(title, body, icon, NotificationUrgency::Normal);
        if let Some(n) = self.notifications.iter_mut().find(|n| n.id == id) {
            n.has_progress = true;
            n.progress = progress.min(100);
            n.duration_ticks = 0; // Don't auto-dismiss progress notifications
        }
        id
    }

    /// Push a notification with action buttons
    pub fn push_with_actions(
        &mut self,
        title: &str,
        body: &str,
        icon: NotificationIcon,
        urgency: NotificationUrgency,
        actions: &[(&str, &str, bool)], // (label, action_id, primary)
    ) -> u32 {
        let id = self.push(title, body, icon, urgency);
        if let Some(n) = self.notifications.iter_mut().find(|n| n.id == id) {
            n.actions = actions
                .iter()
                .take(3) // Max 3 buttons
                .map(|(label, action_id, primary)| NotificationAction {
                    label: String::from(*label),
                    action_id: String::from(*action_id),
                    primary: *primary,
                })
                .collect();
            // Don't auto-dismiss notifications with actions
            n.duration_ticks = 0;
        }
        id
    }

    /// Update progress on an existing notification
    pub fn update_progress(&mut self, id: u32, progress: u8) {
        if let Some(n) = self.notifications.iter_mut().find(|n| n.id == id) {
            n.progress = progress.min(100);
            if progress >= 100 {
                n.duration_ticks = DEFAULT_DURATION;
                n.created_tick = crate::interrupts::get_ticks();
            }
        }
    }

    /// Dismiss a notification by ID
    pub fn dismiss(&mut self, id: u32) {
        if let Some(n) = self.notifications.iter_mut().find(|n| n.id == id) {
            if !n.dismissed {
                n.sliding_out = true;
            }
        }
    }

    /// Clear all active toasts (dismiss them); history is preserved
    pub fn clear_all(&mut self) {
        for n in self.notifications.iter_mut() {
            n.dismissed = true;
            n.read = true;
            n.sliding_out = false;
            n.anim_progress = 0;
        }
        self.unread_count = 0;
    }

    /// Purge all notification history (remove everything)
    pub fn purge_history(&mut self) {
        self.notifications.retain(|n| !n.dismissed);
        self.unread_count = self.notifications.iter().filter(|n| !n.read).count() as u32;
    }

    /// Get all notifications for the panel (both active and dismissed history)
    pub fn all_for_panel(&self) -> Vec<&Notification> {
        self.notifications.iter().rev().collect()
    }

    /// Mark all currently visible panel notifications as read
    pub fn mark_visible_read(&mut self) {
        for n in self.notifications.iter_mut() {
            if !n.read {
                n.read = true;
            }
        }
        self.unread_count = 0;
    }

    /// Dismiss a single notification from the panel (by ID) — keeps in history
    pub fn dismiss_from_panel(&mut self, id: u32) {
        if let Some(n) = self.notifications.iter_mut().find(|n| n.id == id) {
            n.dismissed = true;
            n.read = true;
            n.sliding_out = false;
            n.anim_progress = 0;
        }
    }

    /// Handle scroll in notification panel
    pub fn scroll_panel(&mut self, delta: i32) {
        let total = self.notifications.len();
        if delta > 0 {
            // Scroll down
            if self.scroll_offset + 6 < total {
                self.scroll_offset += 1;
            }
        } else if delta < 0 {
            // Scroll up
            self.scroll_offset = self.scroll_offset.saturating_sub(1);
        }
    }

    /// Get active (non-dismissed) toast notifications to display
    pub fn active_toasts(&self) -> Vec<&Notification> {
        self.notifications
            .iter()
            .filter(|n| !n.dismissed)
            .rev()
            .take(TOAST_MAX_VISIBLE)
            .collect()
    }

    /// Check if there are any visible (non-dismissed) toasts
    pub fn has_visible_toasts(&self) -> bool {
        self.notifications.iter().any(|n| !n.dismissed)
    }

    /// Tick animation and auto-dismiss logic
    pub fn tick(&mut self) {
        let current_tick = crate::interrupts::get_ticks();

        for n in self.notifications.iter_mut() {
            if n.dismissed {
                continue;
            }

            // Slide-in animation
            if !n.sliding_out && n.anim_progress < 255 {
                n.anim_progress = n.anim_progress.saturating_add(30);
            }

            // Slide-out animation
            if n.sliding_out {
                if n.anim_progress == 0 {
                    n.dismissed = true;
                    continue;
                }
                n.anim_progress = n.anim_progress.saturating_sub(30);
                if n.anim_progress == 0 {
                    n.dismissed = true;
                }
                continue;
            }

            // Auto-dismiss after duration
            if n.duration_ticks > 0 && current_tick - n.created_tick > n.duration_ticks {
                n.sliding_out = true;
            }
        }

        // Prune oldest dismissed notifications beyond history limit
        while self.notifications.len() > HISTORY_MAX {
            if let Some(pos) = self.notifications.iter().position(|n| n.dismissed) {
                self.notifications.remove(pos);
            } else {
                break;
            }
        }
    }
}

/// Toggle notification panel visibility
pub fn toggle_panel() {
    let mut nc = NOTIFICATIONS.lock();
    nc.panel_open = !nc.panel_open;
    if nc.panel_open {
        nc.mark_visible_read();
    }
    drop(nc);
    crate::gui::gpui_platform::notify_damage();
}

/// Close notification panel
pub fn close_panel() {
    let mut nc = NOTIFICATIONS.lock();
    if nc.panel_open {
        nc.panel_open = false;
        drop(nc);
        crate::gui::gpui_platform::notify_damage();
    }
}

/// Get unread notification count (for badge display)
pub fn unread_count() -> u32 {
    NOTIFICATIONS.lock().unread_count
}

/// Check if panel is open
pub fn is_panel_open() -> bool {
    NOTIFICATIONS.lock().panel_open
}

/// Whether any toast banner is still on screen.
pub fn has_visible_toasts() -> bool {
    NOTIFICATIONS.lock().has_visible_toasts()
}

// ═══════════════════════════════════════════════════════════════════════════
// CONVENIENCE API — Easy notification creation
// ═══════════════════════════════════════════════════════════════════════════

/// Show an info notification
pub fn info(title: &str, body: &str) {
    NOTIFICATIONS.lock().push(
        title,
        body,
        NotificationIcon::Info,
        NotificationUrgency::Normal,
    );
    super::request_redraw();
}

/// Show a success notification
pub fn success(title: &str, body: &str) {
    NOTIFICATIONS.lock().push(
        title,
        body,
        NotificationIcon::Success,
        NotificationUrgency::Normal,
    );
    super::request_redraw();
}

/// Show a warning notification
pub fn warning(title: &str, body: &str) {
    NOTIFICATIONS.lock().push(
        title,
        body,
        NotificationIcon::Warning,
        NotificationUrgency::Normal,
    );
    super::request_redraw();
}

/// Show an error notification
pub fn error(title: &str, body: &str) {
    NOTIFICATIONS.lock().push(
        title,
        body,
        NotificationIcon::Error,
        NotificationUrgency::Critical,
    );
    super::request_redraw();
}

/// Show a system notification
pub fn system(title: &str, body: &str) {
    NOTIFICATIONS.lock().push(
        title,
        body,
        NotificationIcon::System,
        NotificationUrgency::Normal,
    );
    super::request_redraw();
}

/// Show a notification with action buttons
/// `actions` is a slice of (label, action_id, is_primary) tuples
pub fn with_actions(
    title: &str,
    body: &str,
    icon: NotificationIcon,
    actions: &[(&str, &str, bool)],
) {
    NOTIFICATIONS
        .lock()
        .push_with_actions(title, body, icon, NotificationUrgency::Normal, actions);
    super::request_redraw();
}

/// Initialize the notification system with a welcome notification
pub fn init() {
    crate::serial_println!("[KnoxOS] Notification system initialized");
}

// ═══════════════════════════════════════════════════════════════════════════
// APP-SOURCED NOTIFICATION API
// ═══════════════════════════════════════════════════════════════════════════

/// Show an info notification from a named application
pub fn app_info(app_name: &str, title: &str, body: &str) {
    NOTIFICATIONS.lock().push_from_app(
        app_name,
        title,
        body,
        NotificationIcon::App,
        NotificationUrgency::Normal,
    );
    super::request_redraw();
}

/// Show a warning notification from a named application
pub fn app_warning(app_name: &str, title: &str, body: &str) {
    NOTIFICATIONS.lock().push_from_app(
        app_name,
        title,
        body,
        NotificationIcon::Warning,
        NotificationUrgency::Normal,
    );
    super::request_redraw();
}

/// Show an error notification from a named application
pub fn app_error(app_name: &str, title: &str, body: &str) {
    NOTIFICATIONS.lock().push_from_app(
        app_name,
        title,
        body,
        NotificationIcon::Error,
        NotificationUrgency::Critical,
    );
    super::request_redraw();
}

/// Get a summary of per-app notification counts (for settings UI)
pub fn app_notification_summary() -> Vec<(String, u32, bool)> {
    let nc = NOTIFICATIONS.lock();
    let prefs = APP_NOTIF_PREFS.lock();

    let mut counts: BTreeMap<String, u32> = BTreeMap::new();
    for n in nc.notifications.iter() {
        *counts.entry(n.app_name.clone()).or_insert(0) += 1;
    }

    counts
        .into_iter()
        .map(|(name, count)| {
            let muted = prefs.get(&name).map(|p| p.muted).unwrap_or(false);
            (name, count, muted)
        })
        .collect()
}

// ═══════════════════════════════════════════════════════════════════════
// NOTIFICATION SOUND EFFECTS (via HDA / PulseAudio)
// ═══════════════════════════════════════════════════════════════════════

/// Notification sound type
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum NotifSound {
    Default,
    Message,
    Email,
    Alarm,
    Reminder,
    None,
}

/// Play a notification sound based on urgency and app preference
pub fn play_notification_sound(app: &str, urgency: NotificationUrgency) {
    let prefs = APP_NOTIF_PREFS.lock();
    if let Some(pref) = prefs.get(app) {
        if !pref.play_sound {
            return;
        }
    }
    drop(prefs);

    // Pick sound based on urgency
    match urgency {
        NotificationUrgency::Critical => {
            super::sounds::error();
            crate::serial_println!("[notif] Playing critical alert sound");
        }
        NotificationUrgency::Normal | NotificationUrgency::Low => {
            super::sounds::notification();
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════
// LOCK SCREEN NOTIFICATIONS
// ═══════════════════════════════════════════════════════════════════════

lazy_static::lazy_static! {
    /// Notifications queued for lock screen display
    static ref LOCK_SCREEN_QUEUE: Mutex<Vec<LockScreenNotif>> = Mutex::new(Vec::new());
}

/// A notification visible on the lock screen
#[derive(Clone)]
pub struct LockScreenNotif {
    pub app_name: String,
    pub title: String,
    pub body: String,
    pub urgency: NotificationUrgency,
    pub timestamp: u64,
    pub show_content: bool, // false = "1 new notification" for privacy
}

/// Queue a notification for lock screen display
pub fn queue_lock_screen_notification(
    app: &str,
    title: &str,
    body: &str,
    urgency: NotificationUrgency,
    show_content: bool,
) {
    let mut queue = LOCK_SCREEN_QUEUE.lock();
    if queue.len() >= 10 {
        queue.remove(0); // Drop oldest
    }
    queue.push(LockScreenNotif {
        app_name: String::from(app),
        title: String::from(title),
        body: if show_content {
            String::from(body)
        } else {
            String::from("New notification")
        },
        urgency,
        timestamp: crate::hpet::read_counter(),
        show_content,
    });
}

/// Get lock screen notifications (for lock_screen.rs to render)
pub fn get_lock_screen_notifications() -> Vec<LockScreenNotif> {
    LOCK_SCREEN_QUEUE.lock().clone()
}

/// Clear lock screen notification queue (on unlock)
pub fn clear_lock_screen_queue() {
    LOCK_SCREEN_QUEUE.lock().clear();
}

/// Draw lock screen notifications
pub fn draw_lock_screen_notifications(
    fb: &mut super::framebuffer::FrameBuffer,
    center_x: usize,
    start_y: usize,
) {
    let queue = LOCK_SCREEN_QUEUE.lock();
    let notifs = queue.clone();
    drop(queue);

    let mut y = start_y;
    for notif in notifs.iter().take(5) {
        let text_color = super::framebuffer::Pixel::new(220, 220, 220, 255);
        let muted_color = super::framebuffer::Pixel::new(160, 160, 160, 255);

        // Draw app name + title
        let header = alloc::format!("{}: {}", notif.app_name, notif.title);
        let x = center_x.saturating_sub(header.len() * 4);
        super::fonts::draw_string_compact(fb, x as i32, y as i32, &header, text_color, 1);

        // Draw body (truncated)
        if notif.show_content && !notif.body.is_empty() {
            let body_display: String = if notif.body.len() > 60 {
                let mut s = String::from(&notif.body[..57]);
                s.push_str("...");
                s
            } else {
                notif.body.clone()
            };
            super::fonts::draw_string_compact(
                fb,
                x as i32,
                (y + 16) as i32,
                &body_display,
                muted_color,
                1,
            );
            y += 36;
        } else {
            y += 22;
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════
// VPN CONNECTION INDICATOR (for system tray)
// ═══════════════════════════════════════════════════════════════════════

static VPN_CONNECTED: core::sync::atomic::AtomicBool = core::sync::atomic::AtomicBool::new(false);
static MIC_MUTED: core::sync::atomic::AtomicBool = core::sync::atomic::AtomicBool::new(false);

/// Set VPN connection status (for system tray indicator)
pub fn set_vpn_status(connected: bool) {
    VPN_CONNECTED.store(connected, core::sync::atomic::Ordering::Relaxed);
}

/// Is VPN connected?
pub fn is_vpn_connected() -> bool {
    VPN_CONNECTED.load(core::sync::atomic::Ordering::Relaxed)
}

/// Set microphone mute status (for system tray indicator)
pub fn set_mic_muted(muted: bool) {
    MIC_MUTED.store(muted, core::sync::atomic::Ordering::Relaxed);
}

/// Is microphone muted?
pub fn is_mic_muted() -> bool {
    MIC_MUTED.load(core::sync::atomic::Ordering::Relaxed)
}
