// SPDX-License-Identifier: MIT
//! Desktop icons, rubber-band, and context menu (D9.7).
//! Knox GPUI views, not `desktop/icons.rs` / `context_menu.rs` paint.
//!
//! the Longbridge kit is a design reference only (Apache-2.0). This file is new
//! `no_std` code. KnoxOS shell chrome, not a kit catalog control.

use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;

use knox_gpui::{
    CursorStyle, Div, FluentBuilder, IntoElement, MouseButton, Pixels, Styled, div, px, text,
};

use crate::icon::{Icon, IconName};
use crate::interact::{Handler, IndexHandler};
use crate::menu::{MenuItem, PopupMenu};
use crate::theme::Theme;

/// Grid cell matching the compositor icon metrics.
pub const ICON_CELL_W: i32 = 112;
pub const ICON_CELL_H: i32 = 92;
const ICON_GLYPH: i32 = 40;

/// One desktop shortcut / folder cell.
#[derive(Clone, Debug)]
pub struct DesktopIconEntry {
    pub name: String,
    pub icon: IconName,
    pub x: i32,
    pub y: i32,
    pub selected: bool,
    pub ghost: bool,
}

impl DesktopIconEntry {
    pub fn new(name: impl Into<String>, icon: IconName, x: i32, y: i32) -> Self {
        Self {
            name: name.into(),
            icon,
            x,
            y,
            selected: false,
            ghost: false,
        }
    }

    pub fn selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }

    pub fn ghost(mut self, ghost: bool) -> Self {
        self.ghost = ghost;
        self
    }
}

/// Icon layer over the compositor wallpaper (D9.7).
pub struct DesktopIcons {
    theme: Theme,
    width: Pixels,
    height: Pixels,
    origin_x: i32,
    origin_y: i32,
    icons: Vec<DesktopIconEntry>,
    on_icon: Option<IndexHandler>,
    on_empty: Option<Handler>,
}

impl DesktopIcons {
    pub fn new() -> Self {
        Self {
            theme: Theme::default(),
            width: px(ICON_CELL_W),
            height: px(ICON_CELL_H),
            origin_x: 0,
            origin_y: 0,
            icons: Vec::new(),
            on_icon: None,
            on_empty: None,
        }
    }

    pub fn themed(mut self, theme: &Theme) -> Self {
        self.theme = *theme;
        self
    }

    pub fn size(mut self, width: Pixels, height: Pixels) -> Self {
        self.width = width;
        self.height = height;
        self
    }

    /// Screen origin of this layer, so icon coordinates can stay absolute.
    pub fn origin(mut self, x: i32, y: i32) -> Self {
        self.origin_x = x;
        self.origin_y = y;
        self
    }

    pub fn icons(mut self, icons: impl IntoIterator<Item = DesktopIconEntry>) -> Self {
        self.icons = icons.into_iter().collect();
        self
    }

    pub fn on_icon(mut self, f: impl Fn(usize) + Send + Sync + 'static) -> Self {
        self.on_icon = Some(Arc::new(f));
        self
    }

    pub fn on_empty(mut self, f: impl Fn() + Send + Sync + 'static) -> Self {
        self.on_empty = Some(Arc::new(f));
        self
    }
}

impl Default for DesktopIcons {
    fn default() -> Self {
        Self::new()
    }
}

fn icon_cell(
    theme: &Theme,
    entry: &DesktopIconEntry,
    local_x: i32,
    local_y: i32,
    index: usize,
    on_icon: Option<IndexHandler>,
) -> Div {
    let selected = entry.selected;
    let ghost = entry.ghost;
    let fill = if ghost {
        theme.colors.muted.alpha(0.35)
    } else if selected {
        theme.colors.accent.alpha(0.45)
    } else {
        knox_gpui::Hsla::TRANSPARENT
    };
    let border = if selected {
        theme.colors.primary
    } else {
        theme.colors.border.alpha(0.0)
    };
    let glyph = if ghost {
        theme.colors.muted_foreground
    } else if selected {
        theme.colors.primary
    } else {
        theme.colors.foreground
    };
    let pick = on_icon;
    div()
        .absolute()
        .left(px(local_x))
        .top(px(local_y))
        .w(px(ICON_CELL_W))
        .h(px(ICON_CELL_H))
        .flex_col()
        .items_center()
        .rounded(theme.radius.md)
        .bg(fill)
        .border_1()
        .border_color(border)
        .cursor(CursorStyle::PointingHand)
        .hover(|s| s.bg(theme.colors.accent.alpha(0.28)))
        .on_mouse_down(move |e| {
            if e.button == MouseButton::Left {
                if let Some(p) = &pick {
                    p(index);
                }
            }
        })
        .child(
            Icon::new(entry.icon)
                .themed(theme)
                .color(glyph)
                .pixel_size(ICON_GLYPH),
        )
        .child(
            div().mt(theme.spacing.xs).child(
                text(entry.name.clone())
                    .color(if selected {
                        theme.colors.primary
                    } else {
                        knox_gpui::Hsla::WHITE
                    })
                    .size(11),
            ),
        )
}

