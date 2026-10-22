// SPDX-License-Identifier: MIT
//! Input, Textarea, InputGroup (GPUI kit catalog, reimplemented).
//!
//! the Longbridge kit is a design reference only (Apache-2.0). This file is new
//! `no_std` code: a [`GapBuffer`] of Unicode scalars (not Ropey) plus caret,
//! selection, IME, and clipboard callbacks. The host (kernel
//! `gui/clipboard.rs`) owns the OS buffer.

use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::ops::Range;

use knox_gpui::{
    Bounds, CursorStyle, Div, Edges, Element, HitboxId, IntoElement, Key, KeyDownEvent,
    LayoutHints, MouseEvent, PaintEnv, Pixels, Point, Quad, Scene, Size, Styled, WindowInput, div,
    px, ring_color, text,
};

use crate::size::{ControlSize, Disableable, Sizable};
use crate::theme::Theme;

const MASKED: char = '\u{2022}';
const MIN_GAP: usize = 8;

type FieldHandler = Arc<dyn Fn(FieldState) + Send + Sync>;
type CopyHandler = Arc<dyn Fn(&str) + Send + Sync>;
type PasteSource = Arc<dyn Fn() -> Option<String> + Send + Sync>;
type KeyHandler = Arc<dyn Fn(&KeyDownEvent) -> bool + Send + Sync>;

/// Gap buffer of Unicode scalars. Inserts and deletes at the caret are O(gap).
#[derive(Clone, Debug)]
pub struct GapBuffer {
    data: Vec<char>,
    gap_start: usize,
    gap_end: usize,
}

impl GapBuffer {
    pub const fn new() -> Self {
        Self {
            data: Vec::new(),
            gap_start: 0,
            gap_end: 0,
        }
    }

    pub fn from_str(s: &str) -> Self {
        let mut buf = Self::new();
        buf.insert_str(s);
        buf
    }

    pub fn len(&self) -> usize {
        self.data.len() - self.gap_len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    fn gap_len(&self) -> usize {
        self.gap_end - self.gap_start
    }

    pub fn caret(&self) -> usize {
        self.gap_start
    }

    pub fn char_at(&self, index: usize) -> Option<char> {
        if index >= self.len() {
            return None;
        }
        if index < self.gap_start {
            Some(self.data[index])
        } else {
            Some(self.data[index + self.gap_len()])
        }
    }

    pub fn chars(&self) -> impl Iterator<Item = char> + '_ {
        self.data[..self.gap_start]
            .iter()
            .copied()
            .chain(self.data[self.gap_end..].iter().copied())
    }

    pub fn to_string(&self) -> String {
        self.chars().collect()
    }

    fn ensure_gap(&mut self, need: usize) {
        let gap = self.gap_len();
        if gap >= need {
            return;
        }
        let extra = (need - gap).max(MIN_GAP).max(self.data.len() / 2 + 1);
        let suffix: Vec<char> = self.data[self.gap_end..].to_vec();
        self.data.truncate(self.gap_start);
        self.data.resize(self.gap_start + extra, '\0');
        self.gap_end = self.data.len();
        self.data.extend(suffix);
    }

    pub fn move_gap(&mut self, index: usize) {
        let index = index.min(self.len());
        if index < self.gap_start {
            let n = self.gap_start - index;
            for i in 0..n {
                self.data[self.gap_end - 1 - i] = self.data[self.gap_start - 1 - i];
            }
            self.gap_start = index;
            self.gap_end -= n;
        } else if index > self.gap_start {
            let n = index - self.gap_start;
            for i in 0..n {
                self.data[self.gap_start + i] = self.data[self.gap_end + i];
            }
            self.gap_start += n;
            self.gap_end += n;
        }
    }

    pub fn insert_str(&mut self, s: &str) {
        let n = s.chars().count();
        if n == 0 {
            return;
        }
        self.ensure_gap(n);
        for c in s.chars() {
            self.data[self.gap_start] = c;
            self.gap_start += 1;
        }
    }

    pub fn insert_char(&mut self, c: char) {
        self.ensure_gap(1);
        self.data[self.gap_start] = c;
        self.gap_start += 1;
    }

    pub fn delete_before(&mut self, n: usize) {
        let n = n.min(self.gap_start);
        self.gap_start -= n;
    }

    pub fn delete_after(&mut self, n: usize) {
        let avail = self.data.len() - self.gap_end;
        let n = n.min(avail);
        self.gap_end += n;
    }
}

impl Default for GapBuffer {
    fn default() -> Self {
        Self::new()
    }
}

impl PartialEq for GapBuffer {
    fn eq(&self, other: &Self) -> bool {
        self.len() == other.len() && self.chars().eq(other.chars())
    }
}

impl Eq for GapBuffer {}

/// Result of a key press on a field.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FieldKey {
    Ignored,
    Edited,
    Copy(String),
    Cut(String),
    Paste,
}

