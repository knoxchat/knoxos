// SPDX-License-Identifier: MIT
//! Button, ButtonGroup, DropdownButton (GPUI kit catalog, reimplemented).
//!
//! the Longbridge kit is a design reference only (Apache-2.0). This file is new
//! `no_std` code.

use alloc::format;
use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;

use knox_gpui::{Corners, Div, Hsla, IntoElement, Pixels, Styled, div, px, text};

use crate::icon::{Icon, IconName};
use crate::interact::{Handler, IndexHandler, bind_activate};
use crate::size::{ControlSize, Disableable, Selectable, Sizable};
use crate::spinner::Spinner;
use crate::theme::Theme;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ButtonCustomVariant {
    pub color: Hsla,
    pub foreground: Hsla,
    pub hover: Hsla,
    pub active: Hsla,
}

impl ButtonCustomVariant {
    pub fn new(color: Hsla) -> Self {
        Self {
            color,
            foreground: color,
            hover: color.opacity(0.12),
            active: color.opacity(0.22),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum ButtonVariant {
    #[default]
    Default,
    Primary,
    Secondary,
    Danger,
    Warning,
    Success,
    Info,
    Ghost,
    Link,
    Text,
    Custom(ButtonCustomVariant),
}

pub trait ButtonVariants: Sized {
    fn with_variant(self, variant: ButtonVariant) -> Self;

    fn primary(self) -> Self {
        self.with_variant(ButtonVariant::Primary)
    }

    fn secondary(self) -> Self {
        self.with_variant(ButtonVariant::Secondary)
    }

    fn danger(self) -> Self {
        self.with_variant(ButtonVariant::Danger)
    }

    fn warning(self) -> Self {
        self.with_variant(ButtonVariant::Warning)
    }

    fn success(self) -> Self {
        self.with_variant(ButtonVariant::Success)
    }

    fn info(self) -> Self {
        self.with_variant(ButtonVariant::Info)
    }

    fn ghost(self) -> Self {
        self.with_variant(ButtonVariant::Ghost)
    }

    fn link(self) -> Self {
        self.with_variant(ButtonVariant::Link)
    }

    fn text(self) -> Self {
        self.with_variant(ButtonVariant::Text)
    }

    fn custom(self, style: ButtonCustomVariant) -> Self {
        self.with_variant(ButtonVariant::Custom(style))
    }
}

/// Styled push button (kit `Button`).
pub struct Button {
    id: String,
    theme: Theme,
    label: String,
    variant: ButtonVariant,
    size: ControlSize,
    disabled: bool,
    selected: bool,
    outline: bool,
    compact: bool,
    dropdown_caret: bool,
    loading: bool,
    min_width: Option<Pixels>,
    width: Option<Pixels>,
    corners: Option<Corners>,
    fill: bool,
    icon: Option<IconName>,
    align_start: bool,
    omit_left_border: bool,
    omit_top_border: bool,
    on_click: Option<Handler>,
    on_press: Option<Handler>,
}

impl Button {
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            theme: Theme::default(),
            label: String::new(),
            variant: ButtonVariant::Default,
            size: ControlSize::Medium,
            disabled: false,
            selected: false,
            outline: false,
            compact: false,
            dropdown_caret: false,
            loading: false,
            min_width: None,
            width: None,
            corners: None,
            fill: false,
            icon: None,
            align_start: false,
            omit_left_border: false,
            omit_top_border: false,
            on_click: None,
            on_press: None,
        }
    }

    pub fn themed(mut self, theme: &Theme) -> Self {
        self.theme = *theme;
        self
    }

    pub fn label(mut self, label: impl Into<String>) -> Self {
        self.label = label.into();
        self
    }

    pub fn icon(mut self, name: IconName) -> Self {
        self.icon = Some(name);
        self
    }

    pub fn outline(mut self) -> Self {
        self.outline = true;
        self
    }

    pub fn compact(mut self) -> Self {
        self.compact = true;
        self
    }

    pub fn dropdown_caret(mut self, on: bool) -> Self {
        self.dropdown_caret = on;
        self
    }

    pub fn loading(mut self, loading: bool) -> Self {
        self.loading = loading;
        self
    }

    pub fn min_w(mut self, width: Pixels) -> Self {
        self.min_width = Some(width);
        self
    }

    /// Fixed width (calendar day cells). Clips only when the box is explicit.
    pub fn w(mut self, width: Pixels) -> Self {
        self.width = Some(width);
        self
    }

    /// Stretch with the parent flex (calculator keys, split panes).
    pub fn fill(mut self) -> Self {
        self.fill = true;
        self
    }

    pub fn align_start(mut self) -> Self {
        self.align_start = true;
        self
    }

    pub fn on_click(mut self, f: impl Fn() + Send + Sync + 'static) -> Self {
        self.on_click = Some(Arc::new(f));
        self
    }

    /// Activate on mouse down (start-menu rows). `on_click` still fires on
    /// mouse up; use one or the other so a press does not launch twice.
    pub fn on_press(mut self, f: impl Fn() + Send + Sync + 'static) -> Self {
        self.on_press = Some(Arc::new(f));
        self
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn is_disabled(&self) -> bool {
        self.disabled
    }

    pub fn is_selected(&self) -> bool {
        self.selected
    }

    pub(crate) fn with_corners(mut self, corners: Corners) -> Self {
        self.corners = Some(corners);
        self
    }

    pub(crate) fn omit_left_border(mut self) -> Self {
        self.omit_left_border = true;
        self
    }

    pub(crate) fn omit_top_border(mut self) -> Self {
        self.omit_top_border = true;
        self
    }

    pub(crate) fn take_click(&self) -> Option<Handler> {
        self.on_click.clone()
    }
}

impl ButtonVariants for Button {
    fn with_variant(mut self, variant: ButtonVariant) -> Self {
        self.variant = variant;
        self
    }
}

impl Sizable for Button {
    fn with_size(mut self, size: ControlSize) -> Self {
        self.size = size;
        self
    }
}

impl Disableable for Button {
    fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
}

impl Selectable for Button {
    fn selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }
}

struct Face {
    bg: Hsla,
    fg: Hsla,
    border: Hsla,
    hover: Hsla,
    active: Hsla,
    has_border: bool,
}

fn hover_of(base: Hsla, dark: bool) -> Hsla {
    if dark {
        base.lighten(0.10)
    } else {
        base.darken(0.06)
    }
}

fn active_of(base: Hsla, dark: bool) -> Hsla {
    if dark {
        base.darken(0.10)
    } else {
        base.darken(0.12)
    }
}

fn face_for(button: &Button) -> Face {
    let theme = button.theme;
    let c = theme.colors;
    let dark = theme.mode.is_dark();
    let transparent = Hsla::TRANSPARENT;

    if button.disabled {
        let ghost = matches!(
            button.variant,
            ButtonVariant::Ghost | ButtonVariant::Link | ButtonVariant::Text
        );
        return Face {
            bg: if ghost {
                transparent
            } else {
                c.muted.opacity(0.55)
            },
            fg: c.muted_foreground,
            border: c.border.opacity(0.5),
            hover: c.muted.opacity(0.55),
            active: c.muted.opacity(0.55),
            has_border: button.outline && !ghost,
        };
    }

    let filled = |bg: Hsla, fg: Hsla| Face {
        bg,
        fg,
        border: bg,
        hover: hover_of(bg, dark),
        active: active_of(bg, dark),
        has_border: false,
    };
    let outlined = |color: Hsla| Face {
        bg: color.opacity(0.10),
        fg: color,
        border: color.opacity(0.55),
        hover: color.opacity(0.20),
        active: color.opacity(0.40),
        has_border: true,
    };

    let mut face = match (button.variant, button.outline) {
        (ButtonVariant::Primary, false) => filled(c.primary, c.primary_foreground),
        (ButtonVariant::Primary, true) => outlined(c.primary),
        (ButtonVariant::Danger, false) => filled(c.destructive, c.destructive_foreground),
        (ButtonVariant::Danger, true) => outlined(c.destructive),
        (ButtonVariant::Warning, false) => filled(c.warning, c.primary_foreground),
        (ButtonVariant::Warning, true) => outlined(c.warning),
        (ButtonVariant::Success, false) => filled(c.success, Hsla::from_rgb(250, 246, 242)),
        (ButtonVariant::Success, true) => outlined(c.success),
        (ButtonVariant::Info, false) => filled(c.info, Hsla::from_rgb(250, 246, 242)),
        (ButtonVariant::Info, true) => outlined(c.info),
        (ButtonVariant::Custom(style), false) => Face {
            bg: style.color,
            fg: style.foreground,
            border: style.color,
            hover: style.hover,
            active: style.active,
            has_border: true,
        },
        (ButtonVariant::Custom(style), true) => Face {
            bg: style.color.opacity(0.12),
            fg: style.foreground,
            border: style.color,
            hover: style.hover,
            active: style.active,
            has_border: true,
        },
        (ButtonVariant::Secondary, false) => Face {
            bg: c.secondary,
            fg: c.secondary_foreground,
            border: c.border,
            hover: hover_of(c.secondary, dark),
            active: active_of(c.secondary, dark),
            has_border: true,
        },
        (ButtonVariant::Ghost, true) => Face {
            bg: transparent,
            fg: c.foreground,
            border: c.border,
            hover: c.accent,
            active: c.accent.darken(0.06),
            has_border: true,
        },
        (ButtonVariant::Ghost, false) => Face {
            bg: transparent,
            fg: c.foreground,
            border: transparent,
            hover: c.accent,
            active: c.accent.darken(0.06),
            has_border: false,
        },
        (ButtonVariant::Link, _) => Face {
            bg: transparent,
            fg: c.primary,
            border: transparent,
            hover: c.primary.opacity(0.14),
            active: c.primary.opacity(0.24),
            has_border: false,
        },
        (ButtonVariant::Text, _) => Face {
            bg: transparent,
            fg: c.foreground,
            border: transparent,
            hover: c.muted,
            active: c.accent,
            has_border: false,
        },
        (_, true) => Face {
            bg: transparent,
            fg: c.foreground,
            border: c.border,
            hover: c.muted,
            active: c.accent,
            has_border: true,
        },
        _ => Face {
            bg: c.secondary,
            fg: c.secondary_foreground,
            border: c.border,
            hover: hover_of(c.secondary, dark),
            active: active_of(c.secondary, dark),
            has_border: true,
        },
    };

    if button.selected {
        face.bg = face.active;
        face.hover = active_of(face.active, dark);
    }
    face
}

impl IntoElement for Button {
    type Element = Div;

