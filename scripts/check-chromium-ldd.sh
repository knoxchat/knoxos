#!/bin/bash
# Host ldd/readelf stand-in for Chromium DT_NEEDED trees (`GATE_CHR ldd`).
# Fails if a `\x7fELF` + zeros stub would still count as a shared object.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
exec cargo test -p knoxos-host-tests chromium_ldd_rejects_stub_so -- --exact
