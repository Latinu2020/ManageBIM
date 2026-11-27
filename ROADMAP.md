# ManageBIM roadmap

What is planned next. Done items move to the [CHANGELOG](CHANGELOG.md).

## 0.29.1 (next, fixes for 0.29.0)
- **Ray Trace view showed only a thin strip** of the image in 0.29.0 (the band-by-band sampling left OpenGL's clip rectangle on its last band) - *fixed, to be released*.
- **Ray Trace while orbiting**: the view filled only from the bottom up and flashed empty when the camera stopped; now a quarter-resolution image while moving and the previous image under the first full-resolution sample - *fixed, to be released*.
- **Brighter rows while refining** (rows of the sample in progress were divided by one sample too few) in the Ray Trace view and the Path Trace window preview - *fixed, to be released*.
- **The navigation hint overlapped the Ray Trace label** in the top-left corner of the 3D view - *fixed, to be released*.
- **CI "quality" job fails** (clippy / rustfmt check on GitHub) since rc2 - it does not block the release, but must pass again.
- **FreeBSD CI build failed** on rc3 - check the log.
- **A test opens a browser window** (`mb_smoke_*/out/model.html`, "file couldn't be accessed") while `cargo test` runs - tests must not open anything on screen.
- **Windows CI takes ~55 min for the tests** after a cache-invalidating change (crt-static): build the tests once and reuse them, or split the job.
- **Test the live Ray Trace view on screen** before a release, not only through captures (the strip bug passed the capture tests).

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

## Rendering
- **Ray Trace view**: more samples per frame on fast graphics cards; more than one bounce of indirect light.
