// SPDX-License-Identifier: MIT
//! Boot splash raster: Knox UI Splash card → framebuffer BGRA blit.

use knox_gpui::{
    IntoElement, PaintEnv, Point, RasterBuffer, Scene, Size, SpriteAtlas, WindowInput,
    paint_element_with_input, px,
};
use knox_ui::{SPLASH_CARD_H, SPLASH_CARD_W, Splash, Theme, splash_status};

use crate::gui::framebuffer::{FrameBuffer, Rect};

use super::text_system::KnoxosTextSystem;

pub fn card_rect(fb_w: i32, fb_h: i32) -> Rect {
    Rect::new(
        (fb_w - SPLASH_CARD_W) / 2,
        (fb_h - SPLASH_CARD_H) / 2,
        SPLASH_CARD_W as u32,
        SPLASH_CARD_H as u32,
    )
}

pub fn paint_card(percent: u32) -> RasterBuffer {
    let theme = Theme::aurora_dark();
    let percent = percent.min(100);
    let mut el = Splash::new()
        .themed(&theme)
        .size(px(SPLASH_CARD_W), px(SPLASH_CARD_H))
        .progress(percent as f32)
        .status(splash_status(percent))
        .into_element();
    let mut atlas = SpriteAtlas::new();
    let sys = KnoxosTextSystem;
    let mut env = PaintEnv {
        atlas: &mut atlas,
        text: Some(&sys),
    };
    let mut scene = Scene::new();
    let mut input = WindowInput::new();
    paint_element_with_input(
        &mut el,
        Point::ZERO,
        Size::wh(SPLASH_CARD_W, SPLASH_CARD_H),
        &mut scene,
        &mut input,
        &mut env,
    );
    let mut buf = RasterBuffer::new(SPLASH_CARD_W as usize, SPLASH_CARD_H as usize);
    buf.draw_with_atlas(&scene, Some(&atlas));
    buf
}

pub fn blit_card(fb: &mut FrameBuffer, percent: u32) {
    let buf = paint_card(percent);
    let dest = card_rect(fb.width as i32, fb.height as i32);
    fb.blit_bgra(dest.x, dest.y, dest.width, dest.height, buf.as_bytes());
}
