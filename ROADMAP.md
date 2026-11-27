# ManageBIM roadmap

What is planned next. Done items move to the [CHANGELOG](CHANGELOG.md).

## Add-ins (Revit, Navisworks, AutoCAD / Plant 3D / Civil 3D)
- **Test every add-in** on the other supported versions (only 2025 is installed here; all versions already build): checklist per command - export, Live Link, SwitchBack, coordination objects, properties.
- **Sign the add-ins** (code-signing certificate) so Revit no longer asks to allow them after every update; the Revit self-test then runs unattended.
- **Improvements from testing**: clearer messages, faster exports on large models, fewer clicks.

## Code care
- **Split the remaining large files**: the `viewport()` function itself (~1700 lines) into tools, `app.rs` by area.
- **Group the rest of the `App` state** (render settings, sections, viewpoints), as done for clash, materials, MCP, levels and path tracing.
- **Lower the clippy baseline** (130 warnings today) step by step.

## Stability
- **A longer `cargo-fuzz` run** (coverage-guided); the robustness test covers IFC, DXF, OBJ, PLY, FBX, STL, .bimc, LAS, E57 and glTF.
- **Fewer `unwrap`** outside tests (49 today, mostly safe length-checked conversions).
- **Software renderer without OpenGL 3.3** (VMs, old Remote Desktop); 0.28.8 already shows a clear message.

## Rendering
- **Ray Trace view**: more samples per frame on fast graphics cards; more than one bounce of indirect light.
