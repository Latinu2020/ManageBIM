<div align="center">

<img src="packaging/linux/managebim.png" width="88" alt="ManageBIM logo">

# ManageBIM

### Federated BIM review & coordination — fast, light, free

Open **IFC, Revit, Navisworks, AutoCAD / Plant 3D, STEP and point clouds** together, run clash detection and model checks,<br>
manage issues and markups, publish drawings — and stay in sync with Revit and Navisworks.

[![Download for Windows](https://img.shields.io/badge/Download-Windows%20installer%200.24.0-2563eb?style=for-the-badge&logo=windows)](https://github.com/Latinu2020/ManageBIM/raw/main/download/ManageBIM-Setup-0.24.0.exe)
&nbsp;
[![Linux & FreeBSD](https://img.shields.io/badge/Linux%20%C2%B7%20FreeBSD-Releases-334155?style=for-the-badge&logo=linux&logoColor=white)](https://github.com/Latinu2020/ManageBIM/releases)

[![Build](https://github.com/Latinu2020/ManageBIM/actions/workflows/build.yml/badge.svg)](https://github.com/Latinu2020/ManageBIM/actions/workflows/build.yml)
![Version](https://img.shields.io/badge/version-0.22.0-blue)
![Platforms](https://img.shields.io/badge/platforms-Windows%20%7C%20Linux%20%7C%20FreeBSD-0078d4)
![Rust](https://img.shields.io/badge/built%20with-Rust-b7410e)
![No admin](https://img.shields.io/badge/install-no%20admin%20rights-16a34a)
![License](https://img.shields.io/badge/license-MIT-green)

<img src="docs/images/main-window.png" alt="ManageBIM main window" width="920">

</div>

---

## Why ManageBIM

| | |
|---|---|
| ⚡ **Fast and light** | Native Rust + OpenGL. Opens large federated models in seconds; adaptive level of detail keeps navigation fluid. |
| 🔗 **Works with your tools** | Add-ins for **Revit**, **Navisworks** and **AutoCAD / Plant 3D**: send models, bring issues back, sync the selection live, load ManageBIM models as references. |
| 🧭 **Familiar** | Ribbon, Selection Tree, Find Items, Clash Detective, TimeLiner, viewpoints — the workflow you know from Navisworks. |
| 📐 **Open standards** | IFC 2x3 / IFC4 in and out, **BCF 2.1 / 3.0**, **buildingSMART IDS 1.0**, DWG / DXF, E57 / LAS, glTF. |
| 🆓 **Free and open** | MIT licence, no account, no subscription, no administrator rights to install. |

---

## Screenshots

<table>
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
| **Windows 10 / 11** (x64) | [**ManageBIM-Setup-0.24.0.exe**](https://github.com/Latinu2020/ManageBIM/raw/main/download/ManageBIM-Setup-0.24.0.exe) | Wizard installer with the Revit, Navisworks and AutoCAD / Plant 3D add-ins. Per-user, **no administrator rights**. |
| **Linux** (x86-64) | `ManageBIM-<version>-x86_64.AppImage` on [Releases](https://github.com/Latinu2020/ManageBIM/releases) | One file: `chmod +x`, then run. No installation, no root. |
| **FreeBSD** (x86-64) | `ManageBIM-<version>-freebsd-x86_64.tar.gz` on [Releases](https://github.com/Latinu2020/ManageBIM/releases) | Unpack and run `ManageBIM/ManageBIM`. Needs `pkg install mesa-libs libxkbcommon`. |

> **Windows:** the installer is not digitally signed yet — if SmartScreen asks, choose *More info › Run anyway*.
> Silent install: `ManageBIM-Setup-0.24.0.exe /S` (add `/NoAddins` for the application only).
>
> **Linux / FreeBSD:** the add-ins (Revit, Navisworks, AutoCAD) and the native STEP / IGES reader exist only on Windows; everything else is the same.

New here? Open **Help › Demo** for a sample project (architecture, structure, MEP and a 4D schedule), press **Ctrl+K** for the command palette, or read **Help › Manual**.

---

## Features

<details open>
<summary><b>Model review</b></summary>

- **Federated models** — IFC 2x3 / IFC4 (`.ifczip` too) with openings cut, systems, classifications and georeference, Revit and Navisworks (through the add-ins), AutoCAD / Plant 3D DWG with Plant 3D properties, DXF, STEP / IGES / BREP / FCStd, FBX, glTF / GLB, OBJ, STL, PLY.
- **Point clouds** — E57 and LAS, density and point size control, one item per scan.
- **Selection Tree** (group by any property, category colours, counts), **Property Tree**, **Find Items** with condition chips, selection and search **sets**, **Select Same Type / Category**.
- **Navigation** like Navisworks — orbit, pan, walk with gravity and collision, ViewCube, walk paths, viewpoints, animations; adaptive LOD on large models.
- **Visual styles** — Realistic, Shaded, Concept, Wireframe; fine edges, soft contact shadows, **sun shadows by date, time and location**, PBR material library (`.zip` packs from freepbr.com, ambientCG…).
- **Command palette** (Ctrl+K) — every command and every element name, in one search box.
- **Interface in English or Romanian** (View › Language).
- **Undo / Redo** of the view (Ctrl+Z / Ctrl+Y) and **DWG / DXF overlay**: lay a 2D drawing over the model at a level.
</details>

<details open>
<summary><b>Coordination</b></summary>

- **Clash Detective** — hard and clearance tests with **severity levels** (Critical / Major / Minor) and **ready-made templates** by discipline (Structure vs MEP, clearances, duplicates), **smart rules** (normal connections ignored, responsible discipline assigned automatically, grouping by system), rules, grouping by level / grid location / type, status tracking between runs, **clash matrix** per pair of models; HTML, CSV, BCF and Navisworks reports.
- **Model Checker** — rules or **buildingSMART IDS 1.0** files (import **and export**): properties, attributes, classifications, materials; exact values, lists, patterns, ranges, optional / prohibited, **unique values** (duplicate marks, GUIDs). Excel and BCF reports. **Quick checks without IDS**: no level, name, classification, material or geometry, duplicates, duplicate GUIDs.
- **Live Link** — the selection follows **Revit** and **Navisworks** both ways (matched by UniqueId / IFC GUID).
- **As-built check** — the model against the point cloud: built, partial, missing, not scanned; colours on the model and an Excel report.
- **Sections** — boxes with stretch arrows (like Revit), planes, **sections by level** from the status bar, filled cuts.
- **Grids and levels** — from IFC, from DWG / DXF, or created in the app; clash locations like **“C-4 / Level 2”**.
- **Measure** with snapping, **relocate** models (point to point, rotate, align to a face), **compare** model versions (geometry and **parameter by parameter**, old → new), **TimeLiner** 4D (CSV, **MS Project** and **Primavera P6** XML; videos with date and legend) and **quantities**.
- **Colour by parameter** with a legend in the view and on images / videos; **add parameters** to elements that have no data (saved with the project, exported to IFC).
</details>

<details open>
<summary><b>Issues & collaboration</b></summary>

- **Viewpoints and issues** — status, priority, assignee, comments and markups (line, arrow, rectangle, ellipse, cloud, text). **Folders** (drag and drop, export per folder, kept in Navisworks).
- **BCF 2.1 / 3.0** import and export (Revit, ACC, Solibri, BIMcollab); **Navisworks XML** viewpoints, search sets and clash tests.
- **Shared issues** through a common folder (network drive, SharePoint / OneDrive) with automatic sync.
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
- Images and **videos** (MP4, AVI, GIF) with materials, lighting and ambient occlusion.
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
cargo build --release      # the application
cargo test --release       # the test suite
```

<details>
<summary>Windows: add-ins, OpenCascade and the installer</summary>

```powershell
# OpenCascade runtime (STEP / IGES / BREP / ACIS solids), once
powershell -ExecutionPolicy Bypass -File tools\build-occt.ps1

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

Every push is built and tested on Windows, Linux and FreeBSD by [GitHub Actions](.github/workflows/build.yml); a `v*` tag publishes the packages to Releases.
</details>

| Folder | Content |
|---|---|
| `src/` | application (egui / OpenGL), readers (IFC, DWG / DXF, E57, LAS, FBX…), clash, model checker, IFC / DWG / PDF export, renderer |
| `revit-addin/`, `acad-addin/`, `navis-addin/` | C# add-ins |
| `installers/`, `packaging/` | Windows setup wizard; Linux AppImage and FreeBSD packaging |
| `tests/` | integration tests |

File formats: [BIMC_FORMAT.md](BIMC_FORMAT.md) · changes: [CHANGELOG.md](CHANGELOG.md) · third-party components: [THIRD_PARTY.md](THIRD_PARTY.md)

---

## License

ManageBIM is released under the [MIT License](LICENSE). Third-party components keep their own licences — Open CASCADE (LGPL-2.1, dynamically linked), ufbx (MIT), Inter (SIL OFL 1.1), Phosphor icons (MIT) and Rust crates under MIT / Apache-2.0; see [THIRD_PARTY.md](THIRD_PARTY.md).

<sub>Autodesk, Revit, Navisworks, AutoCAD and Plant 3D are trademarks of Autodesk, Inc. ManageBIM is an independent project and is not affiliated with Autodesk.</sub>
