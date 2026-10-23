//! Aurora Command Center geometry constants.

// ═══════════════════════════════════════════════════════════════════════════
// LAYOUT CONSTANTS — Aurora Command Center Geometry
// ═══════════════════════════════════════════════════════════════════════════

/// Total panel width
pub const PANEL_WIDTH: u32 = 480;
/// Maximum panel height
pub const PANEL_MAX_HEIGHT: u32 = 660;
/// Corner radius for main panel
pub(super) const PANEL_RADIUS: u32 = 18;
/// Horizontal padding inside panel
pub(super) const PAD_H: i32 = 20;
/// Gap between panel bottom and dock top
pub(super) const DOCK_GAP: i32 = 12;

/// Search bar height
pub(super) const SEARCH_BAR_H: u32 = 40;
/// Search bar corner radius
pub(super) const SEARCH_BAR_RADIUS: u32 = 14;

/// Tab bar height
pub(super) const TAB_BAR_H: u32 = 32;

/// Menu item row height
pub const MENU_ITEM_HEIGHT: u32 = 44;
/// Menu item horizontal inset
pub(super) const ITEM_INSET: i32 = 10;
/// Icon size in app list
pub(super) const ITEM_ICON_SIZE: i32 = 24;
/// Gap between icon and text
pub(super) const ICON_TEXT_GAP: i32 = 14;

/// Bottom bar height (user + power)
pub(super) const BOTTOM_BAR_H: u32 = 48;

/// AI chat input bar height
pub(super) const AI_INPUT_H: u32 = 44;
/// AI message bubble max width ratio
pub(super) const AI_BUBBLE_MAX_W_PCT: u32 = 85; // % of panel content width

/// Quick action button size
pub(super) const QUICK_ACTION_SIZE: u32 = 56;
/// Quick action grid columns
pub(super) const QUICK_ACTION_COLS: u32 = 4;
/// Quick action grid gap
pub(super) const QUICK_ACTION_GAP: u32 = 12;

/// Agent task item height
pub(super) const TASK_ITEM_H: u32 = 52;

// ═══════════════════════════════════════════════════════════════════════════
// LEGACY ALIASES (used by other modules that reference old constants)
// ═══════════════════════════════════════════════════════════════════════════

/// Start menu width (legacy alias)
pub const START_MENU_WIDTH: u32 = PANEL_WIDTH;
/// Start menu max height (legacy alias)
pub const START_MENU_MAX_HEIGHT: u32 = PANEL_MAX_HEIGHT;
/// Sidebar width (legacy, unused)
pub const SIDEBAR_WIDTH: u32 = 52;