impl IntoElement for DesktopIcons {
    type Element = Div;

    fn into_element(self) -> Div {
        let on_empty = self.on_empty.clone();
        let mut root = div().w(self.width).h(self.height).on_mouse_down(move |e| {
            if e.button == MouseButton::Left {
                if let Some(h) = &on_empty {
                    h();
                }
            }
        });
        for (i, entry) in self.icons.iter().enumerate() {
            let lx = entry.x - self.origin_x;
            let ly = entry.y - self.origin_y;
            root = root.child(icon_cell(
                &self.theme,
                entry,
                lx,
                ly,
                i,
                self.on_icon.clone(),
            ));
        }
        root
    }
}

/// Rubber-band selection overlay. Window size is the band rect.
pub struct DesktopRubberBand {
    theme: Theme,
    width: Pixels,
    height: Pixels,
}

impl DesktopRubberBand {
    pub fn new() -> Self {
        Self {
            theme: Theme::default(),
            width: px(1),
            height: px(1),
        }
    }

    pub fn themed(mut self, theme: &Theme) -> Self {
        self.theme = *theme;
        self
    }

    pub fn size(mut self, width: Pixels, height: Pixels) -> Self {
        self.width = width;
        self.height = height;
        self
    }
}

impl Default for DesktopRubberBand {
    fn default() -> Self {
        Self::new()
    }
}

impl IntoElement for DesktopRubberBand {
    type Element = Div;

    fn into_element(self) -> Div {
        // Rounded so the raster blends translucent fill (sharp quads skip alpha).
        div()
            .w(self.width)
            .h(self.height)
            .rounded(px(2))
            .bg(self.theme.colors.primary.alpha(0.18))
            .border_1()
            .border_color(self.theme.colors.primary.alpha(0.55))
    }
}

/// Desktop right-click menu (drawn `PopupMenu`, no native host).
pub struct DesktopMenu {
    theme: Theme,
    items: Vec<MenuItem>,
    on_pick: Option<IndexHandler>,
    on_dismiss: Option<Handler>,
}

impl DesktopMenu {
    pub const WIDTH: i32 = 180;

    pub fn new() -> Self {
        Self {
            theme: Theme::default(),
            items: Vec::new(),
            on_pick: None,
            on_dismiss: None,
        }
    }

    pub fn themed(mut self, theme: &Theme) -> Self {
        self.theme = *theme;
        self
    }

    pub fn items(mut self, items: impl IntoIterator<Item = MenuItem>) -> Self {
        self.items = items.into_iter().collect();
        self
    }

    pub fn on_pick(mut self, f: impl Fn(usize) + Send + Sync + 'static) -> Self {
        self.on_pick = Some(Arc::new(f));
        self
    }

    pub fn on_dismiss(mut self, f: impl Fn() + Send + Sync + 'static) -> Self {
        self.on_dismiss = Some(Arc::new(f));
        self
    }

    /// Backing-store size matching the drawn `PopupMenu` rows.
    pub fn panel_size(items: &[MenuItem]) -> (i32, i32) {
        let mut h = 8;
        for item in items {
            h += if item.separator { 8 } else { 28 };
        }
        (Self::WIDTH, h.max(36))
    }
}

impl Default for DesktopMenu {
    fn default() -> Self {
        Self::new()
    }
}

impl IntoElement for DesktopMenu {
    type Element = Div;

    fn into_element(self) -> Div {
        PopupMenu::new("desktop-ctx")
            .themed(&self.theme)
            .items(self.items)
            .open(true)
            .on_pick({
                let pick = self.on_pick.clone();
                move |i| {
                    if let Some(p) = &pick {
                        p(i);
                    }
                }
            })
            .when(self.on_dismiss.is_some(), |menu| {
                let d = self.on_dismiss.clone();
                menu.on_dismiss(move || {
                    if let Some(h) = &d {
                        h();
                    }
                })
            })
            .into_element()
    }
}

#[cfg(test)]
mod tests {
    use super::{
        DesktopIconEntry, DesktopIcons, DesktopMenu, DesktopRubberBand, ICON_CELL_H, ICON_CELL_W,
    };
    use crate::icon::IconName;
    use crate::menu::MenuItem;
    use crate::theme::Theme;
    use alloc::sync::Arc;
    use alloc::vec;
    use core::sync::atomic::{AtomicU32, Ordering};
    use knox_gpui::{
        Element, HostTextSystem, IntoElement, PaintEnv, Point, RasterBuffer, Scene, Size,
        SpriteAtlas, WindowInput, dispatch_mouse_down, paint_element_with_input, px,
    };

