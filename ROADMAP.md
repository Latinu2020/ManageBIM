# ManageBIM roadmap

What is planned next. Done items move to the [CHANGELOG](CHANGELOG.md).

## Add-ins (Revit, Navisworks, AutoCAD / Plant 3D / Civil 3D)
- **Test every add-in** on the other supported versions (only 2025 is installed here; all versions already build): checklist per command - export, Live Link, SwitchBack, coordination objects, properties.
- **More automated add-in tests**: `tools\test-addins.ps1` covers AutoCAD (BIMCSEND, MBLEVELS) and the Navisworks export; next: Navisworks Levels / Walk / Issues, reference loading, and Revit through a journal file.
- **Improvements from testing**: clearer messages, faster exports on large models, fewer clicks.
- **Show add-in errors in ManageBIM** too (today they go to addin.log and a message in the host program).

## Code care
- **Split the remaining large files**: `ui/viewport.rs` (~2200 lines) by tool, `app.rs` (~2500 lines) by area.
- **Group the rest of the `App` state by subsystem** (`ClashState`, `MaterialsState`, `McpState` …), as done for the levels (`LevelState`).
- **Lower the clippy baseline** (130 warnings today) step by step.
- **English comments** in the older code (the new code already is), so others can contribute.

## Stability
- **Fuzz more readers**: 0.28.8 tests damaged IFC, DXF, OBJ, PLY, FBX, STL and .bimc files; still to add E57, LAS / LAZ, glTF and a longer `cargo-fuzz` run.
- **Fewer `unwrap` / `expect`** outside tests (~150 / ~37 today): clear error messages in the readers and exporters instead.
- **Lower density for large point clouds**: 0.28.8 warns before files that do not fit in the free memory; next, offer to load a point cloud at a lower density.
- **Endurance test**: open and close models many times to catch memory leaks.
- **Software renderer without OpenGL 3.3** (VMs, old Remote Desktop); 0.28.8 already shows a clear message.

## Rendering
- **Path tracing on the graphics card**: the photorealistic render runs on the processor today (all cores); on the GPU (compute shaders) it would be many times faster, with the same materials, lights and denoiser.