    fn into_element(self) -> Div {
        let face = face_for(&self);
        let radius = self.theme.radius.md;
        let corners = self.corners.unwrap_or(Corners::all(radius));
        let h = px(self.size.height());
        let pad_x = px(self.size.pad_x(
            self.compact
                || self.variant == ButtonVariant::Link
                || self.variant == ButtonVariant::Text,
        ));
        let font = self.size.font();
        let caret = self.dropdown_caret;
        let label = self.label.clone();
        let loading = self.loading;

        let icon_only = label.is_empty()
            && (self.icon.is_some() || loading)
            && !caret
            && self.width.is_none()
            && !self.fill;
        let mut el = div()
            .flex()
            .items_center()
            .rounded_corners(corners)
            .bg(face.bg);
        el = if self.align_start {
            el.justify_start()
        } else {
            el.justify_center()
        };
        if icon_only {
            el = el.w(h).h(h);
        } else {
            el = el.pad_x(pad_x).h(h);
        }
        if let Some(w) = self.width {
            el = el.w(w).overflow_hidden();
        } else if self.fill {
            el = el.grow();
        }
        if let Some(min_w) = self.min_width {
            el = el.min_w(min_w);
        }
        if face.has_border {
            el = el.border_1().border_color(face.border);
            if self.omit_left_border {
                el = el.border_l_0();
            }
            if self.omit_top_border {
                el = el.border_t_0();
            }
        }

        let interactive = !self.disabled && !loading;
        if interactive {
            let hover = face.hover;
            let active = face.active;
            let ring = self.theme.colors.ring;
            el = el
                .hover(move |s| s.bg(hover))
                .active(move |s| s.bg(active))
                .focus(move |s| s.border_color(ring));
        }

        let show_icon = self.icon.is_some() || loading || caret;
        if show_icon && !label.is_empty() {
            el = el.gap_1();
        }
        if loading {
            el = el.child(
                Spinner::new()
                    .themed(&self.theme)
                    .color(face.fg)
                    .with_size(self.size)
                    .phase(0.18),
            );
        } else if let Some(name) = self.icon {
            el = el.child(
                Icon::new(name)
                    .themed(&self.theme)
                    .color(face.fg)
                    .with_size(self.size),
            );
        }
        if !label.is_empty() {
            el = el.child(text(label).color(face.fg).size(font));
        }
        if caret {
            el = el.child(
                Icon::new(IconName::ChevronDown)
                    .themed(&self.theme)
                    .color(face.fg)
                    .with_size(self.size),
            );
        }

        if interactive {
            if let Some(handler) = self.on_press {
                let press = handler;
                el = el.on_mouse_down(move |_| press());
            }
            if let Some(handler) = self.on_click {
                el = bind_activate(el, handler);
            } else {
                el = el.focusable();
            }
        }
        el
    }
}

/// Joined row/column of buttons (kit `ButtonGroup`).
pub struct ButtonGroup {
    id: String,
    theme: Theme,
    children: Vec<Button>,
    multiple: bool,
    disabled: bool,
    compact: bool,
    outline: bool,
    variant: Option<ButtonVariant>,
    size: Option<ControlSize>,
    vertical: bool,
    on_click: Option<IndexHandler>,
}

impl ButtonGroup {
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            theme: Theme::default(),
            children: Vec::new(),
            multiple: false,
            disabled: false,
            compact: false,
            outline: false,
            variant: None,
            size: None,
            vertical: false,
            on_click: None,
        }
    }

    pub fn themed(mut self, theme: &Theme) -> Self {
        self.theme = *theme;
        self
    }

    pub fn child(mut self, child: Button) -> Self {
        self.children.push(child);
        self
    }

    pub fn multiple(mut self, multiple: bool) -> Self {
        self.multiple = multiple;
        self
    }

    pub fn compact(mut self) -> Self {
        self.compact = true;
        self
    }

    pub fn outline(mut self) -> Self {
        self.outline = true;
        self
    }

    pub fn vertical(mut self) -> Self {
        self.vertical = true;
        self
    }

    pub fn on_click(mut self, f: impl Fn(usize) + Send + Sync + 'static) -> Self {
        self.on_click = Some(Arc::new(f));
        self
    }

    pub fn id(&self) -> &str {
        &self.id
    }
}