    fn paint(
        mut el: knox_gpui::Div,
        origin: Point,
        size: Size,
        input: &mut WindowInput,
    ) -> RasterBuffer {
        let mut atlas = SpriteAtlas::new();
        let sys = HostTextSystem;
        let mut env = PaintEnv {
            atlas: &mut atlas,
            text: Some(&sys),
        };
        let mut scene = Scene::new();
        paint_element_with_input(&mut el, origin, size, &mut scene, input, &mut env);
        let mut buf =
            RasterBuffer::new(size.width.0.max(1) as usize, size.height.0.max(1) as usize);
        buf.draw_with_atlas(&scene, Some(&atlas));
        buf
    }

    #[test]
    fn selected_icon_paints_fill() {
        let theme = Theme::aurora_dark();
        let size = Size::wh(ICON_CELL_W + 8, ICON_CELL_H + 8);
        let rest = paint(
            DesktopIcons::new()
                .themed(&theme)
                .size(px(size.width.0), px(size.height.0))
                .icons([DesktopIconEntry::new("Files", IconName::Folder, 4, 4)])
                .into_element(),
            Point::ZERO,
            size,
            &mut WindowInput::new(),
        );
        let selected = paint(
            DesktopIcons::new()
                .themed(&theme)
                .size(px(size.width.0), px(size.height.0))
                .icons([DesktopIconEntry::new("Files", IconName::Folder, 4, 4).selected(true)])
                .into_element(),
            Point::ZERO,
            size,
            &mut WindowInput::new(),
        );
        let a = rest.pixel(8, 8).unwrap();
        let b = selected.pixel(8, 8).unwrap();
        assert_ne!(a, b, "selected cell must differ from idle");
        assert!(b.a > 0);
    }

    #[test]
    fn icon_press_fires_index() {
        let hit = Arc::new(AtomicU32::new(99));
        let id = hit.clone();
        let theme = Theme::aurora_dark();
        let size = Size::wh(ICON_CELL_W, ICON_CELL_H);
        let mut el = DesktopIcons::new()
            .themed(&theme)
            .size(px(ICON_CELL_W), px(ICON_CELL_H))
            .icons([DesktopIconEntry::new("Files", IconName::Folder, 0, 0)])
            .on_icon(move |i| id.store(i as u32, Ordering::SeqCst))
            .into_element();
        let mut input = WindowInput::new();
        el.request_layout(Point::ZERO, size);
        el.prepaint(el.computed(), &mut input);
        dispatch_mouse_down(&mut el, &mut input, Point::xy(40, 24));
        assert_eq!(hit.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn rubber_band_has_alpha_falloff() {
        let theme = Theme::aurora_dark();
        let size = Size::wh(48, 32);
        let buf = paint(
            DesktopRubberBand::new()
                .themed(&theme)
                .size(px(48), px(32))
                .into_element(),
            Point::ZERO,
            size,
            &mut WindowInput::new(),
        );
        let mid = buf.pixel(24, 16).unwrap();
        assert!(
            mid.a > 0 && mid.a < 255,
            "translucent fill, got a={}",
            mid.a
        );
        let corner = buf.pixel(0, 0).unwrap();
        assert!(
            corner.a > 0 && corner.a < 255,
            "rounded corner neither empty nor stair, a={}",
            corner.a
        );
    }

    #[test]
    fn context_menu_paints_when_open() {
        let theme = Theme::aurora_dark();
        let items = vec![
            MenuItem::new("Open Files"),
            MenuItem::separator(),
            MenuItem::new("Refresh Desktop"),
        ];
        let (w, h) = DesktopMenu::panel_size(&items);
        let buf = paint(
            DesktopMenu::new()
                .themed(&theme)
                .items(items)
                .into_element(),
            Point::ZERO,
            Size::wh(w, h),
            &mut WindowInput::new(),
        );
        let px = buf.pixel(8, 8).unwrap();
        assert!(px.a > 0, "menu surface must be visible");
    }

    #[test]
    fn menu_pick_fires() {
        let hit = Arc::new(AtomicU32::new(99));
        let id = hit.clone();
        let theme = Theme::aurora_dark();
        let items = vec![MenuItem::new("Open Files"), MenuItem::new("Open Terminal")];
        let (w, h) = DesktopMenu::panel_size(&items);
        let mut el = DesktopMenu::new()
            .themed(&theme)
            .items(items)
            .on_pick(move |i| id.store(i as u32, Ordering::SeqCst))
            .into_element();
        let mut input = WindowInput::new();
        el.request_layout(Point::ZERO, Size::wh(w, h));
        el.prepaint(el.computed(), &mut input);
        let p = input
            .hitboxes()
            .iter()
            .filter(|hb| hb.bounds.height() >= 12 && hb.bounds.width() >= 80)
            .min_by_key(|hb| hb.bounds.y())
            .map(|hb| Point::xy(hb.bounds.x() + 12, hb.bounds.y() + 8))
            .expect("menu row");
        dispatch_mouse_down(&mut el, &mut input, p);
        knox_gpui::dispatch_mouse_up(&mut el, &mut input, p);
        assert_eq!(hit.load(Ordering::SeqCst), 0);
    }
}
