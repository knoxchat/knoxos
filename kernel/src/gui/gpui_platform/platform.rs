// SPDX-License-Identifier: MIT
//! KnoxOS GPUI platform: one `App` on the UI task.

use knox_gpui::App;
use spin::Mutex;

use super::atlas::SpriteAtlas;

/// Process-wide App. UI task only; `App` is `Send` (no `Rc`).
pub static APP: Mutex<Option<App>> = Mutex::new(None);

pub struct KnoxosPlatform {
    pub atlas: SpriteAtlas,
}

impl KnoxosPlatform {
    pub fn new() -> Self {
        Self {
            atlas: SpriteAtlas::new(),
        }
    }
}

pub fn boot_app() {
    let mut slot = APP.lock();
    if slot.is_none() {
        let mut app = App::new();
        app.keymap_mut()
            .bind("cmd-k", "command_palette", "Window")
            .bind("?", "shortcuts", "Window")
            .bind("escape", "dismiss", "Overlay");
        knox_ui::Theme::install(&mut app, super::proof::theme_from_kernel());
        *slot = Some(app);
    }
}
