// SPDX-License-Identifier: MIT
//! Pointer, key, scroll, focus, and per-window input state.
//!
//! Type names follow Zed GPUI (`Hitbox`, `MouseButton`, `Modifiers`, `Keystroke`)
//! as a design reference (Apache-2.0). This file is new `no_std` code.

use alloc::vec::Vec;

use crate::geometry::{Bounds, Point};

/// Stable id for a hitbox registered during `prepaint`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct HitboxId(u32);

impl HitboxId {
    pub const fn from_u32(id: u32) -> Self {
        Self(id)
    }

    pub const fn as_u32(self) -> u32 {
        self.0
    }
}

/// Focus handle wrapping the current tab-stop / click target.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct FocusHandle {
    id: HitboxId,
}

impl FocusHandle {
    pub const fn from_hitbox(id: HitboxId) -> Self {
        Self { id }
    }

    pub const fn id(self) -> HitboxId {
        self.id
    }
}

/// Software cursor requested by a hitbox (D6.5).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum CursorStyle {
    #[default]
    Arrow,
    IBeam,
    PointingHand,
    ResizeEast,
    ResizeWest,
    ResizeNorth,
    ResizeSouth,
    ResizeNorthEast,
    ResizeNorthWest,
    ResizeSouthEast,
    ResizeSouthWest,
}

/// Axis-aligned hit region, later-painted on top.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Hitbox {
    pub id: HitboxId,
    pub bounds: Bounds,
    pub cursor: CursorStyle,
    pub tab_stop: bool,
    pub accepts_ime: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum MouseButton {
    #[default]
    Left,
    Right,
    Middle,
}

/// Ctrl / Alt / Shift / Super (GPUI `platform` / cmd).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Modifiers {
    pub control: bool,
    pub alt: bool,
    pub shift: bool,
    pub platform: bool,
}

impl Modifiers {
    pub const NONE: Self = Self {
        control: false,
        alt: false,
        shift: false,
        platform: false,
    };

    pub const fn new(control: bool, alt: bool, shift: bool, platform: bool) -> Self {
        Self {
            control,
            alt,
            shift,
            platform,
        }
    }

    pub fn is_empty(self) -> bool {
        self == Self::NONE
    }
}

/// Logical key. Character keys are stored lowercase.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Key {
    Char(char),
    Tab,
    Enter,
    Escape,
    Backspace,
    Delete,
    Left,
    Right,
    Up,
    Down,
    Home,
    End,
    PageUp,
    PageDown,
    Space,
}

impl Key {
    pub fn from_name(name: &str) -> Option<Self> {
        if name.len() == 1 {
            let c = name.chars().next()?.to_ascii_lowercase();
            return Some(Self::Char(c));
        }
        let lower = name.to_ascii_lowercase();
        Some(match lower.as_str() {
            "tab" => Self::Tab,
            "enter" | "return" => Self::Enter,
            "escape" | "esc" => Self::Escape,
            "backspace" => Self::Backspace,
            "delete" => Self::Delete,
            "left" => Self::Left,
            "right" => Self::Right,
            "up" => Self::Up,
            "down" => Self::Down,
            "home" => Self::Home,
            "end" => Self::End,
            "pageup" => Self::PageUp,
            "pagedown" => Self::PageDown,
            "space" => Self::Space,
            _ => return None,
        })
    }
}

/// Key plus modifiers. Parse strings like `cmd-k` / `shift-tab`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Keystroke {
    pub key: Key,
    pub modifiers: Modifiers,
}

impl Keystroke {
    pub fn new(key: Key, modifiers: Modifiers) -> Self {
        Self { key, modifiers }
    }

