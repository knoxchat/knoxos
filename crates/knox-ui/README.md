# knox-ui

`no_std` + `alloc` component catalog for the KnoxOS desktop (buttons, lists, overlays, …). Built on `knox-gpui`.

Zed GPUI and Longbridge GPUI Kit are **design references** (Apache-2.0). They are not dependencies and are not in this repo. Credit and license notes: [root README](../../README.md#acknowledgments).

Do not add `std`, upstream GPUI, or crates.io kit packages.

D15.2 C ABI (Ring 3 SHM, no `std`): `knox_ui_raster_button`, `knox_ui_raster_calculator`.
