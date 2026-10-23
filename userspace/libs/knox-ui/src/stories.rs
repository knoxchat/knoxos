// SPDX-License-Identifier: MIT
//! GPUI Kit story gallery, reimplemented for Knox UI (`no_std`).

use alloc::format;
use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;

use knox_gpui::{Div, FluentBuilder, FontId, Hsla, IntoElement, Point, Styled, div, px};

use crate::VERSION;
use crate::alert::Alert;
use crate::avatar::{Avatar, AvatarGroup};
use crate::badge::Badge;
use crate::button::{Button, ButtonCustomVariant, ButtonGroup, ButtonVariants, DropdownButton};
use crate::checkbox::Checkbox;
use crate::chrome::{DescriptionList, GroupBox, Sidebar, StatusBar};
use crate::clipboard::Clipboard;
use crate::color_picker::ColorPicker;
use crate::empty::Empty;
use crate::form::{Field, Form};
use crate::gallery::{
    D8ButtonsAction, D8ButtonsSnapshot, D8DateAction, D8DateSnapshot, D8FeedbackAction,
    D8FeedbackSnapshot, D8FormsAction, D8FormsSnapshot, D8InputAction, D8InputSnapshot,
    D8OverlayAction, D8OverlaySnapshot, D8SelectAction, D8SelectSnapshot, d8_date_gallery,
    d8_overlay_gallery, d8_select_gallery,
};
use crate::icon::{Icon, IconName};
use crate::image::Image;
use crate::input::{FieldState, Input, InputGroup, Textarea};
use crate::kbd::Kbd;
use crate::label::Label;
use crate::number::NumberInput;
use crate::otp::OtpInput;
use crate::progress::{Progress, ProgressCircle};
use crate::radio::{Radio, RadioGroup};
use crate::scroll::ScrollArea;
use crate::separator::Separator;
use crate::shimmer::Shimmer;
use crate::showcase::{
    D8RestAction, D8RestSnapshot, d8_chrome_dock_gallery, d8_data_gallery, d8_media_gallery,
    d8_menus_gallery, d8_scroll_nav_gallery, d8_theme_gallery, d8_toast_host_gallery,
};
use crate::size::{Disableable, Selectable, Sizable};
use crate::skeleton::Skeleton;
use crate::slider::Slider;
use crate::spinner::Spinner;
use crate::switch::Switch;
use crate::tag::Tag;
use crate::textview::TextView;
use crate::theme::Theme;
use crate::titlebar::TitleBar;
use crate::toggle::{Toggle, ToggleGroup, ToggleVariants};
use crate::tooltip::Tooltip;

pub const GALLERY_SIDEBAR_W: i32 = 255;
pub const GALLERY_TITLE_H: i32 = 36;
pub const GALLERY_STATUS_H: i32 = 28;
pub const GALLERY_HEADER_H: i32 = 88;

const OPT_DISABLED: u8 = 0;
const OPT_LOADING: u8 = 1;
const OPT_SELECTED: u8 = 2;
const OPT_COMPACT: u8 = 3;
const OPT_MULTIPLE: u8 = 4;

/// One catalog entry, matching GPUI Kit story titles and descriptions.
#[derive(Clone, Copy, Debug)]
pub struct KitStory {
    pub name: &'static str,
    pub description: &'static str,
}

pub const KIT_STORIES: &[KitStory] = &[
    KitStory {
        name: "Introduction",
        description: "UI components for building fantastic desktop application by using GPUI.",
    },
    KitStory {
        name: "Accordion",
        description: "The accordion uses collapse internally to make it collapsible.",
    },
    KitStory {
        name: "Alert",
        description: "Communicate important status changes without interrupting the user's workflow.",
    },
    KitStory {
        name: "AlertDialog",
        description: "Require a response before the user can continue.",
    },
    KitStory {
        name: "Attachment",
        description: "Composable file and media attachments with lifecycle states and actions.",
    },
    KitStory {
        name: "Avatar",
        description: "Represent a person or organization with an image or fallback.",
    },
    KitStory {
        name: "Badge",
        description: "A red dot that indicates the number of unread messages.",
    },
    KitStory {
        name: "Breadcrumb",
        description: "A breadcrumb navigation element that shows the current location in a hierarchy.",
    },
    KitStory {
        name: "Bubble",
        description: "A styleable chat surface for text, rich content, and reactions.",
    },
    KitStory {
        name: "Button",
        description: "Displays a button or a component that looks like a button.",
    },
    KitStory {
        name: "Calendar",
        description: "A calendar to select a date or date range.",
    },
    KitStory {
        name: "Carousel",
        description: "A carousel for browsing a set of related items with keyboard and pointer navigation.",
    },
    KitStory {
        name: "Chart",
        description: "Beautiful Charts & Graphs.",
    },
    KitStory {
        name: "Checkbox",
        description: "Select one or more independent options.",
    },
    KitStory {
        name: "Clipboard",
        description: "Copy text or generated values to the clipboard.",
    },
    KitStory {
        name: "Collapsible",
        description: "An interactive element that expands/collapses.",
    },
    KitStory {
        name: "ColorPicker",
        description: "Choose and preview a color value.",
    },
    KitStory {
        name: "Combobox",
        description: "An autocomplete input paired with a searchable dropdown list.",
    },
    KitStory {
        name: "Command",
        description: "A searchable list of commands and quick actions.",
    },
    KitStory {
        name: "DataTable",
        description: "A complex data table with selection, sorting, column moving, and loading more.",
    },
    KitStory {
        name: "DatePicker",
        description: "A date picker to select a date or date range.",
    },
    KitStory {
        name: "DescriptionList",
        description: "Present labels and values in a structured summary.",
    },
    KitStory {
        name: "Dialog",
        description: "Present focused content above the current view.",
    },
    KitStory {
        name: "Diff",
        description: "Readonly unified and Git patches, merge conflicts and source files, with review comments.",
    },
    KitStory {
        name: "Dock",
        description: "Drag tabs between groups or towards an edge to split the workspace.",
    },
    KitStory {
        name: "DropdownButton",
        description: "A button with an attached dropdown menu for additional options.",
    },
    KitStory {
        name: "Editor",
        description: "Code editor with theme-aware syntax highlighting and folding.",
    },
    KitStory {
        name: "Empty",
        description: "Compose empty states with icons, avatars, actions, and custom content.",
    },
    KitStory {
        name: "Form",
        description: "Form to collect multiple inputs.",
    },
    KitStory {
        name: "GroupBox",
        description: "A styled container element with an optional title to group related content together.",
    },
    KitStory {
        name: "HoverCard",
        description: "A hover card displays content when hovering over a trigger element, with configurable delays.",
    },
    KitStory {
        name: "Icon",
        description: "SVG Icons based on Lucide.dev",
    },
    KitStory {
        name: "Image",
        description: "Image and SVG image supported.",
    },
    KitStory {
        name: "Input",
        description: "Capture and validate short-form text, credentials, identifiers, and formatted values.",
    },
    KitStory {
        name: "Input Group",
        description: "Compose inputs and textareas with icons, text, actions, and toolbars in one frame.",
    },
    KitStory {
        name: "Kbd",
        description: "A tag style to display keyboard shortcuts",
    },
    KitStory {
        name: "Label",
        description: "Display concise text with hierarchy, highlighting, and masking.",
    },
    KitStory {
        name: "List",
        description: "A list displays a series of items.",
    },
    KitStory {
        name: "Menu",
        description: "Popup menu and context menu",
    },
    KitStory {
        name: "Marker",
        description: "A compact row for conversation status, notifications, and separators.",
    },
    KitStory {
        name: "Message",
        description: "Compose sender identity, metadata, rich content, and message actions.",
    },
    KitStory {
        name: "MessageScroller",
        description: "A virtualized message list with tail following, unread navigation, and anchor preservation.",
    },
    KitStory {
        name: "NativeMenu",
        description: "A menu rendered by the operating system. Unlike PopupMenu, it is drawn by the OS and can extend beyond the window bounds.",
    },
    KitStory {
        name: "Notification",
        description: "Show transient feedback without interrupting the current task.",
    },
    KitStory {
        name: "NumberInput",
        description: "Adjust constrained numeric values precisely with typing or increment and decrement controls.",
    },
    KitStory {
        name: "OtpInput",
        description: "Enter short verification and recovery codes with clear grouping and masking controls.",
    },
    KitStory {
        name: "Pagination",
        description: "Pagination with page navigation, next and previous links.",
    },
    KitStory {
        name: "Popover",
        description: "Show focused content beside a trigger.",
    },
    KitStory {
        name: "Progress",
        description: "Show task completion with determinate or loading indicators.",
    },
    KitStory {
        name: "Questionnaire",
        description: "Composable multi-step questions with answers, validation, progress, and navigation.",
    },
    KitStory {
        name: "Radio",
        description: "Choose one option from a set.",
    },
    KitStory {
        name: "Rating",
        description: "A simple interactive star rating component.",
    },
    KitStory {
        name: "Resizable",
        description: "The resizable panels.",
    },
    KitStory {
        name: "Scrollbar",
        description: "Add scrollbar to a scrollable element.",
    },
    KitStory {
        name: "Select",
        description: "Displays a list of options for the user to pick from—triggered by a button.",
    },
    KitStory {
        name: "Separator",
        description: "A separator that can be either vertical or horizontal.",
    },
    KitStory {
        name: "Settings",
        description: "A collection of settings groups and items for the application.",
    },
    KitStory {
        name: "Sheet",
        description: "Sheet for open a popup in the edge of the window",
    },
    KitStory {
        name: "Shimmer",
        description: "Reusable, theme-aware text loading effects with composable timing and appearance.",
    },
    KitStory {
        name: "Sidebar",
        description: "A composable, themeable and customizable sidebar component.",
    },
    KitStory {
        name: "Skeleton",
        description: "Use to show a placeholder while content is loading.",
    },
    KitStory {
        name: "Slider",
        description: "Displays a slider control for selecting a value within a range.",
    },
    KitStory {
        name: "Speech",
        description: "Dictate text through the system recognizer or your own.",
    },
    KitStory {
        name: "Spinner",
        description: "Displays an spinner showing the completion progress of a task.",
    },
    KitStory {
        name: "StatusBar",
        description: "A horizontal bar with left/center/right regions, usually placed at the bottom.",
    },
    KitStory {
        name: "Stepper",
        description: "A step-by-step process for users to navigate through a series of steps.",
    },
    KitStory {
        name: "Switch",
        description: "Turn a setting on or off.",
    },
    KitStory {
        name: "Table",
        description: "A basic table component for directly rendering tabular data.",
    },
    KitStory {
        name: "Tabs",
        description: "A set of layered sections of content—known as tab panels—that are displayed one at a time.",
    },
    KitStory {
        name: "Tag",
        description: "A short item that can be used to categorize or label content.",
    },
    KitStory {
        name: "Textarea",
        description: "Input with multi-line mode.",
    },
    KitStory {
        name: "Theme Colors",
        description: "A color theme viewer to explore colors organized by categories.",
    },
    KitStory {
        name: "TimeField",
        description: "Edit a time of day segment by segment, on a 24-hour or 12-hour clock.",
    },
    KitStory {
        name: "Toggle",
        description: "Turn an option on or off, alone or in a group.",
    },
    KitStory {
        name: "Toolbar",
        description: "Groups commands and controls into one keyboard-navigable row.",
    },
    KitStory {
        name: "Tooltip",
        description: "Describe a control on hover.",
    },
    KitStory {
        name: "Tree",
        description: "A hierarchical list of items.",
    },
    KitStory {
        name: "VirtualList",
        description: "Add vertical or horizontal, or both scrollbars to a container, and use virtual_list to render a large number of items.",
    },
];

