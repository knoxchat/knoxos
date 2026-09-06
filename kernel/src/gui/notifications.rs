/// Notification System — Toast notifications and notification center
/// Provides non-intrusive desktop notifications with auto-dismiss, stacking,
/// click-to-dismiss, and a notification center accessible from the system tray.
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use spin::Mutex;

use super::colors;
use super::fonts;
use super::framebuffer::{FrameBuffer, Pixel, Rect};

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

/// Toast notification display constants — scaled for 1920×1080
const TOAST_WIDTH: u32 = 400;
const TOAST_HEIGHT: u32 = 90;
const TOAST_MARGIN: i32 = 16;
const TOAST_PADDING: i32 = 14;
const TOAST_MAX_VISIBLE: usize = 5;
const TOAST_CORNER_RADIUS: u32 = 10;
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

/// Get the accent color for a notification icon type
fn icon_accent(icon: NotificationIcon) -> Pixel {
    match icon {
        NotificationIcon::Info => Pixel::rgb(82, 139, 255),
        NotificationIcon::Success => Pixel::rgb(158, 206, 106),
        NotificationIcon::Warning => Pixel::rgb(224, 175, 104),
        NotificationIcon::Error => Pixel::rgb(247, 118, 142),
        NotificationIcon::System => Pixel::rgb(187, 154, 247),
        NotificationIcon::App => Pixel::rgb(112, 184, 255),
        NotificationIcon::Network => Pixel::rgb(125, 207, 255),
        NotificationIcon::Battery => Pixel::rgb(158, 206, 106),
        NotificationIcon::Volume => Pixel::rgb(192, 202, 218),
        NotificationIcon::Update => Pixel::rgb(112, 203, 255),
    }
}

/// Draw a small icon (16×16) for the notification type
fn draw_notification_icon(fb: &mut FrameBuffer, x: i32, y: i32, icon: NotificationIcon) {
    let accent = icon_accent(icon);

    match icon {
        NotificationIcon::Info => {
            fb.fill_circle_aa(x + 8, y + 8, 7, accent);
            fonts::draw_char_bold_compact(fb, x + 4, y + 3, 'i', Pixel::rgb(255, 255, 255), 1);
        }
        NotificationIcon::Success => {
            fb.fill_circle_aa(x + 8, y + 8, 7, accent);
            // Checkmark
            fb.draw_line_aa(x + 4, y + 8, x + 7, y + 11, Pixel::rgb(255, 255, 255));
            fb.draw_line_aa(x + 7, y + 11, x + 12, y + 5, Pixel::rgb(255, 255, 255));
        }
        NotificationIcon::Warning => {
            // Triangle
            for i in 0..14 {
                let w = (i * 14 / 14) as u32;
                fb.fill_rect(
                    Rect::new(x + 8 - w as i32 / 2, y + 2 + i, w.max(1), 1),
                    accent,
                );
            }
            fonts::draw_char_bold_compact(fb, x + 5, y + 5, '!', Pixel::rgb(0, 0, 0), 1);
        }
        NotificationIcon::Error => {
            fb.fill_circle_aa(x + 8, y + 8, 7, accent);
            // X mark
            let white = Pixel::rgb(255, 255, 255);
            fb.draw_line_aa(x + 4, y + 4, x + 12, y + 12, white);
            fb.draw_line_aa(x + 12, y + 4, x + 4, y + 12, white);
        }
        NotificationIcon::System | NotificationIcon::App => {
            fb.fill_rounded_rect_aa(Rect::new(x + 1, y + 1, 14, 14), Pixel::rgb(60, 60, 60), 3);
            fb.draw_rounded_rect(Rect::new(x + 1, y + 1, 14, 14), accent, 3, 1);
            // Gear tooth representation
            fb.fill_circle_aa(x + 8, y + 8, 3, accent);
        }
        NotificationIcon::Network => {
            // WiFi bars — rounded
            for i in 0..4u32 {
                let bar_h = 3 + i * 2;
                fb.fill_rounded_rect_aa(
                    Rect::new(x + 2 + i as i32 * 3, y + 14 - bar_h as i32, 2, bar_h),
                    accent,
                    1,
                );
            }
        }
        NotificationIcon::Battery => {
            fb.fill_rounded_rect_aa(Rect::new(x + 2, y + 4, 12, 8), accent, 2);
            fb.draw_rounded_rect(
                Rect::new(x + 2, y + 4, 12, 8),
                Pixel::rgb(255, 255, 255),
                2,
                1,
            );
            fb.fill_rounded_rect_aa(Rect::new(x + 14, y + 6, 2, 4), Pixel::rgb(255, 255, 255), 1);
        }
        NotificationIcon::Volume => {
            fb.fill_rounded_rect_aa(Rect::new(x + 3, y + 6, 4, 6), accent, 1);
            fb.fill_rounded_rect_aa(Rect::new(x + 7, y + 4, 2, 10), accent, 1);
            // Sound waves
            fb.draw_circle_aa(x + 12, y + 8, 3, accent);
        }
        NotificationIcon::Update => {
            fb.draw_circle_aa(x + 8, y + 8, 6, accent);
            // Down arrow — rounded shaft
            fb.fill_rounded_rect_aa(Rect::new(x + 7, y + 4, 2, 6), accent, 1);
            fb.draw_line_aa(x + 4, y + 9, x + 8, y + 13, accent);
            fb.draw_line_aa(x + 12, y + 9, x + 8, y + 13, accent);
        }
    }
}

