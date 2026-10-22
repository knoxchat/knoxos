# Inter

KnoxOS UI face. Six weights (Light through ExtraBold), Latin subset.
SIL Open Font License 1.1 — see `OFL.txt`.

Upstream: https://rsms.me/inter/ (same subset files as `os/kernel/knox-kernel/ui/fonts/`)

The kernel embeds these files (`include_bytes!`) and rasterizes them with
`fontdue`, matching the `os` Cognition Mesh text path. They are also
installed at boot to `/usr/share/fonts/truetype/inter/`.
This directory is the source of truth for UI type.
