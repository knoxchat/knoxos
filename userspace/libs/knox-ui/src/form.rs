// SPDX-License-Identifier: MIT
//! Form and Field (GPUI kit catalog, reimplemented).
//!
//! the Longbridge kit is a design reference only (Apache-2.0). This file is new
//! `no_std` code.

use alloc::string::String;
use alloc::vec::Vec;

use knox_gpui::{Div, IntoElement, Styled, div, px, text};

use crate::theme::Theme;

/// One labelled control with optional error text.
pub struct Field {
    theme: Theme,
    label: String,
    description: String,
    error: String,
    required: bool,
    child: Option<Div>,
}

impl Field {
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            theme: Theme::default(),
            label: label.into(),
            description: String::new(),
            error: String::new(),
            required: false,
            child: None,
        }
    }

    pub fn themed(mut self, theme: &Theme) -> Self {
        self.theme = *theme;
        self
    }

    pub fn description(mut self, description: impl Into<String>) -> Self {
        self.description = description.into();
        self
    }

    pub fn error(mut self, error: impl Into<String>) -> Self {
        self.error = error.into();
        self
    }

    pub fn required(mut self, required: bool) -> Self {
        self.required = required;
        self
    }

    pub fn child(mut self, child: impl IntoElement) -> Self {
        self.child = Some(div().child(child));
        self
    }
}

impl IntoElement for Field {
    type Element = Div;

    fn into_element(self) -> Div {
        let mut title = self.label.clone();
        if self.required && !title.is_empty() {
            title.push_str(" *");
        }
        let mut col = div().flex_col().gap_1();
        if !title.is_empty() {
            col = col.child(text(title).color(self.theme.colors.foreground).size(12));
        }
        if !self.description.is_empty() {
            col = col.child(
                text(self.description)
                    .color(self.theme.colors.muted_foreground)
                    .size(12),
            );
        }
        if let Some(child) = self.child {
            col = col.child(child);
        }
        if !self.error.is_empty() {
            col = col.child(
                text(self.error)
                    .color(self.theme.colors.destructive)
                    .size(12),
            );
        }
        col
    }
}

/// Vertical stack of fields (kit `Form`).
pub struct Form {
    theme: Theme,
    children: Vec<Div>,
}

impl Form {
    pub fn new() -> Self {
        Self {
            theme: Theme::default(),
            children: Vec::new(),
        }
    }

    pub fn themed(mut self, theme: &Theme) -> Self {
        self.theme = *theme;
        self
    }

    pub fn child(mut self, field: Field) -> Self {
        self.children.push(field.themed(&self.theme).into_element());
        self
    }
}

impl Default for Form {
    fn default() -> Self {
        Self::new()
    }
}

impl IntoElement for Form {
    type Element = Div;

    fn into_element(self) -> Div {
        let mut col = div().flex_col().gap(px(10));
        for child in self.children {
            col = col.child(child);
        }
        col
    }
}

#[cfg(test)]
mod tests {
    use super::{Field, Form};
    use crate::input::Input;
    use crate::theme::Theme;
    use knox_gpui::IntoElement;

    #[test]
    fn form_wraps_field() {
        let theme = Theme::aurora_dark();
        let _ = Form::new()
            .themed(&theme)
            .child(
                Field::new("Name")
                    .themed(&theme)
                    .required(true)
                    .error("Required")
                    .child(Input::new("n").themed(&theme)),
            )
            .into_element();
        let _ = Input::new("x");
    }
}