/// Persistent editor state (parent-owned; elements are immediate).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FieldState {
    buf: GapBuffer,
    sel_anchor: Option<usize>,
    scroll_x: i32,
}

impl FieldState {
    pub const fn empty() -> Self {
        Self {
            buf: GapBuffer::new(),
            sel_anchor: None,
            scroll_x: 0,
        }
    }

    pub fn from(value: impl AsRef<str>) -> Self {
        Self {
            buf: GapBuffer::from_str(value.as_ref()),
            sel_anchor: None,
            scroll_x: 0,
        }
    }

    pub fn value(&self) -> String {
        self.buf.to_string()
    }

    pub fn len(&self) -> usize {
        self.buf.len()
    }

    pub fn is_empty(&self) -> bool {
        self.buf.is_empty()
    }

    pub fn caret(&self) -> usize {
        self.buf.caret()
    }

    pub fn scroll_x(&self) -> i32 {
        self.scroll_x
    }

    pub fn set_scroll_x(&mut self, x: i32) {
        self.scroll_x = x.max(0);
    }

    pub fn has_selection(&self) -> bool {
        match self.sel_anchor {
            Some(a) => a != self.caret(),
            None => false,
        }
    }

    pub fn selection(&self) -> Range<usize> {
        match self.sel_anchor {
            Some(a) if a != self.caret() => {
                let c = self.caret();
                if a < c { a..c } else { c..a }
            }
            _ => {
                let c = self.caret();
                c..c
            }
        }
    }

    pub fn selected_text(&self) -> String {
        let range = self.selection();
        self.buf
            .chars()
            .skip(range.start)
            .take(range.end - range.start)
            .collect()
    }

    pub fn select_all(&mut self) {
        if self.len() == 0 {
            self.sel_anchor = None;
            return;
        }
        self.sel_anchor = Some(0);
        self.buf.move_gap(self.len());
    }

    pub fn set_caret(&mut self, index: usize, select: bool) {
        let index = index.min(self.len());
        if select {
            if self.sel_anchor.is_none() {
                self.sel_anchor = Some(self.caret());
            }
        } else {
            self.sel_anchor = None;
        }
        self.buf.move_gap(index);
        if self.sel_anchor == Some(index) {
            self.sel_anchor = None;
        }
    }

    fn delete_selection(&mut self) -> bool {
        let range = self.selection();
        if range.start == range.end {
            return false;
        }
        self.buf.move_gap(range.start);
        self.buf.delete_after(range.end - range.start);
        self.sel_anchor = None;
        true
    }

    pub fn insert(&mut self, s: &str) {
        self.delete_selection();
        self.buf.insert_str(s);
        self.sel_anchor = None;
    }

    pub fn insert_char(&mut self, c: char) {
        self.delete_selection();
        self.buf.insert_char(c);
        self.sel_anchor = None;
    }

    pub fn backspace(&mut self) {
        if self.delete_selection() {
            return;
        }
        self.buf.delete_before(1);
    }

    pub fn delete(&mut self) {
        if self.delete_selection() {
            return;
        }
        self.buf.delete_after(1);
    }

    pub fn copy(&self) -> String {
        self.selected_text()
    }

    pub fn cut(&mut self) -> String {
        let text = self.selected_text();
        self.delete_selection();
        text
    }

    pub fn paste(&mut self, s: &str) {
        self.insert(s);
    }

    pub fn move_by(&mut self, delta: i32, select: bool) {
        if !select && self.has_selection() {
            let range = self.selection();
            self.sel_anchor = None;
            self.buf
                .move_gap(if delta < 0 { range.start } else { range.end });
            return;
        }
        if select && self.sel_anchor.is_none() {
            self.sel_anchor = Some(self.caret());
        }
        let next = (self.caret() as i32 + delta).clamp(0, self.len() as i32) as usize;
        self.set_caret(next, select);
    }

    fn line_bounds(&self, index: usize) -> Range<usize> {
        let chars: Vec<char> = self.buf.chars().collect();
        let index = index.min(chars.len());
        let start = chars[..index]
            .iter()
            .rposition(|&c| c == '\n')
            .map(|i| i + 1)
            .unwrap_or(0);
        let end = chars[index..]
            .iter()
            .position(|&c| c == '\n')
            .map(|i| index + i)
            .unwrap_or(chars.len());
        start..end
    }

    pub fn move_home(&mut self, select: bool, multiline: bool) {
        let dest = if multiline {
            self.line_bounds(self.caret()).start
        } else {
            0
        };
        self.set_caret(dest, select);
    }

    pub fn move_end(&mut self, select: bool, multiline: bool) {
        let dest = if multiline {
            self.line_bounds(self.caret()).end
        } else {
            self.len()
        };
        self.set_caret(dest, select);
    }