/// Host wiring for the live gallery window.
pub struct KitGalleryProps<'a> {
    pub theme: &'a Theme,
    pub rest: D8RestSnapshot,
    pub buttons: D8ButtonsSnapshot,
    pub feedback: D8FeedbackSnapshot,
    pub inputs: D8InputSnapshot,
    pub forms: D8FormsSnapshot,
    pub selects: D8SelectSnapshot,
    pub dates: D8DateSnapshot,
    pub overlays: D8OverlaySnapshot,
    pub search: FieldState,
    pub cmd_query: FieldState,
    pub maximized: bool,
    pub card_w: i32,
    pub card_h: i32,
    pub on_rest: Arc<dyn Fn(D8RestAction) + Send + Sync>,
    pub on_buttons: Arc<dyn Fn(D8ButtonsAction) + Send + Sync>,
    pub on_feedback: Arc<dyn Fn(D8FeedbackAction) + Send + Sync>,
    pub on_input: Arc<dyn Fn(D8InputAction) + Send + Sync>,
    pub on_forms: Arc<dyn Fn(D8FormsAction) + Send + Sync>,
    pub on_select: Arc<dyn Fn(D8SelectAction) + Send + Sync>,
    pub on_date: Arc<dyn Fn(D8DateAction) + Send + Sync>,
    pub on_overlay: Arc<dyn Fn(D8OverlayAction) + Send + Sync>,
    pub on_search: Arc<dyn Fn(FieldState) + Send + Sync>,
    pub on_cmd: Arc<dyn Fn(FieldState) + Send + Sync>,
    pub paste: Arc<dyn Fn() -> Option<String> + Send + Sync>,
    pub on_min: Arc<dyn Fn() + Send + Sync>,
    pub on_max: Arc<dyn Fn() + Send + Sync>,
    pub on_close: Arc<dyn Fn() + Send + Sync>,
    pub on_drag: Arc<dyn Fn(Point) + Send + Sync>,
}

fn row() -> Div {
    div().flex().flex_wrap().grow().gap(px(16)).items_center()
}

fn stack(theme: &Theme) -> Div {
    div().flex_col().gap(theme.spacing.md)
}

fn section(
    theme: &Theme,
    title: &'static str,
    description: &'static str,
    body: impl IntoElement,
) -> Div {
    GroupBox::new(title)
        .themed(theme)
        .description(description)
        .outline()
        .child(body)
        .into_element()
}

fn story_desc(name: &str) -> &'static str {
    KIT_STORIES
        .iter()
        .find(|s| s.name == name)
        .map(|s| s.description)
        .unwrap_or("")
}

fn framed(
    theme: &Theme,
    title: &'static str,
    description: &'static str,
    body: impl IntoElement,
) -> Div {
    stack(theme).child(section(theme, title, description, body))
}

fn toolbar(p: &KitGalleryProps<'_>, options: bool) -> Div {
    let theme = p.theme;
    let size = p.rest.control_size();
    let fire = p.on_rest.clone();
    let mut sizes = ButtonGroup::new("story-size")
        .themed(theme)
        .outline()
        .compact();
    for (i, label) in ["XSmall", "Small", "Medium", "Large"].iter().enumerate() {
        let fire = fire.clone();
        sizes = sizes.child(
            Button::new(format!("sz-{i}"))
                .themed(theme)
                .small()
                .outline()
                .label(*label)
                .selected(p.rest.story_size == i as u8)
                .on_click(move || fire(D8RestAction::StorySize(i as u8))),
        );
    }
    let mut bar = div()
        .flex()
        .justify_end()
        .items_center()
        .gap_2()
        .child(
            Label::new(format!("Size: {}", size.as_str()))
                .themed(theme)
                .muted(true)
                .size(theme.typo.caption),
        )
        .child(sizes);
    if options {
        let flags = [
            (OPT_DISABLED, "Disabled"),
            (OPT_LOADING, "Loading"),
            (OPT_SELECTED, "Selected"),
            (OPT_COMPACT, "Compact"),
            (OPT_MULTIPLE, "Multiple"),
        ];
        let mut opts = ButtonGroup::new("story-opt")
            .themed(theme)
            .outline()
            .compact();
        for (bit, label) in flags {
            let fire = fire.clone();
            opts = opts.child(
                Button::new(format!("opt-{bit}"))
                    .themed(theme)
                    .small()
                    .outline()
                    .label(label)
                    .selected(p.rest.opt_on(bit))
                    .on_click(move || fire(D8RestAction::ToggleOpt(bit))),
            );
        }
        bar = bar.child(opts);
    }
    bar
}

