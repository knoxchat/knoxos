// SPDX-License-Identifier: MIT
//! Command palette + no_std fuzzy scorer (GPUI kit catalog, reimplemented).
//!
//! the Longbridge kit is a design reference only (Apache-2.0). This file is new
//! `no_std` code. Not GPL Zed command_palette.

use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;

use knox_gpui::{Div, IntoElement, Pixels, Styled, div, px};

use crate::input::{FieldState, Input};
use crate::interact::IndexHandler;
use crate::overlay::{Dialog, FocusTrap};
use crate::select::{ListNav, SearchableList, pick_with_keys};
use crate::theme::Theme;

/// Sequential-match score. Higher is better; `None` is not a match.
pub fn fuzzy_score(query: &str, candidate: &str) -> Option<i32> {
    if query.is_empty() {
        return Some(0);
    }
    let q: Vec<char> = query.chars().flat_map(char::to_lowercase).collect();
    let c: Vec<char> = candidate.chars().flat_map(char::to_lowercase).collect();
    if q.len() > c.len() {
        return None;
    }
    let mut qi = 0;
    let mut score = 0i32;
    let mut prev_match = false;
    for (i, ch) in c.iter().enumerate() {
        if qi < q.len() && *ch == q[qi] {
            score += 8;
            if i == 0 || matches!(c.get(i.wrapping_sub(1)), Some(&'-' | &' ' | &'_' | &'/')) {
                score += 12;
            }
            if prev_match {
                score += 6;
            }
            qi += 1;
            prev_match = true;
        } else {
            prev_match = false;
        }
    }
    if qi == q.len() {
        Some(score - (c.len() as i32 - q.len() as i32))
    } else {
        None
    }
}

/// Rank `items` by [`fuzzy_score`], dropping misses.
pub fn fuzzy_filter(query: &str, items: &[String]) -> Vec<usize> {
    fuzzy_filter_with(query, items, &[])
}

/// Rank `items`, scoring each row as `max(name, extra)` when `extra` is aligned.
pub fn fuzzy_filter_with(query: &str, items: &[String], extra: &[String]) -> Vec<usize> {
    let mut ranked: Vec<(i32, usize)> = items
        .iter()
        .enumerate()
        .filter_map(|(i, s)| {
            let name = fuzzy_score(query, s);
            let more = extra.get(i).and_then(|e| fuzzy_score(query, e));
            name.into_iter().chain(more).max().map(|sc| (sc, i))
        })
        .collect();
    ranked.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
    ranked.into_iter().map(|(_, i)| i).collect()
}

/// Command palette (kit `Command`).
pub struct Command {
    theme: Theme,
    open: bool,
    inline: bool,
    query: FieldState,
    items: Vec<String>,
    keywords: Vec<String>,
    highlight: usize,
    width: Pixels,
    placeholder: String,
    on_query: Option<Arc<dyn Fn(FieldState) + Send + Sync>>,
    on_select: Option<IndexHandler>,
    on_highlight: Option<IndexHandler>,
    on_dismiss: Option<Arc<dyn Fn() + Send + Sync>>,
}

impl Command {
    pub fn new() -> Self {
        Self {
            theme: Theme::default(),
            open: false,
            inline: false,
            query: FieldState::empty(),
            items: Vec::new(),
            keywords: Vec::new(),
            highlight: 0,
            width: px(360),
            placeholder: String::from("Type a command…"),
            on_query: None,
            on_select: None,
            on_highlight: None,
            on_dismiss: None,
        }
    }

    pub fn themed(mut self, theme: &Theme) -> Self {
        self.theme = *theme;
        self
    }

    pub fn open(mut self, open: bool) -> Self {
        self.open = open;
        self
    }

    /// Skip the dialog dim; used by the session launcher (D9.2).
    pub fn inline(mut self) -> Self {
        self.inline = true;
        self
    }

    pub fn w(mut self, width: Pixels) -> Self {
        self.width = width;
        self
    }

    pub fn placeholder(mut self, placeholder: impl Into<String>) -> Self {
        self.placeholder = placeholder.into();
        self
    }

