// SPDX-License-Identifier: MIT
//! Slider (GPUI kit catalog, reimplemented).
//!
//! the Longbridge kit is a design reference only (Apache-2.0). This file is new
//! `no_std` code.

use alloc::sync::Arc;

use knox_gpui::{
    Bounds, CursorStyle, Element, HitboxId, Key, KeyDownEvent, LayoutHints, MouseEvent, PaintEnv,
    Pixels, Point, Quad, Scene, Size, WindowInput, px,
};

use crate::size::{ControlSize, Disableable, Sizable};
use crate::theme::Theme;

type F32Handler = Arc<dyn Fn(f32) + Send + Sync>;

/// Linear range control. Values are in `[min, max]` (default 0..=100).
pub struct Slider {
    theme: Theme,
    value: f32,
    min: f32,
    max: f32,
    step: f32,
    width: Pixels,
    size: ControlSize,
    disabled: bool,
    on_change: Option<F32Handler>,
    computed: Bounds,
    hitbox_id: Option<HitboxId>,
}

impl Slider {
    pub fn new() -> Self {
        Self {
            theme: Theme::default(),
            value: 0.0,
            min: 0.0,
            max: 100.0,
            step: 1.0,
            width: px(160),
            size: ControlSize::Medium,
            disabled: false,
            on_change: None,
            computed: Bounds::ZERO,
            hitbox_id: None,
        }
    }

    pub fn themed(mut self, theme: &Theme) -> Self {
        self.theme = *theme;
        self
    }

    pub fn value(mut self, value: f32) -> Self {
        self.value = value;
        self
    }

    pub fn min(mut self, min: f32) -> Self {
        self.min = min;
        self
    }

    pub fn max(mut self, max: f32) -> Self {
        self.max = max;
        self
    }

    pub fn step(mut self, step: f32) -> Self {
        self.step = step.max(0.0);
        self
    }

    pub fn w(mut self, width: Pixels) -> Self {
        self.width = width;
        self
    }

    pub fn on_change(mut self, f: impl Fn(f32) + Send + Sync + 'static) -> Self {
        self.on_change = Some(Arc::new(f));
        self
    }

    pub fn value_of(&self) -> f32 {
        self.clamp(self.value)
    }

    fn span(&self) -> f32 {
        let s = self.max - self.min;
        if s.abs() < f32::EPSILON { 1.0 } else { s }
    }

    fn clamp(&self, value: f32) -> f32 {
        let mut v = value.clamp(self.min.min(self.max), self.max.max(self.min));
        if self.step > 0.0 {
            let n = libm::roundf((v - self.min) / self.step);
            v = self.min + n * self.step;
        }
        v.clamp(self.min.min(self.max), self.max.max(self.min))
    }

    fn emit(&mut self, value: f32) {
        let value = self.clamp(value);
        if (value - self.value).abs() < f32::EPSILON {
            return;
        }
        self.value = value;
        if let Some(handler) = &self.on_change {
            handler(value);
        }
    }

    fn value_from_x(&self, x: i32, bounds: Bounds) -> f32 {
        let w = bounds.width().max(1) as f32;
        let t = ((x - bounds.x()) as f32 / w).clamp(0.0, 1.0);
        self.min + t * self.span()
    }

    fn is_armed(&self, cx: &WindowInput) -> bool {
        self.hitbox_id.is_some()
            && (self.hitbox_id == cx.dispatch_target() || self.hitbox_id == cx.mouse_down_hitbox())
    }
}

impl Default for Slider {
    fn default() -> Self {
        Self::new()
    }
}

impl Sizable for Slider {
    fn with_size(mut self, size: ControlSize) -> Self {
        self.size = size;
        self
    }
}

impl Disableable for Slider {
    fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
}

impl Element for Slider {
    fn request_layout(&mut self, origin: Point, _available: Size) -> Bounds {
        let h = match self.size {
            ControlSize::XSmall => 14,
            ControlSize::Small => 16,
            ControlSize::Medium => 20,
            ControlSize::Large => 24,
        };
        self.computed = Bounds::from_xywh(origin.x.0, origin.y.0, self.width.0, h);
        self.computed
    }

    fn layout_hints(&self) -> LayoutHints {
        LayoutHints::default()
    }

