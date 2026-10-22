# knox-gpui

`no_std` + `alloc` UI core for the KnoxOS desktop (entity, element, scene, keymap).

Zed GPUI and Longbridge GPUI Kit are **design references** (Apache-2.0). They are not dependencies and are not in this repo. Credit and license notes: [root README](../../README.md#acknowledgments).

Do not add `std`, upstream GPUI, or crates.io kit packages.

D15: `raster_scene_to_shm` fills a caller-owned BGRA buffer (fake SHM / Ring 3). The C ABI for controls lives in `knox-ui` (`knox_ui_raster_button`, `knox_ui_raster_calculator`). Pick **C ABI** until a musl Rust target exists.

Host tests: `cargo test -p knox-gpui`. D0.3: 32×32 opaque quad matches packed BGRA. D1.1: known HSLA → BGRA. D1.2: two quads iterate in paint order. D1.3: 8×8 Scene rasters to exact BGRA bytes. D1.4: 16×16 rounded quad has AA corner coverage (not a hard stair) and a locked FNV-1a golden. D1.5: box-shadow alpha falloff outside the quad. D1.6: layer clip discards a child quad outside the clip. D2.1: entity notify marks dirty. D2.2: App opens one window handle. D2.3: delayed job runs on the next fake-clock pump. D2.4: `Window::draw` fills the backing store. D2.6: `set_origin` does not repaint. D3.1: a `div` paints into a Scene. D3.2: `px_3` + `bg` set padding and fill. D3.3: two children layout in a row with `gap_2`. D3.4: pointer-in-bounds selects hover bg. D3.5: click (down+up on the same hitbox) increments a counter. D4.1: row, column, and grow-split. D4.2: absolute overlay is not in flex flow. D4.3: overflow clip of a tall child; scroll offset. D5.1: `'A'` rasterizes non-empty. D5.2: `text` paints mono sprites. D5.3: grayscale coverage is not a 1-bit stencil. D5.4: tiny atlas evicts. D6.1: mouse events carry modifiers; scroll dispatches. D6.2: Tab moves between two buttons and paints a `ring` token. D6.3: `cmd-k` fires only in the matching keymap context. D6.4: focused `text_input` receives IME commits. D6.5: input hitbox reports ibeam.