    pub fn query(mut self, query: FieldState) -> Self {
        self.query = query;
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

    pub fn keywords<I, S>(mut self, keywords: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.keywords = keywords.into_iter().map(Into::into).collect();
        self
    }

    pub fn highlight(mut self, highlight: usize) -> Self {
        self.highlight = highlight;
        self
    }

    pub fn on_query(mut self, f: impl Fn(FieldState) + Send + Sync + 'static) -> Self {
        self.on_query = Some(Arc::new(f));
        self
    }

    pub fn on_select(mut self, f: impl Fn(usize) + Send + Sync + 'static) -> Self {
        self.on_select = Some(Arc::new(f));
        self
    }

    pub fn on_highlight(mut self, f: impl Fn(usize) + Send + Sync + 'static) -> Self {
        self.on_highlight = Some(Arc::new(f));
        self
    }

    pub fn on_dismiss(mut self, f: impl Fn() + Send + Sync + 'static) -> Self {
        self.on_dismiss = Some(Arc::new(f));
        self
    }
}

impl Default for Command {
    fn default() -> Self {
        Self::new()
    }
}

impl IntoElement for Command {
    type Element = Div;

    fn into_element(self) -> Div {
        if !self.open {
            return div();
        }
        let q = self.query.value();
        let filtered = fuzzy_filter_with(&q, &self.items, &self.keywords);
        let labels: Vec<String> = filtered
            .iter()
            .filter_map(|i| self.items.get(*i).cloned())
            .collect();
        let map = filtered;
        let mut list = SearchableList::new("cmd-list")
            .themed(&self.theme)
            .items(labels)
            .query(FieldState::empty())
            .query_bar(false)
            .w(self.width)
            .highlight(self.highlight)
            .on_select({
                let pick = self.on_select.clone();
                let map = map.clone();
                move |i| {
                    if let Some(orig) = map.get(i).copied() {
                        if let Some(p) = &pick {
                            p(orig);
                        }
                    }
                }
            });
        if let Some(hi) = self.on_highlight.clone() {
            list = list.on_highlight(move |i| hi(i));
        }
        let mut field = Input::new("cmd-q")
            .themed(&self.theme)
            .placeholder(self.placeholder)
            .w(self.width)
            .state(self.query);
        if let Some(on_q) = self.on_query.clone() {
            field = field.on_change(move |s| on_q(s));
        }
        let pick = self.on_select.clone();
        let hi = self.on_highlight.clone();
        let dismiss = self.on_dismiss.clone();
        let n = map.len();
        let highlight = self.highlight.min(n.saturating_sub(1));
        field = field.on_key({
            let map = map.clone();
            move |e| {
                if let Some(nav) = pick_with_keys(e, &map, highlight) {
                    match nav {
                        ListNav::Highlight(h) => {
                            if let Some(cb) = &hi {
                                cb(h);
                            }
                        }
                        ListNav::Select(orig) => {
                            if let Some(p) = &pick {
                                p(orig);
                            }
                        }
                        ListNav::Close => {
                            if let Some(d) = &dismiss {
                                d();
                            }
                        }
                    }
                    return true;
                }
                false
            }
        });
        let body = div()
            .flex_col()
            .gap_2()
            .w(self.width)
            .child(field)
            .child(list);
        if self.inline {
            return body;
        }
        let dialog = Dialog::new()
            .themed(&self.theme)
            .title("Command")
            .open(true)
            .w(px(400))
            .child(body)
            .on_dismiss({
                let d = self.on_dismiss.clone();
                move || {
                    if let Some(h) = &d {
                        h();
                    }
                }
            });
        div().child(FocusTrap::new(dialog.into_element()).on_escape({
            let d = self.on_dismiss;
            move || {
                if let Some(h) = &d {
                    h();
                }
            }
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::{Command, fuzzy_filter, fuzzy_score};
    use crate::input::FieldState;
    use crate::theme::Theme;
    use alloc::string::String;
    use knox_gpui::IntoElement;

    #[test]
    fn type_to_filter_ranks_matches() {
        let items = [
            String::from("Open Terminal"),
            String::from("Open Files"),
            String::from("Settings"),
        ];
        let ranked = fuzzy_filter("term", &items);
        assert_eq!(ranked.first().copied(), Some(0));
        assert!(fuzzy_score("zzz", "Settings").is_none());
        let _ = Command::new()
            .themed(&Theme::aurora_dark())
            .open(true)
            .query(FieldState::from("te"))
            .items(items)
            .into_element();
        let shell = super::fuzzy_filter_with(
            "shell",
            &[String::from("Terminal")],
            &[String::from("shell bash cli")],
        );
        assert_eq!(shell.first().copied(), Some(0));
        let _ = Command::new()
            .themed(&Theme::aurora_dark())
            .inline()
            .open(true)
            .placeholder("Search apps…")
            .items(["Terminal", "Files"])
            .into_element();
    }
}