    pub fn move_vert(&mut self, delta_lines: i32, select: bool) {
        if delta_lines == 0 {
            return;
        }
        let chars: Vec<char> = self.buf.chars().collect();
        let caret = self.caret().min(chars.len());
        let line = self.line_bounds(caret);
        let col = caret - line.start;
        let mut cursor = if delta_lines < 0 {
            line.start
        } else {
            line.end
        };
        let mut remaining = delta_lines.abs();
        if delta_lines > 0 && cursor < chars.len() && chars.get(cursor) == Some(&'\n') {
            cursor += 1;
        }
        if delta_lines < 0 && cursor > 0 {
            cursor -= 1;
        }
        while remaining > 0 {
            if delta_lines < 0 {
                if cursor == 0 {
                    break;
                }
                let start = self.line_bounds(cursor).start;
                if start == 0 {
                    cursor = 0;
                    break;
                }
                cursor = start - 1;
                remaining -= 1;
            } else {
                if cursor >= chars.len() {
                    break;
                }
                let end = self.line_bounds(cursor).end;
                if end >= chars.len() {
                    cursor = chars.len();
                    break;
                }
                cursor = end + 1;
                remaining -= 1;
            }
        }
        let dest_line = self.line_bounds(cursor.min(chars.len()));
        let dest = (dest_line.start + col).min(dest_line.end);
        self.set_caret(dest, select);
    }

    /// Apply a key. Printable characters without chords also insert here so
    /// host tests can drive the field without going through IME.
    pub fn handle_key(&mut self, event: &KeyDownEvent, multiline: bool) -> FieldKey {
        let mods = event.keystroke.modifiers;
        let select = mods.shift;
        let chord = (mods.control || mods.platform) && !mods.alt;
        if chord {
            return match event.key {
                Key::Char('a') => {
                    self.select_all();
                    FieldKey::Edited
                }
                Key::Char('c') => FieldKey::Copy(self.copy()),
                Key::Char('x') => FieldKey::Cut(self.cut()),
                Key::Char('v') => FieldKey::Paste,
                _ => FieldKey::Ignored,
            };
        }
        match event.key {
            Key::Backspace => {
                self.backspace();
                FieldKey::Edited
            }
            Key::Delete => {
                self.delete();
                FieldKey::Edited
            }
            Key::Left => {
                self.move_by(-1, select);
                FieldKey::Edited
            }
            Key::Right => {
                self.move_by(1, select);
                FieldKey::Edited
            }
            Key::Up if multiline => {
                self.move_vert(-1, select);
                FieldKey::Edited
            }
            Key::Down if multiline => {
                self.move_vert(1, select);
                FieldKey::Edited
            }
            Key::Home => {
                self.move_home(select, multiline);
                FieldKey::Edited
            }
            Key::End => {
                self.move_end(select, multiline);
                FieldKey::Edited
            }
            Key::Enter if multiline => {
                self.insert("\n");
                FieldKey::Edited
            }
            Key::Space => {
                self.insert(" ");
                FieldKey::Edited
            }
            Key::Char(c) if !c.is_control() => {
                self.insert_char(c);
                FieldKey::Edited
            }
            _ => FieldKey::Ignored,
        }
    }
}

impl Default for FieldState {
    fn default() -> Self {
        Self::empty()
    }
}

fn em_width(size_px: u16) -> i32 {
    (size_px as i32 * 3 / 5).max(1)
}

fn line_height(size_px: u16) -> i32 {
    (size_px as i32 + 6).max(14)
}

fn display_value(state: &FieldState, password: bool) -> String {
    if password {
        core::iter::repeat(MASKED).take(state.len()).collect()
    } else {
        state.value()
    }
}

fn index_from_offset(text: &str, x: i32, y: i32, size_px: u16, multiline: bool) -> usize {
    let em = em_width(size_px);
    let lh = line_height(size_px);
    let chars: Vec<char> = text.chars().collect();
    if chars.is_empty() {
        return 0;
    }
    if !multiline {
        if x <= 0 {
            return 0;
        }
        let idx = ((x + em / 2) / em) as usize;
        return idx.min(chars.len());
    }
    let line = (y / lh).max(0) as usize;
    let mut start = 0usize;
    let mut line_i = 0usize;
    for (i, &c) in chars.iter().enumerate() {
        if line_i == line {
            break;
        }
        if c == '\n' {
            line_i += 1;
            start = i + 1;
        }
    }
    let last_line = chars.iter().filter(|&&c| c == '\n').count();
    if line > last_line {
        start = chars
            .iter()
            .rposition(|&c| c == '\n')
            .map(|i| i + 1)
            .unwrap_or(0);
    }
    let end = chars[start..]
        .iter()
        .position(|&c| c == '\n')
        .map(|i| start + i)
        .unwrap_or(chars.len());
    if x <= 0 {
        return start;
    }
    let col = ((x + em / 2) / em) as usize;
    (start + col).min(end)
}

