# ManageBIM roadmap

What is planned next. Done items move to the [CHANGELOG](CHANGELOG.md).

## Features
- **Smarter search sets** (much easier than in Navisworks):
  - build a set by picking **property values** from a list (category, level, system, type, any Pset property), with live element counts as you choose;
  - **suggestions** from the selection ("these 12 elements share System = Supply Air, Level = Level 2 - make a set?") and ready-made sets by discipline / system / level;
  - plain-language search ("ducts on level 2 larger than 400 mm") turned into rules you can still edit;
  - sets that **update themselves** when the model changes, with a preview of what was added / removed;
  - **one click to add a set to a clash test's exclusions** (or to Selection A / B), from the set list, the Find panel or a clash result ("ignore everything like this").

## Interface
- **One Undo for every model change**: properties, visibility and materials, not only the view.

## Add-ins (Revit, Navisworks, AutoCAD / Plant 3D / Civil 3D)
- **Walk paths to Navisworks**: a walk path drawn in ManageBIM becomes a Navisworks viewpoint animation (one key viewpoint per path point, timing from distance and walk speed, eye height kept), created by the ManageBIM add-in in Saved Viewpoints and played with Play; fallback without the add-in: a viewpoints XML with one folder of viewpoints per path.
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
