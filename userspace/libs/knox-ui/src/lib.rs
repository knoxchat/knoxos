// SPDX-License-Identifier: MIT
//! Knox UI — `no_std` + `alloc` component catalog for KnoxOS.
//!
//! Zed GPUI and the Longbridge kit are design references (Apache-2.0).
//! Those trees are not shipped (`desktopDE-impl.md` D14.7). This crate must
//! not depend on them or on `std`.
#![no_std]

extern crate alloc;

pub mod ai_chat;
pub mod alert;
pub mod avatar;
pub mod badge;
pub mod browser;
pub mod button;
pub mod calculator;
pub mod calendar;
pub mod carousel;
pub mod chart;
pub mod checkbox;
pub mod chrome;
pub mod clipboard;
pub mod color_picker;
pub mod command;
pub mod desktop;
pub mod dialogs;
pub mod diff;
pub mod dock;
pub mod editor;
pub mod editor_app;
pub mod empty;
pub mod ffi;
pub mod files;
pub mod form;
pub mod gallery;
pub mod icon;
pub mod image;
pub mod input;
pub mod kbd;
pub mod label;
pub mod launcher;
pub mod link;
pub mod list;
pub mod menu;
pub mod message;
pub mod motion;
pub mod nav;
pub mod notify;
pub mod number;
pub mod osd;
pub mod otp;
pub mod overlay;
pub mod overlay_host;
pub mod panel;
pub mod progress;
pub mod radio;
pub mod scroll;
pub mod select;
pub mod separator;
pub mod session;
pub mod settings;
pub mod shimmer;
pub mod shortcuts;
pub mod showcase;
pub mod size;
pub mod skeleton;
pub mod slider;
pub mod speech;
pub mod spinner;
pub mod splash;
pub mod switch;
pub mod switcher;
pub mod table;
pub mod tag;
pub mod task_manager;
pub mod terminal;
pub mod textview;
pub mod theme;
pub mod titlebar;
pub mod toast;
pub mod toggle;
pub mod tooltip;
pub mod utility;
pub mod webview;

mod interact;