fn caret_offset(text: &str, index: usize, size_px: u16, multiline: bool) -> (i32, i32) {
    let em = em_width(size_px);
    let lh = line_height(size_px);
    let index = index.min(text.chars().count());
    if !multiline {
        return ((index as i32) * em, 0);
    }
    let mut x = 0i32;
    let mut y = 0i32;
    for (i, c) in text.chars().enumerate() {
        if i == index {
            break;
        }
        if c == '\n' {
            x = 0;
            y += lh;
        } else {
            x += em;
        }
    }
    (x, y)
}

/// Single-line (or multiline) text field (kit `Input`).
pub struct Input {
    id: String,
    theme: Theme,
    state: FieldState,
    placeholder: String,
    prefix: String,
    suffix: String,
    disabled: bool,
    password: bool,
    multiline: bool,
    rows: u8,
    embedded: bool,
    size: ControlSize,
    width: Option<Pixels>,
    height: Option<Pixels>,
    grow: u32,
    on_change: Option<FieldHandler>,
    on_copy: Option<CopyHandler>,
    paste_from: Option<PasteSource>,
    on_key: Option<KeyHandler>,
    computed: Bounds,
    hitbox_id: Option<HitboxId>,
}

impl Input {
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            theme: Theme::default(),
            state: FieldState::empty(),
            placeholder: String::new(),
            prefix: String::new(),
            suffix: String::new(),
            disabled: false,
            password: false,
            multiline: false,
            rows: 1,
            embedded: false,
            size: ControlSize::Medium,
            width: None,
            height: None,
            grow: 0,
            on_change: None,
            on_copy: None,
            paste_from: None,
            on_key: None,
            computed: Bounds::ZERO,
            hitbox_id: None,
        }
    }

    pub fn themed(mut self, theme: &Theme) -> Self {
        self.theme = *theme;
        self
    }

    pub fn state(mut self, state: FieldState) -> Self {
        self.state = state;
        self
    }

    pub fn value(self, value: impl AsRef<str>) -> Self {
        self.state(FieldState::from(value))
    }

    pub fn placeholder(mut self, placeholder: impl Into<String>) -> Self {
        self.placeholder = placeholder.into();
        self
    }

    pub fn prefix(mut self, prefix: impl Into<String>) -> Self {
        self.prefix = prefix.into();
        self
    }

    pub fn suffix(mut self, suffix: impl Into<String>) -> Self {
        self.suffix = suffix.into();
        self
    }

    pub fn password(mut self, on: bool) -> Self {
        self.password = on;
        self
    }

    pub fn multiline(mut self, on: bool) -> Self {
        self.multiline = on;
        if on && self.rows < 2 {
            self.rows = 3;
        }
        self
    }

    pub fn rows(mut self, rows: u8) -> Self {
        self.rows = rows.max(1);
        self
    }

    pub fn embedded(mut self, on: bool) -> Self {
        self.embedded = on;
        self
    }

    pub fn w(mut self, width: Pixels) -> Self {
        self.width = Some(width);
        self
    }

    pub fn h(mut self, height: Pixels) -> Self {
        self.height = Some(height);
        self
    }

    pub fn grow(mut self) -> Self {
        self.grow = 1;
        self
    }

    pub fn on_change(mut self, f: impl Fn(FieldState) + Send + Sync + 'static) -> Self {
        self.on_change = Some(Arc::new(f));
        self
    }

    pub fn on_copy(mut self, f: impl Fn(&str) + Send + Sync + 'static) -> Self {
        self.on_copy = Some(Arc::new(f));
        self
    }

    pub fn paste_from(mut self, f: impl Fn() -> Option<String> + Send + Sync + 'static) -> Self {
        self.paste_from = Some(Arc::new(f));
        self
    }

    /// Extra key handler (list navigation). Runs before editing; `true` consumes.
    pub fn on_key(mut self, f: impl Fn(&KeyDownEvent) -> bool + Send + Sync + 'static) -> Self {
        self.on_key = Some(Arc::new(f));
        self
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn field_state(&self) -> &FieldState {
        &self.state
    }

    fn control_size(&self) -> ControlSize {
        self.size
    }

    fn width_hint(&self) -> Option<Pixels> {
        self.width
    }

    fn emit(&self) {
        if let Some(handler) = &self.on_change {
            handler(self.state.clone());
        }
    }

    fn emit_copy(&self, text: &str) {
        if text.is_empty() {
            return;
        }
        if let Some(handler) = &self.on_copy {
            handler(text);
        }
    }

    fn is_target(&self, cx: &WindowInput) -> bool {
        self.hitbox_id.is_some() && self.hitbox_id == cx.focused()
    }

    fn pad_x(&self) -> i32 {
        if self.embedded {
            self.size.pad_x(true)
        } else {
            self.size.pad_x(false)
        }
    }

    fn prefix_width(&self) -> i32 {
        if self.prefix.is_empty() {
            0
        } else {
            self.prefix.chars().count() as i32 * em_width(self.size.font()) + 6
        }
    }

    fn suffix_width(&self) -> i32 {
        if self.suffix.is_empty() {
            0
        } else {
            self.suffix.chars().count() as i32 * em_width(self.size.font()) + 6
        }
    }

    fn text_origin(&self, bounds: Bounds) -> Point {
        Point::xy(
            bounds.x() + self.pad_x() + self.prefix_width(),
            bounds.y()
                + (if self.multiline {
                    6
                } else {
                    (bounds.height() - line_height(self.size.font())).max(0) / 2
                }),
        )
    }

    fn inner_text_width(&self, bounds: Bounds) -> i32 {
        (bounds.width() - self.pad_x() * 2 - self.prefix_width() - self.suffix_width()).max(8)
    }

    fn shown(&self) -> String {
        display_value(&self.state, self.password)
    }

    fn apply_scroll(&mut self, bounds: Bounds) {
        if self.multiline {
            return;
        }
        let font = self.size.font();
        let shown = self.shown();
        let (cx, _) = caret_offset(&shown, self.state.caret(), font, false);
        let inner = self.inner_text_width(bounds);
        let mut scroll = self.state.scroll_x();
        if cx - scroll > inner - 4 {
            scroll = cx - inner + 4;
        }
        if cx - scroll < 0 {
            scroll = cx;
        }
        self.state.set_scroll_x(scroll.max(0));
    }

    fn index_at(&self, p: Point, bounds: Bounds) -> usize {
        let origin = self.text_origin(bounds);
        let shown = self.shown();
        let x = p.x.0 - origin.x.0
            + if self.multiline {
                0
            } else {
                self.state.scroll_x()
            };
        let y = p.y.0 - origin.y.0;
        index_from_offset(&shown, x, y, self.size.font(), self.multiline)
    }

    fn apply_key(&mut self, event: &KeyDownEvent) -> bool {
        if let Some(handler) = &self.on_key {
            if handler(event) {
                return true;
            }
        }
        match self.state.handle_key(event, self.multiline) {
            FieldKey::Ignored => false,
            FieldKey::Edited => {
                self.emit();
                true
            }
            FieldKey::Copy(text) => {
                self.emit_copy(&text);
                true
            }
            FieldKey::Cut(text) => {
                self.emit_copy(&text);
                self.emit();
                true
            }
            FieldKey::Paste => {
                if let Some(src) = &self.paste_from {
                    if let Some(text) = src() {
                        self.state.paste(&text);
                        self.emit();
                    }
                }
                true
            }
        }
    }
}

