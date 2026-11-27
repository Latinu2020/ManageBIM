# ManageBIM roadmap

What is planned next. Done items move to the [CHANGELOG](CHANGELOG.md).

## Status - 0.29.1 (published)
Done in 0.29.1 (details in the [CHANGELOG](CHANGELOG.md)):
- [x] Ray Trace: more than one bounce of indirect light (1 - 4, slider in the Path Trace window), CPU and GPU
- [x] Ray Trace: up to 64 samples per frame on fast graphics cards
- [x] Fewer `unwrap` outside tests (47 -> 24) and fixes for the risky ones
- [x] `viewport()` split (1,770 -> about 700 lines); `App` state grouped (4D, point cloud, updates, bSDD, viewpoints, sections)
- [x] Clippy 130 -> 33 warnings (CI baseline 33)
- [x] Longer robustness run (6,559 damaged files) - found and fixed a crash on damaged LAS files
- [x] MCP tool `render_settings`; `.gitattributes` for the same line endings on every computer; README rewritten
- [x] Checked: 257 tests pass on Linux; clippy on Windows 0 errors; local installer built on Windows

Still to check by hand after the release:
- [ ] `tools\test-addins.ps1` with Revit and a functional test of the installed 0.29.1 (Ray Trace with the Light bounces slider, sections, measure, 4D, a LAS point cloud)
- [ ] Optional: copy `packaging\ci\build.yml.proposed` over `.github\workflows\build.yml` so the release page shows the changelog and a download table

## Add-ins (Revit, Navisworks, AutoCAD / Plant 3D / Civil 3D)
- **Test every add-in** on the other supported versions (only 2025 is installed here; all versions already build): checklist per command - export, Live Link, SwitchBack, coordination objects, properties.
- **Sign the add-ins** (code-signing certificate) so Revit no longer asks to allow them after every update; the Revit self-test then runs unattended.
- **Improvements from testing**: clearer messages, faster exports on large models, fewer clicks.

## Code care
- **Split further**: the light gizmo and walk-path tools out of `viewport()` (about 700 lines left after 0.29.1), `app.rs` by area.
- **Lower the clippy baseline** (33 after 0.29.1): `chunks_exact` -> `as_chunks` (Rust 1.88+), once every build target has that compiler.

## Stability
- **Coverage-guided fuzzing** (`cargo-fuzz`, needs the nightly compiler); 0.29.1 ran the robustness test with 16x more damaged variants of every format.
