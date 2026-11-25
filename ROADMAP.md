# ManageBIM roadmap

What is planned next. Done items move to the [CHANGELOG](CHANGELOG.md).

## Features
- **AI connection, part 2**: `capture_view` (the assistant sees the 3D view as an image) and clash workflow tools (status, assignee, comment, BCF export).
- **Egress + headroom checks**: clear height under beams, ducts and slabs, plus escape route distances, with results as clashes / issues.

## Interface
- **Smaller bottom panel at start**: Find Items opens collapsed and remembers its last state.
- **Progress and Cancel for long loads** (large LAS / LAZ / E57, big IFC): percentage and a Cancel button.
- **Notification history**: a list of the recent messages (errors, exports) instead of toasts that disappear.
- **Shortcut sheet** (F1 or "?"), generated from the configurable key bindings.
- **Display check** at 125 % / 150 % Windows scaling, on 4K screens and in the Light theme (contrast, clipped text).
- **One Undo for every model change**: properties, visibility and materials, not only the view.

- **Application icon tuning**: a sharper icon at every size (16 – 256 px) for the window, taskbar, installer and file types.

## Add-ins (Revit, Navisworks, AutoCAD / Plant 3D / Civil 3D)
- **Test every add-in** in all supported versions (Revit, Navisworks, AutoCAD 2022 →), with a checklist per command: export, Live Link, SwitchBack, coordination objects, properties.
- **Automated add-in tests** where possible (headless export on sample files, compared with the expected result), so a new version does not break them.
- **Improvements from testing**: clearer messages, faster exports on large models, fewer clicks.
- **Stability**: no add-in error may close Revit / Navisworks / AutoCAD; errors are logged and shown in the ManageBIM window.

## Code care
- **Split the largest files**: `ui.rs` (~6500 lines) by ribbon tab and window, `app.rs` (~3300 lines) by area.
- **Group the `App` state by subsystem** (`ClashState`, `MaterialsState`, `McpState` …) so it is easier to follow and test.
- **`cargo fmt --check` and clippy in CI**, blocking only new warnings.
- **`cargo-deny` / `cargo-audit` in CI** for dependency vulnerabilities and licences, plus Dependabot.
- **One language for code comments** (English), so others can contribute.

## Stability
- **Fuzz the file readers** (IFC, DXF, E57, LAS, glTF) with `cargo-fuzz`: a damaged file must never close the app.
- **Fewer `unwrap` / `expect`** outside tests (~150 / ~37 today): clear error messages in the readers and exporters instead.
- **Memory check before large files**: warn and offer a lower density when a point cloud or IFC will not fit in RAM.
- **Endurance test**: open and close models many times to catch memory leaks.
- **Fallback without OpenGL 3.3** (VMs, old Remote Desktop): a software renderer or a clear message.