/// Draw active toast notifications on the screen (top-right corner)
pub fn draw_toasts(fb: &mut FrameBuffer) {
    let nc = NOTIFICATIONS.lock();
    let toasts = nc.active_toasts();

    if toasts.is_empty() {
        return;
    }

    let screen_w = fb.width as i32;
    let base_x = screen_w - TOAST_WIDTH as i32 - TOAST_MARGIN;

    let mut cumulative_y = 0i32;

    for (i, toast) in toasts.iter().enumerate() {
        // Dynamic height: taller when actions are present
        let has_actions = !toast.actions.is_empty();
        let toast_h = if has_actions {
            TOAST_HEIGHT + 32 // Extra row for action buttons
        } else {
            TOAST_HEIGHT
        };

        let anim_offset = if toast.anim_progress < 255 {
            let progress = toast.anim_progress as i32;
            (TOAST_WIDTH as i32 + TOAST_MARGIN) * (255 - progress) / 255
        } else {
            0
        };

        let toast_x = base_x + anim_offset;
        let toast_y = TOAST_MARGIN + cumulative_y;
        cumulative_y += toast_h as i32 + 8;

        // Shadow (AA)
        fb.fill_rounded_rect_aa(
            Rect::new(toast_x + 3, toast_y + 3, TOAST_WIDTH, toast_h),
            Pixel::new(0, 0, 0, 80),
            TOAST_CORNER_RADIUS,
        );

        // Background (AA)
        fb.fill_rounded_rect_aa(
            Rect::new(toast_x, toast_y, TOAST_WIDTH, toast_h),
            Pixel::new(38, 38, 42, 240),
            TOAST_CORNER_RADIUS,
        );

        // Border (AA rounded)
        fb.draw_rounded_rect(
            Rect::new(toast_x, toast_y, TOAST_WIDTH, toast_h),
            Pixel::rgb(60, 60, 64),
            TOAST_CORNER_RADIUS,
            1,
        );

        // Left accent bar (3px) — rounded
        let accent = icon_accent(toast.icon);
        fb.fill_rounded_rect_aa(
            Rect::new(
                toast_x + 1,
                toast_y + TOAST_CORNER_RADIUS as i32,
                3,
                toast_h - TOAST_CORNER_RADIUS * 2,
            ),
            accent,
            1,
        );

        // Icon (16×16)
        draw_notification_icon(
            fb,
            toast_x + TOAST_PADDING,
            toast_y + TOAST_PADDING,
            toast.icon,
        );

        // Title (bold white)
        let title_x = toast_x + TOAST_PADDING + 20;
        let title_y = toast_y + TOAST_PADDING;
        let max_title_chars = ((TOAST_WIDTH as i32 - TOAST_PADDING * 2 - 24) / 8) as usize;
        let title = if toast.title.len() > max_title_chars {
            let mut t = String::from(&toast.title[..max_title_chars.saturating_sub(3)]);
            t.push_str("...");
            t
        } else {
            toast.title.clone()
        };
        fonts::draw_string_bold_compact(fb, title_x, title_y, &title, colors::WHITE, 1);

        // Body text (gray, up to 2 lines)
        let body_y = title_y + 16;
        let max_body_chars = ((TOAST_WIDTH as i32 - TOAST_PADDING * 2 - 24) / 8) as usize;
        let body_line1 = if toast.body.len() > max_body_chars {
            let mut b = String::from(&toast.body[..max_body_chars.saturating_sub(3)]);
            b.push_str("...");
            b
        } else {
            toast.body.clone()
        };
        fonts::draw_string_compact(
            fb,
            title_x,
            body_y,
            &body_line1,
            Pixel::rgb(180, 180, 180),
            1,
        );

        // App name (dim, bottom-left)
        let app_y = toast_y + toast_h as i32 - TOAST_PADDING - 12;
        fonts::draw_string_compact(
            fb,
            title_x,
            app_y,
            &toast.app_name,
            Pixel::rgb(100, 100, 100),
            1,
        );

        // Time ago (dim, bottom-right)
        let current_tick = crate::interrupts::get_ticks();
        let elapsed_secs = (current_tick - toast.created_tick) / 18;
        let time_str = if elapsed_secs < 60 {
            alloc::format!("{}s ago", elapsed_secs)
        } else {
            alloc::format!("{}m ago", elapsed_secs / 60)
        };
        let time_w = time_str.len() as i32 * 8;
        fonts::draw_string_compact(
            fb,
            toast_x + TOAST_WIDTH as i32 - TOAST_PADDING - time_w,
            app_y,
            &time_str,
            Pixel::rgb(100, 100, 100),
            1,
        );

        // Progress bar (if enabled)
        if toast.has_progress {
            let bar_y = body_y + 16;
            let bar_w = TOAST_WIDTH - (TOAST_PADDING as u32 * 2) - 24;
            fb.fill_rounded_rect_aa(
                Rect::new(title_x, bar_y, bar_w, 4),
                Pixel::rgb(50, 50, 55),
                2,
            );
            let fill_w = bar_w * toast.progress as u32 / 100;
            if fill_w > 0 {
                fb.fill_rounded_rect_aa(Rect::new(title_x, bar_y, fill_w, 4), accent, 2);
            }
        }

        // Close button (×) in top-right corner
        let close_x = toast_x + TOAST_WIDTH as i32 - 16;
        let close_y = toast_y + 6;
        let close_color = Pixel::rgb(120, 120, 120);
        fb.draw_line_aa(close_x, close_y, close_x + 8, close_y + 8, close_color);
        fb.draw_line_aa(close_x + 8, close_y, close_x, close_y + 8, close_color);

        // Action buttons (if any) — drawn in a row at the bottom
        if has_actions {
            let btn_y = toast_y + TOAST_HEIGHT as i32 - 2; // Below the standard content
            let btn_h: u32 = 24;
            let btn_gap: i32 = 8;
            let available_w = TOAST_WIDTH as i32 - TOAST_PADDING * 2 - 24;
            let btn_count = toast.actions.len() as i32;
            let btn_w = ((available_w - btn_gap * (btn_count - 1)) / btn_count).max(50) as u32;

            for (j, action) in toast.actions.iter().enumerate() {
                let btn_x = title_x + j as i32 * (btn_w as i32 + btn_gap);
                let btn_rect = Rect::new(btn_x, btn_y, btn_w, btn_h);

                if action.primary {
                    // Primary: filled cyan button
                    fb.fill_rounded_rect_aa(btn_rect, Pixel::new(0, 180, 255, 200), 5);
                    fb.draw_rounded_rect(btn_rect, Pixel::new(0, 220, 255, 120), 5, 1);
                    // Label centered
                    let label_w = fonts::measure_string_width_compact(&action.label, 1) as i32;
                    let label_x = btn_x + (btn_w as i32 - label_w) / 2;
                    fonts::draw_string_bold_compact(
                        fb,
                        label_x,
                        btn_y + 6,
                        &action.label,
                        Pixel::rgb(255, 255, 255),
                        1,
                    );
                } else {
                    // Secondary: outlined button
                    fb.draw_rounded_rect(btn_rect, Pixel::new(120, 160, 220, 140), 5, 1);
                    let label_w = fonts::measure_string_width_compact(&action.label, 1) as i32;
                    let label_x = btn_x + (btn_w as i32 - label_w) / 2;
                    fonts::draw_string_bold_compact(
                        fb,
                        label_x,
                        btn_y + 6,
                        &action.label,
                        Pixel::new(160, 190, 230, 220),
                        1,
                    );
                }
            }
        }
    }
}