impl Sizable for Input {
    fn with_size(mut self, size: ControlSize) -> Self {
        self.size = size;
        self
    }
}

impl Disableable for Input {
    fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
}

impl Element for Input {
    fn request_layout(&mut self, origin: Point, available: Size) -> Bounds {
        let font = self.size.font();
        let default_h = if self.multiline {
            px(line_height(font) * self.rows as i32 + 12)
        } else {
            px(self.size.height())
        };
        let h = self.height.unwrap_or(default_h);
        let w = self.width.unwrap_or(if available.width.0 > 0 {
            available.width
        } else {
            px(180)
        });
        self.computed = Bounds::new(
            origin,
            Size {
                width: w,
                height: h,
            },
        );
        self.computed
    }

    fn layout_hints(&self) -> LayoutHints {
        LayoutHints {
            grow: self.grow,
            ..LayoutHints::default()
        }
    }

    fn prepaint(&mut self, _bounds: Bounds, cx: &mut WindowInput) {
        let cursor = if self.disabled {
            CursorStyle::Arrow
        } else {
            CursorStyle::IBeam
        };
        self.hitbox_id =
            Some(cx.insert_hitbox_ex(self.computed, cursor, !self.disabled, !self.disabled));
    }

    fn paint(
        &mut self,
        bounds: Bounds,
        scene: &mut Scene,
        input: &WindowInput,
        env: &mut PaintEnv,
    ) {
        let bounds = if bounds.is_empty() {
            self.computed
        } else {
            bounds
        };
        self.apply_scroll(bounds);
        let focused = input.focused() == self.hitbox_id && self.hitbox_id.is_some();
        let radius = self.theme.radius.md;
        if !self.embedded {
            if focused && !self.disabled {
                scene.push_quad(Quad::new(bounds.dilate(px(2)), ring_color()).rounded(radius));
            }
            let mut quad = Quad::new(bounds, self.theme.colors.input).rounded(radius);
            quad.border_widths = Edges::all(px(1));
            quad.border_color = if focused {
                self.theme.colors.ring
            } else {
                self.theme.colors.border
            };
            scene.push_quad(quad);
        }

        scene.start_layer(bounds);
        let font = self.size.font();
        let origin = self.text_origin(bounds);
        let muted = self.theme.colors.muted_foreground;
        let fg = if self.disabled {
            muted
        } else {
            self.theme.colors.foreground
        };

        if !self.prefix.is_empty() {
            let mut pre = text(self.prefix.clone()).color(muted).size(font);
            let po = Point::xy(bounds.x() + self.pad_x(), origin.y.0);
            let pb = pre.request_layout(po, Size::wh(self.prefix_width(), line_height(font)));
            pre.paint(pb, scene, input, env);
        }
        if !self.suffix.is_empty() {
            let mut suf = text(self.suffix.clone()).color(muted).size(font);
            let so = Point::xy(
                bounds.x() + bounds.width() - self.pad_x() - self.suffix_width(),
                origin.y.0,
            );
            let sb = suf.request_layout(so, Size::wh(self.suffix_width(), line_height(font)));
            suf.paint(sb, scene, input, env);
        }

        let shown = self.shown();
        let scroll = if self.multiline {
            0
        } else {
            self.state.scroll_x()
        };
        let text_origin = Point::xy(origin.x.0 - scroll, origin.y.0);
        let inner_w = self.inner_text_width(bounds);
        let inner_h = (bounds.height() - 8).max(line_height(font));

        if shown.is_empty() {
            if !self.placeholder.is_empty() {
                let mut ph = text(self.placeholder.clone()).color(muted).size(font);
                let pb = ph.request_layout(text_origin, Size::wh(inner_w, inner_h));
                ph.paint(pb, scene, input, env);
            }
        } else {
            if self.state.has_selection() {
                let range = self.state.selection();
                let em = em_width(font);
                let lh = line_height(font);
                let sel = self.theme.colors.selection;
                let chars: Vec<char> = shown.chars().collect();
                let mut x = 0i32;
                let mut y = 0i32;
                for (i, &c) in chars.iter().enumerate() {
                    if i >= range.start && i < range.end && c != '\n' {
                        scene.push_quad(Quad::new(
                            Bounds::from_xywh(text_origin.x.0 + x, text_origin.y.0 + y, em, lh),
                            sel,
                        ));
                    }
                    if c == '\n' {
                        x = 0;
                        y += lh;
                    } else {
                        x += em;
                    }
                }
            }
            let mut label = text(shown.clone()).color(fg).size(font);
            let lb = label.request_layout(
                text_origin,
                Size::wh(inner_w.max(shown.len() as i32 * em_width(font)), inner_h),
            );
            label.paint(lb, scene, input, env);
        }

        if focused && !self.disabled {
            let (cx, cy) = caret_offset(&shown, self.state.caret(), font, self.multiline);
            let caret = Bounds::from_xywh(
                text_origin.x.0 + cx,
                text_origin.y.0 + cy + 1,
                1,
                line_height(font) - 2,
            );
            scene.push_quad(Quad::new(caret, self.theme.colors.primary));
        }
        scene.end_layer();
    }