impl ButtonVariants for ButtonGroup {
    fn with_variant(mut self, variant: ButtonVariant) -> Self {
        self.variant = Some(variant);
        self
    }
}

impl Sizable for ButtonGroup {
    fn with_size(mut self, size: ControlSize) -> Self {
        self.size = Some(size);
        self
    }
}

impl Disableable for ButtonGroup {
    fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
}

impl IntoElement for ButtonGroup {
    type Element = Div;

    fn into_element(self) -> Div {
        let n = self.children.len();
        let radius = self.theme.radius.md;
        let group_click = self.on_click;
        let mut row = if self.vertical {
            div().flex_col()
        } else {
            div().flex()
        };
        row = row.items_center();

        for (i, mut child) in self.children.into_iter().enumerate() {
            child.theme = self.theme;
            if let Some(v) = self.variant {
                child.variant = v;
            }
            if let Some(s) = self.size {
                child.size = s;
            }
            if self.disabled {
                child.disabled = true;
            }
            if self.compact {
                child.compact = true;
            }
            if self.outline {
                child.outline = true;
            }
            let corners = if n <= 1 {
                Corners::all(radius)
            } else if self.vertical {
                if i == 0 {
                    Corners::top(radius)
                } else if i + 1 == n {
                    Corners::bottom(radius)
                } else {
                    Corners::ZERO
                }
            } else if i == 0 {
                Corners::left(radius)
            } else if i + 1 == n {
                Corners::right(radius)
            } else {
                Corners::ZERO
            };
            child = child.with_corners(corners);
            if i > 0 {
                child = if self.vertical {
                    child.omit_top_border()
                } else {
                    child.omit_left_border()
                };
            }
            if let Some(g) = &group_click {
                let g = g.clone();
                let own = child.take_click();
                child.on_click = Some(Arc::new(move || {
                    if let Some(own) = &own {
                        own();
                    }
                    g(i);
                }));
            }
            row = row.child(child);
        }
        row
    }
}