pub use ai_chat::AiChat;
pub use alert::{Alert, AlertVariant};
pub use avatar::{Avatar, AvatarGroup};
pub use badge::Badge;
pub use browser::BrowserChrome;
pub use button::{Button, ButtonGroup, ButtonVariant, ButtonVariants, DropdownButton};
pub use calculator::{Calculator, DISPLAY_H as CALC_DISPLAY_H, KEY_ROWS as CALC_KEY_ROWS};
pub use calendar::{Calendar, Date, DatePicker, Time, TimeField, days_in_month, is_leap, weekday};
pub use carousel::Carousel;
pub use chart::{Chart, ChartKind, Sample};
pub use checkbox::Checkbox;
pub use chrome::{DescriptionList, GroupBox, Sidebar, StatusBar, Toolbar};
pub use clipboard::Clipboard;
pub use color_picker::ColorPicker;
pub use command::{Command, fuzzy_filter, fuzzy_filter_with, fuzzy_score};
pub use desktop::{
    DesktopIconEntry, DesktopIcons, DesktopMenu, DesktopRubberBand, ICON_CELL_H, ICON_CELL_W,
};
pub use dialogs::{FilePickerDialog, ScreenshotDialog, WizardFlow};
pub use diff::{Diff, DiffLine, parse_unified};
pub use dock::{DOCK_BAR_H, Dock, DockArea, DockLayout, Resizable, SplitAxis, nudge_ratio};
pub use editor::{Editor, FoldSet, Highlight, highlight_line, many_lines};
pub use editor_app::EditorApp;
pub use empty::Empty;
pub use ffi::{
    knox_ui_raster_button, knox_ui_raster_calculator, raster_button_to_shm,
    raster_calculator_state_to_shm, raster_calculator_to_shm, shm_len,
};
pub use files::{FileEntry, FilesView};
pub use form::{Field, Form};
pub use gallery::{
    D8ButtonsAction, D8ButtonsSnapshot, D8DateAction, D8DateSnapshot, D8FeedbackAction,
    D8FeedbackSnapshot, D8FormsAction, D8FormsSnapshot, D8InputAction, D8InputSnapshot,
    D8OverlayAction, D8OverlaySnapshot, D8SelectAction, D8SelectSnapshot, d8_buttons_catalog_types,
    d8_buttons_construct_all, d8_buttons_gallery, d8_date_catalog_types, d8_date_construct_all,
    d8_date_gallery, d8_display_catalog_types, d8_display_construct_all, d8_display_gallery,
    d8_feedback_catalog_types, d8_feedback_construct_all, d8_feedback_gallery,
    d8_forms_catalog_types, d8_forms_construct_all, d8_forms_gallery, d8_input_catalog_types,
    d8_input_construct_all, d8_input_gallery, d8_overlay_catalog_types, d8_overlay_construct_all,
    d8_overlay_gallery, d8_select_catalog_types, d8_select_construct_all, d8_select_gallery,
};
pub use icon::{Icon, IconName};
pub use image::{Image, ObjectFit};
pub use input::{FieldKey, FieldState, GapBuffer, Input, InputGroup, Textarea};
pub use kbd::Kbd;
pub use knox_gpui::VERSION as GPUI_VERSION;
pub use label::{HighlightsMatch, Label};
pub use launcher::{Launcher, LauncherApp, LauncherCategory, PowerAction};
pub use link::Link;
pub use list::{List, Tree, TreeNode, toggle_visible};
pub use menu::{AppMenuBar, ContextMenu, DropdownMenu, MenuItem, NativeMenu, PopupMenu};
pub use message::{
    Attachment, Bubble, BubbleSide, ChatMessage, Marker, Message, MessageScroller, Questionnaire,
    Rating,
};
pub use motion::{
    Easing, MotionTokens, Spring, SpringDriver, enter_opacity_y, exit_opacity_y, tween,
};
pub use nav::{
    Accordion, Breadcrumb, Collapsible, NavStack, NavStackState, Pagination, Stepper, TabBar, Tabs,
};
pub use notify::{NotificationCenter, NotifyAction, NotifyItem, NotifyKind, ToastStack};
pub use number::NumberInput;
pub use osd::{FlyoutKind, OsdFlyout};
pub use otp::OtpInput;
pub use overlay::{AlertDialog, Dialog, FocusTrap, HoverCard, Popover, Positioner, Sheet};
pub use overlay_host::OverlayHost;
pub use panel::{Panel, PanelEntry, TrayIcon, TrayMenu, TrayStatus};
pub use progress::{Progress, ProgressCircle};
pub use radio::{Radio, RadioGroup};
pub use scroll::{ScrollArea, Scrollbar, VirtualList, virtual_paint_count, visible_range};
pub use select::{Combobox, ListNav, SearchableList, Select, pick_with_keys};
pub use separator::{Separator, SeparatorStyle};
pub use session::{LockView, LoginView};
pub use settings::{SettingGroup, SettingItem, SettingPage, SettingsWidget};
pub use shimmer::Shimmer;
pub use shortcuts::{D6_ACTIONS, ShortcutRow, ShortcutsOverlay};
pub use showcase::{
    D8RestAction, D8RestSnapshot, KIT_PAGES, apply_d8_rest, d8_all_catalog_types,
    d8_all_construct_all, d8_chrome_dock_gallery, d8_data_gallery, d8_kit_chrome, d8_kit_status,
    d8_media_gallery, d8_menus_gallery, d8_page_bar, d8_rest_gallery, d8_scroll_nav_gallery,
    d8_theme_gallery, d8_toast_host_gallery,
};
pub use size::{ControlSize, Disableable, Selectable, Sizable};
pub use skeleton::Skeleton;
pub use slider::Slider;
pub use speech::{Speech, SpeechState, Waveform};
pub use spinner::Spinner;
pub use splash::{SPLASH_CARD_H, SPLASH_CARD_W, Splash, brand_lockup, splash_status};
pub use switch::Switch;
pub use switcher::{AltTab, Overview, SwitcherEntry};
pub use table::{DataTable, TableColumn, TableHead, sort_rows};
pub use tag::{Tag, TagVariant};
pub use task_manager::{TAB_LABELS as TASK_MANAGER_TABS, TaskManagerView};
pub use terminal::{TERM_TAB_H, TerminalChrome};
pub use textview::{MdBlock, TextView, parse_markdown};
pub use theme::{
    ActiveTheme, ColorTokens, RadiusTokens, ShadowToken, ShadowTokens, SpacingTokens, Theme,
    ThemeMode, TypeTokens,
};
pub use titlebar::{TitleBar, WindowBorder};
pub use toast::{NotificationList, Toast, ToastData, tick_toasts};
pub use toggle::{Toggle, ToggleGroup, ToggleVariant, ToggleVariants};
pub use tooltip::Tooltip;
pub use utility::UtilityApp;
pub use webview::WebView;