    fn prepaint(&mut self, _bounds: Bounds, cx: &mut WindowInput) {
        self.hitbox_id = Some(cx.insert_hitbox_ex(
            self.computed,
            CursorStyle::PointingHand,
            !self.disabled,
            false,
        ));
    }

    fn paint(
        &mut self,
        bounds: Bounds,
        scene: &mut Scene,
        _input: &WindowInput,
        _env: &mut PaintEnv,
    ) {
        let bounds = if bounds.is_empty() {
            self.computed
        } else {
            bounds
        };
        let track_h = 6;
        let track = Bounds::from_xywh(
            bounds.x(),
            bounds.y() + (bounds.height() - track_h) / 2,
            bounds.width(),
            track_h,
        );
        scene.push_quad(Quad::new(track, self.theme.colors.muted).rounded(self.theme.radius.full));
        let t = ((self.clamp(self.value) - self.min) / self.span()).clamp(0.0, 1.0);
        let fill_w = ((bounds.width() as f32) * t) as i32;
        if fill_w > 0 {
            scene.push_quad(
                Quad::new(
                    Bounds::from_xywh(track.x(), track.y(), fill_w, track.height()),
                    self.theme.colors.primary,
                )
                .rounded(self.theme.radius.full),
            );
        }
        let thumb = 12;
        let tx = bounds.x() + fill_w - thumb / 2;
        let ty = bounds.y() + (bounds.height() - thumb) / 2;
        scene.push_quad(
            Quad::new(
                Bounds::from_xywh(tx, ty, thumb, thumb),
                self.theme.colors.primary_foreground,
            )
            .rounded(self.theme.radius.full),
        );
        scene.push_quad(
            Quad::new(
                Bounds::from_xywh(tx + 2, ty + 2, thumb - 4, thumb - 4),
                self.theme.colors.primary,
            )
            .rounded(self.theme.radius.full),
        );
    }

    fn mouse_down(&mut self, event: &MouseEvent, cx: &mut WindowInput) {
        if self.disabled || !self.is_armed(cx) {
            return;
        }
        self.emit(self.value_from_x(event.position.x.0, self.computed));
    }

    fn mouse_move(&mut self, event: &MouseEvent, cx: &mut WindowInput) {
        if self.disabled || !cx.is_mouse_down() || self.hitbox_id != cx.mouse_down_hitbox() {
            return;
        }
        self.emit(self.value_from_x(event.position.x.0, self.computed));
    }

    fn key_down(&mut self, event: &KeyDownEvent, cx: &mut WindowInput) -> bool {
        if self.disabled || self.hitbox_id != cx.focused() {
            return false;
        }
        let step = if self.step > 0.0 {
            self.step
        } else {
            self.span() / 20.0
        };
        match event.key {
            Key::Left | Key::Down => {
                self.emit(self.value - step);
                true
            }
            Key::Right | Key::Up => {
                self.emit(self.value + step);
                true
            }
            Key::Home => {
                self.emit(self.min);
                true
            }
            Key::End => {
                self.emit(self.max);
                true
            }
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Slider;
    use alloc::sync::Arc;
    use core::sync::atomic::{AtomicU32, Ordering};
    use knox_gpui::{
        Key, KeyDownEvent, Modifiers, Point, Size, WindowInput, dispatch_key_down,
        dispatch_mouse_down, prepare_element,
    };

    #[test]
    fn pointer_and_keyboard_commit() {
        let n = Arc::new(AtomicU32::new(0));
        let c = n.clone();
        let mut el = Slider::new().value(0.0).on_change(move |v| {
            c.store(v as u32, Ordering::SeqCst);
        });
        let mut input = WindowInput::new();
        prepare_element(&mut el, Point::ZERO, Size::wh(160, 20), &mut input);
        dispatch_mouse_down(&mut el, &mut input, Point::xy(80, 10));
        assert!(n.load(Ordering::SeqCst) >= 40);
        let id = input.hitboxes()[0].id;
        input.focus(id);
        dispatch_key_down(
            &mut el,
            &mut input,
            &KeyDownEvent::new(Key::Right, Modifiers::NONE),
        );
        assert!(n.load(Ordering::SeqCst) > 40);
    }
}
