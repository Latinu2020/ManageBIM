<div align="center">

<img src="packaging/linux/managebim.png" width="88" alt="ManageBIM logo">

# ManageBIM

### Federated BIM review, coordination and presentation — fast, light, free

Open **IFC, Revit, Navisworks, AutoCAD / Plant 3D, STEP, CityJSON, 3D Tiles, USD and point clouds** together, run clash detection and model checks,<br>
plan voids and clearance zones, produce **Inventor-style 2D drawings**, render with **scanned PBR materials** — and stay in sync with Revit and Navisworks.

[![Download for Windows](https://img.shields.io/badge/Download-Windows%20installer%200.28.4-2563eb?style=for-the-badge&logo=windows)](https://github.com/Latinu2020/ManageBIM/releases/download/v0.28.4/ManageBIM-Setup-0.28.4.exe)
&nbsp;
[![Linux & FreeBSD](https://img.shields.io/badge/Linux%20%C2%B7%20FreeBSD-Build%20from%20source-334155?style=for-the-badge&logo=linux&logoColor=white)](#building-from-source)

[![Build](https://github.com/Latinu2020/ManageBIM/actions/workflows/build.yml/badge.svg)](https://github.com/Latinu2020/ManageBIM/actions/workflows/build.yml)
![Version](https://img.shields.io/badge/version-0.28.4-blue)
![Platforms](https://img.shields.io/badge/platforms-Windows%20%7C%20Linux%20%7C%20FreeBSD-0078d4)
![Rust](https://img.shields.io/badge/built%20with-Rust-b7410e)
![No admin](https://img.shields.io/badge/install-no%20admin%20rights-16a34a)
![License](https://img.shields.io/badge/license-MIT-green)

<img src="docs/images/main-window.png" alt="ManageBIM main window" width="920">

**[Download](#download) · [In action](#see-it-in-action) · [What's new](#whats-new) · [Highlights](#highlights) · [Features](#features) · [Add-ins](#add-ins-windows) · [Build from source](#building-from-source)**

</div>

---

## Why ManageBIM

| | |
|---|---|
| ⚡ **Fast and light** | Native Rust + OpenGL. Opens large federated models in seconds; adaptive level of detail keeps navigation fluid. |
| 🔗 **Works with your tools** | Add-ins for **Revit**, **Navisworks** and **AutoCAD / Plant 3D**: send models, bring issues back, sync the selection live, load ManageBIM models as references. |
| 🧭 **Familiar** | Ribbon, Selection Tree, Find Items, Clash Detective, TimeLiner, viewpoints — the workflow you know from Navisworks. |
| 🧱 **Coordination, not just viewing** | Clearance zones, provisions for void (found automatically where services cross walls and slabs) and placeholder volumes take part in clash detection. |
| 📐 **Drawings and presentation** | Pick a face of the section box and get a 2D view with visible and hidden lines (PDF / DWG); photoreal renders with 28 scanned PBR materials, lights and sun. |
| 🌐 **Open standards** | IFC 2x3 / IFC4 in and out (MVD), **ifcJSON / ifcXML**, **BCF 2.1 / 3.0**, **buildingSMART IDS 1.0**, **bSDD**, **CityJSON**, **3D Tiles**, **USD**, **3MF**, DWG / DXF, E57 / LAS / LAZ, glTF (with structural metadata), MaterialX. |
| 🆓 **Free and open** | MIT licence, no account, no subscription, no administrator rights to install. |

---

## See it in action

<table>
<tr>
<td width="50%"><img src="docs/gifs/compare-heatmap.gif" alt="Compare with deviation heatmap"><br><sub><b>Geometric diff between revisions</b> — Compare measures how far every surface moved and paints a <b>deviation heatmap</b> per triangle (green in place → red at the scale): a building moved 600 mm, a roof raised 2.5 m, one removed, one added.</sub></td>
<td width="50%"><img src="docs/gifs/multiview.gif" alt="Multi-view drawing"><br><sub><b>Multi-view drawing</b> — all faces of the section box on one sheet like Inventor: choose the views and the spacing, see the live preview, export PDF or DWG at 1:1.</sub></td>
</tr>
<tr>
<td width="50%"><img src="docs/gifs/walk-path-scan.gif" alt="Walk path on a point cloud"><br><sub><b>Walk path on a scan</b> — click on the floor of a point cloud (snaps onto scan points), drag points, insert on the line; segment lengths, direction arrows and a live preview.</sub></td>
<td width="50%"><img src="docs/gifs/viewcube.gif" alt="ViewCube"><br><sub><b>ViewCube</b> — 26 directions with chamfered edges and corners, a compass ring, and arrows to the neighbouring faces when you look straight at one.</sub></td>
</tr>
</table>

---

## Highlights

<table>
<tr>
<td width="50%"><img src="docs/images/drawing-2d.png" alt="2D drawing preview"><br><sub><b>2D drawings like Inventor</b> — the view from a face of the section box: thick cut outline, visible edges, dashed hidden lines, levels and dimensions on a sheet with a title block. Export to PDF or DWG in real coordinates (attach it back as an overlay and it lands in place).</sub></td>
<td width="50%"><img src="docs/images/pick-face.png" alt="Pick a face of the section box"><br><sub><b>Pick face</b> — hover a face of the section box, it turns light green; click it and the drawing is made from that face (Top, Bottom, Front, Back, Left, Right).</sub></td>
</tr>
<tr>
<td width="50%"><img src="docs/images/coordination-objects.png" alt="Coordination objects"><br><sub><b>Coordination objects</b> — <b>Auto voids</b> proposes a provision for void wherever a duct, pipe or cable tray crosses a wall or slab; clearance zones and placeholder volumes complete the set. They appear in the tree, in Clash Detective and in the IFC export.</sub></td>
<td width="50%"><img src="docs/images/materials-pbr.png" alt="Scanned PBR materials"><br><sub><b>Scanned PBR materials included</b> — 28 ambientCG (CC0) materials with colour, normal, roughness, metallic, AO and height maps, applied in one click by IFC type. OpenPBR / MaterialX import and export.</sub></td>
</tr>
<tr>
<td width="50%"><img src="docs/images/lighting.png" alt="Lighting panel"><br><sub><b>Lighting like Navisworks</b> — Point, Spot and Distant lights lit in real time, a Move / Rotate gizmo, unlimited power and range, and Sun & sky (position by date, time and location, shadows, night mode) in one panel.</sub></td>
<td width="50%"><img src="docs/images/bsdd.png" alt="bSDD classification"><br><sub><b>bSDD classification</b> — search the buildingSMART Data Dictionary online (IFC, Uniclass, ETIM, NL-SfB…) or a local catalogue and assign codes to the selection; exported as <code>IfcClassificationReference</code> with the bSDD URI.</sub></td>
</tr>
<tr>
<td width="50%"><img src="docs/images/hidden-line.png" alt="Hidden line style"><br><sub><b>Hidden line style</b> — white technical-drawing look with every edge of the shapes in black and a light grey tone by face orientation.</sub></td>
<td width="50%"><img src="docs/images/installer.png" alt="Setup wizard"><br><sub><b>Setup wizard</b> — per user, no administrator rights, choose the folder with Browse, Revit / AutoCAD / Navisworks add-ins detected automatically.</sub></td>
</tr>
</table>

---

## Screenshots

<table>
<tr>
<td width="50%"><img src="docs/images/clash-detective.png" alt="Clash Detective"><br><sub><b>Clash Detective like Navisworks</b> — tests with New / Active / Reviewed / Approved / Resolved columns, Add Test, Reset / Compact / Delete All, <b>Update All</b>; the clash is shown in the model and the elements' properties next to it.</sub></td>
<td width="50%"><img src="docs/images/clash-update-all.png" alt="Update All summary"><br><sub><b>Update All</b> runs every test again and shows what changed: new, still active and resolved clashes per test.</sub></td>
</tr>
<tr>
<td width="50%"><img src="docs/images/clash-matrix.png" alt="Clash matrix by test"><br><sub><b>Clash matrix by test</b> — Selection A on the rows, Selection B on the columns, the tests and their statuses in each cell; Excel export with a sheet per status. Tests are created from a <b>sets matrix</b> and imported / exported as <b>Navisworks XML</b> or CSV.</sub></td>
<td width="50%"><img src="docs/images/animations-4d.png" alt="Animations window"><br><sub><b>Animations</b> in their own window — Record, Orbit 360°, viewpoints, walk path and <b>Import 4D</b>; ▶ Play on each one, then <b>export the video</b>, optionally rendered with materials.</sub></td>
</tr>
<tr>
<td width="50%"><img src="docs/images/light-theme.png" alt="Light theme"><br><sub><b>Five themes</b> — Studio, Dark, Light, Tokyo Night, Nord. Card ribbon in three sizes, vector icons, panels that slide up from the status bar.</sub></td>
<td width="50%"><img src="docs/images/concept-style.png" alt="Concept visual style"><br><sub><b>Visual styles</b> — Realistic (materials, textures), Shaded, Concept (white study model with edges) and Wireframe.</sub></td>
</tr>
<tr>
<td width="50%"><img src="docs/images/pdf-sheet.png" alt="PDF sheet"><br><sub><b>PDF sheets</b> from viewpoints — A3 to A0, with an editable title block. Section cuts go to <b>DWG</b> the same way.</sub></td>
<td width="50%"><img src="docs/images/web-viewer.png" alt="Web viewer"><br><sub><b>Web viewer</b> — the model in a single .html file: tree, search, properties, section, measure. Opens offline in any browser.</sub></td>
</tr>
<tr>
<td width="50%"><img src="docs/images/start-page.png" alt="Start page"><br><sub><b>Start page</b> with recent projects, a demo project and the integrated manual.</sub></td>
<td width="50%"><img src="docs/images/point-cloud.png" alt="Point cloud"><br><sub><b>Point clouds</b> (E57, LAS) next to the model, with the <b>as-built check</b>: built, partial, missing.</sub></td>
</tr>
<tr>
<td width="50%"><img src="docs/images/plan-2d-sheet.png" alt="2D plan on sheet"><br><sub><b>Complete 2D plans</b> from a section — hatched cut walls and columns, door / window and wall tags, grid bubbles and dimension chains, on a sheet with the title block (DWG).</sub></td>
<td width="50%"><img src="docs/images/romanian-ui.png" alt="Romanian interface"><br><sub><b>English / Română</b> — the interface language switches from View › Language, the File menu or Ctrl+K.</sub></td>
</tr>
</table>

---

## Download

| Platform | Package | Notes |
|---|---|---|
| **Windows 10 / 11** (x64) | [**ManageBIM-Setup-0.28.4.exe**](https://github.com/Latinu2020/ManageBIM/releases/download/v0.28.4/ManageBIM-Setup-0.28.4.exe) | Wizard installer with the Revit, Navisworks and AutoCAD / Plant 3D add-ins, the OpenCascade runtime, Intel Open Image Denoise and the scanned PBR material library. Per-user, **no administrator rights**, folder of your choice. |
| **Linux** (x86-64) | [**ManageBIM-0.28.4-x86_64.AppImage**](https://github.com/Latinu2020/ManageBIM/releases/download/v0.28.4/ManageBIM-0.28.4-x86_64.AppImage) | Single file: `chmod +x` and run. Or [build from source](#building-from-source). |
| **FreeBSD** (x86-64) | [**ManageBIM-0.28.4-freebsd-x86_64.tar.gz**](https://github.com/Latinu2020/ManageBIM/releases/download/v0.28.4/ManageBIM-0.28.4-freebsd-x86_64.tar.gz) | Unpack and run `./ManageBIM`. Needs `pkg install mesa-libs libxkbcommon`. |

## What's new

> **0.28.4** — **connect AI assistants** (Claude Code / Desktop, OpenCode, VS Code, local Ollama models through an MCP client) with the built-in **MCP server** (Share › AI connection): they can find elements, read properties, select, hide / isolate, set properties, run clash tests and scripts. **MaterialX** import / export now works with standard files (ambientCG, GPUOpen, Substance: node graphs, standard_surface / OpenPBR / UsdPreviewSurface) and saves to your library. Prebuilt **Linux AppImage** and **FreeBSD** packages are back.
>
> **0.28.3** — **new open formats with per-object properties**: 3MF, CityJSON / CityJSONSeq, ifcJSON and ifcXML (converted with the official IFC schemas), 3D Tiles 1.1 / 1.0, USD (.usda) and LAZ. Richer DXF (block attributes, XDATA), LAS (one item per classification), PLY (segments) and glTF (EXT_structural_metadata). **Geometric diff between revisions** with a per-triangle **deviation heatmap**.
>
> **0.28.2** — a **Property editor** for standard IFC property sets (typed in the IFC export) and custom data, with original values never lost; a new **ViewCube** with compass; viewpoints that keep their own look next to View Templates; a **Multi-view** window with chosen views, spacing and live preview; nicer **point clouds** (relief, real size, elevation colours, snapping); easier **walk path** editing; Plant 3D / Civil 3D object properties from the AutoCAD add-in; self-contained **.bimd**.

Older versions: see the full **[CHANGELOG](CHANGELOG.md)**. What comes next: **[ROADMAP](ROADMAP.md)**.

> **Windows:** the installer is not digitally signed yet — if SmartScreen asks, choose *More info › Run anyway*.
> Silent install: `ManageBIM-Setup-0.28.4.exe /S` (add `/NoAddins` for the application only).
>
> **Linux / FreeBSD:** the add-ins (Revit, Navisworks, AutoCAD) and the native STEP / IGES reader exist only on Windows; everything else is the same.

New here? Open **Help › Demo** for a sample project (architecture, structure, MEP and a 4D schedule), press **Ctrl+K** for the command palette, or read **Help › Manual**.

---

## Features

<details open>
<summary><b>Model review</b></summary>

- **Federated models** — IFC 2x3 / IFC4 (`.ifczip`, **ifcXML**, **ifcJSON** too) with openings cut, systems, classifications and georeference, Revit and Navisworks (through the add-ins), AutoCAD / Plant 3D DWG with Plant 3D properties, DXF (with block attributes and XDATA), STEP / IGES / BREP / FCStd, FBX, glTF / GLB (with EXT_structural_metadata), OBJ, STL, PLY (with segments and properties).
- **Open city and web formats** — **CityJSON / CityJSONSeq** (attributes, LoD, semantic surfaces), **3D Tiles 1.1 / 1.0** (tilesets, b3dm batch tables, shown upright from ECEF), **USD** text (.usda: prims, transforms, customData), **3MF** (objects, materials, metadata).
- **Point clouds** — E57, LAS and **LAZ**; one item per scan, LAS split **by classification** (Ground, Building, Vegetation…) with intensity / returns / GPS time; round points with relief, real point size, colour by **elevation**; Measure snaps onto scan points.
- **Selection Tree** (group by any property, category colours, counts), **Property Tree**, **Find Items** with condition chips, selection and search **sets**, **Select Same Type / Category**.
- **Navigation** like Navisworks — orbit, pan, walk with gravity and collision, ViewCube, walk paths, viewpoints, animations; adaptive LOD on large models.
- **Visual styles** — Realistic, Shaded, Concept, Wireframe and **Hidden line** (technical-drawing look, every edge in black); fine edges, soft contact shadows, **sun shadows by date, time and location**.
- **Professional PBR materials included** — 28 scanned materials from ambientCG (CC0): concrete, exposed concrete, brick, plaster, steel, aluminium, zinc, copper, oak, parquet, tiles, clay roof tiles, asphalt, grass, marble, granite, paving… with colour, normal, roughness, metallic, AO and height maps, applied automatically by IFC type. Optional 2K download; import your own `.zip` packs (freepbr.com, Poly Haven, Poliigon) and **OpenPBR / MaterialX** `.mtlx`.
- **Lighting like Navisworks** — Point, Spot and Distant lights lit in **real time** in the 3D view, with a **Move / Rotate gizmo** (drag the arrows to move, drag the circles to aim), unlimited power and range, and **Sun & sky** (brightness, ambient, sun position by date / time / location, shadows) plus a **Night** mode — all in one Lighting panel.
- **Command palette** (Ctrl+K) — every command and every element name, in one search box.
- **Arrangeable panels** — dock left / right / bottom or float, auto-hide, and a **Blender-style Properties bar**: add any panel as a tab there; **Model focus** hides the ribbon and panels but keeps Properties floating. UI settings (clash filter, groups, visual style, 4D) are saved between sessions.
- **Interface in English or Romanian** (View › Language).
- **Undo / Redo** of the view (Ctrl+Z / Ctrl+Y) and **DWG / DXF overlay**: lay a 2D drawing over the model at a level.
</details>

<details open>
<summary><b>Coordination</b></summary>

- **Clash Detective** like Navisworks — test list with New / Active / Reviewed / Approved / Resolved columns and test status (New / Done / Old), **Add Test, Reset All, Compact All, Delete All, Update All** (with a summary of what changed), Rules / Select / Results / Report tabs; hard and clearance tests with **severity levels** (Critical / Major / Minor) and **ready-made templates** by discipline, **smart rules** (normal connections ignored, responsible discipline assigned automatically, grouping by system), grouping by level / grid location / type / model / assignee / status; **Filter by Selection** (inclusive / exclusive); Approved / Resolved keep **who and when**; **SwitchBack** sends a clash to Revit / Navisworks with the camera and the two elements; **Scan vs Model** turns as-built discrepancies into clashes; **import / export of clash tests** as Navisworks XML (with the selection sets) or CSV / Excel; **clash matrix by test** (Selection A × Selection B, a sheet per status) and tests created from a **sets matrix**; HTML, CSV, BCF and Navisworks reports.
- **Model Checker** — rules or **buildingSMART IDS 1.0** files (import **and export**): properties, attributes, classifications, materials; exact values, lists, patterns, ranges, optional / prohibited, **unique values** (duplicate marks, GUIDs). Excel and BCF reports. **Quick checks without IDS**: no level, name, classification, material or geometry, duplicates, duplicate GUIDs.
- **Classification & bSDD** — search the **buildingSMART Data Dictionary** online (IFC 4.3, Uniclass, ETIM, NL-SfB… with a dictionary filter) or a local Uniclass / OmniClass catalogue, and assign codes to the selection; exported to IFC as `IfcClassificationReference` with the bSDD URI.
- **Coordination objects** — **clearance zones** around equipment, **provisions for void** (Auto voids where pipes, ducts and cable trays cross walls and slabs, with the host element) and **placeholder volumes** for equipment not modelled yet; part of Clash Detective, saved with the project, exported to IFC.
- **Live Link** — the selection follows **Revit** and **Navisworks** both ways (matched by UniqueId / IFC GUID).
- **As-built check** — the model against the point cloud: built, partial, missing, not scanned; colours on the model and an Excel report.
- **Sections & Annotation** — boxes with stretch arrows (like Revit), planes, **sections by level** from the status bar, filled cuts (solid cap hatched on the cut plane). The **Annotation** tab exports the cut to **DWG / DXF / PDF** with hatching, element tags, grid bubbles and **dimension chains**; separate **Dimensions** / **Levels** toggles let you drop what you don't need. **Pick face**: click a face of the section box (it turns light green) to draw the view from it, **like an Inventor drawing view** — thick cut outline, visible edges beyond the cut and dashed **hidden lines**, with technical line weights. The **2D Drawing Preview** (cream sheet, title block, zoom / pan) shows exactly what is exported, in real coordinates.
- **Grids and levels** — from IFC, from DWG / DXF, or created in the app; clash locations like **“C-4 / Level 2”**.
- **Measure** with snapping, **relocate** models (point to point, rotate, align to a face), **compare** model versions (geometry and **parameter by parameter**, old → new, with a per-triangle **deviation heatmap**), **TimeLiner** 4D (CSV, **MS Project** and **Primavera P6** XML; videos with date and legend) and **quantities**.
- **Colour by parameter** with a legend in the view and on images / videos.
- **User data like Navisworks** — Properties › Custom › Add User Data Tab / Insert New Property: your own properties and values on any element (also on geometry without data: STEP, DWG, OBJ), saved with the project, used by Find, Profiler, Model Checker and IFC export. **Import properties from Excel / CSV** (matched by GlobalId, Name or any property) and export an Excel template.
</details>

<details open>
<summary><b>Issues & collaboration</b></summary>

- **Viewpoints and issues** — status, priority, assignee, comments and markups (line, arrow, rectangle, ellipse, cloud, text). **Folders** (drag and drop, export per folder, kept in Navisworks).
- **BCF 2.1 / 3.0** import and export (Revit, ACC, Solibri, BIMcollab); **Navisworks XML** viewpoints, search sets and clash tests.
- **Shared issues** through a common folder (network drive, SharePoint / OneDrive) with automatic sync.
- **AI connection (MCP)** — a local Model Context Protocol server (Share › AI connection, token-protected, 127.0.0.1 only) lets **Claude Code / Claude Desktop, OpenCode, VS Code** or local **Ollama** models (through an MCP client) find elements, read properties, select, hide / isolate, set properties, run clash tests and scripts; ready-to-copy client configurations, read-only mode.
- **Projects** — `.bimr` (references, like NWF) and `.bimd` (everything in one file, like NWD); autosave and crash recovery.
</details>

<details open>
<summary><b>Output</b></summary>

- **IFC4 export** of the visible items or the selection — closed solids, colours, levels, original GUIDs, all properties.
- **Complete 2D plans** — hatch on cut walls / columns / slabs, door / window numbers and wall types, grid bubbles with dimension chains; level lines and cut grids on sections.
- **DWG / DXF** — section cuts as 2D in model space, or on an **A3 / A2 / A1 / A0 sheet** at a standard scale with the **title block**; written directly (AutoCAD 2018 format), no converter needed. Grid axes too.
- **PDF sheets** from viewpoints with the title block; HTML / Excel reports.
- **Web viewer** — one offline `.html` file with the model.
- **Reference models** for Revit and AutoCAD / Plant 3D (`.mbref`) with Load / Reload / Unload.
- **Animations window** — Record your navigation, Orbit 360°, walkthrough of viewpoints, walk path, **Import 4D** (the TimeLiner sequence); ▶ Play each one with a full player (loop, ping-pong), animate objects (colour, transparency, visibility).
- **Images and path tracing** — quick PNG of the current view, or **progressive CPU path tracing** (GGX, HDRI environment, lamps with soft shadows, Intel Open Image Denoise) in a separate window (resolution, samples, time limit, Cancel, export the current image at any time).
- Images and **videos** (MP4, AVI, GIF, PNG frames), optionally **rendered with materials** (Realistic style, textures, sun shadows).
</details>

---

## Add-ins (Windows)

| Program | Commands |
|---|---|
| **Revit 2023 – 2026** | *Export BIMC*, *Send to ManageBIM* (live update), *ManageBIM Issues*, *ManageBIM Reference*, *Sync Selection* |
| **Navisworks Manage / Simulate 2022 – 2026** | *Export BIMC*, *Send to ManageBIM*, *ManageBIM Issues*, *Sync Selection* |
| **AutoCAD / Plant 3D 2021+** | `BIMCEXPORT`, `BIMCSEND`, `MBREFERENCE`, `MBREFRELOAD`, `MBREFUNLOAD` |

The setup installs them per user — no administrator rights.

---

## System requirements

- **Windows** 10 / 11 64-bit, **Linux** x86-64 (X11 or Wayland), or **FreeBSD** 14 x86-64
- OpenGL 3.3 graphics (any recent integrated or dedicated GPU)
- 8 GB RAM recommended for large federated models and point clouds

---

## Building from source

Requirements: [Rust](https://rustup.rs) (stable). On Windows also Visual Studio 2022 Build Tools (C++ and .NET) and CMake.

```bash
git clone https://github.com/Latinu2020/ManageBIM.git
cd ManageBIM
cargo build --release --features addins   # application, bimc-convert and the three add-in installers
cargo test --release       # the test suite
```

The repository is self-contained: everything needed to build the application is in it. OpenCascade (STEP / IGES / BREP solids) and the Autodesk add-ins are optional and built with the scripts below; without them the application still builds and runs (those formats report that the runtime is missing).

<details>
<summary>Windows installer from Linux (cross-compile)</summary>

```bash
rustup target add x86_64-pc-windows-gnu
sudo apt install gcc-mingw-w64-x86-64 g++-mingw-w64-x86-64
cargo build --release --target x86_64-pc-windows-gnu --bin ManageBIM --bin bimc-convert
```
The icon is compiled with `x86_64-w64-mingw32-windres` when `rc.exe` (Windows SDK) is not available.
</details>

<details>
<summary>Windows: add-ins, OpenCascade and the installer</summary>

```powershell
# OpenCascade runtime (STEP / IGES / BREP / ACIS solids), once
powershell -ExecutionPolicy Bypass -File tools\build-occt.ps1

# default PBR material library (28 ambientCG CC0 materials, ~84 MB -> third_party\pbr-library), once
cargo run --release --bin fetch_pro_library

# add-ins (the Autodesk API reference assemblies are restored by the scripts)
powershell -ExecutionPolicy Bypass -File revit-addin\build.ps1
powershell -ExecutionPolicy Bypass -File acad-addin\build.ps1
powershell -ExecutionPolicy Bypass -File navis-addin\build.ps1

# single-file installer (builds with --features addins)
powershell -ExecutionPolicy Bypass -File tools\make-backup.ps1
```
</details>

<details>
<summary>Linux and FreeBSD packages</summary>

```bash
# Debian / Ubuntu build dependencies
sudo apt install pkg-config libx11-dev libxcursor-dev libxrandr-dev libxi-dev libxkbcommon-dev libwayland-dev libgl1-mesa-dev
cargo build --release --bin ManageBIM
bash packaging/linux/build-appimage.sh      # -> dist/ManageBIM-<version>-x86_64.AppImage

# FreeBSD
pkg install rust pkgconf libxkbcommon wayland mesa-libs
cargo build --release --bin ManageBIM
sh packaging/freebsd/build-tarball.sh       # -> dist/ManageBIM-<version>-freebsd-x86_64.tar.gz
```

Every push is built and tested on Windows, Linux and FreeBSD by [GitHub Actions](.github/workflows/build.yml); a `v*` tag publishes the Windows setup to Releases (the Linux and FreeBSD packages are added again once their builds pass).
</details>

| Folder | Content |
|---|---|
| `src/` | application (egui / OpenGL), readers (IFC, DWG / DXF, E57, LAS, FBX…), clash (`clash.rs`, `clash_io.rs`: Navisworks XML / CSV, matrix), model checker, animations, IFC / DWG / PDF export, renderer |
| `src/bin/` | `bimc-convert` (command-line converter) |
| `revit-addin/`, `acad-addin/`, `navis-addin/` | C# add-ins |
| `installers/`, `packaging/` | Windows setup wizard; Linux AppImage and FreeBSD packaging |
| `tests/` | integration tests |

File formats: [BIMC_FORMAT.md](BIMC_FORMAT.md) · changes: [CHANGELOG.md](CHANGELOG.md) · third-party components: [THIRD_PARTY.md](THIRD_PARTY.md)

---

## License

ManageBIM is released under the [MIT License](LICENSE). Third-party components keep their own licences — Open CASCADE (LGPL-2.1, dynamically linked), ufbx (MIT), Inter (SIL OFL 1.1), Phosphor icons (MIT) and Rust crates under MIT / Apache-2.0; see [THIRD_PARTY.md](THIRD_PARTY.md).

<sub>Autodesk, Revit, Navisworks, AutoCAD and Plant 3D are trademarks of Autodesk, Inc. ManageBIM is an independent project and is not affiliated with Autodesk.</sub>
