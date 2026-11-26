# ManageBIM roadmap

What is planned next. Done items move to the [CHANGELOG](CHANGELOG.md).

## Add-ins (Revit, Navisworks, AutoCAD / Plant 3D / Civil 3D)
- **Levels from ManageBIM in Plant 3D and Navisworks** (they have no real levels): send the levels defined in ManageBIM (from IFC, DWG or created in the app) to Plant 3D - level lines / named UCS and a "Level" property on the objects by elevation - and to Navisworks - grids & levels, plus search sets and a "Level" property per item - so models can be filtered, sectioned and clash-grouped by level there too.
- **Test every add-in** in all supported versions (Revit, Navisworks, AutoCAD 2022 →), with a checklist per command: export, Live Link, SwitchBack, coordination objects, properties.
- **Automated add-in tests** where possible (headless export on sample files, compared with the expected result), so a new version does not break them.
- **Improvements from testing**: clearer messages, faster exports on large models, fewer clicks.
- **Stability**: no add-in error may close Revit / Navisworks / AutoCAD; errors are logged and shown in the ManageBIM window.

## Code care
- **Split the largest files**: `ui.rs` (~6500 lines) by ribbon tab and window, `app.rs` (~3300 lines) by area.
- **Group the `App` state by subsystem** (`ClashState`, `MaterialsState`, `McpState` …) so it is easier to follow and test.
- **Lower the clippy baseline** (242 warnings today) step by step.
- **One language for code comments** (English), so others can contribute.

## Stability
- **Fuzz the file readers** (IFC, DXF, E57, LAS, glTF) with `cargo-fuzz`: a damaged file must never close the app.
- **Fewer `unwrap` / `expect`** outside tests (~150 / ~37 today): clear error messages in the readers and exporters instead.
- **Memory check before large files**: warn and offer a lower density when a point cloud or IFC will not fit in RAM.
- **Endurance test**: open and close models many times to catch memory leaks.
- **Fallback without OpenGL 3.3** (VMs, old Remote Desktop): a software renderer or a clear message.
