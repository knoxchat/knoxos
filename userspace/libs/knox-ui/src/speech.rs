// SPDX-License-Identifier: MIT
//! Speech widget: button, waveform, state machine on KnoxOS audio/mic.
//!
//! the Longbridge kit is a design reference only (Apache-2.0). This file is new
//! `no_std` code. The recognizer may return empty until dictation exists.

use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;

use knox_gpui::{
    Bounds, Div, Element, IntoElement, LayoutHints, PaintEnv, Pixels, Point, Quad, Scene, Size,
    Styled, WindowInput, div, px, text,
};

use crate::button::{Button, ButtonVariants};
use crate::empty::Empty;
use crate::interact::Handler;
use crate::theme::Theme;

/// Widget state. `NoRecognizer` is the honest empty path.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SpeechState {
    #[default]
    Idle,
    Recording,
    NoRecognizer,
}

/// Mic / dictation control (kit `Speech`).
pub struct Speech {
    theme: Theme,
    state: SpeechState,
    samples: Vec<i16>,
    on_start: Option<Handler>,
    on_stop: Option<Handler>,
}

impl Speech {
    pub fn new() -> Self {
        Self {
            theme: Theme::default(),
            state: SpeechState::Idle,
            samples: Vec::new(),
            on_start: None,
            on_stop: None,
        }
    }

    pub fn themed(mut self, theme: &Theme) -> Self {
        self.theme = *theme;
        self
    }

    pub fn state(mut self, state: SpeechState) -> Self {
        self.state = state;
        self
    }

    pub fn samples(mut self, samples: impl IntoIterator<Item = i16>) -> Self {
        self.samples = samples.into_iter().collect();
        self
    }

    pub fn on_start(mut self, f: impl Fn() + Send + Sync + 'static) -> Self {
        self.on_start = Some(Arc::new(f));
        self
    }

    pub fn on_stop(mut self, f: impl Fn() + Send + Sync + 'static) -> Self {
        self.on_stop = Some(Arc::new(f));
        self
    }
}

impl Default for Speech {
    fn default() -> Self {
        Self::new()
    }
}

/// Waveform bars from PCM samples.
pub struct Waveform {
    theme: Theme,
    samples: Vec<i16>,
    width: Pixels,
    height: Pixels,
    computed: Bounds,
}

impl Waveform {
    pub fn new(samples: Vec<i16>) -> Self {
        Self {
            theme: Theme::default(),
            samples,
            width: px(160),
            height: px(32),
            computed: Bounds::ZERO,
        }
    }

    pub fn themed(mut self, theme: &Theme) -> Self {
        self.theme = *theme;
        self
    }
}

impl Element for Waveform {
    fn request_layout(&mut self, origin: Point, _available: Size) -> Bounds {
        self.computed = Bounds::new(origin, Size::wh(self.width.0, self.height.0));
        self.computed
    }

    fn layout_hints(&self) -> LayoutHints {
        LayoutHints::default()
    }

    fn paint(
        &mut self,
        bounds: Bounds,
        scene: &mut Scene,
        _input: &WindowInput,
        _env: &mut PaintEnv,
    ) {
        scene.push_quad(Quad::new(bounds, self.theme.colors.muted.opacity(0.4)));
        let n = self.samples.len().max(1);
        let bars = bounds.width().min(n as i32).max(1);
        let step = n / bars as usize;
        let mid = bounds.y() + bounds.height() / 2;
        for i in 0..bars {
            let idx = (i as usize * step).min(self.samples.len().saturating_sub(1));
            let amp = self.samples.get(idx).copied().unwrap_or(0) as i32;
            let h = ((amp.abs() * bounds.height()) / 32768).max(1);
            scene.push_quad(Quad::new(
                Bounds::from_xywh(bounds.x() + i, mid - h / 2, 1, h),
                self.theme.colors.primary,
            ));
        }
    }
}

impl IntoElement for Speech {
    type Element = Div;

    fn into_element(self) -> Div {
        let mut col = div().flex_col().gap_2().w(px(220));
        match self.state {
            SpeechState::NoRecognizer => {
                return col.child(
                    Empty::new()
                        .themed(&self.theme)
                        .title("No recognizer")
                        .description("Dictation is not available yet."),
                );
            }
            SpeechState::Idle => {
                let mut btn = Button::new("mic").themed(&self.theme).label("Record");
                if let Some(s) = self.on_start {
                    btn = btn.on_click(move || s());
                }
                col = col.child(btn);
            }
            SpeechState::Recording => {
                let mut btn = Button::new("mic")
                    .themed(&self.theme)
                    .danger()
                    .label("Stop");
                if let Some(s) = self.on_stop {
                    btn = btn.on_click(move || s());
                }
                col = col
                    .child(btn)
                    .child(Waveform::new(self.samples).themed(&self.theme));
            }
        }
        col.child(
            text(String::from(match self.state {
                SpeechState::Idle => "Idle",
                SpeechState::Recording => "Recording",
                SpeechState::NoRecognizer => "No recognizer",
            }))
            .color(self.theme.colors.muted_foreground)
            .size(12),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::{Speech, SpeechState};
    use crate::theme::Theme;
    use knox_gpui::IntoElement;

    #[test]
    fn no_recognizer_is_honest_empty() {
        let theme = Theme::aurora_dark();
        let _ = Speech::new()
            .themed(&theme)
            .state(SpeechState::NoRecognizer)
            .into_element();
        let _ = Speech::new()
            .themed(&theme)
            .state(SpeechState::Recording)
            .samples([0, 8000, -4000, 12000])
            .into_element();
    }
}
