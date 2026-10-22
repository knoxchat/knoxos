// SPDX-License-Identifier: MIT
//! Shared pointer / keyboard activation for catalog controls.

use alloc::sync::Arc;

use knox_gpui::{Div, Key};

pub(crate) type Handler = Arc<dyn Fn() + Send + Sync>;
pub(crate) type BoolHandler = Arc<dyn Fn(bool) + Send + Sync>;
pub(crate) type IndexHandler = Arc<dyn Fn(usize) + Send + Sync>;
pub(crate) type StringHandler = Arc<dyn Fn(&str) + Send + Sync>;
pub(crate) type CharHandler = Arc<dyn Fn(char) + Send + Sync>;

pub(crate) fn bind_activate(el: Div, handler: Handler) -> Div {
    let click = handler.clone();
    let key = handler;
    el.on_click(move |_| click()).on_key_down(move |e| {
        if matches!(e.key, Key::Space | Key::Enter) {
            key();
            true
        } else {
            false
        }
    })
}