/// Draw the notification center panel (opened from system tray)
pub fn draw_notification_panel(fb: &mut FrameBuffer) {
    let nc = NOTIFICATIONS.lock();
    if !nc.panel_open {
        return;
    }

    let screen_w = fb.width as i32;
    let taskbar_y = fb.height as i32 - super::scale::taskbar_height() as i32;
    let panel_w: u32 = 380;
    let panel_h: u32 = 440;
    let panel_x = screen_w - panel_w as i32 - 8;
    let panel_y = taskbar_y - panel_h as i32 - 4;

    // Shadow (AA)
    fb.fill_rounded_rect_aa(
        Rect::new(panel_x + 4, panel_y + 4, panel_w, panel_h),
        Pixel::new(0, 0, 0, 90),
        8,
    );

    // Background (AA)
    fb.fill_rounded_rect_aa(
        Rect::new(panel_x, panel_y, panel_w, panel_h),
        Pixel::new(33, 33, 36, 245),
        8,
    );

    // Border (AA rounded)
    fb.draw_rounded_rect(
        Rect::new(panel_x, panel_y, panel_w, panel_h),
        Pixel::rgb(60, 60, 64),
        8,
        1,
    );

    // Header
    fonts::draw_string_bold_compact(
        fb,
        panel_x + 16,
        panel_y + 14,
        "Notifications",
        colors::WHITE,
        1,
    );

    // Unread count badge
    let unread = nc.unread_count;
    if unread > 0 {
        let badge_text = alloc::format!("{}", unread);
        let badge_w = (badge_text.len() as u32 * 8 + 10).max(20);
        let badge_x = panel_x + 120;
        fb.fill_rounded_rect_aa(
            Rect::new(badge_x, panel_y + 10, badge_w, 18),
            Pixel::rgb(82, 139, 255),
            9,
        );
        fonts::draw_string_bold_compact(
            fb,
            badge_x + (badge_w as i32 - badge_text.len() as i32 * 8) / 2,
            panel_y + 14,
            &badge_text,
            colors::WHITE,
            1,
        );
    }

    // DND toggle
    let dnd_text = if nc.dnd_mode { "DND: On" } else { "DND: Off" };
    let dnd_color = if nc.dnd_mode {
        Pixel::rgb(247, 118, 142)
    } else {
        Pixel::rgb(120, 120, 120)
    };
    fonts::draw_string_bold_compact(
        fb,
        panel_x + panel_w as i32 - 80,
        panel_y + 14,
        dnd_text,
        dnd_color,
        1,
    );

    // Clear all button
    fonts::draw_string_bold_compact(
        fb,
        panel_x + panel_w as i32 - 80,
        panel_y + 30,
        "Clear all",
        Pixel::rgb(82, 139, 255),
        1,
    );

    // Group toggle button
    let group_text = if nc.group_by_app { "Ungroup" } else { "Group" };
    fonts::draw_string_bold_compact(
        fb,
        panel_x + panel_w as i32 - 150,
        panel_y + 30,
        group_text,
        Pixel::rgb(120, 160, 220),
        1,
    );

    // Separator
    fb.draw_hline(
        panel_x + 12,
        panel_y + 46,
        panel_w - 24,
        Pixel::rgb(60, 60, 64),
    );

    let current_tick = crate::interrupts::get_ticks();

    if nc.notifications.is_empty() {
        fonts::draw_string_compact(
            fb,
            panel_x + panel_w as i32 / 2 - 60,
            panel_y + panel_h as i32 / 2 - 6,
            "No notifications",
            Pixel::rgb(100, 100, 100),
            1,
        );
    } else if nc.group_by_app {
        // ── Grouped view ── group notifications by app name
        // Build ordered groups: preserve order of most recent notification per app
        let mut group_order: Vec<String> = Vec::new();
        let mut group_map: BTreeMap<String, Vec<&Notification>> = BTreeMap::new();
        for notif in nc.notifications.iter().rev() {
            if !group_order.contains(&notif.app_name) {
                group_order.push(notif.app_name.clone());
            }
            group_map
                .entry(notif.app_name.clone())
                .or_default()
                .push(notif);
        }

        let mut draw_y = panel_y + 54;
        let content_bottom = panel_y + panel_h as i32 - 8;
        let item_h: i32 = 48;
        let group_header_h: i32 = 26;

        for app_name in &group_order {
            if draw_y >= content_bottom {
                break;
            }
            let notifs = match group_map.get(app_name) {
                Some(n) => n,
                None => continue,
            };
            let count = notifs.len();
            let is_collapsed = nc.collapsed_groups.contains(app_name);

            // Group header
            let arrow = if is_collapsed { "▸" } else { "▾" };
            let header_text = alloc::format!("{} {} ({})", arrow, app_name, count);
            let unread_in_group = notifs.iter().filter(|n| !n.read).count();

            fb.fill_rounded_rect_aa(
                Rect::new(panel_x + 8, draw_y, panel_w - 16, group_header_h as u32),
                Pixel::new(55, 55, 60, 180),
                4,
            );
            let header_color = if unread_in_group > 0 {
                colors::WHITE
            } else {
                Pixel::rgb(170, 170, 170)
            };
            fonts::draw_string_bold_compact(
                fb,
                panel_x + 16,
                draw_y + 7,
                &header_text,
                header_color,
                1,
            );
            // Unread dot on group header
            if unread_in_group > 0 {
                fb.fill_circle_aa(
                    panel_x + panel_w as i32 - 20,
                    draw_y + group_header_h / 2,
                    4,
                    Pixel::rgb(82, 139, 255),
                );
            }
            draw_y += group_header_h + 2;

            if !is_collapsed {
                for notif in notifs.iter().take(5) {
                    if draw_y + item_h > content_bottom {
                        break;
                    }
                    let item_rect =
                        Rect::new(panel_x + 12, draw_y, panel_w - 24, item_h as u32 - 4);
                    let bg = if !notif.read {
                        Pixel::new(50, 52, 60, 200)
                    } else {
                        Pixel::new(40, 40, 44, 180)
                    };
                    fb.fill_rounded_rect_aa(item_rect, bg, 4);

                    // Accent bar
                    let accent = icon_accent(notif.icon);
                    fb.fill_rounded_rect_aa(
                        Rect::new(panel_x + 12, draw_y + 4, 2, item_h as u32 - 12),
                        accent,
                        1,
                    );

                    // Icon
                    draw_notification_icon(fb, panel_x + 20, draw_y + 4, notif.icon);

                    // Title (compact)
                    let max_chars = 28;
                    let title = if notif.title.len() > max_chars {
                        let mut t = String::from(&notif.title[..max_chars - 3]);
                        t.push_str("...");
                        t
                    } else {
                        notif.title.clone()
                    };
                    let title_color = if !notif.read {
                        colors::WHITE
                    } else {
                        Pixel::rgb(190, 190, 190)
                    };
                    fonts::draw_string_bold_compact(
                        fb,
                        panel_x + 40,
                        draw_y + 4,
                        &title,
                        title_color,
                        1,
                    );

                    // Body
                    let body = if notif.body.len() > max_chars {
                        let mut b = String::from(&notif.body[..max_chars - 3]);
                        b.push_str("...");
                        b
                    } else {
                        notif.body.clone()
                    };
                    fonts::draw_string_compact(
                        fb,
                        panel_x + 40,
                        draw_y + 18,
                        &body,
                        Pixel::rgb(130, 130, 130),
                        1,
                    );

                    // Time ago
                    let elapsed_secs = (current_tick - notif.created_tick) / 18;
                    let time_str = if elapsed_secs < 60 {
                        alloc::format!("{}s", elapsed_secs)
                    } else if elapsed_secs < 3600 {
                        alloc::format!("{}m", elapsed_secs / 60)
                    } else if elapsed_secs < 86400 {
                        alloc::format!("{}h", elapsed_secs / 3600)
                    } else {
                        alloc::format!("{}d", elapsed_secs / 86400)
                    };
                    let time_w = time_str.len() as i32 * 8;
                    fonts::draw_string_compact(
                        fb,
                        panel_x + panel_w as i32 - 16 - time_w,
                        draw_y + 4,
                        &time_str,
                        Pixel::rgb(80, 80, 80),
                        1,
                    );

                    // Dismiss X
                    let close_x = panel_x + panel_w as i32 - 28;
                    let close_y = draw_y + 18;
                    let close_color = Pixel::rgb(80, 80, 80);
                    fb.draw_line_aa(close_x, close_y, close_x + 7, close_y + 7, close_color);
                    fb.draw_line_aa(close_x + 7, close_y, close_x, close_y + 7, close_color);

                    draw_y += item_h;
                }
                // Show "+N more" if group has more than 5
                if notifs.len() > 5 {
                    let more_text = alloc::format!("+{} more", notifs.len() - 5);
                    fonts::draw_string_compact(
                        fb,
                        panel_x + 40,
                        draw_y + 2,
                        &more_text,
                        Pixel::rgb(82, 139, 255),
                        1,
                    );
                    draw_y += 16;
                }
            }

            draw_y += 4; // Gap between groups
        }
    } else {
        // ── Chronological view ── standard list
        let items: Vec<&Notification> = nc
            .notifications
            .iter()
            .rev()
            .skip(nc.scroll_offset)
            .take(6)
            .collect();

        let total_count = nc.notifications.len();
        let item_h: i32 = 60;

        for (i, notif) in items.iter().enumerate() {
            let item_y = panel_y + 54 + (i as i32 * item_h);
            let item_rect = Rect::new(panel_x + 8, item_y, panel_w - 16, item_h as u32 - 4);

            // Item background — slightly brighter for unread
            let bg = if !notif.read {
                Pixel::new(50, 52, 60, 220)
            } else {
                Pixel::new(42, 42, 46, 200)
            };
            fb.fill_rounded_rect_aa(item_rect, bg, 6);

            // Icon
            draw_notification_icon(fb, panel_x + 16, item_y + 8, notif.icon);

            // Unread indicator — blue dot on the left
            if !notif.read {
                fb.fill_circle_aa(panel_x + 12, item_y + 28, 3, Pixel::rgb(82, 139, 255));
            }

            // Title
            let max_chars = 30;
            let title_color = if !notif.read {
                colors::WHITE
            } else {
                Pixel::rgb(200, 200, 200)
            };
            let title = if notif.title.len() > max_chars {
                let mut t = String::from(&notif.title[..max_chars - 3]);
                t.push_str("...");
                t
            } else {
                notif.title.clone()
            };
            fonts::draw_string_bold_compact(fb, panel_x + 36, item_y + 6, &title, title_color, 1);

            // Time ago — top right
            let elapsed_secs = (current_tick - notif.created_tick) / 18;
            let time_str = if elapsed_secs < 60 {
                alloc::format!("{}s", elapsed_secs)
            } else if elapsed_secs < 3600 {
                alloc::format!("{}m", elapsed_secs / 60)
            } else if elapsed_secs < 86400 {
                alloc::format!("{}h", elapsed_secs / 3600)
            } else {
                alloc::format!("{}d", elapsed_secs / 86400)
            };
            let time_w = time_str.len() as i32 * 8;
            fonts::draw_string_compact(
                fb,
                panel_x + panel_w as i32 - 16 - time_w,
                item_y + 6,
                &time_str,
                Pixel::rgb(90, 90, 90),
                1,
            );

            // Body
            let body = if notif.body.len() > max_chars {
                let mut b = String::from(&notif.body[..max_chars - 3]);
                b.push_str("...");
                b
            } else {
                notif.body.clone()
            };
            fonts::draw_string_compact(
                fb,
                panel_x + 36,
                item_y + 22,
                &body,
                Pixel::rgb(140, 140, 140),
                1,
            );

            // App name — bottom left
            fonts::draw_string_compact(
                fb,
                panel_x + 36,
                item_y + 38,
                &notif.app_name,
                Pixel::rgb(80, 80, 90),
                1,
            );

            // Dismiss X button — right side
            let close_x = panel_x + panel_w as i32 - 24;
            let close_y = item_y + 22;
            let close_color = Pixel::rgb(90, 90, 90);
            fb.draw_line_aa(close_x, close_y, close_x + 8, close_y + 8, close_color);
            fb.draw_line_aa(close_x + 8, close_y, close_x, close_y + 8, close_color);

            // Accent bar (left edge)
            let accent = icon_accent(notif.icon);
            fb.fill_rounded_rect_aa(
                Rect::new(panel_x + 8, item_y + 6, 2, item_h as u32 - 16),
                accent,
                1,
            );
        }

        // Scroll indicator (if there are more notifications)
        if total_count > 6 {
            let scrollbar_track_y = panel_y + 54;
            let scrollbar_track_h = (6 * 60) as u32;
            let thumb_h = (scrollbar_track_h * 6 / total_count as u32).max(20);
            let thumb_offset = if total_count > 6 {
                nc.scroll_offset as u32 * (scrollbar_track_h - thumb_h) / (total_count as u32 - 6)
            } else {
                0
            };
            fb.fill_rounded_rect_aa(
                Rect::new(
                    panel_x + panel_w as i32 - 6,
                    scrollbar_track_y + thumb_offset as i32,
                    3,
                    thumb_h,
                ),
                Pixel::new(80, 80, 80, 120),
                2,
            );
        }
    }
}