/// Split action + menu trigger (kit `DropdownButton`). Menu items are a
/// drawn overlay; D8.9 replaces this with `PopupMenu`.
pub struct DropdownButton {
    id: String,
    theme: Theme,
    button: Option<Button>,
    items: Vec<String>,
    open: bool,
    disabled: bool,
    outline: bool,
    variant: Option<ButtonVariant>,
    size: Option<ControlSize>,
    on_toggle: Option<Handler>,
    on_item: Option<IndexHandler>,
}

impl DropdownButton {
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            theme: Theme::default(),
            button: None,
            items: Vec::new(),
            open: false,
            disabled: false,
            outline: false,
            variant: None,
            size: None,
            on_toggle: None,
            on_item: None,
        }
    }

    pub fn themed(mut self, theme: &Theme) -> Self {
        self.theme = *theme;
        self
    }

    pub fn button(mut self, button: Button) -> Self {
        self.button = Some(button);
        self
    }

    pub fn items<I, S>(mut self, items: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.items = items.into_iter().map(Into::into).collect();
        self
    }

    pub fn open(mut self, open: bool) -> Self {
        self.open = open;
        self
    }

    pub fn outline(mut self) -> Self {
        self.outline = true;
        self
    }

    pub fn on_toggle(mut self, f: impl Fn() + Send + Sync + 'static) -> Self {
        self.on_toggle = Some(Arc::new(f));
        self
    }

    pub fn on_item(mut self, f: impl Fn(usize) + Send + Sync + 'static) -> Self {
        self.on_item = Some(Arc::new(f));
        self
    }

    pub fn id(&self) -> &str {
        &self.id
    }
}

