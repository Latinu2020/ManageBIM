# ManageBIM roadmap

What remains. Done items are in [CHANGELOG.md](CHANGELOG.md); a full feature index is in
[FEATURE_BACKLOG.md](FEATURE_BACKLOG.md).

## Current build — 0.29.2 (local, not published)

- [x] Interactive Ray Trace: correct divisor for the band in progress, scissor state restored, full
      cache invalidation (hidden elements, lamp kind, samples, denoise), no black rectangle at startup.
- [x] Damaged files: HDR over-allocation / RLE panic and LAZ over-allocation fixed.
- [x] OpenGL: incomplete FBOs are not cached; failed SSAO / shader allocations are freed.
- [x] Release: `build-release.ps1` passes `--features addins` and includes the Navisworks installer.
- [x] Verification: MCP `get_render_status` + `capture_view(live: true)`, `tools\test-raytrace.ps1`,
      `tools\verify-release.ps1`, `fuzz\` targets, CI guard against "0 tests ran".
- [x] Windows installer built and Ray Trace smoke test passed (256/256 samples, no GPU error).
- [x] Functional audit fixes: light placement face normal, OSNAP Surface toggle, viewpoint Update/Go to state and selection, MEP classification order, IFC angle units, AutoCAD install status, MCP `run_script` double run, path-trace capture no longer falls back silently.
- [ ] Final `cargo test --release` and clippy baseline (the full test link takes >30 min here).
- [ ] Review the 0.29.2 installer by the user, then publish **only on request**.

## Focus now — stability of the existing application

- [ ] Lower the clippy baseline from 33 to 0 (`as_chunks` is stable in Rust 1.100).
- [ ] Run the fuzz targets (`tools\fuzz.ps1`, needs nightly + cargo-fuzz) for HDR / BIMC / LAS.
- [ ] Repeat the damaged-file robustness run (6,559 variants in 0.29.1) and the 40-reload endurance test.
- [ ] Add-in matrix on every installed Autodesk version. Result here: AutoCAD 2025 4/4 PASS,
      Revit 2025 PASS, Navisworks 2025 blocked by COM Automation (`CO_E_SERVER_EXEC_FAILURE`) —
      environment issue, not a code regression.
- [ ] Manual pass on the installed build: Ray Trace with the Light bounces slider, sections, measure,
      4D, a LAS point cloud.

## Features still open (after stability)

- **C1** instancing + occlusion culling in the viewport.
- **C2** progressive IFC import with progress bar and cancel.
- Scheduled headless checks through MCP, BCF live sync, property write-back to host applications.

## Not planned

Tauri rewrite, Eevee, MaterialX C++ FFI, code signing (excluded for this round).