    fn mouse_down(&mut self, event: &MouseEvent, cx: &mut WindowInput) {
        if self.disabled || self.hitbox_id != cx.dispatch_target() {
            return;
        }
        let index = self.index_at(event.position, self.computed);
        self.state.set_caret(index, event.modifiers.shift);
        if self.state.has_selection() {
            self.emit_copy(&self.state.selected_text());
        }
        self.emit();
    }

    fn mouse_move(&mut self, event: &MouseEvent, cx: &mut WindowInput) {
        if self.disabled || !cx.is_mouse_down() || self.hitbox_id != cx.mouse_down_hitbox() {
            return;
        }
        let index = self.index_at(event.position, self.computed);
        self.state.set_caret(index, true);
        self.emit();
    }

    fn key_down(&mut self, event: &KeyDownEvent, cx: &mut WindowInput) -> bool {
        if self.disabled || !self.is_target(cx) {
            return false;
        }
        self.apply_key(event)
    }

    fn ime_commit(&mut self, text: &str, cx: &mut WindowInput) {
        if self.disabled || !self.is_target(cx) || text.is_empty() {
            return;
        }
        self.state.insert(text);
        self.emit();
    }
}

/// Multi-line field (kit `Textarea`).
pub struct Textarea {
    inner: Input,
}

impl Textarea {
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            inner: Input::new(id).multiline(true).rows(3),
        }
    }

    pub fn themed(mut self, theme: &Theme) -> Self {
        self.inner = self.inner.themed(theme);
        self
    }

    pub fn state(mut self, state: FieldState) -> Self {
        self.inner = self.inner.state(state);
        self
    }

    pub fn value(mut self, value: impl AsRef<str>) -> Self {
        self.inner = self.inner.value(value);
        self
    }

    pub fn placeholder(mut self, placeholder: impl Into<String>) -> Self {
        self.inner = self.inner.placeholder(placeholder);
        self
    }

    pub fn rows(mut self, rows: u8) -> Self {
        self.inner = self.inner.rows(rows);
        self
    }

    pub fn w(mut self, width: Pixels) -> Self {
        self.inner = self.inner.w(width);
        self
    }

    pub fn on_change(mut self, f: impl Fn(FieldState) + Send + Sync + 'static) -> Self {
        self.inner = self.inner.on_change(f);
        self
    }

    pub fn on_copy(mut self, f: impl Fn(&str) + Send + Sync + 'static) -> Self {
        self.inner = self.inner.on_copy(f);
        self
    }

    pub fn paste_from(mut self, f: impl Fn() -> Option<String> + Send + Sync + 'static) -> Self {
        self.inner = self.inner.paste_from(f);
        self
    }

    pub fn field_state(&self) -> &FieldState {
        self.inner.field_state()
    }

    pub fn into_input(self) -> Input {
        self.inner
    }
}

