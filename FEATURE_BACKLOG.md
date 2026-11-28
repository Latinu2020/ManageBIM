# Feature status — verified 2026-11-28

The old stage-by-stage tables were outdated: almost everything in them was shipped between 0.27 and
0.29. Details and dates are in [CHANGELOG.md](CHANGELOG.md). This file now keeps only what is **still
open**; there is no need to re-read thousands of lines of old notes.

## Still open

| Item | Why | Status |
|---|---|---|
| **C1 — Instancing + occlusion culling in the viewport** | Repeated elements (columns, bolts) are still drawn one by one; large models such as the 500k-triangle HVAC sample need culling on AABB / hierarchy and an FPS counter is already in the status bar. | open |
| **C2 — Progressive IFC import (progress + cancel)** | Loading already runs in the background, but there is no progress bar or cancel button; a big model looks frozen. | open |
| **Clippy baseline 33 → 0** | Only `chunks_exact` → `as_chunks` suggestions from Rust 1.88 remain; the toolchain here is 1.100, so they can be fixed and the baseline lowered. | in progress |
| **Scheduled headless checks through MCP** | Run IDS / clash / model checks on a newly delivered model from CI or a scheduler, using the MCP server that already exists. | idea |
| **BCF live sync (server, not just file)** | BCF export/import exists; synchronising issues and comments with a BCF server does not. | idea |
| **Property write-back to the host applications** | Voids and levels already go back to Revit/Navisworks; writing arbitrary user properties back (by GUID) does not. | idea |

## Already shipped (kept here only as an index)

- **Foundation / quick wins**: OSNAP and real units, split-view comparison, custom keybindings, batch
  export (PDF / HTML / Excel / BCF), shareable `.mbprofile`.
- **BIM**: space checks (Headroom / Egress), OpenPBR parameters + MaterialX XML subset, IBL/HDRI and
  GGX with up to four bounces, bSDD + IDS + IFC + MVD, the local MCP server (with `render_settings`,
  `get_render_status`, `capture_view(live: true)`), rotation circles as light paths.
- **Interface**: full ribbon labels, Phosphor vector icons, EN/RO interface, no overlapping level
  labels or duplicate ribbon commands, remembered floating windows, consistent panels.
- **Coordination**: voids approved in ManageBIM are cut into host walls in Revit; coordination objects
  (clearance / void / placeholder) have 3D face handles; multi-view drawing sheets at 1:1 in model
  space; geometric diff with deviation heatmap between model revisions.
- **Workflow**: in-app update button that downloads and starts the installer, undo/redo for properties
  and model changes, add-ins built for Revit 2023–2026 / AutoCAD 2021–2025+ / Navisworks 2022–2026.
- **Stability**: automated add-in test, damaged-file robustness run, endurance test, app-local C
  runtime + Mesa fallback, app-local Open Image Denoise.

## Deliberately not planned

Tauri rewrite, Eevee, MaterialX C++ FFI, code signing (out of scope for this round).
