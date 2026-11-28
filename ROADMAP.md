# ManageBIM roadmap

What remains. Done items are in [CHANGELOG.md](CHANGELOG.md); a full feature index is in
[FEATURE_BACKLOG.md](FEATURE_BACKLOG.md).

## Current build — 0.29.5 (local, not published)

- [x] Headless `ManageBIM --check` (quick checks + IDS, JSON report, CI exit codes), tested on the demo IFC with 2 IDS rules.
- [x] 5D cost: Excel / CSV price catalog, priced take-off, budget vs. variance, Excel / CSV export, saved in the project.
- [x] MCP test tools: `get_performance`, `set_performance`, `run_check`, `get_log`, `load_price_catalog`, `get_cost`, `export_cost`.
- [x] Performance counters: drawn vs. total triangles / elements, cull reasons, draw calls, FPS (the renderer already did frustum culling + adaptive LOD).
- [x] Notifications on several lines and a close confirmation dialog.
- [x] Clippy baseline 0 (`as_chunks`, Rust 1.88), `quality.sh` guard fixed, FreeBSD memory test fixed.
- [x] `cargo clippy --all-targets`: 0 warnings / 0 errors; `cargo test --release --lib`: 123 passed, 0 failed, 2 ignored.
- [ ] Verify the 0.29.5 build through MCP on a fresh instance, then review the installer and publish **only on request**.
- [ ] Full `cargo test --release` on CI (the full test link takes >30 min locally).

## Focus now — stability of the existing application

- [ ] Run the fuzz targets (`tools\fuzz.ps1`, needs nightly + cargo-fuzz) for HDR / BIMC / LAS.
- [ ] Repeat the damaged-file robustness run (6,559 variants in 0.29.1) and the 40-reload endurance test.
- [ ] Add-in matrix on every installed Autodesk version. Result here: AutoCAD 2025 4/4 PASS,
      Revit 2025 PASS, Navisworks 2025 blocked by COM Automation (`CO_E_SERVER_EXEC_FAILURE`) —
      environment issue, not a code regression.
- [ ] Manual pass on the installed build: Ray Trace with the Light bounces slider, sections, measure,
      4D, a LAS point cloud, 5D cost and the close confirmation.

## Features still open (after stability)

- **C1** instancing / occlusion culling beyond the current frustum culling + adaptive LOD (large refactor:
      geometry is flattened and drawn with `draw_arrays`). The drawn / culled counters are in place to
      measure any change.
- **C2** progressive IFC import with progress bar and cancel.
- Scheduled headless checks through an external scheduler (the `--check` CLI is ready); property
  write-back to host applications.

## Not planned

Tauri rewrite, Eevee, MaterialX C++ FFI, BCF live sync, code signing (excluded for this round).