/// Handle a click on a toast notification — returns true if consumed.
/// Detects action button clicks and returns the action_id if clicked.
pub fn handle_toast_click(x: i32, y: i32, screen_w: i32) -> bool {
    let mut nc = NOTIFICATIONS.lock();
    let base_x = screen_w - TOAST_WIDTH as i32 - TOAST_MARGIN;

    // Collect toast info: (id, has_actions, action_count)
    let toasts: Vec<(u32, bool, usize)> = nc
        .active_toasts()
        .iter()
        .map(|t| (t.id, !t.actions.is_empty(), t.actions.len()))
        .collect();

    let mut cumulative_y = 0i32;

    for (i, &(id, has_actions, action_count)) in toasts.iter().enumerate() {
        let toast_h = if has_actions {
            TOAST_HEIGHT + 32
        } else {
            TOAST_HEIGHT
        };
        let toast_y = TOAST_MARGIN + cumulative_y;
        cumulative_y += toast_h as i32 + 8;

        let toast_rect = Rect::new(base_x, toast_y, TOAST_WIDTH, toast_h);

        if toast_rect.contains(x, y) {
            // Check if clicking on an action button
            if has_actions && action_count > 0 {
                let title_x = base_x + TOAST_PADDING + 20;
                let btn_y = toast_y + TOAST_HEIGHT as i32 - 2;
                let btn_h: u32 = 24;
                let btn_gap: i32 = 8;
                let available_w = TOAST_WIDTH as i32 - TOAST_PADDING * 2 - 24;
                let btn_w = ((available_w - btn_gap * (action_count as i32 - 1))
                    / action_count as i32)
                    .max(50) as u32;

                for j in 0..action_count {
                    let btn_x = title_x + j as i32 * (btn_w as i32 + btn_gap);
                    let btn_rect = Rect::new(btn_x, btn_y, btn_w, btn_h);
                    if btn_rect.contains(x, y) {
                        // Action button clicked — get the action_id
                        if let Some(notif) = nc.notifications.iter().find(|n| n.id == id) {
                            if let Some(action) = notif.actions.get(j) {
                                let action_id = action.action_id.clone();
                                crate::serial_println!(
                                    "[KnoxOS] Notification action: {} (id={})",
                                    action.label,
                                    action_id,
                                );
                            }
                        }
                        nc.dismiss(id);
                        return true;
                    }
                }
            }

            // Clicking anywhere else on the toast dismisses it
            nc.dismiss(id);
            return true;
        }
    }
    false
}

