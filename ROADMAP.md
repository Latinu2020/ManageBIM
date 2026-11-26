# ManageBIM roadmap

What is planned next. Done items move to the [CHANGELOG](CHANGELOG.md).

## Add-ins (Revit, Navisworks, AutoCAD / Plant 3D / Civil 3D)
- **Test every add-in** on the other supported versions (only 2025 is installed here; all versions already build): checklist per command - export, Live Link, SwitchBack, coordination objects, properties.
- **Revit automated test** through a journal file (AutoCAD and Navisworks are covered by `tools\test-addins.ps1`: 8 checks).
- **Improvements from testing**: clearer messages, faster exports on large models, fewer clicks.

## Code care
- **Split the remaining large files**: `ui/viewport.rs` (~2200 lines) by tool, `app.rs` (~2500 lines) by area.
- **Group the rest of the `App` state by subsystem** (`ClashState`, `MaterialsState`, `McpState` …), as done for `LevelState`, `RtState` and `PathTraceState`.
- **Lower the clippy baseline** (130 warnings today) step by step.
- **English comments** in the older code (the new code already is), so others can contribute.

## Stability
- **Fuzz E57** (no writer to make test files yet) and a longer `cargo-fuzz` run; IFC, DXF, OBJ, PLY, FBX, STL, .bimc, LAS and glTF are covered.
- **Fewer `unwrap`** outside tests (49 today, mostly safe length-checked conversions).
- **Software renderer without OpenGL 3.3** (VMs, old Remote Desktop); 0.28.8 already shows a clear message.

## Rendering
- **Ray Trace view: denoise while moving** (a quick GPU denoiser) and more samples per frame on fast graphics cards; global illumination with more than one bounce.