impl Sizable for Textarea {
    fn with_size(mut self, size: ControlSize) -> Self {
        self.inner = self.inner.with_size(size);
        self
    }
}

impl Disableable for Textarea {
    fn disabled(mut self, disabled: bool) -> Self {
        self.inner = self.inner.disabled(disabled);
        self
    }
}

impl Element for Textarea {
    fn request_layout(&mut self, origin: Point, available: Size) -> Bounds {
        self.inner.request_layout(origin, available)
    }
    fn layout_hints(&self) -> LayoutHints {
        self.inner.layout_hints()
    }
    fn prepaint(&mut self, bounds: Bounds, cx: &mut WindowInput) {
        self.inner.prepaint(bounds, cx);
    }
    fn paint(
        &mut self,
        bounds: Bounds,
        scene: &mut Scene,
        input: &WindowInput,
        env: &mut PaintEnv,
    ) {
        self.inner.paint(bounds, scene, input, env);
    }
    fn mouse_down(&mut self, event: &MouseEvent, cx: &mut WindowInput) {
        self.inner.mouse_down(event, cx);
    }
    fn mouse_move(&mut self, event: &MouseEvent, cx: &mut WindowInput) {
        self.inner.mouse_move(event, cx);
    }
    fn key_down(&mut self, event: &KeyDownEvent, cx: &mut WindowInput) -> bool {
        self.inner.key_down(event, cx)
    }
    fn ime_commit(&mut self, text: &str, cx: &mut WindowInput) {
        self.inner.ime_commit(text, cx);
    }
}

/// Prefix / input / suffix cluster (kit `InputGroup`).
pub struct InputGroup {
    theme: Theme,
    prefix: String,
    suffix: String,
    input: Input,
}

impl InputGroup {
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            theme: Theme::default(),
            prefix: String::new(),
            suffix: String::new(),
            input: Input::new(id),
        }
    }

    pub fn themed(mut self, theme: &Theme) -> Self {
        self.theme = *theme;
        self.input = self.input.themed(theme);
        self
    }

    pub fn prefix(mut self, prefix: impl Into<String>) -> Self {
        self.prefix = prefix.into();
        self
    }

    pub fn suffix(mut self, suffix: impl Into<String>) -> Self {
        self.suffix = suffix.into();
        self
    }

    pub fn field(mut self, input: Input) -> Self {
        self.input = input;
        self
    }

    pub fn state(mut self, state: FieldState) -> Self {
        self.input = self.input.state(state);
        self
    }

    pub fn on_change(mut self, f: impl Fn(FieldState) + Send + Sync + 'static) -> Self {
        self.input = self.input.on_change(f);
        self
    }

    pub fn on_copy(mut self, f: impl Fn(&str) + Send + Sync + 'static) -> Self {
        self.input = self.input.on_copy(f);
        self
    }

    pub fn paste_from(mut self, f: impl Fn() -> Option<String> + Send + Sync + 'static) -> Self {
        self.input = self.input.paste_from(f);
        self
    }
}

impl IntoElement for InputGroup {
    type Element = Div;

    fn into_element(self) -> Div {
        let radius = self.theme.radius.md;
        let mut row = div()
            .flex()
            .items_center()
            .h(px(self.input.control_size().height()))
            .bg(self.theme.colors.input)
            .border_1()
            .border_color(self.theme.colors.border)
            .rounded(radius)
            .overflow_hidden();
        if let Some(w) = self.input.width_hint() {
            row = row.w(w);
        }
        if !self.prefix.is_empty() {
            row = row.child(
                div().pad_x(px(8)).child(
                    text(self.prefix)
                        .color(self.theme.colors.muted_foreground)
                        .size(13),
                ),
            );
        }
        row = row.child(self.input.embedded(true).grow());
        if !self.suffix.is_empty() {
            row = row.child(
                div().pad_x(px(8)).child(
                    text(self.suffix)
                        .color(self.theme.colors.muted_foreground)
                        .size(13),
                ),
            );
        }
        row
    }
}

#[cfg(test)]
mod tests {
    use super::{FieldKey, FieldState, GapBuffer, Input, InputGroup, Textarea};
    use crate::theme::Theme;
    use alloc::string::String;
    use alloc::sync::Arc;
    use core::sync::atomic::{AtomicBool, Ordering};
    use knox_gpui::{
        CursorStyle, IntoElement, Key, KeyDownEvent, Modifiers, Point, Size, WindowInput,
        dispatch_ime_commit, dispatch_key_down, dispatch_mouse_down, dispatch_mouse_move,
        prepare_element,
    };