/// Handle a click on the notification panel — returns true if consumed
pub fn handle_panel_click(x: i32, y: i32, screen_w: i32, screen_h: i32) -> bool {
    let mut nc = NOTIFICATIONS.lock();
    if !nc.panel_open {
        return false;
    }

    let taskbar_y = screen_h - super::scale::taskbar_height() as i32;
    let panel_w: i32 = 380;
    let panel_h: i32 = 440;
    let panel_x = screen_w - panel_w - 8;
    let panel_y = taskbar_y - panel_h - 4;

    let panel_rect = Rect::new(panel_x, panel_y, panel_w as u32, panel_h as u32);

    if !panel_rect.contains(x, y) {
        nc.panel_open = false;
        return true;
    }

    // Clear all button
    if y >= panel_y + 26 && y <= panel_y + 42 && x >= panel_x + panel_w - 80 {
        nc.clear_all();
        return true;
    }

    // Group toggle button
    if y >= panel_y + 26
        && y <= panel_y + 42
        && x >= panel_x + panel_w - 150
        && x < panel_x + panel_w - 85
    {
        nc.group_by_app = !nc.group_by_app;
        nc.scroll_offset = 0;
        return true;
    }

    // DND toggle
    if y >= panel_y + 10 && y <= panel_y + 26 && x >= panel_x + panel_w - 80 {
        nc.dnd_mode = !nc.dnd_mode;
        return true;
    }

    if nc.group_by_app {
        // Handle clicks in grouped view — detect group header clicks for collapse/expand
        let mut check_y = panel_y + 54;
        let content_bottom = panel_y + panel_h - 8;
        let item_h: i32 = 48;
        let group_header_h: i32 = 26;

        // Rebuild group order to match rendering
        let mut group_order: Vec<String> = Vec::new();
        let mut group_counts: BTreeMap<String, usize> = BTreeMap::new();
        for notif in nc.notifications.iter().rev() {
            if !group_order.contains(&notif.app_name) {
                group_order.push(notif.app_name.clone());
            }
            *group_counts.entry(notif.app_name.clone()).or_insert(0) += 1;
        }

        for app_name in &group_order {
            if check_y >= content_bottom {
                break;
            }
            let is_collapsed = nc.collapsed_groups.contains(app_name);

            // Group header click
            if y >= check_y && y < check_y + group_header_h {
                // Toggle collapse
                if is_collapsed {
                    nc.collapsed_groups.retain(|g| g != app_name);
                } else {
                    nc.collapsed_groups.push(app_name.clone());
                }
                return true;
            }
            check_y += group_header_h + 2;

            if !is_collapsed {
                let count = group_counts.get(app_name).copied().unwrap_or(0);
                let visible = count.min(5);
                // Get notification IDs for this group in display order
                let notif_ids: Vec<u32> = nc
                    .notifications
                    .iter()
                    .rev()
                    .filter(|n| n.app_name == *app_name)
                    .take(5)
                    .map(|n| n.id)
                    .collect();

                for (j, &nid) in notif_ids.iter().enumerate() {
                    if check_y + item_h > content_bottom {
                        break;
                    }
                    // Dismiss X region
                    let close_x = panel_x + panel_w - 28;
                    let close_y_btn = check_y + 18;
                    if x >= close_x
                        && x <= close_x + 10
                        && y >= close_y_btn
                        && y <= close_y_btn + 10
                    {
                        nc.dismiss_from_panel(nid);
                        return true;
                    }
                    // Click on item — mark as read
                    if y >= check_y && y < check_y + item_h {
                        if let Some(notif) = nc.notifications.iter_mut().find(|n| n.id == nid) {
                            notif.read = true;
                        }
                        return true;
                    }
                    check_y += item_h;
                }
                if count > 5 {
                    check_y += 16;
                }
            }
            check_y += 4;
        }
    } else {
        // Handle clicks in chronological view
        let item_h: i32 = 60;
        let item_ids: Vec<u32> = nc
            .notifications
            .iter()
            .rev()
            .skip(nc.scroll_offset)
            .take(6)
            .map(|n| n.id)
            .collect();

        for (i, id) in item_ids.iter().enumerate() {
            let item_y = panel_y + 54 + (i as i32 * item_h);

            // Dismiss X button click region (right side, 16x16)
            let close_x = panel_x + panel_w - 24;
            let close_y = item_y + 22;
            if x >= close_x && x <= close_x + 12 && y >= close_y && y <= close_y + 12 {
                nc.dismiss_from_panel(*id);
                return true;
            }

            // Click on notification item — mark as read
            let item_rect = Rect::new(panel_x + 8, item_y, panel_w as u32 - 16, item_h as u32 - 4);
            if item_rect.contains(x, y) {
                if let Some(notif) = nc.notifications.iter_mut().find(|n| n.id == *id) {
                    notif.read = true;
                }
                return true;
            }
        }
    }

    true
}

/// Handle mouse wheel scroll on the notification panel
pub fn handle_panel_scroll(delta: i32, screen_w: i32, screen_h: i32) -> bool {
    let mut nc = NOTIFICATIONS.lock();
    if !nc.panel_open {
        return false;
    }
    nc.scroll_panel(delta);
    true
}

/// Toggle notification panel visibility
pub fn toggle_panel() {
    let mut nc = NOTIFICATIONS.lock();
    nc.panel_open = !nc.panel_open;
    if nc.panel_open {
        nc.mark_visible_read();
    }
}

/// Close notification panel
pub fn close_panel() {
    let mut nc = NOTIFICATIONS.lock();
    if nc.panel_open {
        nc.panel_open = false;
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
