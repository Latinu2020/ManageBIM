# Feature status — verified 2026-11-28

The old stage-by-stage tables were outdated: almost everything in them was shipped between 0.27 and
0.29. Details and dates are in [CHANGELOG.md](CHANGELOG.md). This file now keeps only what is **still
open**; there is no need to re-read thousands of lines of old notes.

## Still open

| Item | Why | Status |
|---|---|---|
| **C1 — Instancing + occlusion culling in the viewport** | Repeated elements (columns, bolts) are still drawn one by one; the renderer already does frustum culling + adaptive LOD, and `get_performance` now reports drawn / total triangles and cull reasons so a change can be measured. | open |
| **C2 — Progressive IFC import (progress + cancel)** | Loading already runs in the background, but there is no progress bar or cancel button; a big model looks frozen. | open |
| **Property write-back to the host applications** | Voids and levels already go back to Revit/Navisworks; writing arbitrary user properties back (by GUID) does not. | idea |
| **BCF live sync (server, not just file)** | BCF export/import exists; synchronising issues and comments with a BCF server does not. | not planned for 0.29.5 |

## Already shipped (kept here only as an index)

- **Foundation / quick wins**: OSNAP and real units, split-view comparison, custom keybindings, batch
  export (PDF / HTML / Excel / BCF), shareable `.mbprofile`.
- **BIM**: space checks (Headroom / Egress), OpenPBR parameters + MaterialX XML subset, IBL/HDRI and
  GGX with up to four bounces, bSDD + IDS + IFC + MVD, the local MCP server (with `render_settings`,
  `get_render_status`, `capture_view(live: true)`, `get_performance`, `run_check`, `get_log` and the
  5D tools), rotation circles as light paths.
- **Checks / 5D**: quick quality checks and IDS rules in the GUI and in headless `--check` runs;
  Excel / CSV price catalog, priced take-off and budget vs. actual.
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