    /// Split on `-`. Last token is the key; earlier tokens are modifiers.
    pub fn parse(spec: &str) -> Option<Self> {
        let mut modifiers = Modifiers::NONE;
        let mut key = None;
        for part in spec.split('-') {
            let p = part.trim();
            if p.is_empty() {
                continue;
            }
            match p {
                "cmd" | "super" | "win" | "platform" | "meta" => modifiers.platform = true,
                "ctrl" | "control" => modifiers.control = true,
                "alt" | "option" => modifiers.alt = true,
                "shift" => modifiers.shift = true,
                other => {
                    key = Some(Key::from_name(other)?);
                }
            }
        }
        Some(Self {
            key: key?,
            modifiers,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MouseEvent {
    pub position: Point,
    pub button: MouseButton,
    pub modifiers: Modifiers,
}

impl MouseEvent {
    pub fn new(position: Point, button: MouseButton) -> Self {
        Self {
            position,
            button,
            modifiers: Modifiers::NONE,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ClickEvent {
    pub position: Point,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ScrollEvent {
    pub position: Point,
    pub delta: Point,
    pub modifiers: Modifiers,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct KeyDownEvent {
    pub key: Key,
    pub keystroke: Keystroke,
}

impl KeyDownEvent {
    pub fn new(key: Key, modifiers: Modifiers) -> Self {
        Self {
            key,
            keystroke: Keystroke { key, modifiers },
        }
    }
}

/// Per-window pointer / focus / hit-test state. Lives on [`crate::Window`].
///
/// Hitboxes are rebuilt each `prepare` / paint. Pointer position, press, and
/// focus persist across frames so hover/active resolve on the next paint.
pub struct WindowInput {
    pointer: Point,
    mouse_down: bool,
    mouse_down_hitbox: Option<HitboxId>,
    focused: Option<HitboxId>,
    dispatch_target: Option<HitboxId>,
    modifiers: Modifiers,
    context: &'static str,
    next_id: u32,
    hitboxes: Vec<Hitbox>,
    /// Inclusive hitbox-id range that Tab cycles inside (D8.8 FocusTrap).
    tab_trap: Option<(u32, u32)>,
}

impl WindowInput {
    pub const DEFAULT_CONTEXT: &'static str = "Window";
    pub const TEXT_CONTEXT: &'static str = "TextInput";
    pub const LAUNCHER_CONTEXT: &'static str = "Launcher";

    pub fn new() -> Self {
        Self {
            pointer: Point::ZERO,
            mouse_down: false,
            mouse_down_hitbox: None,
            focused: None,
            dispatch_target: None,
            modifiers: Modifiers::NONE,
            context: Self::DEFAULT_CONTEXT,
            next_id: 1,
            hitboxes: Vec::new(),
            tab_trap: None,
        }
    }

    pub fn pointer(&self) -> Point {
        self.pointer
    }

    pub fn is_mouse_down(&self) -> bool {
        self.mouse_down
    }

    pub fn mouse_down_hitbox(&self) -> Option<HitboxId> {
        self.mouse_down_hitbox
    }

    pub fn focused(&self) -> Option<HitboxId> {
        self.focused
    }

    pub fn focus_handle(&self) -> Option<FocusHandle> {
        self.focused.map(FocusHandle::from_hitbox)
    }

    pub fn dispatch_target(&self) -> Option<HitboxId> {
        self.dispatch_target
    }

    pub fn modifiers(&self) -> Modifiers {
        self.modifiers
    }

    pub fn set_modifiers(&mut self, modifiers: Modifiers) {
        self.modifiers = modifiers;
    }

    pub fn context(&self) -> &'static str {
        self.context
    }

    pub fn set_context(&mut self, context: &'static str) {
        self.context = context;
    }

    pub fn hitboxes(&self) -> &[Hitbox] {
        &self.hitboxes
    }

    /// Update cursor position without changing press/focus (hover tracking).
    pub fn set_pointer_position(&mut self, position: Point) {
        self.pointer = position;
    }

    pub fn set_pointer(&mut self, position: Point, mouse_down: bool) {
        self.pointer = position;
        self.mouse_down = mouse_down;
        if !mouse_down {
            self.mouse_down_hitbox = None;
        }
    }

    /// Drop last frame's hitboxes; keep pointer / press / focus.
    pub fn begin_hitboxes(&mut self) {
        self.hitboxes.clear();
        self.next_id = 1;
        self.tab_trap = None;
    }

    pub fn peek_next_hitbox(&self) -> u32 {
        self.next_id
    }

    /// Restrict Tab to hitboxes whose ids fall in `from..=to`.
    pub fn set_tab_trap(&mut self, from: u32, to: u32) {
        self.tab_trap = Some((from.min(to), from.max(to)));
    }

    pub fn insert_hitbox(&mut self, bounds: Bounds) -> HitboxId {
        self.insert_hitbox_ex(bounds, CursorStyle::Arrow, false, false)
    }

    pub fn insert_hitbox_ex(
        &mut self,
        bounds: Bounds,
        cursor: CursorStyle,
        tab_stop: bool,
        accepts_ime: bool,
    ) -> HitboxId {
        let id = HitboxId(self.next_id);
        self.next_id = self.next_id.saturating_add(1);
        self.hitboxes.push(Hitbox {
            id,
            bounds,
            cursor,
            tab_stop,
            accepts_ime,
        });
        id
    }

    pub fn topmost_at(&self, p: Point) -> Option<HitboxId> {
        self.hitbox_at(p).map(|h| h.id)
    }

    pub fn hitbox_at(&self, p: Point) -> Option<&Hitbox> {
        self.hitboxes.iter().rev().find(|h| h.bounds.contains(p))
    }

    pub fn hitbox(&self, id: HitboxId) -> Option<&Hitbox> {
        self.hitboxes.iter().find(|h| h.id == id)
    }

    pub fn bounds_of(&self, id: HitboxId) -> Option<Bounds> {
        self.hitbox(id).map(|h| h.bounds)
    }

    /// Cursor for the topmost hitbox at `p` (Arrow if none).
    pub fn cursor_at(&self, p: Point) -> CursorStyle {
        self.hitbox_at(p)
            .map(|h| h.cursor)
            .unwrap_or(CursorStyle::Arrow)
    }

    pub fn focused_accepts_ime(&self) -> bool {
        self.focused
            .and_then(|id| self.hitbox(id))
            .map(|h| h.accepts_ime)
            .unwrap_or(false)
    }

    pub fn focused_bounds(&self) -> Option<Bounds> {
        self.focused.and_then(|id| self.bounds_of(id))
    }

    /// Bitmask of which registered hitboxes currently contain the pointer.
    pub fn hover_mask(&self) -> u64 {
        let mut mask = 0u64;
        for (i, hit) in self.hitboxes.iter().enumerate() {
            if i >= 64 {
                break;
            }
            if hit.bounds.contains(self.pointer) {
                mask |= 1u64 << i;
            }
        }
        mask
    }

    pub fn element_state(&self, id: Option<HitboxId>) -> crate::style::ElementState {
        let Some(id) = id else {
            return crate::style::ElementState::default();
        };
        let hovered = self
            .bounds_of(id)
            .map(|b| b.contains(self.pointer))
            .unwrap_or(false);
        crate::style::ElementState {
            hovered,
            active: self.mouse_down && self.mouse_down_hitbox == Some(id),
            focused: self.focused == Some(id),
        }
    }

    pub fn focus(&mut self, id: HitboxId) {
        self.focused = Some(id);
        self.sync_context_from(id);
    }

    pub fn blur(&mut self) {
        self.focused = None;
        self.context = Self::DEFAULT_CONTEXT;
    }

    /// Cycle tab stops. Returns whether focus changed.
    pub fn tab(&mut self, reverse: bool) -> bool {
        let stops: Vec<(HitboxId, bool)> = self
            .hitboxes
            .iter()
            .filter(|h| h.tab_stop)
            .filter(|h| match self.tab_trap {
                Some((lo, hi)) => {
                    let id = h.id.as_u32();
                    id >= lo && id <= hi
                }
                None => true,
            })
            .map(|h| (h.id, h.accepts_ime))
            .collect();
        if stops.is_empty() {
            return false;
        }
        let idx = self
            .focused
            .and_then(|f| stops.iter().position(|(id, _)| *id == f));
        let next = match (idx, reverse) {
            (None, false) => 0,
            (None, true) => stops.len() - 1,
            (Some(i), false) => (i + 1) % stops.len(),
            (Some(i), true) => {
                if i == 0 {
                    stops.len() - 1
                } else {
                    i - 1
                }
            }
        };
        self.focused = Some(stops[next].0);
        self.context = if stops[next].1 {
            Self::TEXT_CONTEXT
        } else {
            Self::DEFAULT_CONTEXT
        };
        true
    }

    pub(crate) fn arm_press(&mut self, position: Point) {
        self.pointer = position;
        self.mouse_down = true;
        let target = self.topmost_at(position);
        self.mouse_down_hitbox = target;
        self.dispatch_target = target;
        match target {
            Some(id) => self.focus(id),
            None => self.blur(),
        }
    }

    pub(crate) fn arm_release(&mut self, position: Point) -> Option<HitboxId> {
        self.pointer = position;
        self.mouse_down = false;
        let up = self.topmost_at(position);
        self.dispatch_target = up;
        let down = self.mouse_down_hitbox.take();
        if down.is_some() && down == up {
            up
        } else {
            None
        }
    }

    pub(crate) fn arm_move(&mut self, position: Point) {
        self.pointer = position;
        self.dispatch_target = self.topmost_at(position);
    }

    fn sync_context_from(&mut self, id: HitboxId) {
        self.context = if self.hitbox(id).map(|h| h.accepts_ime).unwrap_or(false) {
            Self::TEXT_CONTEXT
        } else {
            Self::DEFAULT_CONTEXT
        };
    }
}

impl Default for WindowInput {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::{
        CursorStyle, HitboxId, Key, KeyDownEvent, Modifiers, MouseButton, MouseEvent, WindowInput,
    };
    use crate::element::{dispatch_key_down, dispatch_scroll, prepare_element};
    use crate::geometry::{Bounds, Point, Size, px};
    use crate::style::Styled;
    use crate::{div, text_input};
    use alloc::sync::Arc;
    use core::sync::atomic::{AtomicI32, Ordering};

    #[test]
    fn hit_test_prefers_later_hitbox() {
        let mut input = WindowInput::new();
        let a = input.insert_hitbox(Bounds::from_xywh(0, 0, 10, 10));
        let b = input.insert_hitbox(Bounds::from_xywh(0, 0, 10, 10));
        assert_eq!(input.topmost_at(Point::xy(5, 5)), Some(b));
        assert_eq!(a, HitboxId::from_u32(1));
        input.set_pointer_position(Point::xy(5, 5));
        assert_eq!(input.hover_mask(), 0b11);
        input.set_pointer_position(Point::xy(20, 20));
        assert_eq!(input.hover_mask(), 0);
    }

    /// D6.1: mouse events carry modifiers; scroll reaches a handler.
    #[test]
    fn mouse_modifiers_and_scroll_dispatch() {
        let dy = Arc::new(AtomicI32::new(0));
        let c = dy.clone();
        let mut root = div().w(px(10)).h(px(10)).on_scroll(move |e| {
            c.store(e.delta.y.0, Ordering::SeqCst);
        });
        let mut input = WindowInput::new();
        input.set_modifiers(Modifiers::new(true, false, true, false));
        prepare_element(&mut root, Point::ZERO, Size::wh(10, 10), &mut input);
        input.set_pointer_position(Point::xy(2, 2));
        dispatch_scroll(&mut root, &mut input, Point::xy(0, 3));
        assert_eq!(dy.load(Ordering::SeqCst), 3);
        let ev = MouseEvent {
            position: Point::xy(1, 1),
            button: MouseButton::Right,
            modifiers: input.modifiers(),
        };
        assert!(ev.modifiers.control && ev.modifiers.shift);
        assert_eq!(ev.button, MouseButton::Right);
    }

    /// D6.2: Tab moves between two tab stops.
    #[test]
    fn tab_trap_stays_inside_range() {
        let mut input = WindowInput::new();
        let a = input.insert_hitbox_ex(
            Bounds::from_xywh(0, 0, 10, 10),
            CursorStyle::Arrow,
            true,
            false,
        );
        let b = input.insert_hitbox_ex(
            Bounds::from_xywh(0, 12, 10, 10),
            CursorStyle::Arrow,
            true,
            false,
        );
        let c = input.insert_hitbox_ex(
            Bounds::from_xywh(0, 24, 10, 10),
            CursorStyle::Arrow,
            true,
            false,
        );
        input.set_tab_trap(b.as_u32(), c.as_u32());
        assert!(input.tab(false));
        assert_eq!(input.focused(), Some(b));
        assert!(input.tab(false));
        assert_eq!(input.focused(), Some(c));
        assert!(input.tab(false));
        assert_eq!(input.focused(), Some(b));
        let _ = a;
    }

    /// D6.2: Tab moves between two tab stops.
    #[test]
    fn tab_moves_between_two_buttons() {
        let mut root = div()
            .flex_col()
            .gap_2()
            .child(div().w(px(10)).h(px(10)).on_click(|_| {}))
            .child(div().w(px(10)).h(px(10)).on_click(|_| {}));
        let mut input = WindowInput::new();
        prepare_element(&mut root, Point::ZERO, Size::wh(10, 40), &mut input);
        assert!(input.focused().is_none());
        assert!(dispatch_key_down(
            &mut root,
            &mut input,
            &KeyDownEvent::new(Key::Tab, Modifiers::NONE)
        ));
        let first = input.focused();
        assert!(first.is_some());
        assert!(dispatch_key_down(
            &mut root,
            &mut input,
            &KeyDownEvent::new(Key::Tab, Modifiers::NONE)
        ));
        let second = input.focused();
        assert_ne!(first, second);
        assert!(dispatch_key_down(
            &mut root,
            &mut input,
            &KeyDownEvent::new(Key::Tab, Modifiers::new(false, false, true, false))
        ));
        assert_eq!(input.focused(), first);
    }

    /// D6.5: hitbox cursor style (ibeam on an input).
    #[test]
    fn input_hitbox_reports_ibeam() {
        let mut field = text_input("");
        let mut input = WindowInput::new();
        prepare_element(&mut field, Point::ZERO, Size::wh(80, 24), &mut input);
        assert_eq!(input.cursor_at(Point::xy(4, 4)), CursorStyle::IBeam);
        assert_eq!(input.cursor_at(Point::xy(200, 200)), CursorStyle::Arrow);
        assert!(input.hitboxes().iter().any(|h| h.accepts_ime && h.tab_stop));
    }
}