fn btn(p: &KitGalleryProps<'_>, id: &'static str) -> Button {
    Button::new(id)
        .themed(p.theme)
        .with_size(p.rest.control_size())
        .disabled(p.rest.opt_on(OPT_DISABLED))
        .selected(p.rest.opt_on(OPT_SELECTED))
        .loading(p.rest.opt_on(OPT_LOADING))
        .when(p.rest.opt_on(OPT_COMPACT), |b| b.compact())
}

pub fn kit_gallery(p: &KitGalleryProps<'_>) -> Div {
    let theme = p.theme;
    let story_ix = (p.rest.page as usize).min(KIT_STORIES.len().saturating_sub(1));
    let story = &KIT_STORIES[story_ix];
    let query = p.search.value().to_ascii_lowercase();
    let visible: Vec<(usize, &'static KitStory)> = KIT_STORIES
        .iter()
        .enumerate()
        .filter(|(_, s)| query.is_empty() || s.name.to_ascii_lowercase().contains(&query))
        .collect();
    let selected_visible = visible
        .iter()
        .position(|(i, _)| *i == story_ix)
        .unwrap_or(0);
    let names: Vec<String> = visible.iter().map(|(_, s)| String::from(s.name)).collect();
    let on_rest = p.on_rest.clone();
    let indices: Vec<u8> = visible.iter().map(|(i, _)| *i as u8).collect();

    let sidebar_h = p.card_h - GALLERY_TITLE_H - GALLERY_STATUS_H;
    let sidebar = Sidebar::new()
        .themed(theme)
        .w(px(GALLERY_SIDEBAR_W))
        .h(px(sidebar_h))
        .header("Knox UI", "Component showcase")
        .items(names)
        .selected(selected_visible)
        .offset(p.rest.nav_scroll)
        .on_offset({
            let fire = p.on_rest.clone();
            move |o| fire(D8RestAction::NavScroll(o))
        })
        .search(
            Input::new("kit-search")
                .themed(theme)
                .small()
                .embedded(true)
                .placeholder("Search…")
                .w(px(GALLERY_SIDEBAR_W - 36))
                .state(p.search.clone())
                .on_change({
                    let fire = p.on_search.clone();
                    move |s| fire(s)
                }),
        )
        .on_select(move |i| {
            if let Some(idx) = indices.get(i).copied() {
                on_rest(D8RestAction::Page(idx));
            }
        });
    let collapsed = p.rest.collapsed;

    let content_w = if p.rest.collapsed {
        p.card_w
    } else {
        p.card_w - GALLERY_SIDEBAR_W
    };
    let scroll_h = (p.card_h - GALLERY_TITLE_H - GALLERY_STATUS_H - GALLERY_HEADER_H).max(120);

    let header = div()
        .flex_col()
        .gap_1()
        .p(theme.spacing.lg)
        .border_b_1()
        .border_color(theme.colors.border)
        .child(
            Label::new(story.name)
                .themed(theme)
                .font(FontId::UiBold)
                .size(24),
        )
        .child(
            Label::new(story.description)
                .themed(theme)
                .muted(true)
                .size(theme.typo.caption),
        );

    let on_scroll = p.on_rest.clone();
    let body = ScrollArea::new()
        .themed(theme)
        .size(px(content_w), px(scroll_h))
        .offset(p.rest.scroll)
        .on_offset(move |o| on_scroll(D8RestAction::Scroll(o)))
        .child(
            div()
                .flex_col()
                .w(px(content_w))
                .pad_x(theme.spacing.lg)
                .pad_y(theme.spacing.lg)
                .child(story_body(p, story.name)),
        );

    let content = div()
        .flex_col()
        .w(px(content_w))
        .h(px(p.card_h - GALLERY_TITLE_H - GALLERY_STATUS_H))
        .bg(theme.colors.background)
        .child(header)
        .child(body);

    let on_collapse = p.on_rest.clone();
    let status = StatusBar::new()
        .themed(theme)
        .prefix(
            Button::new("toggle-sidebar")
                .themed(theme)
                .ghost()
                .xsmall()
                .compact()
                .icon(if collapsed {
                    IconName::ChevronRight
                } else {
                    IconName::ChevronLeft
                })
                .on_click(move || on_collapse(D8RestAction::Collapse(!collapsed))),
        )
        .left(format!("{} components", KIT_STORIES.len()))
        .extra(story.name)
        .right(format!("{}  v{}", theme.display_name(), VERSION));

    let on_min = p.on_min.clone();
    let on_max = p.on_max.clone();
    let on_close = p.on_close.clone();
    let on_drag = p.on_drag.clone();

    div()
        .flex_col()
        .w(px(p.card_w))
        .h(px(p.card_h))
        .overflow_hidden()
        .bg(theme.colors.background)
        .rounded(theme.radius.xl)
        .child(
            TitleBar::new("Kit Gallery")
                .themed(theme)
                .icon(IconName::LayoutGrid)
                .maximized(p.maximized)
                .on_drag(move |pt| on_drag(pt))
                .on_min(move || on_min())
                .on_max(move || on_max())
                .on_close(move || on_close()),
        )
        .child(
            div()
                .flex()
                .grow()
                .when(!collapsed, |d| d.child(sidebar))
                .child(content),
        )
        .child(status)
}

fn story_body(p: &KitGalleryProps<'_>, name: &str) -> Div {
    match name {
        "Introduction" => story_intro(p),
        "Button" => story_button(p),
        "Toggle" => story_toggle(p),
        "Switch" => story_switch(p),
        "Checkbox" => story_checkbox(p),
        "Radio" => story_radio(p),
        "Alert" => story_alert(p),
        "Empty" => story_empty(p),
        "Badge" => story_badge(p),
        "Tag" => story_tag(p),
        "Avatar" => story_avatar(p),
        "Label" => story_label(p),
        "Kbd" => story_kbd(p),
        "Separator" => story_separator(p),
        "Icon" => story_icon(p),
        "Image" => story_image(p),
        "Progress" => story_progress(p),
        "Spinner" => story_spinner(p),
        "Skeleton" | "Shimmer" => story_motion(p, name),
        "Tooltip" => story_tooltip(p),
        "Clipboard" => story_clipboard(p),
        "Input" => story_input(p),
        "Textarea" => story_textarea(p),
        "Input Group" => story_input_group(p),
        "NumberInput" => story_number(p),
        "OtpInput" => story_otp(p),
        "Slider" => story_slider(p),
        "ColorPicker" => story_color(p),
        "Form" => story_form(p),
        "Select" | "Combobox" => framed(
            p.theme,
            "Default",
            story_desc(name),
            d8_select_gallery(p.theme, p.selects.clone(), p.on_select.clone()),
        ),
        "Calendar" | "DatePicker" | "TimeField" => framed(
            p.theme,
            "Default",
            story_desc(name),
            d8_date_gallery(p.theme, p.dates, p.on_date.clone()),
        ),
        "Dialog" | "AlertDialog" | "Sheet" | "Popover" | "HoverCard" => framed(
            p.theme,
            "Default",
            story_desc(name),
            d8_overlay_gallery(p.theme, p.overlays, p.on_overlay.clone()),
        ),
        "DropdownButton" => story_dropdown(p),
        "GroupBox" => story_group_box(p),
        "Sidebar" | "Toolbar" | "StatusBar" | "DescriptionList" | "Resizable" | "Dock" => framed(
            p.theme,
            "Default",
            story_desc(name),
            d8_chrome_dock_gallery(p.theme, p.rest, p.on_rest.clone()),
        ),
        "Accordion" | "Collapsible" | "Breadcrumb" | "Pagination" | "Stepper" | "Tabs"
        | "Scrollbar" | "VirtualList" => framed(
            p.theme,
            "Default",
            story_desc(name),
            d8_scroll_nav_gallery(p.theme, p.rest, p.on_rest.clone()),
        ),
        "Menu" | "NativeMenu" => framed(
            p.theme,
            "Default",
            story_desc(name),
            d8_menus_gallery(p.theme, p.rest, p.on_rest.clone()),
        ),
        "Notification" => framed(
            p.theme,
            "Default",
            story_desc(name),
            d8_toast_host_gallery(p.theme, p.rest, p.on_rest.clone()),
        ),
        "List" | "Tree" | "Table" | "DataTable" | "Command" | "Settings" | "Editor" | "Diff" => {
            framed(
                p.theme,
                "Default",
                story_desc(name),
                d8_data_gallery(
                    p.theme,
                    p.rest,
                    p.on_rest.clone(),
                    p.cmd_query.clone(),
                    p.on_cmd.clone(),
                ),
            )
        }
        "Chart" | "Carousel" | "Message" | "MessageScroller" | "Bubble" | "Marker"
        | "Attachment" | "Questionnaire" | "Rating" | "Speech" => framed(
            p.theme,
            "Default",
            story_desc(name),
            d8_media_gallery(p.theme, p.rest, p.on_rest.clone()),
        ),
        "Theme Colors" => framed(
            p.theme,
            "Palette",
            story_desc(name),
            d8_theme_gallery(p.theme, p.rest, p.on_rest.clone()),
        ),
        _ => story_intro(p),
    }
}

fn story_intro(p: &KitGalleryProps<'_>) -> Div {
    let theme = p.theme;
    stack(theme)
        .child(
            TextView::new()
                .themed(theme)
                .markdown(
                    "# Knox UI\n\n\
UI components for building fantastic desktop applications on KnoxOS.\n\n\
This gallery follows the GPUI Kit catalog: a searchable sidebar, one story per component, and GroupBox sections with Size / Options controls.\n\n\
- 78 stories covering buttons, inputs, overlays, navigation, data, and media\n\
- Semantic themes: Aurora Dark, Arctic Light, Sunset Warm\n\
- Sizes: XSmall, Small, Medium, Large — same density steps as GPUI Kit\n\
- Native feel: quiet, precise controls; default arrow cursor; tokens before literals\n\n\
Use the sidebar to open a component story. Search filters the catalog. Size and Options on each story match GPUI Kit behavior.",
                ),
        )
        .child(section(
            theme,
            "Start here",
            "Open Button to see variants, sizes, and states.",
            row()
                .child(
                    Button::new("intro-btn")
                        .themed(theme)
                        .primary()
                        .label("Open Button story")
                        .on_click({
                            let fire = p.on_rest.clone();
                            let ix = KIT_STORIES
                                .iter()
                                .position(|s| s.name == "Button")
                                .unwrap_or(9) as u8;
                            move || fire(D8RestAction::Page(ix))
                        }),
                )
                .child(
                    Button::new("intro-toggle")
                        .themed(theme)
                        .secondary()
                        .label("Open Toggle")
                        .on_click({
                            let fire = p.on_rest.clone();
                            let ix = KIT_STORIES
                                .iter()
                                .position(|s| s.name == "Toggle")
                                .unwrap_or(0) as u8;
                            move || fire(D8RestAction::Page(ix))
                        }),
                ),
        ))
}

fn story_button(p: &KitGalleryProps<'_>) -> Div {
    let theme = p.theme;
    let custom = ButtonCustomVariant::new(Hsla::from_rgb(192, 80, 180));
    stack(theme)
        .child(toolbar(p, true))
        .child(section(
            theme,
            "Variants",
            "Visual treatments communicate action priority.",
            row()
                .child(btn(p, "b-def").label("Default"))
                .child(btn(p, "b-pri").primary().label("Primary"))
                .child(btn(p, "b-sec").secondary().label("Secondary"))
                .child(btn(p, "b-dan").danger().label("Danger"))
                .child(btn(p, "b-warn").warning().label("Warning"))
                .child(btn(p, "b-ok").success().label("Success"))
                .child(btn(p, "b-info").info().label("Info"))
                .child(btn(p, "b-gh").ghost().label("Ghost"))
                .child(btn(p, "b-link").link().label("Link"))
                .child(btn(p, "b-txt").text().label("Text")),
        ))
        .child(section(
            theme,
            "Icons",
            "Icons can lead labels or appear in custom content.",
            row()
                .child(
                    btn(p, "bi-ok")
                        .outline()
                        .icon(IconName::Check)
                        .label("Confirm"),
                )
                .child(
                    btn(p, "bi-no")
                        .outline()
                        .icon(IconName::Close)
                        .label("Abort"),
                )
                .child(
                    btn(p, "bi-max")
                        .outline()
                        .icon(IconName::Maximize2)
                        .label("Maximize"),
                )
                .child(
                    btn(p, "bi-gh")
                        .ghost()
                        .icon(IconName::Check)
                        .label("Confirm"),
                )
                .child(btn(p, "bi-link").link().icon(IconName::Check).label("Link"))
                .child(
                    btn(p, "bi-txt")
                        .text()
                        .icon(IconName::Check)
                        .label("Text Button"),
                ),
        ))
        .child(section(
            theme,
            "Progress",
            "Buttons can show determinate progress.",
            row()
                .child(
                    btn(p, "bp-1")
                        .primary()
                        .icon(IconName::RefreshCw)
                        .label("Installing...")
                        .loading(true),
                )
                .child(
                    Button::new("bp-2")
                        .themed(theme)
                        .with_size(p.rest.control_size())
                        .icon(IconName::RefreshCw)
                        .label("Installing...")
                        .loading(true),
                )
                .child(
                    Button::new("bp-3")
                        .themed(theme)
                        .with_size(p.rest.control_size())
                        .icon(IconName::RefreshCw)
                        .label("Installing...")
                        .loading(true),
                ),
        ))
        .child(section(
            theme,
            "Outline",
            "Outlined treatments keep actions visually quiet.",
            row()
                .child(btn(p, "bo-p").primary().outline().label("Primary Button"))
                .child(btn(p, "bo-n").outline().label("Normal Button"))
                .child(btn(p, "bo-d").danger().outline().label("Danger Button"))
                .child(btn(p, "bo-w").warning().outline().label("Warning Button"))
                .child(btn(p, "bo-s").success().outline().label("Success Button"))
                .child(btn(p, "bo-i").info().outline().label("Info Button"))
                .child(btn(p, "bo-g").ghost().outline().label("Ghost Button")),
        ))
        .child(section(
            theme,
            "Dropdown",
            "A caret indicates an attached menu.",
            row()
                .child(
                    btn(p, "bd-p")
                        .primary()
                        .dropdown_caret(true)
                        .label("Primary Button"),
                )
                .child(btn(p, "bd-d").dropdown_caret(true).label("Default Button"))
                .child(
                    btn(p, "bd-s")
                        .secondary()
                        .dropdown_caret(true)
                        .label("Secondary Button"),
                )
                .child(
                    btn(p, "bd-g")
                        .ghost()
                        .dropdown_caret(true)
                        .label("Ghost Button"),
                )
                .child(
                    btn(p, "bd-l")
                        .link()
                        .dropdown_caret(true)
                        .label("Link Button"),
                )
                .child(
                    btn(p, "bd-o")
                        .outline()
                        .dropdown_caret(true)
                        .label("Small Button"),
                ),
        ))
        .child(section(
            theme,
            "Horizontal group",
            "Joined corners share one outline.",
            ButtonGroup::new("bg-h")
                .themed(theme)
                .outline()
                .disabled(p.rest.opt_on(OPT_DISABLED))
                .child(btn(p, "bg-1").label("One").selected(p.buttons.group == 0))
                .child(btn(p, "bg-2").label("Two").selected(p.buttons.group == 1))
                .child(btn(p, "bg-3").label("Three").selected(p.buttons.group == 2))
                .on_click({
                    let fire = p.on_buttons.clone();
                    move |i| fire(D8ButtonsAction::Group(i as u8))
                })
                .into_element(),
        ))
        .child(section(
            theme,
            "Vertical group",
            "Stacked segments for compact toolbars.",
            ButtonGroup::new("bg-v")
                .themed(theme)
                .outline()
                .vertical()
                .disabled(p.rest.opt_on(OPT_DISABLED))
                .child(btn(p, "bgv-1").label("One"))
                .child(btn(p, "bgv-2").label("Two"))
                .child(btn(p, "bgv-3").label("Three"))
                .into_element(),
        ))
        .child(section(
            theme,
            "Selection group",
            "Groups support single or multiple selection.",
            ButtonGroup::new("bg-sel")
                .themed(theme)
                .outline()
                .compact()
                .multiple(p.rest.opt_on(OPT_MULTIPLE))
                .child(
                    Button::new("sel-d")
                        .themed(theme)
                        .label("Disabled")
                        .selected(p.rest.opt_on(OPT_DISABLED)),
                )
                .child(
                    Button::new("sel-l")
                        .themed(theme)
                        .label("Loading")
                        .selected(p.rest.opt_on(OPT_LOADING)),
                )
                .child(
                    Button::new("sel-s")
                        .themed(theme)
                        .label("Selected")
                        .selected(p.rest.opt_on(OPT_SELECTED)),
                )
                .child(
                    Button::new("sel-c")
                        .themed(theme)
                        .label("Compact")
                        .selected(p.rest.opt_on(OPT_COMPACT)),
                )
                .on_click({
                    let fire = p.on_rest.clone();
                    move |i| fire(D8RestAction::ToggleOpt(i as u8))
                })
                .into_element(),
        ))
        .child(section(
            theme,
            "Icon-only",
            "Compact actions can omit visible labels.",
            row()
                .child(btn(p, "io-p").primary().icon(IconName::Search))
                .child(btn(p, "io-s").icon(IconName::Info).loading(true))
                .child(btn(p, "io-d").danger().icon(IconName::Close))
                .child(btn(p, "io-o").outline().icon(IconName::Search))
                .child(btn(p, "io-g").ghost().icon(IconName::ArrowLeft)),
        ))
        .child(section(
            theme,
            "Custom size",
            "A fixed pixel size is available for compact icon actions.",
            row().child(btn(p, "cs-heart").ghost().icon(IconName::Star).w(px(24))),
        ))
        .child(section(
            theme,
            "Custom color",
            "Product colors can override the semantic scale.",
            row()
                .child(btn(p, "cc-1").custom(custom).label("Custom Button"))
                .child(
                    btn(p, "cc-2")
                        .outline()
                        .custom(custom)
                        .label("Outline Button"),
                )
                .child(
                    btn(p, "cc-3")
                        .outline()
                        .custom(custom)
                        .icon(IconName::Bell)
                        .label("Icon Button"),
                ),
        ))
}

fn story_toggle(p: &KitGalleryProps<'_>) -> Div {
    let theme = p.theme;
    let size = p.rest.control_size();
    let fire = p.on_buttons.clone();
    stack(theme)
        .child(toolbar(p, false))
        .child(section(
            theme,
            "Default",
            "Text and icon toggles with clear selected states.",
            row()
                .child(
                    Toggle::new("tg-preview")
                        .themed(theme)
                        .with_size(size)
                        .label("Preview")
                        .checked(p.buttons.toggle)
                        .on_change({
                            let fire = fire.clone();
                            move |on| fire(D8ButtonsAction::Toggle(on))
                        }),
                )
                .child(
                    Toggle::new("tg-star")
                        .themed(theme)
                        .with_size(size)
                        .icon(IconName::Star)
                        .checked(p.buttons.toggle_a)
                        .on_change({
                            let fire = fire.clone();
                            move |on| fire(D8ButtonsAction::ToggleGroup { index: 0, on })
                        }),
                ),
        ))
        .child(section(
            theme,
            "Sizes",
            "XSmall through Large match the control density scale.",
            row()
                .child(
                    Toggle::new("tg-xs")
                        .themed(theme)
                        .xsmall()
                        .label("XS")
                        .checked(p.buttons.toggle),
                )
                .child(
                    Toggle::new("tg-sm")
                        .themed(theme)
                        .small()
                        .label("SM")
                        .checked(p.buttons.toggle),
                )
                .child(
                    Toggle::new("tg-md")
                        .themed(theme)
                        .label("MD")
                        .checked(p.buttons.toggle),
                )
                .child(
                    Toggle::new("tg-lg")
                        .themed(theme)
                        .large()
                        .label("LG")
                        .checked(p.buttons.toggle),
                ),
        ))
        .child(section(
            theme,
            "Variants",
            "Ghost and outline treatments for different surfaces.",
            row()
                .child(
                    div()
                        .flex_col()
                        .gap_2()
                        .items_center()
                        .child(Label::new("Ghost").themed(theme).size(theme.typo.caption))
                        .child(
                            ToggleGroup::new("tg-ghost")
                                .child(
                                    Toggle::new("tgg-1")
                                        .themed(theme)
                                        .ghost()
                                        .with_size(size)
                                        .icon(IconName::Bell)
                                        .checked(p.buttons.toggle_a),
                                )
                                .child(
                                    Toggle::new("tgg-2")
                                        .themed(theme)
                                        .ghost()
                                        .with_size(size)
                                        .icon(IconName::Inbox)
                                        .checked(p.buttons.toggle_b),
                                )
                                .child(
                                    Toggle::new("tgg-3")
                                        .themed(theme)
                                        .ghost()
                                        .with_size(size)
                                        .icon(IconName::Check)
                                        .checked(p.buttons.toggle),
                                ),
                        ),
                )
                .child(
                    div()
                        .flex_col()
                        .gap_2()
                        .items_center()
                        .child(Label::new("Outline").themed(theme).size(theme.typo.caption))
                        .child(
                            ToggleGroup::new("tg-out")
                                .child(
                                    Toggle::new("tgo-1")
                                        .themed(theme)
                                        .outline()
                                        .with_size(size)
                                        .icon(IconName::Bell)
                                        .checked(p.buttons.toggle),
                                )
                                .child(
                                    Toggle::new("tgo-2")
                                        .themed(theme)
                                        .outline()
                                        .with_size(size)
                                        .icon(IconName::Inbox)
                                        .checked(p.buttons.toggle_a),
                                )
                                .child(
                                    Toggle::new("tgo-3")
                                        .themed(theme)
                                        .outline()
                                        .with_size(size)
                                        .icon(IconName::Check)
                                        .checked(p.buttons.toggle_b),
                                ),
                        ),
                ),
        ))
}

fn setting_row(
    theme: &Theme,
    title: &'static str,
    hint: &'static str,
    control: impl IntoElement,
) -> Div {
    div()
        .flex()
        .items_center()
        .justify_between()
        .gap(theme.spacing.lg)
        .p(theme.spacing.md)
        .child(
            div()
                .flex_col()
                .gap(px(2))
                .child(
                    Label::new(title)
                        .themed(theme)
                        .font(FontId::UiBold)
                        .size(theme.typo.body),
                )
                .child(
                    Label::new(hint)
                        .themed(theme)
                        .muted(true)
                        .size(theme.typo.caption),
                ),
        )
        .child(control)
}

fn story_switch(p: &KitGalleryProps<'_>) -> Div {
    let theme = p.theme;
    let size = p.rest.control_size();
    let fire = p.on_rest.clone();
    let list = div()
        .flex_col()
        .w(px(420))
        .border_1()
        .border_color(theme.colors.border)
        .rounded(theme.radius.lg)
        .child(setting_row(
            theme,
            "Product updates",
            "New features and release notes.",
            Switch::new("sw-0")
                .themed(theme)
                .with_size(size)
                .checked(p.rest.switch_on(0))
                .on_change({
                    let fire = fire.clone();
                    move |_| fire(D8RestAction::ToggleSwitch(0))
                }),
        ))
        .child(Separator::horizontal().themed(theme))
        .child(setting_row(
            theme,
            "Marketing emails",
            "Occasional tips and announcements.",
            Switch::new("sw-1")
                .themed(theme)
                .with_size(size)
                .checked(p.rest.switch_on(1))
                .on_change({
                    let fire = fire.clone();
                    move |_| fire(D8RestAction::ToggleSwitch(1))
                }),
        ))
        .child(Separator::horizontal().themed(theme))
        .child(setting_row(
            theme,
            "Security alerts",
            "Sign-in and recovery notices.",
            Switch::new("sw-2")
                .themed(theme)
                .with_size(size)
                .checked(p.rest.switch_on(2))
                .on_change({
                    let fire = fire.clone();
                    move |_| fire(D8RestAction::ToggleSwitch(2))
                }),
        ));
    stack(theme)
        .child(toolbar(p, false))
        .child(section(
            theme,
            "Default",
            "Switches work well in a compact settings list.",
            list,
        ))
        .child(section(
            theme,
            "Disabled",
            "Disabled switches keep their value visible.",
            row()
                .child(
                    Switch::new("sw-off")
                        .themed(theme)
                        .with_size(size)
                        .label("Disabled off")
                        .disabled(true),
                )
                .child(
                    Switch::new("sw-on")
                        .themed(theme)
                        .with_size(size)
                        .label("Disabled on")
                        .checked(true)
                        .disabled(true),
                ),
        ))
        .child(section(
            theme,
            "Labeled",
            "A label can sit beside the track.",
            Switch::new("sw-lab")
                .themed(theme)
                .with_size(size)
                .label("Airplane mode")
                .checked(p.rest.switch_on(3))
                .on_change({
                    let fire = p.on_rest.clone();
                    move |_| fire(D8RestAction::ToggleSwitch(3))
                }),
        ))
}

fn story_checkbox(p: &KitGalleryProps<'_>) -> Div {
    let theme = p.theme;
    let size = p.rest.control_size();
    let fire = p.on_rest.clone();
    stack(theme)
        .child(toolbar(p, false))
        .child(section(
            theme,
            "Default",
            "Checked and unchecked options can be mixed freely.",
            row()
                .child(
                    Checkbox::new("cb-0")
                        .themed(theme)
                        .with_size(size)
                        .label("Product updates")
                        .checked(p.rest.check_on(0))
                        .on_change({
                            let fire = fire.clone();
                            move |_| fire(D8RestAction::ToggleCheck(0))
                        }),
                )
                .child(
                    Checkbox::new("cb-1")
                        .themed(theme)
                        .with_size(size)
                        .label("Remember this device")
                        .checked(p.rest.check_on(1))
                        .on_change({
                            let fire = fire.clone();
                            move |_| fire(D8RestAction::ToggleCheck(1))
                        }),
                ),
        ))
        .child(section(
            theme,
            "Without label",
            "The label can be supplied by surrounding content.",
            row().child(
                Checkbox::new("cb-bare")
                    .themed(theme)
                    .with_size(size)
                    .checked(p.rest.check_on(2))
                    .on_change({
                        let fire = fire.clone();
                        move |_| fire(D8RestAction::ToggleCheck(2))
                    }),
            ),
        ))
        .child(section(
            theme,
            "Disabled",
            "Disabled checkboxes keep their value visible.",
            row()
                .child(
                    Checkbox::new("cb-d0")
                        .themed(theme)
                        .with_size(size)
                        .label("Disabled")
                        .disabled(true),
                )
                .child(
                    Checkbox::new("cb-d1")
                        .themed(theme)
                        .with_size(size)
                        .label("Disabled checked")
                        .checked(true)
                        .disabled(true),
                ),
        ))
}

fn story_radio(p: &KitGalleryProps<'_>) -> Div {
    let theme = p.theme;
    let size = p.rest.control_size();
    stack(theme)
        .child(toolbar(p, false))
        .child(section(
            theme,
            "Default",
            "Exactly one option in the group is selected.",
            RadioGroup::new("rg")
                .themed(theme)
                .selected(Some(p.buttons.radio as usize))
                .child(Radio::new("ra").themed(theme).with_size(size).label("A"))
                .child(Radio::new("rb").themed(theme).with_size(size).label("B"))
                .child(Radio::new("rc").themed(theme).with_size(size).label("C"))
                .on_change({
                    let fire = p.on_buttons.clone();
                    move |i| fire(D8ButtonsAction::Radio(i as u8))
                })
                .into_element(),
        ))
        .child(section(
            theme,
            "Disabled",
            "A disabled radio keeps its selection.",
            row().child(
                Radio::new("rd")
                    .themed(theme)
                    .with_size(size)
                    .label("Unavailable")
                    .checked(true)
                    .disabled(true),
            ),
        ))
}

fn story_alert(p: &KitGalleryProps<'_>) -> Div {
    let theme = p.theme;
    stack(theme).child(section(
        theme,
        "Variants",
        "Info, success, warning, and error share one layout.",
        div()
            .flex_col()
            .gap_2()
            .w(px(480))
            .child(
                Alert::info("Gallery controls are Knox GPUI views.")
                    .themed(theme)
                    .title("Knox UI")
                    .visible(p.feedback.alert_open)
                    .on_close({
                        let fire = p.on_feedback.clone();
                        move || fire(D8FeedbackAction::DismissAlert)
                    }),
            )
            .child(Alert::success("Saved").themed(theme).title("Success"))
            .child(
                Alert::warning("Check this setting")
                    .themed(theme)
                    .title("Warning"),
            )
            .child(
                Alert::error("Could not connect")
                    .themed(theme)
                    .title("Error"),
            ),
    ))
}

fn story_empty(p: &KitGalleryProps<'_>) -> Div {
    section(
        p.theme,
        "Default",
        "Compose empty states with icons, titles, and descriptions.",
        Empty::new()
            .themed(p.theme)
            .icon(IconName::Inbox)
            .title("No items")
            .description("Try another filter"),
    )
}

fn story_badge(p: &KitGalleryProps<'_>) -> Div {
    let theme = p.theme;
    section(
        theme,
        "Count · overflow · dot · icon",
        "A red dot that indicates the number of unread messages.",
        row()
            .child(
                Badge::new()
                    .themed(theme)
                    .count(3)
                    .child(Icon::new(IconName::Bell).themed(theme)),
            )
            .child(
                Badge::new()
                    .themed(theme)
                    .count(128)
                    .max(99)
                    .child(Icon::new(IconName::Inbox).themed(theme)),
            )
            .child(
                Badge::new()
                    .themed(theme)
                    .dot()
                    .child(Icon::new(IconName::User).themed(theme)),
            )
            .child(
                Badge::new()
                    .themed(theme)
                    .icon(IconName::Check)
                    .child(Avatar::new().themed(theme).name("Ada Lovelace").small()),
            ),
    )
}

fn story_tag(p: &KitGalleryProps<'_>) -> Div {
    let theme = p.theme;
    section(
        theme,
        "Semantic chips",
        "A short item that can be used to categorize or label content.",
        row()
            .child(Tag::primary().themed(theme).label("New"))
            .child(Tag::success().themed(theme).label("Live"))
            .child(Tag::warning().themed(theme).label("Warn"))
            .child(Tag::info().themed(theme).label("Info"))
            .child(Tag::danger().themed(theme).outline().label("Danger"))
            .child(Tag::secondary().themed(theme).label("Meta")),
    )
}

fn story_avatar(p: &KitGalleryProps<'_>) -> Div {
    let theme = p.theme;
    section(
        theme,
        "Initials · sizes · overflow group",
        "Represent a person or organization with an image or fallback.",
        row()
            .child(Avatar::new().themed(theme).name("Ada Lovelace"))
            .child(Avatar::new().themed(theme).name("Alan Turing").large())
            .child(Avatar::new().themed(theme).small())
            .child(
                AvatarGroup::new()
                    .themed(theme)
                    .limit(3)
                    .child(Avatar::new().name("Ada Lovelace"))
                    .child(Avatar::new().name("Alan Turing"))
                    .child(Avatar::new().name("Grace Hopper"))
                    .child(Avatar::new().name("Edsger Dijkstra")),
            ),
    )
}

fn story_label(p: &KitGalleryProps<'_>) -> Div {
    let theme = p.theme;
    section(
        theme,
        "Hierarchy",
        "Display concise text with hierarchy, highlighting, and masking.",
        row()
            .child(Label::new("Label").themed(theme))
            .child(
                Label::new("Search")
                    .themed(theme)
                    .secondary("docs")
                    .highlights("doc"),
            )
            .child(Label::new("Secret").themed(theme).muted(true))
            .child(Label::new("Hidden").themed(theme).masked(true)),
    )
}

fn story_kbd(p: &KitGalleryProps<'_>) -> Div {
    let theme = p.theme;
    section(
        theme,
        "Keycaps",
        "A tag style to display keyboard shortcuts",
        row()
            .child(Kbd::parse("cmd-k").unwrap().themed(theme))
            .child(Kbd::parse("ctrl-shift-s").unwrap().themed(theme).outline())
            .child(Kbd::parse("esc").unwrap().themed(theme)),
    )
}

fn story_separator(p: &KitGalleryProps<'_>) -> Div {
    let theme = p.theme;
    section(
        theme,
        "Labeled · dashed",
        "A separator that can be either vertical or horizontal.",
        row()
            .child(
                div()
                    .w(px(160))
                    .child(Separator::horizontal().themed(theme).label("or")),
            )
            .child(
                div()
                    .h(px(24))
                    .child(Separator::vertical_dashed().themed(theme)),
            ),
    )
}

fn story_icon(p: &KitGalleryProps<'_>) -> Div {
    let theme = p.theme;
    let mut row = row();
    for name in [
        IconName::Search,
        IconName::Check,
        IconName::Folder,
        IconName::Settings,
        IconName::Bell,
        IconName::Star,
        IconName::Home,
        IconName::LayoutGrid,
    ] {
        row = row.child(Icon::new(name).themed(theme).color(theme.colors.foreground));
    }
    section(theme, "Lucide", "SVG Icons based on Lucide.dev", row)
}

fn story_image(p: &KitGalleryProps<'_>) -> Div {
    section(
        p.theme,
        "Gradient",
        "Image and SVG image supported.",
        Image::demo_gradient(96, 56)
            .themed(p.theme)
            .rounded(p.theme.radius.md),
    )
}

fn story_progress(p: &KitGalleryProps<'_>) -> Div {
    let theme = p.theme;
    section(
        theme,
        "Determinate",
        "Show task completion with determinate or loading indicators.",
        row()
            .child(Progress::new().themed(theme).value(64.0).w(px(180)))
            .child(ProgressCircle::new().themed(theme).value(64.0))
            .child(ProgressCircle::new().themed(theme).value(25.0))
            .child(ProgressCircle::new().themed(theme).value(85.0)),
    )
}

fn story_spinner(p: &KitGalleryProps<'_>) -> Div {
    let theme = p.theme;
    section(
        theme,
        "Loading",
        "Displays an spinner showing the completion progress of a task.",
        row()
            .child(
                Spinner::new()
                    .themed(theme)
                    .phase(p.feedback.phase)
                    .reduced_motion(p.feedback.reduced_motion),
            )
            .child(Spinner::new().themed(theme).small().phase(p.feedback.phase))
            .child(
                Spinner::new()
                    .themed(theme)
                    .large()
                    .color(theme.colors.primary)
                    .phase(p.feedback.phase),
            ),
    )
}

fn story_motion(p: &KitGalleryProps<'_>, name: &str) -> Div {
    let theme = p.theme;
    let title = if name == "Shimmer" {
        "Shimmer"
    } else {
        "Skeleton"
    };
    section(
        theme,
        title,
        "Use to show a placeholder while content is loading.",
        row()
            .child(
                Skeleton::new()
                    .themed(theme)
                    .w(px(96))
                    .phase(p.feedback.phase)
                    .reduced_motion(p.feedback.reduced_motion),
            )
            .child(
                Shimmer::new("Shimmer")
                    .themed(theme)
                    .phase(p.feedback.phase)
                    .reduced_motion(p.feedback.reduced_motion),
            ),
    )
}

fn story_tooltip(p: &KitGalleryProps<'_>) -> Div {
    let theme = p.theme;
    section(
        theme,
        "Hover",
        "Describe a control on hover.",
        Tooltip::new("Open palette")
            .themed(theme)
            .key_binding(Kbd::parse("cmd-k").unwrap().themed(theme))
            .child(Label::new("Hover me").themed(theme)),
    )
}

fn story_clipboard(p: &KitGalleryProps<'_>) -> Div {
    let theme = p.theme;
    section(
        theme,
        "Copy",
        "Copy text or generated values to the clipboard.",
        Clipboard::new()
            .themed(theme)
            .value("knox-gpui")
            .copied(p.feedback.copied)
            .on_copy({
                let fire = p.on_feedback.clone();
                move |_| fire(D8FeedbackAction::Copy)
            }),
    )
}

fn story_input(p: &KitGalleryProps<'_>) -> Div {
    let theme = p.theme;
    let paste = p.paste.clone();
    stack(theme).child(toolbar(p, false)).child(section(
        theme,
        "Default",
        "Capture and validate short-form text.",
        Input::new("st-in")
            .themed(theme)
            .with_size(p.rest.control_size())
            .placeholder("Type, select, copy/paste")
            .w(px(280))
            .state(p.inputs.line.clone())
            .on_change({
                let fire = p.on_input.clone();
                move |s| fire(D8InputAction::Line(s))
            })
            .on_copy({
                let fire = p.on_input.clone();
                move |s| fire(D8InputAction::Copy(String::from(s)))
            })
            .paste_from(move || paste()),
    ))
}

fn story_textarea(p: &KitGalleryProps<'_>) -> Div {
    let theme = p.theme;
    let paste = p.paste.clone();
    section(
        theme,
        "Multiline",
        "Input with multi-line mode.",
        Textarea::new("st-area")
            .themed(theme)
            .placeholder("Multiline")
            .rows(4)
            .w(px(320))
            .state(p.inputs.area.clone())
            .on_change({
                let fire = p.on_input.clone();
                move |s| fire(D8InputAction::Area(s))
            })
            .paste_from(move || paste()),
    )
}

fn story_input_group(p: &KitGalleryProps<'_>) -> Div {
    let theme = p.theme;
    let paste = p.paste.clone();
    section(
        theme,
        "Prefix and suffix",
        "Compose inputs with icons, text, actions, and toolbars in one frame.",
        InputGroup::new("st-ig")
            .themed(theme)
            .prefix("@")
            .suffix(".os")
            .field(
                Input::new("st-ig-in")
                    .themed(theme)
                    .placeholder("host")
                    .w(px(140)),
            )
            .state(p.inputs.group.clone())
            .on_change({
                let fire = p.on_input.clone();
                move |s| fire(D8InputAction::Group(s))
            })
            .paste_from(move || paste()),
    )
}

fn story_number(p: &KitGalleryProps<'_>) -> Div {
    let theme = p.theme;
    let text_state = if p.forms.number_text.is_empty() {
        FieldState::from(format!("{}", p.forms.number))
    } else {
        p.forms.number_text.clone()
    };
    section(
        theme,
        "Constrained numeric",
        "Adjust values with typing or increment and decrement controls.",
        NumberInput::new("st-num")
            .themed(theme)
            .value(p.forms.number)
            .text(text_state)
            .min(0)
            .max(99)
            .w(px(88))
            .on_change({
                let fire = p.on_forms.clone();
                move |n| fire(D8FormsAction::Number(n))
            })
            .on_text({
                let fire = p.on_forms.clone();
                move |s| fire(D8FormsAction::NumberText(s))
            }),
    )
}

fn story_otp(p: &KitGalleryProps<'_>) -> Div {
    section(
        p.theme,
        "Verification code",
        "Enter short verification and recovery codes with clear grouping.",
        OtpInput::new()
            .themed(p.theme)
            .length(4)
            .value(p.forms.otp.clone())
            .on_change({
                let fire = p.on_forms.clone();
                move |s| fire(D8FormsAction::Otp(String::from(s)))
            }),
    )
}

fn story_slider(p: &KitGalleryProps<'_>) -> Div {
    section(
        p.theme,
        "Range",
        "Displays a slider control for selecting a value within a range.",
        Slider::new()
            .themed(p.theme)
            .value(p.forms.slider)
            .w(px(220))
            .on_change({
                let fire = p.on_forms.clone();
                move |v| fire(D8FormsAction::Slider(v))
            }),
    )
}

fn story_color(p: &KitGalleryProps<'_>) -> Div {
    section(
        p.theme,
        "Picker",
        "Choose and preview a color value.",
        ColorPicker::new()
            .themed(p.theme)
            .color(p.forms.color)
            .on_change({
                let fire = p.on_forms.clone();
                move |c| fire(D8FormsAction::Color(c))
            }),
    )
}

fn story_form(p: &KitGalleryProps<'_>) -> Div {
    let theme = p.theme;
    let error = if p.forms.name.is_empty() {
        "Required"
    } else {
        ""
    };
    section(
        theme,
        "Field",
        "Form to collect multiple inputs.",
        Form::new().themed(theme).child(
            Field::new("Name")
                .themed(theme)
                .required(true)
                .error(error)
                .child(
                    Input::new("st-name")
                        .themed(theme)
                        .placeholder("Ada")
                        .w(px(200))
                        .state(p.forms.name.clone())
                        .on_change({
                            let fire = p.on_forms.clone();
                            move |s| fire(D8FormsAction::Name(s))
                        }),
                ),
        ),
    )
}

fn story_dropdown(p: &KitGalleryProps<'_>) -> Div {
    let theme = p.theme;
    let save_label = match p.buttons.dropdown_choice {
        1 => "Export",
        2 => "Save as",
        _ => "Save",
    };
    section(
        theme,
        "Split action",
        "A button with an attached dropdown menu for additional options.",
        DropdownButton::new("st-drop")
            .themed(theme)
            .primary()
            .button(
                Button::new("st-drop-a")
                    .themed(theme)
                    .primary()
                    .with_size(p.rest.control_size())
                    .label(save_label),
            )
            .items(["Save", "Export", "Save as"])
            .open(p.buttons.dropdown_open)
            .on_toggle({
                let fire = p.on_buttons.clone();
                move || fire(D8ButtonsAction::DropdownToggle)
            })
            .on_item({
                let fire = p.on_buttons.clone();
                move |i| fire(D8ButtonsAction::DropdownPick(i as u8))
            }),
    )
}

fn story_group_box(p: &KitGalleryProps<'_>) -> Div {
    let theme = p.theme;
    stack(theme)
        .child(section(
            theme,
            "Outline",
            "A styled container with an optional title.",
            GroupBox::new("General")
                .themed(theme)
                .description("Account preferences")
                .outline()
                .w(px(320))
                .child(DescriptionList::new().themed(theme).row("App", "Gallery")),
        ))
        .child(
            GroupBox::new("Filled")
                .themed(theme)
                .description("Muted surface")
                .fill()
                .w(px(320))
                .child(Label::new("Content").themed(theme)),
        )
}

#[cfg(test)]
mod tests {
    use super::{KIT_STORIES, KitGalleryProps, kit_gallery};
    use crate::gallery::{
        D8ButtonsSnapshot, D8DateSnapshot, D8FeedbackSnapshot, D8FormsSnapshot, D8InputSnapshot,
        D8OverlaySnapshot, D8SelectSnapshot,
    };
    use crate::input::FieldState;
    use crate::showcase::D8RestSnapshot;
    use crate::theme::Theme;
    use alloc::sync::Arc;
    use knox_gpui::Point;

    #[test]
    fn stories_match_gpui_kit_catalog() {
        assert_eq!(KIT_STORIES[0].name, "Introduction");
        assert!(KIT_STORIES.iter().any(|s| s.name == "Button"));
        assert!(KIT_STORIES.iter().any(|s| s.name == "Toggle"));
        assert!(KIT_STORIES.iter().any(|s| s.name == "VirtualList"));
        assert_eq!(KIT_STORIES.len(), 78);
        assert!(!KIT_STORIES.iter().any(|s| s.name == "Shell"));
    }

    fn dummy_props<'a>(theme: &'a Theme, page: u8) -> KitGalleryProps<'a> {
        let mut rest = D8RestSnapshot::INIT;
        rest.page = page;
        KitGalleryProps {
            theme,
            rest,
            buttons: D8ButtonsSnapshot::default(),
            feedback: D8FeedbackSnapshot::default(),
            inputs: D8InputSnapshot::default(),
            forms: D8FormsSnapshot::default(),
            selects: D8SelectSnapshot::default(),
            dates: D8DateSnapshot::default(),
            overlays: D8OverlaySnapshot::default(),
            search: FieldState::empty(),
            cmd_query: FieldState::empty(),
            maximized: false,
            card_w: 1280,
            card_h: 860,
            on_rest: Arc::new(|_| {}),
            on_buttons: Arc::new(|_| {}),
            on_feedback: Arc::new(|_| {}),
            on_input: Arc::new(|_| {}),
            on_forms: Arc::new(|_| {}),
            on_select: Arc::new(|_| {}),
            on_date: Arc::new(|_| {}),
            on_overlay: Arc::new(|_| {}),
            on_search: Arc::new(|_| {}),
            on_cmd: Arc::new(|_| {}),
            paste: Arc::new(|| None),
            on_min: Arc::new(|| {}),
            on_max: Arc::new(|| {}),
            on_close: Arc::new(|| {}),
            on_drag: Arc::new(|_: Point| {}),
        }
    }

    #[test]
    fn gallery_constructs() {
        let theme = Theme::aurora_dark();
        let _ = kit_gallery(&dummy_props(&theme, 0));
    }

    #[test]
    fn every_story_constructs() {
        let theme = Theme::aurora_dark();
        for page in 0..KIT_STORIES.len() {
            let _ = kit_gallery(&dummy_props(&theme, page as u8));
        }
    }
}