    #[test]
    fn gap_buffer_inserts_and_deletes_at_caret() {
        let mut buf = GapBuffer::from_str("knox");
        buf.move_gap(2);
        buf.insert_str("OS");
        assert_eq!(buf.to_string(), "knOSox");
        buf.delete_before(2);
        assert_eq!(buf.to_string(), "knox");
        buf.move_gap(0);
        buf.delete_after(1);
        assert_eq!(buf.to_string(), "nox");
    }

    #[test]
    fn field_select_copy_cut_paste() {
        let mut s = FieldState::from("knox-ui");
        s.select_all();
        assert_eq!(s.copy(), "knox-ui");
        assert_eq!(s.cut(), "knox-ui");
        assert!(s.is_empty());
        s.paste("gpui");
        assert_eq!(s.value(), "gpui");
        s.set_caret(0, false);
        s.set_caret(2, true);
        assert_eq!(s.selected_text(), "gp");
        s.insert("K");
        assert_eq!(s.value(), "Kui");
    }

    #[test]
    fn field_arrows_and_chords() {
        let mut s = FieldState::from("ab");
        s.set_caret(0, false);
        let edited = s.handle_key(
            &KeyDownEvent::new(Key::Right, Modifiers::new(false, false, true, false)),
            false,
        );
        assert_eq!(edited, FieldKey::Edited);
        assert_eq!(s.selected_text(), "a");
        assert_eq!(
            s.handle_key(
                &KeyDownEvent::new(Key::Char('c'), Modifiers::new(true, false, false, false)),
                false
            ),
            FieldKey::Copy(String::from("a"))
        );
        assert_eq!(
            s.handle_key(
                &KeyDownEvent::new(Key::Char('a'), Modifiers::new(true, false, false, false)),
                false
            ),
            FieldKey::Edited
        );
        assert_eq!(s.selected_text(), "ab");
    }

    #[test]
    fn textarea_enter_inserts_newline() {
        let mut s = FieldState::from("a");
        s.handle_key(&KeyDownEvent::new(Key::Enter, Modifiers::NONE), true);
        s.handle_key(&KeyDownEvent::new(Key::Char('b'), Modifiers::NONE), true);
        assert_eq!(s.value(), "a\nb");
        s.move_vert(-1, false);
        assert_eq!(s.caret(), 1);
    }

    fn focus_field(field: &mut Input, input: &mut WindowInput) {
        prepare_element(field, Point::ZERO, Size::wh(180, 32), input);
        let id = input.hitboxes()[0].id;
        input.focus(id);
    }

    #[test]
    fn input_types_selects_and_pastes() {
        let copied = Arc::new(AtomicBool::new(false));
        let flag = copied.clone();
        let mut field = Input::new("t")
            .themed(&Theme::aurora_dark())
            .state(FieldState::empty())
            .on_copy(move |_| flag.store(true, Ordering::SeqCst))
            .paste_from(|| Some(String::from("OS")));
        let mut input = WindowInput::new();
        focus_field(&mut field, &mut input);
        assert_eq!(input.cursor_at(Point::xy(8, 8)), CursorStyle::IBeam);
        dispatch_ime_commit(&mut field, &mut input, "knox");
        assert_eq!(field.field_state().value(), "knox");
        assert!(dispatch_key_down(
            &mut field,
            &mut input,
            &KeyDownEvent::new(Key::Char('a'), Modifiers::new(true, false, false, false))
        ));
        assert_eq!(field.field_state().selected_text(), "knox");
        assert!(dispatch_key_down(
            &mut field,
            &mut input,
            &KeyDownEvent::new(Key::Char('c'), Modifiers::new(true, false, false, false))
        ));
        assert!(copied.load(Ordering::SeqCst));
        assert!(dispatch_key_down(
            &mut field,
            &mut input,
            &KeyDownEvent::new(Key::Char('v'), Modifiers::new(true, false, false, false))
        ));
        assert_eq!(field.field_state().value(), "OS");
    }

    #[test]
    fn drag_selects_range() {
        let mut field = Input::new("t").state(FieldState::from("abcd"));
        let mut input = WindowInput::new();
        prepare_element(&mut field, Point::ZERO, Size::wh(180, 32), &mut input);
        dispatch_mouse_down(&mut field, &mut input, Point::xy(12, 16));
        dispatch_mouse_move(&mut field, &mut input, Point::xy(40, 16));
        assert!(field.field_state().has_selection());
    }

    #[test]
    fn textarea_and_group_construct() {
        let theme = Theme::aurora_dark();
        let area = Textarea::new("a").themed(&theme).placeholder("notes");
        assert_eq!(area.field_state().value(), "");
        let _ = InputGroup::new("g")
            .themed(&theme)
            .prefix("@")
            .suffix(".os")
            .field(Input::new("g-in").themed(&theme).value("docs"))
            .into_element();
    }
}