/// Crate version; referenced by the kernel so the path dep stays live (D0.1).
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg(test)]
mod state_tests;

#[cfg(test)]
mod tests {
    use super::{
        d8_all_catalog_types, d8_buttons_catalog_types, d8_date_catalog_types,
        d8_display_catalog_types, d8_feedback_catalog_types, d8_forms_catalog_types,
        d8_input_catalog_types, d8_overlay_catalog_types, d8_select_catalog_types,
        showcase::{
            d8_carousel_catalog_types, d8_chart_catalog_types, d8_chrome_catalog_types,
            d8_command_catalog_types, d8_diff_catalog_types, d8_dock_catalog_types,
            d8_editor_catalog_types, d8_host_catalog_types, d8_list_catalog_types,
            d8_menu_catalog_types, d8_message_catalog_types, d8_nav_catalog_types,
            d8_scroll_catalog_types, d8_settings_catalog_types, d8_speech_catalog_types,
            d8_table_catalog_types, d8_textview_catalog_types, d8_title_catalog_types,
            d8_toast_catalog_types, d8_webview_catalog_types,
        },
    };

    #[test]
    fn versions_are_set() {
        assert!(!super::VERSION.is_empty());
        assert!(!super::GPUI_VERSION.is_empty());
    }

    #[test]
    fn d81_catalog_is_complete() {
        assert_eq!(d8_buttons_catalog_types().len(), 10);
    }

    #[test]
    fn d82_catalog_is_complete() {
        assert_eq!(d8_display_catalog_types().len(), 9);
    }

    #[test]
    fn d83_catalog_is_complete() {
        assert_eq!(d8_feedback_catalog_types().len(), 9);
    }

    #[test]
    fn d84_catalog_is_complete() {
        assert_eq!(d8_input_catalog_types().len(), 3);
    }

    #[test]
    fn d85_catalog_is_complete() {
        assert_eq!(d8_forms_catalog_types().len(), 6);
    }

    #[test]
    fn d86_catalog_is_complete() {
        assert_eq!(d8_select_catalog_types().len(), 3);
    }

    #[test]
    fn d87_catalog_is_complete() {
        assert_eq!(d8_date_catalog_types().len(), 3);
    }

    #[test]
    fn d88_catalog_is_complete() {
        assert_eq!(d8_overlay_catalog_types().len(), 7);
    }

    #[test]
    fn d89_through_d828_catalog_is_complete() {
        assert_eq!(d8_menu_catalog_types().len(), 5);
        assert_eq!(d8_toast_catalog_types().len(), 2);
        assert_eq!(d8_host_catalog_types().len(), 1);
        assert_eq!(d8_scroll_catalog_types().len(), 3);
        assert_eq!(d8_nav_catalog_types().len(), 8);
        assert_eq!(d8_chrome_catalog_types().len(), 5);
        assert_eq!(d8_dock_catalog_types().len(), 3);
        assert_eq!(d8_title_catalog_types().len(), 2);
        assert_eq!(d8_command_catalog_types().len(), 1);
        assert_eq!(d8_settings_catalog_types().len(), 4);
        assert_eq!(d8_list_catalog_types().len(), 2);
        assert_eq!(d8_table_catalog_types().len(), 2);
        assert_eq!(d8_textview_catalog_types().len(), 1);
        assert_eq!(d8_editor_catalog_types().len(), 1);
        assert_eq!(d8_diff_catalog_types().len(), 1);
        assert_eq!(d8_chart_catalog_types().len(), 7);
        assert_eq!(d8_carousel_catalog_types().len(), 1);
        assert_eq!(d8_message_catalog_types().len(), 7);
        assert_eq!(d8_speech_catalog_types().len(), 1);
        assert_eq!(d8_webview_catalog_types().len(), 1);
        assert_eq!(d8_all_catalog_types().len(), 108);
    }
}
