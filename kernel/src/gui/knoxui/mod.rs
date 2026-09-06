pub mod area;
pub mod collapsing;
pub mod frame;
pub mod grid;
pub mod modal;
/// # KnoxUI — Complete GUI Component Library for KnoxOS Desktop
///
/// A comprehensive, `no_std` immediate-mode GUI toolkit inspired by egui,
/// purpose-built for bare-metal framebuffer rendering. Every component uses
/// integer coordinates, anti-aliased rendering, and the KnoxOS "Nebula Depth"
/// design language.
///
/// ## Architecture
///
/// KnoxUI is layered on top of the existing `gui::ui::Ui` context:
///
/// ```text
///  ┌─────────────────────────────────────────────────┐
///  │  Application Code (windows, desktop apps)       │
///  ├─────────────────────────────────────────────────┤
///  │  KnoxUI Components (this library)               │
///  │  ├── containers: Panel, ScrollArea, Window, Tab │
///  │  ├── widgets: Table, Tree, Menu, ColorPicker    │
///  │  ├── overlays: Modal, Toast, Tooltip, Popover   │
///  │  ├── data: ListView, Grid, PropertyGrid         │
///  │  └── animation: Transition, Easing              │
///  ├─────────────────────────────────────────────────┤
///  │  Core UI Layer (gui::ui::Ui, Response, Layout)  │
///  ├─────────────────────────────────────────────────┤
///  │  Framebuffer + Font Rendering                   │
///  └─────────────────────────────────────────────────┘
/// ```
///
/// ## Usage
///
/// ```ignore
/// use crate::gui::knoxui::prelude::*;
///
/// // Inside a window's content draw:
/// Panel::left("sidebar", 200).show(ui, |ui| {
///     TreeView::new("files").show(ui, &file_tree);
/// });
/// Panel::central("main").show(ui, |ui| {
///     Table::new("data", 4).show(ui, |row| {
///         row.col(|ui| ui.label("Name"));
///         row.col(|ui| ui.label("Size"));
///     });
/// });
/// ```
// ─── Container Components ────────────────────────────────────────────
pub mod panel;
pub mod resize;
pub mod scene;
pub mod scroll_area;
pub mod sides;
pub mod split;
pub mod strip;
pub mod tabs;
pub mod window;
pub mod window_opts;

// ─── Widget Components ───────────────────────────────────────────────
pub mod badge;
pub mod breadcrumb;
pub mod chip;
pub mod color_picker;
pub mod combo_box;
pub mod drag_handle;
pub mod drag_value;
pub mod image_button;
pub mod image_widget;
pub mod link;
pub mod menu_bar;
pub mod progress_bar;
pub mod selectable_label;
pub mod separator;
pub mod spinner_widget;
pub mod status_bar;
pub mod table;
pub mod text_edit;
pub mod toolbar;
pub mod tree_view;

// ─── Data Display Components ─────────────────────────────────────────
pub mod calendar;
pub mod list_view;
pub mod property_grid;
pub mod timeline;

// ─── Overlay Components ──────────────────────────────────────────────
pub mod context_menu;
pub mod popover;
pub mod toast;
pub mod tooltip;

// ─── Interaction Utilities ───────────────────────────────────────────
pub mod drag_drop;

// ─── Animation Utilities ─────────────────────────────────────────────
pub mod animation;

// ─── Internal Helpers ────────────────────────────────────────────────
pub mod text_helpers;

// ─── Prelude — convenient re-exports ─────────────────────────────────
pub mod prelude {
    // Containers
    pub use super::area::Area;
    pub use super::collapsing::CollapsingSection;
    pub use super::frame::Frame;
    pub use super::grid::{Grid, GridUi};
    pub use super::modal::{Modal, ModalResponse};
    pub use super::panel::{CentralPanel, PanelSide, SidePanel, TopBottomPanel, TopBottomSide};
    pub use super::resize::Resize;
    pub use super::scene::{Scene, SceneUi};
    pub use super::scroll_area::ScrollArea;
    pub use super::sides::Sides;
    pub use super::split::SplitView;
    pub use super::strip::{Strip, StripBuilder, StripSize};
    pub use super::tabs::{TabBar, TabBarResponse, TabViewer};
    pub use super::window::Window;
    pub use super::window_opts::{WindowOptions, WindowResponse};

    // Widgets
    pub use super::badge::{Badge, BadgeKind};
    pub use super::breadcrumb::Breadcrumb;
    pub use super::chip::{Chip, ChipResponse};
    pub use super::color_picker::ColorPicker;
    pub use super::combo_box::{ComboBox, ComboBoxResponse};
    pub use super::drag_handle::DragHandle;
    pub use super::drag_value::DragValue;
    pub use super::image_button::ImageButton;
    pub use super::image_widget::ImageWidget;
    pub use super::link::Link;
    pub use super::menu_bar::{MenuBar, MenuBarContext, MenuDropdown, MenuItem};
    pub use super::progress_bar::ProgressBar;
    pub use super::selectable_label::SelectableLabel;
    pub use super::separator::Separator;
    pub use super::spinner_widget::SpinnerWidget;
    pub use super::status_bar::StatusBar;
    pub use super::table::{Column, SortOrder, SortState, Table, TableRow};
    pub use super::text_edit::TextEdit;
    pub use super::toolbar::{Toolbar, ToolbarButtonResult, ToolbarContext, ToolbarItem};
    pub use super::tree_view::{TreeNode, TreeView};

    // Data display
    pub use super::calendar::Calendar;
    pub use super::list_view::ListView;
    pub use super::property_grid::PropertyGrid;
    pub use super::timeline::{Timeline, TimelineKind};

    // Overlays
    pub use super::context_menu::ContextMenu;
    pub use super::popover::{Popover, PopoverDirection};
    pub use super::toast::{Toast, ToastKind, ToastManager};
    pub use super::tooltip::Tooltip;

    // Interaction
    pub use super::drag_drop::{
        DropResult, clear_drag, drag_source, drop_zone, has_drag_payload, peek_drag_tag,
        set_drag_payload, take_drag_payload, update_drag,
    };

    // Animation
    pub use super::animation::{
        animate_bool, animate_i32, ease_in_cubic, ease_in_out_cubic, ease_in_out_quad,
        ease_in_quad, ease_linear, ease_out_bounce, ease_out_cubic, ease_out_elastic,
        ease_out_quad, lerp_i32, lerp_u8, progress_to_f32,
    };
}