impl ButtonVariants for DropdownButton {
    fn with_variant(mut self, variant: ButtonVariant) -> Self {
        self.variant = Some(variant);
        self
    }
}

impl Sizable for DropdownButton {
    fn with_size(mut self, size: ControlSize) -> Self {
        self.size = Some(size);
        self
    }
}

impl Disableable for DropdownButton {
    fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
}

impl IntoElement for DropdownButton {
    type Element = Div;

    fn into_element(self) -> Div {
        let size = self.size.unwrap_or_default();
        let variant = self.variant.unwrap_or_default();
        let radius = self.theme.radius.md;
        let mut action = self
            .button
            .unwrap_or_else(|| Button::new(format!("{}-action", self.id)).label("Action"));
        action.theme = self.theme;
        if self.variant.is_some() {
            action.variant = variant;
        }
        if self.size.is_some() {
            action.size = size;
        }
        if self.disabled {
            action.disabled = true;
        }
        if self.outline {
            action.outline = true;
        }
        action = action.with_corners(Corners::left(radius));

        let mut caret = Button::new(format!("{}-caret", self.id))
            .themed(&self.theme)
            .icon(IconName::ChevronDown)
            .compact()
            .with_variant(variant)
            .with_size(size)
            .disabled(self.disabled)
            .min_w(px(28))
            .with_corners(Corners::right(radius))
            .selected(self.open);
        if self.outline {
            caret = caret.outline();
        }
        if let Some(toggle) = self.on_toggle {
            caret = caret.on_click(move || toggle());
        }

        let h = size.height();
        let mut root = div().flex().items_center().child(action).child(caret);

        if self.open && !self.items.is_empty() {
            let mut menu = div()
                .absolute()
                .top(px(h + 4))
                .left(px(0))
                .flex_col()
                .bg(self.theme.colors.surface)
                .border_1()
                .border_color(self.theme.colors.border)
                .rounded(self.theme.radius.md)
                .p(self.theme.spacing.xs);
            for (i, label) in self.items.into_iter().enumerate() {
                let pick = self.on_item.clone();
                menu = menu.child(
                    Button::new(format!("{}-item-{i}", self.id))
                        .themed(&self.theme)
                        .ghost()
                        .small()
                        .label(label)
                        .on_click(move || {
                            if let Some(pick) = &pick {
                                pick(i);
                            }
                        }),
                );
            }
            root = root.child(menu);
        }
        root
    }
}
