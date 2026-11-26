# ManageBIM roadmap

What is planned next. Done items move to the [CHANGELOG](CHANGELOG.md).

## Add-ins (Revit, Navisworks, AutoCAD / Plant 3D / Civil 3D)
- **Test every add-in** in all supported versions (Revit, Navisworks, AutoCAD 2022 →), with a checklist per command: export, Live Link, SwitchBack, coordination objects, properties.
- **Automated add-in tests** where possible (headless export on sample files, compared with the expected result), so a new version does not break them.
- **Improvements from testing**: clearer messages, faster exports on large models, fewer clicks.
- **Stability**: no add-in error may close Revit / Navisworks / AutoCAD; errors are logged and shown in the ManageBIM window.

## Code care
- **Split the largest files**: `ui.rs` (~6500 lines) by ribbon tab and window, `app.rs` (~3300 lines) by area.
- **Group the `App` state by subsystem** (`ClashState`, `MaterialsState`, `McpState` …) so it is easier to follow and test.
- **Lower the clippy baseline** (226 warnings today) step by step.
- **One language for code comments** (English), so others can contribute.

## Stability
- **Fuzz more readers**: 0.28.8 tests damaged IFC, DXF, OBJ, PLY, FBX, STL and .bimc files; still to add E57, LAS / LAZ, glTF and a longer `cargo-fuzz` run.
- **Fewer `unwrap` / `expect`** outside tests (~150 / ~37 today): clear error messages in the readers and exporters instead.
- **Lower density for large point clouds**: 0.28.8 warns before files that do not fit in the free memory; next, offer to load a point cloud at a lower density.
- **Endurance test**: open and close models many times to catch memory leaks.
- **Software renderer without OpenGL 3.3** (VMs, old Remote Desktop); 0.28.8 already shows a clear message.

## Rendering
- **Path tracing on the graphics card**: the photorealistic render runs on the processor today (all cores); on the GPU (compute shaders) it would be many times faster, with the same materials, lights and denoiser.
