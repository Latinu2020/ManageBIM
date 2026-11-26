# ManageBIM - ce s-a schimbat

Versiunea apare în **View › Help › About** și în proprietățile fișierului `ManageBIM-Setup-<versiune>.exe`.
La fiecare livrare: crește versiunea în `Cargo.toml` (și `installers\setup\Cargo.toml`) și adaugă o secțiune aici.

## 0.28.8 - unreleased
- **Levels in Plant 3D / AutoCAD and Navisworks** (they have no real levels): View › Grids & Levels › **Send Levels** sends the project levels (from IFC, Revit, DWG or your own); then
  - **AutoCAD / Plant 3D - command MBLEVELS**: a level line and label at every elevation (layer MB-Levels), a **named UCS** per level ("Level - Level 1" - work straight on that level), and the **level on every object** by its bottom elevation (XData MANAGEBIM_LEVEL, shown as a property when the DWG is opened in ManageBIM). Running it again replaces the previous levels.
  - **Navisworks - ManageBIM › Levels**: one **selection set per level** (Sets › "ManageBIM levels": hide, filter, use as Selection A / B in Clash Detective) and one **plan viewpoint per level** (top view, section box between the level and the next).

## 0.28.7 - 2026-11-26
- **Smart sets** (Sets › ✨ Smart set…): build a search set by picking a property and ticking its values, with the number of items for every value and a live total (values of one property = OR, different properties = AND); **suggestions from the selection** ("the selected items share System = Supply Air, Level = Level 2" - one click adds them); **type what you want** ("ducts level 2", "walls, EI60", "valves cs150") and the conditions are filled in for you to adjust. Save it, select it, or save it straight **into a clash test's exclusions** or as **Selection A / B**.
- **Sets in Clash Detective with one click**: every set has a ⛔ menu - exclude it from any clash test (tick / untick) or make it Selection A / B of the current test.
- Level labels move up at most two rows before one is hidden, so small models keep all their labels.
- **Dashboard** (Home › Project and Share › Report): one window with the project at a glance - models, open clashes, open issues, Model Checker errors, levels, 4D tasks as cards; every clash test with a status bar and Critical / Major / Minor counts; issues by status, priority and assignee; models, the biggest categories and the levels. **Export as a professional PDF (A4) or Word (.docx) report** with the same content.
- **"Ignore everything like this…"** (right-click a clash result): Smart set opens with Item A, B or both selected and their shared properties suggested; tick what to ignore and **Save & exclude** adds it to that test's exclusions in one step.
- **Search sets show what changed**: after the models are updated, a set that gained or lost items shows "+3 -1" in the Sets panel (hover: which items); click to accept.
- **Headroom / Egress buttons** add the test (or open it) at its settings instead of running it straight away: set the height / distance and grid step, then press Run Test. More can be added from Clash Detective › Templates.
- **Undo / Redo cover model changes too** (Ctrl+Z / Ctrl+Y): properties you wrote, colour overrides, materials and material rules come back together with the selection, visibility, section and camera of that moment.
- **Walk paths to Navisworks**: Viewpoint › Walk Path › **To Navisworks**, then in Navisworks ManageBIM › Walk path (also in the ManageBIM window): the path becomes a **viewpoint animation** in Saved Viewpoints - key frames at equal distances (constant walking speed), at eye height, looking ahead - ready to Play. If Navisworks refuses the animation, the frames land in a folder of viewpoints instead.
- **The model no longer stays transparent** after leaving Clash Detective (or TimeLiner) by lowering the bottom panel: the clash / 4D display is only on while the panel is visible.

## 0.28.6 - 2026-11-26
Fixes found by testing the add-ins in Revit 2025, AutoCAD Plant 3D 2025 and Navisworks Manage 2025.
- **Revit › Send selection** failed with "not a valid exportable view"; it now exports the 3D view and keeps only the selected elements.
- **AutoCAD / Plant 3D**: the installer adds the ManageBIM add-in folder to AutoCAD's trusted locations (every profile; removed on uninstall), so AutoCAD no longer asks "Always Load" for the unsigned add-in at every start.
- **Plant 3D levels**: objects on layer "0" get the project **area** (the drawing's folder in the Plant project, e.g. "Area_2") as level - from the add-in and when a Plant DWG is opened directly.
- **Duplicate properties** with the same value and a name written two ways ("Actuator Height" / "ActuatorHeight") are merged.
- **Level labels** stay at their level; on models with many levels the overlapping ones are hidden (the level lines stay, with a note) instead of piling up in a column.

## 0.28.5 - 2026-11-26
- **Headroom check** (Coordination › Space checks › Headroom): points on every floor slab; the first beam, duct, pipe, cable tray, air terminal or stair above that is lower than the minimum clear height (2.10 m, editable) becomes a result in Clash Detective - obstacle × slab, with how much is missing (Critical from 200 mm, Major from 50 mm). Walls, columns, door heads and furniture standing on the floor are ignored.
- **Egress check** (Space checks › Egress): per level, walking distance around walls to the nearest exit - exterior doors on the lowest level, stairs / ramps above - longer than the limit (45 m, editable), and floor areas or whole levels without any exit. Space (or slab) × nearest exit, with the farthest point.
- Both are ordinary clash tests (templates *Headroom* and *Egress* in Clash Detective): status, assignee, comments, viewpoints, Update All, HTML / BCF reports, and the AI tools work on them; hidden items are left out.
- **AI connection, part 2**: `capture_view` sends the 3D view to the assistant as an image (optionally zoomed to elements, the whole model or a standard view); `get_clash_results`, `update_clash_results` (status, assignee, comment, group), `focus_clash` (two elements highlighted, camera on the clash) and `export_clash_bcf`. Images pass through the Claude Desktop bridge too.
- **Materials window**: Render › Materials opens its own window, as tall as the application window, with a **wide list** and real **texture thumbnails** (cached in the library, updated when a map changes), a filter box, and "PBR · 5 maps · used ×3" under each name.
- **Save a material**: *★ Save to my library* / *✔ In my library* right in the editor, and **Save as file…** - a MaterialX file with its texture maps copied next to it, for another computer, a colleague or Blender / Maya / Houdini / Omniverse.
- **MaterialX import**: ambientCG names are cleaned ("WoodFloor051_1K_JPG" → "WoodFloor051"), the texture maps are **copied into your library** (deleting the downloaded folder no longer breaks the material), and the imported material is selected and scrolled into view.
- **AI connection window** redesigned: switches, status pill, endpoint and token with show / copy / new, one tab per client with numbered steps, and the list of tools the assistant can use (blocked ones greyed out when *Allow changes* is off).
- **Cancel** next to the loading bar: a large file opened by mistake can be stopped (the loading thread stops too).
- **Notifications**: a bell in the status bar keeps the recent messages and errors (with a counter) after the pop-ups disappear; copy all.
- **Keyboard shortcuts** window (F1, Help › Shortcuts), with the configurable ones as set in Settings.
- **The bottom panel starts lowered** (Find Items no longer takes a quarter of the screen at start); your choice is remembered.
- **Status bar fits at any size**: at 125 % / 150 % interface size or on small screens the panel tabs switch to icons; ribbon labels shrink instead of being hyphenated ("Prope-rties").
- **New application icon**: blue tile with an isometric cube (same mark as the start page), sharp at every size from 16 to 256 px - window, taskbar, exe and installer.
- **CI**: clippy may not get new warnings (baseline 242), new .rs files must be rustfmt-formatted, Dependabot weekly updates; the release clean-up keeps the 3 newest versions by version number and no longer marks the release as failed.

## 0.28.4 - 2026-11-26
- **AI connection (MCP server)** - Share › AI connection: a local Model Context Protocol server (off by default, only on 127.0.0.1, protected by a token) lets AI assistants work with the open model. Tools: model info, find elements (category, level, property / value, text), get properties, get / set selection, hide / show / isolate, zoom, set property (undoable), list / run clash tests, run Rhai scripts, open a model. *Allow changes* off = read-only.
  - Ready-to-copy setup for **Claude Code** (`claude mcp add …`), **Claude Desktop** (`ManageBIM.exe --mcp-stdio` bridge), **OpenCode**, **VS Code / Copilot** and any Streamable-HTTP client; local **Ollama** models work through MCP clients such as OpenCode, Continue, Open WebUI or LM Studio.
- **MaterialX import / export fixed**: standard `.mtlx` files now import - ambientCG / GPUOpen / Substance exports with `surfacematerial`, `standard_surface`, `open_pbr_surface`, `UsdPreviewSurface`, `gltf_pbr`, node graphs, `image` / `tiledimage` / `normalmap` nodes, `fileprefix` and displacement; missing AO / roughness / height maps are picked up from sibling files. Imported materials go to **your library**. Export writes standard MaterialX 1.39 (OpenPBR + image nodes) that other tools can read, and the material name survives the round trip. Older ManageBIM `.mtlx` files still open.
- **Linux AppImage and FreeBSD packages** are published again with every release.

## 0.28.3 - 2026-11-25
- **Point clouds**: no more flicker or jumps while zooming (points are no longer dropped during camera moves unless the GPU is slow; gentler wheel zoom, capped point size); **density in the status bar** (☁ Points %) and a Point Cloud button in Analyze › Scan; **automatic classification** (ground / low, medium, high vegetation / buildings) for LAS / LAZ without classes and for E57 - each E57 scan split by class.
- **Materials**: ambientCG / freepbr ZIPs import reliably (ZIP64 archives, clear message for unsupported compression) and *Import Lib* accepts ZIPs; every import (ZIP, folder, images, .bimmat, MaterialX, Excel) is saved to **your library** (★ in the list) and edits to library materials are saved there too.
- **Ribbon fits the window**: no horizontal scrollbar; buttons narrow automatically, tabs shrink, and a tab that still does not fit switches to icons (names in tooltips).
- **Linux / FreeBSD builds**: the only failing test (a Windows-only path in the MaterialX test) is fixed; failures are now reported as annotations.
- **New open formats, with the properties of every object**
  - **3MF**: units, build items, components (also in other model parts), base materials / colour groups, model and object metadata; supports hidden by default.
  - **CityJSON / CityJSONSeq** (`.json`, `.city.json`, `.jsonl`): every city object with its attributes (nested ones flattened), highest LoD only, semantic surface colours (roof, wall, window…), parts under their building, geometry templates, appearance materials.
  - **ifcJSON and ifcXML** (IFC2X3 / IFC4 / IFC4.3): converted to IFC STEP with the official buildingSMART EXPRESS schemas, then read like any `.ifc` - same geometry, property sets and levels; inverse relations written inside objects (isDefinedBy…) become real relationships.
  - **3D Tiles 1.1 / 1.0** (`tileset.json`, `.glb`, `.b3dm`, `.cmpt`): REPLACE / ADD refinement, external tilesets, tile transforms, RTC_CENTER, batch tables as properties; ECEF models shown upright in a local East-North-Up frame with the WGS84 position.
  - **USD ASCII** (`.usda`): prims and transforms (xformOpOrder), meshes and Cube / Sphere / Cylinder / Cone, displayColor / displayOpacity, metersPerUnit and upAxis, customData / assetInfo / custom attributes as properties, `@file.usda@` references. Binary .usdc / .usdz get a clear message.
  - **LAZ** point clouds open directly (LASzip decompression built in, no CloudCompare needed).
- **Richer existing formats**
  - **LAS / LAZ**: one tree item per **classification** (Ground, Building, Vegetation, Water…) with intensity, returns, GPS time and height range - hide or isolate classes.
  - **DXF**: block **attributes** (ATTRIB) and **XDATA** become properties; a TAG / NAME attribute names the element.
  - **PLY**: faces split into elements by object_id / segment / label / material_index, face and vertex properties (confidence, quality…) and header comments as properties; face colours.
  - **glTF**: **EXT_structural_metadata** + **EXT_mesh_features** - one mesh with several objects becomes several elements, each with its row of properties.
- **Geometric diff between revisions**: Compare now measures how far every surface moved (both directions) for changed elements - "Max deviation 300 mm (mean …)" - and shows a **deviation heatmap per triangle** (green in place → red at the scale, automatic or in mm) with a legend. Elements reshaped inside an unchanged bounding box are detected too.

## 0.28.2 - 2026-11-25
- **Export ribbon**: IFC, GLB and BCF have proper file-format icons (document with a coloured IFC / GLB / BCF tag) and are now also in the Export tab (*Open standards*), not only in BIM.
- **Coordination objects are no longer created by surprise**: Clearance / Void / Placeholder in the ribbon open the window with a *New … — Create / Cancel* bar; nothing is added until you press **Create**.
- **New ViewCube**: light-shaded cube with chamfered edge / corner zones (26 directions), labels that turn with the faces, a **compass ring** with N / E / S / W and ticks, Autodesk-style **arrows to the neighbouring faces** when you look straight at a face, Home / PERSP as small pills.
- **Viewpoints vs View Templates**: a new viewpoint no longer silently links to template #1; each viewpoint stores its own look (style, grids, levels, section caps, colours) and comes back exactly as saved. Templates apply only what their *Applies* boxes say; renaming / deleting a template keeps the viewpoints right; applying a template no longer leaves a viewpoint marked as active.
- **Multi-view drawing window** (Annotation › Multi view): choose the views (Front, Right, Back, Left, Top, **Bottom**), the **spacing** (Auto or a value in metres) and titles; live sheet preview with the spacing shown; one button exports PDF / DWG / DXF. Labels no longer run into the next view.
- **Property editor** (Export › Properties, BIM › Properties): pick elements, then fill the **standard IFC property sets** for their class (Pset_WallCommon, Pset_DoorCommon, Pset_WindowCommon… plus Manufacturer, Warranty, Condition, Service life, Environmental) with the right controls (TRUE/FALSE, lists, numbers), or add **custom** sets and properties. Standard values are exported to IFC with their real types (IfcBoolean, IfcThermalTransmittanceMeasure, IfcInteger…).
- **No more lost properties**: overriding an existing property (e.g. FireRating) used to strip it from every element of the model; now only the chosen elements change, and removing the override brings the original value back.
- **Point clouds**: round points with relief, **real point size** (close up the points grow and close the gaps), colour by **Elevation** or Grey, clean look in Hidden-line / SSAO styles (points were inked black); **Measure snaps exactly onto scan points**.
- **Walk path drawing**: drag points to move them, click on the line to insert, right-click a point to delete, Shift for 45°, clicks on a wall drop to the floor; start / end markers, direction arrows, segment lengths and a live preview of the next segment.
- **AutoCAD / Plant 3D / Civil 3D add-in**: objects of other applications (Plant 3D steel members, footings, gratings, equipment; Civil 3D surfaces, corridors, pipes; Architecture / MEP) now send the properties shown in AutoCAD's **Properties palette**; curved geometry is faceted finer (2 mm – 5 cm, 15°), so tanks and large pipes stay round. Plant 3D projects without drawings or links no longer report an error.
- **.bimd is self-contained**: DWG / DXF overlays and the texture images of the materials in use are packed into the file and unpacked on open.

## 0.28.1 - 2026-11-25
- **Move and rotate coordination objects with a gizmo**: in the Coordination objects window choose **Resize** (arrows on the faces), **Move** (X / Y / Z arrows) or **Rotate** (blue circle around the vertical axis, angle shown; Shift snaps to 15°). Objects now have a **Rotation** (also as a number, with 0° / +90° buttons); clash detection, the IFC export and Revit / Navisworks use the rotated box.
- **Coordination objects in Revit and Navisworks - display only**: *Send to Revit / Navisworks* writes the visible clearance zones, voids and placeholders; in Revit (**ManageBIM › Coordination**) and Navisworks (**ManageBIM** window › Coordination objects) a **preview window** lists them with a top view and checkboxes, then shows them as transparent boxes coloured by type. Nothing in the model is cut or changed; a new send replaces (Revit) or hides (Navisworks) the previous objects. The 0.28.0 command that cut voids into walls is removed.
- **Compact add-in ribbons**: Revit shows two large buttons (*Send to ManageBIM* and *ManageBIM*) and the rest as small stacked buttons; Navisworks keeps only *Send to ManageBIM* and *ManageBIM* in Tool add-ins. The **ManageBIM** button opens a window with every command, grouped, each with a short description.
- **All Plant 3D properties after Send**: empty Plant 3D fields are kept (they were dropped), the **line** data (Line Number, Service…) is read from the related Line Group, and the equipment of a nozzle is added. ManageBIM also completes the sent model from the project database **Piping.dcf** (the same properties as opening the DWG directly), without duplicates.
- View templates pill under the ViewCube stays inside the 3D view (the right arrow was cut off); the Coordination objects window opens away from the centre of the view.

## 0.28.0 - 2026-11-24 (stable)
- **Professional interface**
  - Ribbon buttons show their **full name** on one or two lines (no more "Clearan…", "Pick fa…"); slightly larger buttons.
  - **Vector icons everywhere**: 15 ribbon icons that showed as an empty square now have a real icon, coloured by role (green clearance, orange void, violet placeholder, yellow auto).
  - **One notification at a time** (the newest, errors first) with a **+N** counter, instead of three stacked over the model.
  - **Level labels** stay above the navigation bar and never overlap each other.
  - Removed duplicates: the **Cut View** group is only in Annotation (it was in Sections too); the **Highlight plane** checkbox is gone - the chosen face of the section box is always shown light green. Sections has a **Box outline** button instead of the reversed "Hide section box" checkbox.
  - **One language on screen**: the script builder and the script manual follow the interface language (they were in Romanian in the English interface).
- **Coordination objects edited in 3D**: with the Coordination objects window open, the selected object shows arrows on its six faces (drag to resize, like the section box) and a centre handle to move it.
- **Voids sent to Revit**: approved provisions for void are written for the Revit add-in (*Send voids to Revit*); the new **ManageBIM Voids** command in Revit 2023 - 2026 cuts them into the host walls (rectangular opening) and floors / roofs (shaft opening), in shared coordinates. Not yet tested inside Revit.
- **Multi-view drawing** (Annotation › Multi view): Front, Right, Back, Left and Top views of the section box on one drawing, like an Inventor sheet, with view titles. **DWG / DXF at 1:1 in model space** - the Front view stays in real coordinates; or PDF on a sheet.
- **DWG overlay lands exactly where it was cut**: the cut plane is now stored as hidden text inside the drawing itself (survives renaming, DWG conversion and saving in AutoCAD); Back / Right views were placed on the wrong side of the origin and an extra offset could move the drawing - both fixed.
- **Path trace**: large window with the **live preview** next to the settings; **light bounced from lamps** (walls lit by a lamp light the floor and ceiling); fixed **Spot lights that lit nothing** in the final render (the cone was measured the wrong way).
- **View Templates** redesigned: Ctrl+N number, summary of what each holds, **Update** with the current view, move up / down, and **Applies** choices (Camera, Visibility, Section, Style, Colours) like Revit's Include; the camera flies to the view.
- **Check for updates**: at start-up (can be turned off in Settings) or from Ctrl+K; a new version shows an **Update** button in the status bar that downloads and starts the installer.
- **CI**: the GitHub release is published when Windows and the setup build, even if the Linux / FreeBSD builds fail (they blocked every release before).
- Code health: the Script Library and Path trace windows moved out of `ui.rs` into their own modules. 5 new tests.

## 0.27.32 - 2026-11-24
- **Coordination objects** (Coordination tab), like in Navisworks / Solibri:
  - **Clearance zones**: maintenance / access space around the selected equipment (adjustable margin, 0.6 m by default).
  - **Provision for void**: proposed openings for services through walls and slabs. **Auto voids** finds every pipe, duct or cable tray that really crosses a wall or slab (not those running along it) and proposes a void with a margin, recording the host element.
  - **Placeholder volumes**: space reserved for equipment that is not modelled yet, or a corridor for a route.
  - They form a model **"Coordination objects"** that appears in the Selection Tree, Properties, Find and **Clash Detective** (one click creates the test "Coordination objects vs model"), are saved in the project and exported to IFC (IfcBuildingElementProxy with the PredefinedType property). A window lists them with size (mm), centre, status (Proposed / Approved / Rejected / Built), discipline, host and notes, with show / hide, zoom and delete.
- **2D drawings from the section box, like Inventor drawing views**:
  - **Pick face** (Annotation / Sections › Cut View): hover a face of the section box - it turns light green - and click it; the drawing is made from that face (Top, Bottom, Front, Back, Left, Right). The separate orange plane is no longer needed (the cut plane is light green too).
  - The drawing now shows the **visible edges** beyond the cut (the real edges of the shapes, no mesh diagonals) and optional **hidden lines** dashed, besides the thick cut outline, hatch, levels and dimensions.
  - Line weights like a technical drawing in the PDF (cut 1.1 / visible 0.55 / hidden 0.4 dashed / hatch 0.25) and a HIDDEN linetype layer in DWG / DXF. The coordinates stay exact: an exported DWG attached back as an overlay lands in the same place.
  - New **2D Drawing Preview**: cream sheet with frame and title block, resizable, zoom with the wheel, pan by dragging, double-click to fit, toggles for visible edges / hidden lines / hatch / dimensions, PDF and DWG buttons.
- **Hidden line visual style fixed**: it showed only the outer silhouette on white, so the building almost disappeared. Now all the edges of the shapes are drawn in black with a very light grey shading by face orientation, like a technical drawing. The help text in the corner of the view is readable on light backgrounds.
- **Lights**: removed the thin range circles around the selected light.
- **Installer redesigned** (all in English): numbered steps with progress, a banner and feature cards, component cards, a real percentage while installing and a clear finish page. **Choose the install folder** with **Browse…** (warns about folders that need administrator rights); upgrades reuse the folder of the previous installation and uninstall works from any folder.
- 8 new tests (hidden-line removal, coordination objects and voids).

## 0.27.31 - 2026-11-24
- **Rotating a light always works**: dragging a Rotate circle on a **Point** light (the type created by Place in 3D) turns it into a Spot light so it can be aimed - before, nothing happened because a point light has no direction.
- **New lights are created above the model**, aimed down at the roof, so the light and its pool are visible at once (before they were placed inside the building, at its centre).
- **No limit on light power**: the Lux slider goes to 10 000 (logarithmic) and any larger value can be typed; range up to 100 km; sun intensity up to 10.
- **Sun & sky in the Lighting panel**: brightness, ambient light, sun intensity, shadows, sun position (manual or by date / time / location), contact shadows and edges - moved from the separate Lighting window, which is gone. A "Sun on" switch turns the sun off for interiors. View › Lighting opens the same panel.
- **Bottom panels laid out in columns and kept small**: Lighting shows Sun & sky and Lamps side by side, lamp cards are placed next to each other; Lighting, Render Image, Project, Section, Measure and Coordinates scroll inside the panel instead of pushing it up over the 3D view. The bottom panel is at most half the screen (65% when maximised); floating panel windows open at most ~60% of the screen and no longer grow past it.
- **Professional PBR materials by default**: the app now ships with 28 scanned materials from **ambientCG (CC0, free for commercial use)** - concrete, exposed concrete, brick, plaster, gypsum, steel, aluminium, galvanized and stainless steel, copper, oak, dark wood, parquet, ceramic tiles, red clay roof tiles, asphalt, grass, marble, granite, travertine, paving stones, carpet, fabric, porcelain, corrugated steel - each with colour, normal, roughness, metallic, AO and height maps (1K, ~84 MB in the installer). They replace the simple built-in materials with the same name, so "Auto by IFC type" uses them at once; the old procedural textures from `gen_pbr_library` no longer override them (your own materials still do). A 2K version can be downloaded from Render › Materials. Maintainers: `cargo run --release --bin fetch_pro_library` fills `third_party\pbr-library`, which `tools\make-setup.ps1` includes.
- Fixed: the Create Light menu stayed open after choosing a type.

## 0.27.30 - 2026-11-24
- **Light rotation with the gizmo works**: dragging a Rotate circle no longer orbits the camera at the same time (the circles used to run away from the cursor), the grabbed circle stays active for the whole drag, and the angle follows the mouse around the circle (ray / plane intersection) instead of only horizontal movement. The circle colour shows the axis it turns around (blue = Z, green = Y, red = X); the current **Azimuth / Tilt** is shown under the light while rotating.
- **Spot and Distant lights are aimed in the 3D view in real time**: the GPU shader now uses the light direction and the spot cone (before, every light was drawn as a point light, so rotating had no visible effect). The spot cone and the direction arrow are drawn in the viewport. New **Aim** fields (azimuth / tilt, ⬇ straight down) in the Lighting panel.
- **Lights are clearly visible in 3D**: glowing bulb marker with rays in the light colour, readable label on a dark background, disabled lights shown greyed out. **Click a light in the 3D view to select it** and show its gizmo.
- Fixed: lights with **Range 0 (unlimited)** disappeared in the 3D view (the range was sent to the GPU as 0.1 m).
- **Night mode in the viewport**: dark sky, sun and ambient off - the lamps light the model (before, Night only affected the offline render). Lamps also add highlights on glossy surfaces and light the Concept style.
- **Path trace**: fixed **Distant lights shining from the wrong side** (lit back faces, inverted shadows). Point / Spot lamps have a small bulb size and Distant lights a small angular size, so their shadows have **soft, realistic edges**.
- **bSDD classification window** (BIM › bSDD, or the command palette): the button used to open the Properties panel. Now a dedicated window with two tabs - the **Local catalogue** (table with search, remove, import / export) and **bSDD online**: live search in the buildingSMART Data Dictionary (IFC 4.3, Uniclass, ETIM, NL-SfB, … with a dictionary filter), assign a class to the selected elements (code, name and URI are saved), ⭐ keep a class in the local catalogue for offline use. Shows the current classifications of the selection and can clear them.
- **Classifications are exported to IFC** as `IfcClassification` + `IfcClassificationReference` + `IfcRelAssociatesClassification` (with the bSDD URI as Location) - they were only written as a property set before. Classifications read from an IFC file are exported back the same way.
- Classification CSV: fixed broken diacritics and "—" (the file was read byte by byte), `""` quotes inside fields, Excel files with `,` separator or a UTF-8 BOM; the exported CSV opens correctly in Excel.
- 9 new tests (light aiming, bSDD parsing, CSV, IFC classification references, Distant light direction).

## 0.27.29 - 2026-11-24
- **Light gizmo with Move / Rotate modes** (like Navisworks): a **Move** button uses the X/Y/Z arrows to move the light; a **Rotate** button shows three thick circles (XY → Z axis, XZ → Y, YZ → X) that you drag to aim the light. Previously the thin range circles were hard to grab and rotation was not discoverable.
- **No more limited light range**: radius 0 means unlimited (no distance falloff) in both the GPU viewport and the offline render; the range slider now goes up to 5000 m.
- **Night scene**: a "🌙 Night" toggle darkens the sky and reduces ambient light so the lamps in the model light the scene like in the Navisworks example.
- **Local classification catalogue (D6)**: a bSDD / Uniclass / OmniClass panel in the BIM tab with search and one-click assignment of a code to the selected elements (saved as a property "Classification" and exported to IFC). The catalogue can be imported/exported as CSV or JSON. IDS 1.0 import, export and validation were already present in Model Checker.

## 0.27.28 - 2026-11-24
- **Single BIM ribbon tab**: all BIM operations (IFC export with MVD, category-to-IFC-class mapping, bSDD, IDS check, COBie, properties, quantities, glTF, BCF, Navisworks sets) are now in one **BIM** tab. Removed duplicates from Export / Share.
- **IFC MVD export**: choose Reference View, Design Transfer View, Coordination View 2.0 or Site View before export; the chosen view filters which property sets are written and is recorded in the IFC header.
- **IFC class mapping** (like Revit): settings table Source Category → IFC Class Name / IFC Type, used for geometry that does not come from IFC (STEP, OBJ, glTF, DWG). "Not Exported" skips a category. Import/export the table as CSV (compatible with Excel).
- **Light rotation on the drawn range circles**: the XY / XZ / YZ action circles of a Spot/Distant light are now rotation tracks - drag one to rotate the direction around the circle's normal. Increased the GPU light intensity so the falloff is clearly visible in the 3D model.

## 0.27.27 - 2026-11-24
- **Lights illuminate the 3D model in real time**: point lights are now sent to the GPU shader (up to 8 lights with position, colour, power/lux and range), so changing the lux or colour immediately changes how the model is lit - like the Interactive Ray Trace in Navisworks. Previously the lights only affected the offline path trace.
- **Fixed gizmo dragging**: dragging an X/Y/Z arrow no longer orbits the camera at the same time. The arrows are shorter for easier handling, and Spot/Distant lights have a **rotation ring** - drag it to turn the light direction around its position.
- The light marker, colour and lux label update live in the 3D view.

## 0.27.26 - 2026-11-24
- **Fixed light deletion**: the per-row 🗑 button removed the selection instead of the light; **Remove** did nothing when no row was selected. Both now delete the light correctly.
- **New lights are created at the centre of the model** (2 m above it), not at the camera, so they are immediately visible in the scene; the camera zooms to them. Added a **🎯 Focus** button to fly to a light created far away in an older project.
- **Much larger UCS gizmo**: arrows are 3x longer and thicker, with bigger labels, a 3x larger hit area and markers that scale with the light's power. The light's colour / lux / range changes update the 3D marker immediately, and the label shows the name and lux value.

## 0.27.25 - 2026-11-24
- **Fixed the light gizmo in the 3D view**: the previous marker was only a flat painted dot, so the added light was not visible or movable. Lights are now real visible entities with a UCS gizmo - arrows for X (red), Y (green), Z (blue) that you grab and drag with the mouse.
- The three action circles (XY / XZ / YZ) show the light's range; the centre dot scales with the power (lux), and the label shows the name and lux value. Changes to colour / power / range update the 3D marker immediately.
- Spot / Distant lights also show their direction axis in the viewport.

## 0.27.24 - 2026-11-24
- **Lighting like in Navisworks**: a dedicated **Render › Lighting** tab with **Create Light** (Point / Spot / Distant), a light list, and full controls for **lux (power)**, range, colour, position, target and cone angle. Lights are saved in the project.
- **Lights can be placed and moved in the 3D view**: the **Place in 3D** tool drops a point light where you click on a surface; lights are drawn as markers in the viewport, can be selected by clicking the marker and moved by dragging it with the mouse (Esc returns to Select). A Lighting button was added to the Render ribbon.
- Three light types in the path trace: point (inverse-square falloff), spot (cone with soft edge aimed at a target) and distant (parallel rays, like the sun), all with shadows and GGX highlights.

## 0.27.23 - 2026-11-24
- **Photorealistic renders**: the offline renderer now supports a real environment map - load a Radiance `.hdr` or an equirectangular PNG/JPG photo and the sky is replaced with real light and reflections. Added with intensity, rotation, sun direction, exposure and a choice of Outdoor / Studio / Sunset when no image is loaded.
- **Point lights in the model**: add lamps at the scene origin or at the centre of the current selection, then edit position (X/Y/Z), colour, power and radius. They are saved in the project and used by the path trace render with shadows and GGX highlights.
- **Richer PBR textures**: the local library was regenerated with multi-octave detail - pores and stains in concrete, veins in stone, irregular mortar and wear in brick, growth rings and knots in wood, grout and glaze variations in tiles, brushing and scratches in metal, woven threads in fabric. Stronger normal maps and AO. The old user library is replaced (a `.old` backup is kept).
- **Materials window**: the editor is now beside the list and assignments are in a third column, so everything is visible at once.
- **Full detail in 3D**: new option in Settings › Appearance disables the adaptive LOD while the camera moves. The PNG / path trace render always used the full model.

## 0.27.22 - 2026-11-24
- **OpenPBR / MaterialX (subset)**: materials now store `coat` (clearcoat), `coat roughness`, `sheen` and IOR. The Materials window was rebuilt with clear sections (Surface, OpenPBR Surface, Texture maps, Assignments, Library) and a bigger albedo preview. Import / export `.mtlx` MaterialX reads and writes OpenPBR Surface nodes, their parameters and image paths (simple materials - no full graph evaluation, no C++ dependency).
- **Professional path tracing**: image-based lighting with three environments (Outdoor / Studio / Sunset), adjustable intensity and rotation; GGX microfacet BRDF with Fresnel and IOR instead of Blinn-Phong; clearcoat and sheen layers; importance-sampled reflections and 4 bounces; ACES filmic tonemapping (the PNG export and the preview now look the same).
- **Lights**: the Path trace window has controls for environment type, intensity, rotation, sun azimuth / elevation / power and exposure. The sun direction, intensity and exposure from Render settings are now actually used by the offline render.

## 0.27.21 - 2026-11-23
- **Fixed PBR zip import**: packs from freepbr.com / Poliigon with names like `Metal063_2K-PNG_Color.png` were rejected with "no colour map found" because "Metal" in the material name was detected as a metallic map. Roles are now read from the **last words** of the filename; the OpenGL normal map is preferred over DirectX; `_Color`, `_Albedo`, `_BaseColor`, `_Metalness`, `_NormalGL`, `_Roughness`, `_Displacement` are all recognised.
- **Fixed "Not enough GPU texture layers"**: only materials actually used in Assignments load their textures (before, the whole library did). Layer limits raised to 192 at 512 px, 64 at 1024 px, 16 at 2048 px; the error message now explains what to do. The PBR layer index moved into the second material byte, so 127 material pairs can be addressed.
- **Complete PBR material library**: `cargo run --release --bin gen_pbr_library` generates 28 local, licence-free PBR materials (albedo, normal, roughness, metallic, AO) for all common BIM types and installs them in the user library at `%APPDATA%\ManageBIM\Materials\PBR Library\` (about 26 MB). A portable copy is written to Downloads as `ManageBIM-PBR-Library.bimmat`. Materials with the same name replace the built-in ones, so they are applied by "Auto by IFC type" by default.

## 0.27.20 - 2026-11-23
- **Proper PBR materials**: importing a PBR pack (freepbr.com, ambientCG, Poly Haven - .zip or unzipped folder) now keeps **all six maps** (albedo, normal, roughness, metallic, AO, height). Previously only the colour map was kept; roughness and metallic were reduced to one average value and the normal map was discarded.
- **PBR rendering**: the two packed GPU layers feed a shader with **per-pixel normal mapping, roughness and metallic** (in the Realistic style), and the CPU path tracer uses the same maps. The albedo, normal, roughness, metallic, AO and height images can each be replaced or removed individually from the material editor.
- **Adjustable at last**: sliders for **Normal strength**, **Roughness ×** and **Metallic ×** (0..2) with a reset button; changing them updates the GPU layers immediately. A real **albedo preview** is shown in the material editor instead of just a colour swatch.
- **Auto-assign after import**: a dialog asks where to apply the imported material - the current selection, all visible items, by IFC type, or later. Material names are cleaned ("white-gold-ue-xqucc9" → "White gold").
- **Texture resolution**: new option in Settings › Appearance - **512 / 1024 / 2048 px** for the PBR texture array. At 2048 the layer count is reduced (12) to keep GPU memory under control; at 1024 it is 28, at 512 it is 64. Changing the resolution reloads all textures.
- Fixed: importing a PBR pack added the material but never applied it; the "Texture size" field was the only thing you could adjust.

## 0.27.19 - 2026-11-23
- **Path trace with materials**: the CPU render now uses what you see in the 3D view - assigned materials and colour overrides, image textures and the brick / tile / wood / stone / grass / panel patterns, gloss (sun highlight + real reflections of the scene), metal and **glass** (light and the view pass through, soft tinted shadows). Hidden elements are no longer rendered and the **section is respected**. New "🎨 Render with materials" option in the Path trace window (materials are added automatically by IFC type when none are assigned).
- **Render Image with materials**: the "🎨 Render with materials" option is now also in Render › Image (before it existed only for video) - the PNG is made in the Realistic style with materials, without the blue selection tint, and the 3D view goes back to how it was. The path-trace PNG export adds the `.png` extension when it is missing.
- **Export glTF / GLB** (Export › 3D): geometry, colours and PBR materials as in the 3D view, Model › Level › Element hierarchy, GUIDs and all BIM properties (Blender: Object › Custom Properties) - for Blender, Rhino 8, Twinmotion, Unreal, Enscape, 3ds Max. With a selection only the selected items are exported.
- **glTF import**: reads the Custom Properties / `extras` (Blender, ManageBIM GLB) as element properties, including GUID, type and level; fixed external buffers with encoded names (only `%20` was decoded).
- **Open dialog**: FBX added to the 3D meshes filter, new point cloud filter (E57, LAS), `.ifczip` in the IFC filter. Clear instructions for Blender (.blend), Rhino (.3dm) and SketchUp (.skp) files; the old SketchUp message recommended DAE, which is not supported.
- **Quantities**: the Length of diagonal walls, beams and pipes is measured along the element (it was the X / Y bounding box: a 10 m wall at 45° gave 7.07 m).
- **Stability** (from a full audit):
  - No more crash when a clash test is deleted, or a project is closed / opened, while a test or Update All is running; Duplicate / Delete are blocked while tests run.
  - Viewpoint reports: no crash if a viewpoint is deleted during the export, no endless wait when the window is minimized (stops with a message, the pages made so far are saved).
  - Model loading, video export and path trace can no longer stay "in progress" forever after an internal error (autosave kept off, no new render possible); errors are reported instead.
  - Damaged DWG / DXF overlays no longer close the app when the project opens; corrupt DXF arcs no longer hang it; corrupt `.bimc` caches can no longer allocate gigabytes; E57 files with short colour streams load in grey instead of failing.
  - Scripts: a runaway `loop {}` stops with an error instead of freezing the app; selecting many elements from a script is fast (was quadratic).
  - Projects: camera / clash values that are not numbers are cleaned before saving (the project could not be reopened), task dates and render sizes are validated on open; the video frame queue is bounded (memory with 4K supersampling); the clash sort by distance is safe with invalid values.
  - `crash.log` keeps at most ~1 MB.

## 0.27.18 - 2026-11-23
- **Clash Detective (Navisworks style)**:
  - Results can be **sorted by any column** (click the header: ascending, descending, test order); the choice is remembered.
  - **Item 1 / Item 2 panel** under the results: name, model, type, level, GUID and all properties of both elements, with Select / Zoom / Hide.
  - **Saved viewpoint per clash**: 📷 Save viewpoint stores the camera on the result; selecting it again (and the BCF / Navisworks viewpoint export) uses that view. Kept when the test is re-run.
  - **Animate transitions**: the camera flies smoothly to each clash (can be turned off).
  - **Highlight colours** of Item 1 / Item 2 and the **transparency of the dimmed elements** are configurable (🎨 Colours).
  - The **PDF clash report** now continues on as many pages as needed (it stopped after the first page), with page numbers; long element names no longer overlap the next column.
- **Render: Intel Open Image Denoise** (Apache 2.0, bundled in `oidn\`): the path trace is denoised at the end using albedo and normal buffers - clean images with far fewer samples.
- **Render quality**: soft sun shadows, sky light with ambient occlusion, anti-aliasing per pixel, 16:9 and 4:3 formats (1920 px). Fixed: the sky and the sky light used Y instead of Z as "up", and back-facing surfaces were rendered black.
- **Visual script builder** easier to use:
  - Property and value **drop-down lists from the loaded model**, with the number of elements.
  - **Preview** on each step ("Will select 124 elements", "Will find 37 of 412 without FireRating").
  - **Ready-made scenarios** (📋 Templates), **📂 Open** a saved scenario to edit its steps again, **📁 Browse** for the export files, fields with problems marked in **red** and Run disabled until they are fixed, a plain-language explanation on every step.
  - Fixed: "Select by IDs" selected nothing (Rhai integers are 64-bit) and "Create clash test" failed with a whole-number tolerance such as 10.
- **Status bar**: the length unit menu now offers m / mm / in / ft (same as Settings) and shows an **OSNAP** on/off indicator (click or F3).
- **3D background** choice in Settings › Appearance: Theme, White, Grey, Black, Sky.
- **Interface polish**: thin floating scroll bars, roomier window margins, quicker and wider tooltips, section titles in Settings.

## 0.27.17 - 2026-11-23
- **Coloured ribbon icons**: icons are coloured by what they do (files yellow, export blue, delete / clash red, measure green, view purple, section orange, search teal). Can be turned off in Settings › Appearance.
- **Selection highlight**: colour (picker + 5 presets) and strength (20–100%) configurable in Settings › Selection, saved between sessions.
- **Recent commands**: the command palette shows the last 5 commands used at the top when the search box is empty.
- **Quick UI size** buttons 90 / 100 / 125 / 150% next to the Interface size slider.

## 0.27.16 - 2026-11-23
- **Visual script builder**: editor de scripturi fără cod — adaugi pași dintr-o listă (Select by property, Find missing data, Set property, Create clash, Zoom, Export), completezi câmpuri în GUI, vezi codul Rhai generat, rulezi și salvezi ca `.rhai`. Pașii pot fi reordonați (↑/↓) și șterși.
- **Manual de scripturi în română**: reguli de sintaxă explicate simplu + toate cele 9 funcții cu descriere, exemplu și efect în aplicație — direct în fereastra Script Library.

## 0.27.15 - 2026-11-23
- **Tab Future înlăturat** (era colorare/scale, nu modelare); markupuri profesionale (paletă BIM, greutăți, Undo/Clear), View Template legat de viewpoint, split-view cu toolbar unic, duplicaturi din ribbon eliminate, web viewer restilizat.

## 0.27.14 - 2026-11-23
- **Tab „Future"**: colorare non-distructivă a elementelor pe tip IFC (Walls, Columns, Slabs, Beams, Doors, Windows, Roofs, MEP) cu color picker; buton „Clear future". Modelul sursă nu se modifică niciodată.
- **Modelare stil SketchUp**: scale per-element pe axe (Thickness X/Y, Height Z, factor slider, Reset) — non-distructiv, salvat în proiect.
- **View Templates — rename cu mouse-ul**: buton ✏ inline în dropdown-ul de lângă ViewCube.
- **Add-ins profesioniste**: Revit „Send selection" (comandă nouă, exportă doar elementele selectate); log local `addin.log` în toate cele 3 add-in-uri (Revit / Navisworks / AutoCAD); toate add-in-urile recompilate (Revit 2025, Navisworks 2022–2026, AutoCAD R24/R25).
- **Tab Future scos** (nu era modelare, doar colorare/scale — înșelător); markupurile devin profesionale: paletă standard BIM (7 culori), greutăți Fine/Medium/Heavy, Undo/Clear per viewpoint.
- **View Template legat de viewpoint**: la salvare se memorizează stilul activ, iar la restaurarea viewpoint-ului se reaplică.
- **Split-view toolbar**: bara de navigare e un singur Area global, nu se mai dublează; operează pe jumătatea activă.
- **Ribbon fără duplicaturi**: exporturile (DWG/PDF/Title Block/Overlay/Preview) sunt doar în Annotation; Grids/Levels doar în Grids & Levels.
- **Web viewer**: paletă și colțuri rotunjite consecvente cu aplicația.

## 0.27.13 - 2026-11-23
- **UI tuning (stil Mitcad)**: colțuri rotunjite centralizate în `theme.rs` (`RADIUS_SM/MD/LG`) — butoane ribbon cu contur fin pe hover/activ, grupuri de ribbon cu border, tab-uri tip „pill", panouri dock rotunjite, ferestre/menu consecvente. Fără rescriere, fără schimbare funcțională.
- **Split-view reparat**: lista de geometrie e acum per-viewport (capturată în callback), nu partajată în renderer — nu mai dispar elemente din prima jumătate când miști camera în cealaltă. Modelul rămâne complet în ambele panouri.
- **PDF Clash Report**: buton „PDF" în Clash Reports → raport A4 landscape cu antet, tabel de rezultate (status, severitate, elemente, distanță) și cartuș, folosind același scriitor PDF ca plansele.
- **View Templates — preset complet de lucru**: salvează camera 3D, vizibilitatea, secțiunea, stilul vizual, culorile/overrides, selecția, măsurătorile, filtrul de viewpoint, grilele/nivelurile și o notă. Navigare rapidă: selector lângă ViewCube, butoane ◀ ▶ (wrap-around), **Ctrl+1…9** pentru salt direct, template **★ default** aplicat automat la deschiderea proiectului, buton **⧉ Duplicate**. Se salvează în `.bimr`/`.bimd` (compatibil cu proiectele vechi).
- **Clash — bug fix**: adăugat `SelKind::Selection` — poți alege „Current selection" ca Selection A sau B, deci mergi direct cu elementele selectate în model (înainte nu exista opțiunea și elementele păreau ascunse).
- **Materiale de randare profesioniste**: valori PBR realiste (luciu/metalicitate) pentru beton, oțel, aluminiu, sticlă, lemn, cupru, gresie și apă.
- **Installer & startpage**: paleta prototipului (fundal închis, accent albastru/cyan, carduri cu border), etape în română, buton Finish evidențiat; startpage cu carduri rotunjite consecvente.

## 0.27.12 - 2026-11-23
- **OSNAP îmbunătățit**: tipuri selectabile (Vertex / Midpoint / Edge / Surface), rază reglabilă 6–24 px și **F3** pentru on/off — în Settings și direct în viewport.
- **Unități reale mm / m / in / ft** cu precizie configurabilă (0–6 zecimale); înlocuiește vechiul comutator mm, cu migrare automată a setărilor vechi.
- **Split-view**: două jumătăți în aceeași fereastră (View › Compare), separator trasabil (stretch), cameră + secțiune proprii, link/unlink camere.
- **Keybindings customizabile**: editor în Settings (click → taste → Enter), detectare conflicte, reset per comandă / total.
- **Export în lot**: buton „Batch" în Viewpoints → dosar destinație → PDF + HTML + Excel + BCF, exportate secvențial.
- **Profil de setări partajabil**: `.mbprofile` cu temă, panouri, scări, unități, keybindings; Import/Export în Settings (căile locale nu se preiau).
- **Panel Viewpoints profesionist**: toolbar cu carduri compacte și glife Phosphor, badge colorat pentru status, butoare de acțiune stilizate.

## 0.27.11 - 2026-11-23
- **Tab nou „Annotation"** (imediat după Sections, ușor de găsit): grupuri *Cut & Export* (Cut to DWG, Cut on Sheet, Cut to PDF, 2D Preview), *Cut View* și *Annotations* (Title Block, Grids, Levels, Filled cuts, DWG Overlay). Comenzile sunt și în Ctrl+K sub categoria „Annotation".
- **Secțiunea din cub**: eliminat butonul „Behind edges" — tot ce e în cub de la planul de tăiere încolo se vede direct ca geometrie 3D (shaderul taie la cub și desenează capacul hașurat pe plan); nu se mai desenează liniile aplatizate „preview 2D în 3D".
- **Fereastră mică „2D Section Preview"** de tip minimap, închizabilă, cu liniile reale ale tăieturii înainte de export.
- **Vederea Hidden line stil Rhino 3D**: hârtie albă, suprafețe albe cu umbrire fină și **doar conturul general negru** (fără muchii interne / crease); fundalul viewport-ului devine alb în acest stil.
- **Preview 2D refăcut**: fereastră **fixă, proporțională** (470×340, ~A4 landscape), fundal alb, cu hașură ANSI31, cote în lanț, niveluri, grile și cartuș — ca exportul. Nu se mai întinde pe toată aplicația.
- **Butoane separate în Annotation / Settings** pentru **Dimensions** (cote) și **Levels** (niveluri) în exportul DWG/PDF — le poți debifa pe ce nu vrei.
- **Highlight plan**: rămâne **închis by default** (resetat la fiecare pornire, chiar dacă a fost aprins în sesiunea anterioară) și nu se mai aprinde automat când alegi o față; îl deschizi doar când ai nevoie (buton în Annotation / Settings).
- **Desen tehnic profesional (stil Inventor)**: previewul și exportul DWG/PDF folosesc acum **aceeași sursă** (`section_drawing`) — nu mai există discrepanță. Fără linii „din spate" (BEHIND) nicăieri. Hașura pe materialele tăiate e activă implicit, cu buton de debifare. Straturile DXF au culori coerente (negru pentru tăietură, gri pentru hașură/niveluri/grile, albastru pentru cote) în loc de culori aleatoare.
- **Scara 1:1**: desenul exportat rămâne la coordonate reale (metri) pe plansa cu cartuș; doar cartușul/chenarul sunt în mm de hartie.

## 0.27.10 - 2026-11-23
- **Panoul Viewpoints restructurat**: mâner de drag separat de etichetă — clickul și dublu-clickul (rename) pe nume funcționează sigur, dragul mută în folders.
- **Desen tehnic în 3D**: după ce setezi cubul și planul, muchiile tăieturii (intersecția triunghiurilor cu planul, strict în cub) se calculează și se desenează **solid cian**; ce e în spate se desenează **întrerupt (dashed)** — același limbaj ca desenul din DWG.
- **Cote în lanț în DWG**: sectiunile verticale au acum cote între niveluri consecutive și cote orizontale între axele de grilă (linie de cota + linii ajutătoare + bifa 45° + text), ca într-o secțiune arhitecturală; hașurile, nivelurile, grilele și etichetele rămân neschimbate.
- **Butonul Behind edges curățat**: eliminat afișajul vechi `0%` (poziția planului rămânease blocată și părea parte din buton); funcția Behind edges rămâne activă și desenează corect liniile întrerupte.
- **Vederea Hidden line corectată**: stilul „Hidden line" (View › visual style) randază acum **doar muchiile** — fundal închis cu linii luminoase (contur + crease), fără suprafețe umbrite, ca în AutoCAD / desenul tehnic.

## 0.27.9 - 2026-11-23
- **Viewpoints: clickul funcționează** — `dnd_drag_source` Returna un `Response` cu sens de drag, deci `clicked()` nu se declanșa; acum se folosește `Response`-ul etichetei → click pe viewpoint = go to 3D, dublu-click / click dreapta = rename.
- **Muchiile din spate se VĂD** imediat ce cubul e activ (nu mai depind de highlightul planului, care e inchis implicit); linii punctate albastru, cu contrast crescut.
- **Exportul din spate e complet**: toată geometria din cub (inclusiv elementele ascunse) și muchiile care traversează planul (partea din spate), proiectate pe plan — **strict numai ce e în interiorul cubului** (verificare pe toate 3 axe).
- Prototipul de restilizare UI aprobat e în `docs/prototype/design-preview-actual.html` și intră în planul 0.28.

## 0.27.8 - 2026-11-23
- **Viewpoints ca în Navisworks**: click pe un viewpoint → camera și secțiunea se restaurează imediat în 3D; **click dreapta › Rename** (sau dublu-click) → editare inline, Enter confirmă / Esc anulează.
- **Geometria din spatele tăieturii**: între planul de tăiere și peretele opus al cubului, muchiile reale ale elementelor se **proiectează pe plan și se desenează punctat** în 3D; în DWG/DXF se scriu pe stratul **BEHIND** cu linetipul **HIDDEN** (aplatizate pe planul de tăiere).
- Letățurile adiționale: **Hide section box** (ascunde doar cubul, secțiunea rămâne) și **Behind edges** (pornit implicit), ambele în grupul Cut View.
- **Bibliotecă de scripturi cu interfață grafică**: fereastra Script arată acum 9 scripturi grupate pe categorii, cu **descriere**, buton **▶ Run** și **parametri editabili** (ex. categorie, nivel, toleranță, cale BCF) — nu mai trebuie scris cod Rhai manual. Secțiunea „My scripts" listează fișierele `.rhai` cu Run/Delete.
- **Folder de scripturi alegibil din UI**: buton **📁 Choose folder…** (dialog nativ) + **↺ Default** + buton copiere cale, direct în fereastra Script.

## 0.27.7 - 2026-11-22
- **Overlay DWG la locul tăieturii**: desenul exportat prin „Cut to DWG" revine **exact în planul și la coordonatele de unde a fost extras** (metadate citite din DXF / sidecar). Suport de **plan vertical** (Front/Back → YZ, Left/Right → XZ) și oglindire inversată. Din fereastra DWG Overlay a fost eliminat câmpul „Level" — poziția se păstrează, nu se mai atribuie un nivel.
- **Secțiunea limitată la cub**: exportul DWG/PDF conține **doar ce e în interiorul section box-ului** și numai elementele care prind cubul + planul; segmentele se taie **exact la peretele cubului** (clipping parametric), nu doar se resping.
- **Viewpoint-ul păstrează planul de tăiere**: se salvează/restaurează Cut View (Top/Front/…) și poziția lui, ca la viewpoint-urile Navisworks.
- **Hide section box**: opțiune nouă care ascunde doar sârma cubului și săgețile fetelor în 3D — **secțiunea și exporturile rămân active**.
- **Highlight plan implicit oprit**: se aprinde automat când alegi o față în Cut View.
- **OSNAP-urile și unitățile mm/m/in/ft** rămân în planul 0.28 (nu au fost incluse aici ca să păstrăm stabilitatea).

## 0.27.5 - 2026-11-22
- **Tăietura aliniată cu grila**: planul de tăiere (evidențiere + export DWG/PDF) se fixează automat pe **axa de grila / nivelul cel mai apropiat** — Top/Bottom pe niveluri, Front/Back/Left/Right pe liniile de grilă. Muting grila, planul o urmărește; se poate dezactiva din Settings („Snap the cut plane to grids / levels"). Înălțimea în cubul de secțiune rămâne reglabilă („Cut height in box").
- **Cubul de secțiune e format din planuri**: alegând **Top / Bottom / Front / Back / Left / Right** în meniul Cut View, planul portocaliu stă **exact pe fața respectivă a section box-ului** (0% = pe față) și **o urmează când tragi fața cu săgeata**. Îl poți muta și direct în 3D cu mânerul cu săgeți din centru, iar **exportul DWG / PDF taie exact acolo**. Fără cub activ, vederile se aliniaza la grile/niveluri. În modul Current, planul e la cota reglabilă din cub („Cut height in box").
- **Path trace până la 4096×4096** (opțiuni 2048 și 4096 noi), pe toate nucleele (`rayon`).
- **Timerul se oprește la „Ready"**: secundele rămân înghețate la valoarea de la terminare, nu mai cresc la infinit.
- **Luminile path trace**: ambient pe emisferă (cer deasupra, sol dedesubt — nu mai e gri plat), penumbră moale (0.35 în loc de negru total) și intensitatea din Lighting (Sun power).
- **Video mai rapid la rezoluții mari**: la 4K se dezactivează automat supersampling-ul (ar fi randat la 7680×4320) și checkbox-ul explică costul.
- **Fontul și înălțimea textului, din Settings**: alegi **Inter**, **Lato** (nou, inclus, SIL OFL), fontul implicit egui sau **orice font din Windows** (listă cu filtrare); plus **înălțimea textului 10–26 px**. Se aplică imediat, fără repornire.
- **Tăietura din cubul de secțiune**: planul evidențiat în 3D ȘI exportul DWG/PDF folosesc aceeași înălțime, reglabilă în panoul Sections („Cut height in box"). Când tragi fețele Z ale cubului, secțiunea se mută odată cu el.
- **Path trace pe toate nucleele**: pixelii unui sample sunt împărțiți prin `rayon` — de ~6–8× mai rapid pe procesoarele obișnuite (era un singur fir de execuție).
- **Export BCF 3.0**: în Settings › Export alegi 2.1 sau 3.0; la 3.0 comentariile stau în interiorul `<Topic>`, conform specificației (BIMcollab / Solibri noi).
- **Export COBie 2.4**: buton nou în tabul Export („COBie…") — registru xlsx cu Contact, Facility, Floor, Type, Component, System, pentru predarea către facility management.
- **Installer mai prietenos**: pagina de bun venit are carduri colorate cu recomandările aplicației (Clash, review federat, scripturi, path trace) și pagina de final are „Getting started"; fereastra e mai mare și defilabilă. Rămâne **per-utilizator, fără drepturi de administrator**.
- **Verificări**: 156 de teste trec, `cargo check` curat pe aplicație și pe installer.

## 0.27.1 - 2026-11-22
- **Motor Rhai activat**: scripturile `.rhai` din consola Script chiar se executa. API expus: `select_where`, `select_ids`, `selected_count`, `find_where_empty`, `set_prop`, `create_clash_test`, `zoom_to_selection`, `export_bcf`, `export_excel`, `log`. Butoanele **▶ Run** / **Run all** sunt functionale; actiunile se aplica in ordinea inregistrata (selectie, proprietati, teste clash, zoom). 3 teste unitare noi.
- **Fix path trace**: bucla de repaint nu mai continua dupa terminare (aplicatia nu mai „merge" continuu); preview-ul foloseste acum acelasi spatiu de coordonate ca rendererul (nu doar cer); ETA + anunt de finalizare; textele EN/RO.
- **Planuri de taiere opuse**: Front/Back si Left/Right dau acum planuri DIFERITE (distanta fata de latura aleasa, 25% implicit); la fel Top/Bottom.
- **Highlight doar in Section Box**: planul portocaliu de taiere apare doar cand sectiunea e in modul **Box**, nu permanent in vizualizare.
- **Fix overlay DWG**: buton **🎯** in fereastra de overlay pentru zoom la desen (daca nu se vede la cota curenta).
- **Fix clipire UI size**: schimbarea font/UI size se aplica la eliberarea sliderului, nu la fiecare pas de tractiune.
- **Roadmap 0.28 notat** in `docs/ROADMAP_0.28.md` (BIM complet + functionalitati + performanta, fara instancing).
- **Mărimea interfeței / fonturi (DPI)**: setare nouă în **View › Options › Interface size** (75%–200%, pas de 5%), salvată în `settings.json` și aplicată întregii interfețe prin `egui::Context::set_zoom_factor` — texte, tabele, panouri și ferestre se scalează împreună. O valoare invalidă dintr-un `settings.json` stricat revine la 100%.
- **Panouri și ferestre mai vizibile**: contur subtil de 1 px, specific fiecărei teme (`theme::border()`), aplicat panourilor dockuite și tuturor ferestrelor plutitoare — ies în evidență fără să pară grele.
- **Coloane trăsăbile (stretch) în tabele**: componenta comună `src/ui/table.rs` folosește `TableBuilder::resizable(true)` + `Column::initial` în loc de `Column::exact` (care bloca lățimea). Aplicată în tabelul de teste Clash Detective și în TimeLiner; tabelul de rezultate Clash avea deja suport.
- **Redenumire cu mouse-ul**: clic dreapta › **✏ Rename** pe un rând din Clash Detective sau TimeLiner → caseta de nume primește focus automat; Enter / clic în afară confirmă. Crearea rămâne prin **➕ Add Test**, respectiv **➕ Task**.
- **Fereastră Settings dedicată**: butonul **⚙ Settings** din ribbon (și File › Settings, sau Ctrl+K „settings") deschide o fereastră cu toate setările aplicației: aspect (temă, mărime interfață / fonturi, limbă), export DWG, navigare, fișiere și Live Link. Controlul de mărime e preluat și în tabul View.
- **Meniu Script**: tab nou în ribbon cu **Console / New Script / Folder / API Help**. Consola listează fișierele `.rhai` din `%APPDATA%\ManageBIM\Scripts`, creează șabloane și copiază calea dosarului. Motorul de execuție Rhai urmează în 0.28.
- **Secțiuni DWG / DXF / PDF din Front / Back / Left / Right**: în Settings › Export alege direcția și poziția exactă a planului de tăiere (0–100% din bounding box); funcționează și fără secțiune activă. Anotațiile (hatch, etichete, axe, niveluri) se pot dezactiva; pentru Back / Right axa se inversează ca în vederea corespunzătoare. Butonul **Cut to PDF** scrie aceeași tăietură ca desen vectorial pe o plansă A3–A0, cu cartuș, la poziția exactă a anotatiilor din DWG.
- **Fix path trace**: butonul ▶ Start nu mai pornea după o randare terminată (job-ul rămânea în memorie și era considerat „running"); acum pornește o randare nouă, iar firul UI nu se mai blochează la rezolutii mari (`try_lock`). Feedback nou: „Preparing scene / first sample".
- **Fix suprapunere ribbon**: bara cu search / Model focus / temă / Setări / numele proiectului a fost mutată într-un **panel separat deasupra ribbonului**; taburile au acum toată lățimea și nu se mai suprapun.
- **Meniu Cut View** în Sections: alegi **Current / Front / Back / Left / Right / Top / Bottom**, cu **evidențiere vizuală** a planului de tăiere în model (suprafață portocalie translucidă + contur); poziția e reglabilă 0–100%.
- **Import DWG la poziția tăieturii**: fișierul exportat prin „Cut to DWG" conține metadatele planului (comentariu DXF + sidecar `.mbplane`); la import, desenul 2D este repus automat în planul 3D exact din care a fost extras (cu origin și oglindire corecte).
- **Foldere Viewpoints**: redenumirea se confirmă și la click în afară (nu doar Enter); rădăcina „No folder" are meniu (folder nou, redenumire etichetă), iar eticheta e configurabilă în Settings.
- **Scripturi**: folderul cu `.rhai` e configurabil în Settings; consola are buton **Install examples**, iar în `examples/scripts/` sunt 3 scripturi exemplu (selectare, marcare date lipsă, set de teste clash).
- **Path trace complet**: ETA (timp rămas) în progress bar, anunț clar la finalizare, text EN/RO și corectarea spațiului de coordonate (geometria era translată cu `origin` pe când camera era locală → imaginea arăta doar cerul).
- **Instalator 0.27.1 complet**: include runtime-ul OpenCascade (STEP / IGES / BREP / FCStd) și DLL-urile add-in-urilor Revit 2023–2026, AutoCAD R24/R25 și Navisworks 2022–2026.
- **Documentație**: textul învechit care anunța instalatorul 0.25.5 a fost eliminat din README.
- **Verificări**: 148 de teste trec (28 unitare + 120 integrare), 0 eșuate; `cargo check --all-targets` fără erori.

## 0.27.0 - 2026-11-21
- **Build-ul add-in-urilor reparat**: `cargo build --release --features addins` functionează din nou. Cele trei instalere (`ManageBIM-Revit-AddIn.exe`, `ManageBIM-AutoCAD-AddIn.exe`, `ManageBIM-Navisworks-AddIn.exe`) au sursele în `src/bin/addin/` și includ automat DLL-urile din `installers\payload\`; dacă lipseau DLL-urile, primesți un mesaj clar cu scriptul C# de rulat. Logica de instalare e în `src/addin_install.rs`, cu 5 teste noi (Revit 2023–2027, 2027 folosește DLL-ul de 2026, AutoCAD R24/R25, Navisworks vechi/nou, dezinstalare).
- **Monoliții sparți în module**: `ui.rs` (10.311 → ~4.600 linii) și `app.rs` (8.811 → ~2.700 linii) au acum 7, respectiv 16 submodule (`src/ui/`, `src/app/`): coordination, tree_props, time, tools, analysis, clash, grids, project, anim, materials etc. Nicio funcționalitate mutată — doar organizare.
- **Interfață mai prietenoasă**: pagina de start are logo isometric, buton Open evidențiat, carduri „Welcome” cu acțiuni rapide (Open / Demo / Commands) și sublinierea tab-ului activ în ribbon.
- **Corecții**: pachet E57 trunchiat → eroare clară în loc de panic; sincronizarea issue-urilor partajate folosește fișiere temporare unice per proces (fără coliziuni între colegi); GUID-urile invalide din folderul comun sunt ignorate; fișierul `.json.tmp` rămas e curățat; backtrace-ul din `crash.log` păstrează simbolurile (`strip = "debuginfo"`); remove duplicate branches în UI și importul nefolosit.
- **Clash Detective spacios**: buton **⛶ Focus** în panoul de jos (și în meniul contextual) deschide Clash Detective într-o fereastră aproape cât ecranul; Esc sau „Exit focus” revine la model. Panoul de jos are și buton **⤢** de maximizare, iar varianta plutitoare pornește la 1100×760.
- **Fonturi mai vizibile**: Inter cu mărimi mărite (Small 11, Body/Button 14, Heading 19, Mono 13) și ținte de click mai mari.
- **OpenCascade inclus**: scriptul `tools\build-occt.ps1` găsește CMake / Ninja instalați per-user (winget) și compilează toolkit-urile necesare; STEP / IGES / BREP native funcționează.
- **DLL-uri add-in construite**: Revit 2023–2026, AutoCAD R24/R25, Navisworks 2022–2026; scripturile C# folosesc și compilatorul Roslyn din .NET SDK-ul per-user (cu referință `mscorlib` pentru .NET Framework).
- **Test Navisworks clash lifecycle**: verificat New → Active → Resolved → New și păstrarea statusului Approved la re-rulare (ca în documentația Autodesk).
- **Clash Detective — paritate Navisworks**: Auto-group după Model A/B, Assigned To și Status; **Filter by Selection** (None / Inclusive / Exclusive); statusul **Approved / Resolved** salvează cine și când (`approved_by/at`, `resolved_by/at`, cu tooltip în tabel); **⇄ SwitchBack** trimite clash-ul în Revit / Navisworks ca viewpoint cu selecția celor două elemente.
- **Scan vs Model**: diferențele as-built (Missing / Partial) pot deveni test de clash cu norul de puncte; coloana „Item B” arată „Point cloud (scan)”.
- **4D**: opțiunea „Show only scheduled elements” ascunde tot ce nu e legat de o sarcină — vezi doar ce se construiește.
- **Visual style „Hidden line”**: suprafețe neutre, muchii ascunse de geometrie (ca în AutoCAD / Navisworks).
- **Ferestre plutitoare**: buton **⤢ Stretch to screen** / **⤡ Restore size**; redimensionare liberă pe margini și colțuri.
- **Persistență UI**: filtrul, tabul, gruparea, modul de afișare, auto-zoomul și setarea 4D se salvează în `settings.json`.
- **Properties mai dens**: carduri cu padding redus, valori pe o linie cu tooltip complet, antet compact — încap mult mai multe informații.
- **Tema Nord mai inchisa** cu acelasi contrast al textului, apropiata de aspectul din captura ta.
- **Path tracing progresiv (CPU)**: buton „Path trace image…” în Render, cu rezoluție, samples, limită de timp, anulare și export PNG.
- **Cut to DWG**: anotațiile se plasează doar deasupra elementelor tăiate, cu gap, fără ce e dedesubt.
- **Visual style „Hidden line”** adăugat și în ribbon (Visual Style).
- **Teste**: 148 de teste trec (28 unitare + 120 integrare), 0 eșuate.

## 0.25.5 - 2026-11-21
- **Clash Detective ca în Navisworks**: lista testelor cu coloanele Name, Status (New / Done / Old), Clashes, New, Active, Reviewed, Approved, Resolved; butoanele **Add Test, Reset All, Compact All, Delete All, Update All**; tab-urile **Rules / Select / Results / Report**; meniu contextual pe test (Run, Reset, Compact, Duplicate, Delete).
- **Update All**: rulează toate testele și arată sumarul: ce e nou, ce e încă activ, ce s-a rezolvat, pe fiecare test, cu diferența față de rularea anterioară.
- **Import / Export Clash Tests**: XML compatibil Navisworks (Clash Detective › Import / Export), cu seturile de selecție folosite (Model / Type / Discipline devin search sets); CSV / Excel, un rând pe test, de editat și reimportat. Testele cu același nume se actualizează, rezultatele rămân.
- **Clash Matrix pe teste**: Selection A pe rânduri, Selection B pe coloane, în fiecare celulă testele perechii cu stările lor (Open, Total, New, Active, Reviewed, Approved, Resolved); click pe celulă deschide testul. Excel: foaia Matrix, câte o foaie pe stare și lista testelor. **Matricea de seturi**: bifezi perechile și se creează testele.
- **Date pe obiecte ca în Navisworks**: în Properties, butonul **Custom** › Add / Delete / Rename User Data Tab și Insert New Property; proprietățile și valorile se editează direct în tabel și se scriu pe toate elementele selectate (și pe geometrie fără date: STEP, DWG, OBJ). Se salvează în proiect și apar în Find, Profiler, Model Checker și exportul IFC.
- **Import proprietăți din Excel / CSV**: alegi coloana cheie și după ce se potrivește (GlobalId, Name sau orice proprietate); fiecare altă coloană devine o proprietate. **Export Properties** face șablonul Excel (un rând pe element, cu cheia) de completat și reimportat.
- Grupul **Data** din ribbon: Add Property, Import Properties, Export Properties, Sets ⏷ și Schedule ⏷ (meniuri în locul butoanelor duble).
- **Draw Path reparat**: traseul nu mai urcă metri deasupra podelei între puncte la înălțimi diferite (curba e lină doar în plan).
- Bara din vederea 3D are lățime fixă: butoanele nu se mai mută sub mouse când treci în Walk; Path și Play sunt mereu pe bară.
- **Fereastră separată Animations** (Render › Animations), despărțită de Render / Materials: lista animațiilor cu **▶ Play pe fiecare** (Record, Orbit 360°, Viewpoints, Walk path, 4D), playerul (început, keyframe anterior, înapoi, pauză, stop, play, keyframe următor, sfârșit, bara de timp, Loop, Ping-pong) și **Export video** cu rezumatul a ce se exportă. Editarea keyframe-urilor și animația obiectelor (culoare, transparență, vizibilitate) sunt ascunse sub „Edit keyframes (advanced)”.
- **Import 4D**: secvența de construcție din TimeLiner devine animație (~1 s pe săptămână de șantier, din vederea curentă); fără planificare, întâi se alege fișierul (CSV, MS Project XML, Primavera P6 XML).
- **4D reparat**: sarcinile TimeLiner importate înainte ca modelele să se încarce (ex. proiectul demo) se leagă acum automat de elemente - simularea 4D arată din nou construcția.
- **Record**: înregistrează navigarea în timp real ca animație nouă.
- **Render with materials**: videoclipul se exportă în stilul Realistic (materiale, texturi, reflexii, umbre), iar vederea revine apoi la stilul tău.
- Render: tab-ul Image (setări și Render Image) + Materials.
- Sursa **bimc-convert** (`src/bin/bimc_convert.rs`) e acum în proiect: proiectul se compilează pe orice calculator și pe GitHub Actions.

## 0.24.0 - 2026-11-20
- **Compare pe parametri**: lista parametrilor schimbați între două versiuni ale modelului, cu numărul de elemente; click pe un parametru = selectează elementele și arată valoarea veche → nouă. Raport CSV (GUID, element, tip, parametru, vechi, nou).
- **Verificări rapide fără IDS** (Model Checker › ⚡ Quick checks): elemente fără nivel, fără nume, fără clasificare, fără material, fără geometrie, geometrie degenerată, duplicate (același tip și aceeași geometrie în același loc), GUID dublat. Select pe fiecare verificare și raport CSV.
- **Colorare după parametru cu legendă**: legenda Appearance Profiler apare în vederea 3D și se desenează pe imaginile și videoclipurile exportate.
- **Umbre după poziția soarelui**: umbre aruncate (shadow map, margini moi) în vedere, imagini și video; soarele după dată, oră, latitudine / longitudine, fus orar și nordul proiectului (Lighting › „From date, time and location”), cu scurtături pentru 21 martie / iunie / decembrie.
- **Import planificare din MS Project (XML) și Primavera P6 (XML)**, pe lângă CSV. Legarea de elemente: `[Level=Level 1;Type=IfcSlab]` la sfârșitul numelui activității (sau câmpul Text1 în MS Project); activitățile sumar se sar.
- **Video 4D cu dată și legendă**: pe fiecare cadru apare data simulării, progresul și activitățile în lucru, colorate ca în simulare (Render › „4D date & legend”).
- **Clash pe niveluri de gravitate**: Critical / Major / Minor după pătrundere (praguri pe test) sau distanță (clearance), afișate în listă și în rapoartele HTML / CSV. **Șabloane de teste** (📋): Structură vs HVAC / Instalații / Electrice, Arhitectură vs HVAC, distanțe minime între instalații, duplicate. Selecție nouă „Discipline” (Structure, Architecture, MEP...) recunoscută automat.
- **Parametri adăugați pe elemente** (Properties › „Add parameter”): pentru elemente fără date (DWG, OBJ, STEP...) sau informații de coordonare; se salvează în proiect, apar în Find, Profiler, Model Checker și exportul IFC (grupul „User”).
- **Test pe scene mari**: 382 500 elemente / 5,9 milioane triunghiuri — construire 0,2 s, clash 0,05 s, verificări rapide 1 s, ~24 FPS cu umbre; la scene de peste 2 milioane de triunghiuri umbrele se opresc cât timp camera se mișcă.

## 0.23.0 - 2026-11-20
- **Planuri 2D complete din secțiuni** (Cut to DWG / Cut on Sheet): hașură ANSI31 pe pereți, stâlpi, plăci, grinzi tăiate; etichete (numărul ușilor și ferestrelor, tipul pereților) fără suprapuneri; axe cu bule și lanțuri de cote între axe plus cota totală; pe secțiuni verticale: liniile de nivel cu cotă și axele tăiate. Mărimile textelor sunt în mm de hârtie, după scara planșei. Se poate opri din Title Block („Complete 2D drawing”).
- **Interfața în limba română** (EN / RO): View › Language, meniul File sau Ctrl+K „Language”. Se traduc filele, grupele și butoanele ribbon-ului, tooltip-urile frecvente, titlurile panourilor și meniul File; alegerea se salvează.
- **Stilul Realistic fotorealist**: reflexii ale cerului cu Fresnel, sticlă care reflectă la unghiuri razante, tone mapping filmic (ACES), materiale atribuite automat după tipul IFC când nu există atribuiri.
- Path tracing (randare fotorealistă statică) mutat în versiunea următoare.

## 0.22.0 - 2026-11-20

- **IFC mai bun**: golurile (usi, ferestre, treceri - IfcRelVoidsElement) sunt **taiate din pereti si placi**; **sistemele / zonele** (IfcSystem, IfcZone) ca proprietati "Systems"; **clasificarile** (Uniclass, OmniClass... - IfcClassificationReference) in grupul "Classification"; **georeferentierea** (IfcMapConversion: E, N, H, rotatie, CRS) in informatiile modelului.
- **Model Checker**: **export IDS 1.0** (buildingSMART) - dus-intors exact cu importul; cerinta noua "unique" (valori duplicate: Mark, GUID...).
- **Clash inteligent**: conexiunile normale (perete-placa, grinda-stalp, teava-suport, izolatie...) ignorate pana la o patrundere aleasa; **responsabil automat** dupa disciplina (structura > arhitectura > HVAC > sanitare > electrice); **grupare pe sistem**.
- **Foldere pentru viewpoints / markup-uri**: tragi viewpoint-urile in foldere (si subfoldere), redenumesti, exporti doar un folder (BCF, PDF, Excel, HTML, Navisworks XML); folderele trec in Navisworks (XML si add-in-ul Issues).
- **Undo / Redo al vizualizarii** (Ctrl+Z / Ctrl+Y, butoane in bara de jos): selectie, elemente ascunse, sectiune, camera.
- **Suprapunere DWG / DXF 2D** peste model (Sections › DWG Overlay): linii, polilinii, arce, cercuri, blocuri; la cota unui nivel, cu deplasare, rotatie, culoare; salvata in proiect.
- **Versiunile Autodesk**: NWD / NWC / RVT se deschid cu versiunea Navisworks suportata (2026 -> 2022), cu trecere automata la urmatoarea daca una esueaza si alegerea versiunii in setari; add-in-urile se instaleaza doar in versiunile suportate (Revit 2023-2026, AutoCAD / Plant 3D 2024-2026).
- **Stabilitate**: proiectul si setarile se salveaza sigur (fisier temporar + copie .bak, recuperare automata din .bak); blocarile interne nu mai pot opri aplicatia in bucla dupa o eroare.
- Manerele section box-ului mai mari si prinse pe toata sageata; chenarul de randare debifat implicit (Render › Show frame).
- **Verificare generala automata**: proiectul demo trece prin toate functiile principale (selectie, Undo, clash, Model Checker, IDS, foldere, exporturi IFC / web / DWG, salvare si redeschidere).

## 0.21.0 - 2026-11-20

- **Linux si FreeBSD**: aplicatia compileaza pentru Linux (x86-64, X11 / Wayland) si FreeBSD; folderele de date dupa standardul XDG (`~/.config/ManageBIM`, `~/.local/share/ManageBIM`), dialoguri de fisiere prin xdg-desktop-portal (cu camp text ca rezerva), deschiderea fisierelor cu `xdg-open`. Pachete: **AppImage** (Linux, un singur fisier, fara root) si arhiva `.tar.gz` (FreeBSD). Add-in-urile si cititorul STEP nativ raman doar pe Windows.
- **GitHub Actions**: la fiecare push se compileaza si se testeaza pe Windows, Linux si FreeBSD; un tag `v*` publica pachetele in Releases.
- Programele de instalare a add-in-urilor se construiesc doar cu `--features addins` (scriptul de Setup), ca proiectul sa compileze dintr-un clone curat.
- **Export DWG verificat**: recitit si comparat cu sursa (layere, linii, texte, unitati, coordonate). Corectat: layerul textelor din cartus (`SHEET_TEXT`) nu era definit.
- Pagina de start cu iconite vectoriale; **README** nou, cu capturi actuale.

## 0.20.1 - 2026-11-20

- **DWG fara ODA File Converter**: exportul DWG (taietura sectiunii, planse, axe) foloseste scriitorul DWG integrat (format AutoCAD 2018); fisierul scris e recitit ca verificare. ODA ramane doar ca rezerva; DXF-ul intermediar se sterge.

## 0.20.0 - 2026-11-20

- **Paleta de comenzi Ctrl+K** (si campul „Search or Ctrl+K” din dreapta sus): peste 70 de comenzi cu cautare aproximativa (si cuvinte in romana: „sectiune”, „planse”, „cartus”...), navigare cu sagetile + Enter; scrii numele unui element si il selecteaza cu zoom.
- **Stiluri de afisare** (View / Render › Visual Style si Ctrl+K): **Realistic** (materiale, texturi), **Shaded** (culorile din fisier), **Concept** (macheta alba, mata, margini accentuate), **Wireframe**; stilul ramane salvat.
- **Cut to DWG**: implicit **2D in Model space** la coordonatele reale; nou **Cut on Sheet** - pe plansa **A3 / A2 / A1 / A0**, la scara automata sau fixa (1:20 - 1:5000), cu chenar si cartus.
- **Editorul de cartus** (Sections / Viewpoint › Title Block): firma, proiect, beneficiar, desenat, verificat, prefixul numarului de plansa, revizie, note, formatul foii si scara - salvat in proiect, folosit de plansele PDF si de DWG. Plansele PDF respecta formatul ales.
- **Manual integrat** (Help › Manual): 14 capitole cu cautare. **Proiect demo** (Help › Demo, si pe pagina de start): Block A cu arhitectura + structura, instalatii si planificarea 4D.
- **Model Checker**: cerinta noua **„unique”** - valoarea trebuie sa fie unica (Mark, numar de usa, GlobalId); duplicatele apar ca erori.
- **Viewer web**: **sectiune pe inaltime** (glisor) si **masurare** intre doua puncte (tastele C si M).
- Verificare completa a codului (clippy: corectitudine, cod suspect, performanta) si corecturi; cautarea din paleta e memorata; Find Items fara butoane dublate; iconita vectoriala pentru bara de navigare.

## 0.19.0 - 2026-11-20

- **Interfata**: font **Inter** (OFL) si **iconite vectoriale Phosphor** (MIT) in locul emoji-urilor - ribbon, bara de navigare, titlurile panourilor.
  - **Panourile de jos in bara de stare**: filele (Find, Clash, TimeLiner, Viewpoints...) stau jos in bara de stare; clic = panoul se ridica (cu animatie), clic pe fila deschisa = coboara. Butoanele din ribbon il ridica automat.
  - **Arborele**: punct colorat pe categorie / culoarea elementului, numarul elementelor intr-o insigna, randul selectat cu nuanta de accent si bara de 2 px.
  - **Find Items cu chip-uri**: `IFC Type = Wall ×`, `+ Condition`, `Find All`, si „N conditions · M elements in scope”.
  - Animatii: hover cu fade in ribbon si arbore, tranzitie la schimbarea tab-ului.
  - Imaginea 3D: **margini fine** pe elemente (Render › Edges), umbre de contact mai moi, fundal cu „orizont” luminos.
- **Live Link - selectie sincronizata** cu **Revit** (ManageBIM › Sync Selection) si **Navisworks** (Tool Add-ins › Sync Selection), in ambele sensuri; in ManageBIM: Select › Live Link. Potrivire dupa UniqueId / IfcGUID / GlobalId; zoom optional pe selectia primita; starea in bara de jos.
- **As-built check** (Analyze › Scan vs Model): modelul fata de norul de puncte E57 / LAS - construit / partial / lipsa / nescanat, cu toleranta si praguri, culori pe model, Select / Isolate, raport Excel.
- **Planse PDF** din viewpoints (Viewpoint › PDF Sheets): A3 landscape, imaginea mare, descriere si comentarii, cartus (proiect, viewpoint, stare, responsabil, data, numar plansa).
- **Taietura sectiunii in DWG / DXF** (Sections › Cut to DWG): sectiunea pe nivel = plan la +1.20 m, planul de sectiune = sectiune / fatada; un layer pe tip de element.
- **LOD adaptiv**: in miscare, pe modele mari, obiectele mici se ascund temporar dupa FPS (pana la 24 px) si revin cand camera se opreste.

## 0.18.0 - 2026-11-19

- **Interfata noua (redesign)**: ribbon pe **carduri fara chenare** (adancimea vine din suprafete, nu din linii), **11 tab-uri**: Home, Select, Viewpoint, Review, Sections, Coordination, Analyze, Render, Export, Share, View.
  - Trei trepte ale ribbon-ului, din sageata din dreapta sus: **Full** (iconite + etichete), **Compact** (doar iconite, numele in tooltip), **Mini** (doar tab-urile); preferinta ramane salvata. Dublu-clic pe un tab = Mini / inapoi.
  - Toate temele: hover fara contur, selectie ca nuanta de accent, separatoare abia vizibile, ferestre plutitoare doar cu umbra; rotunjimile temelor functioneaza (erau fixate la 3.5).
  - Panourile laterale au filele ca un control segmentat (Tree / Prop Tree / Sets), fara linii.
- **Select Same Type** (Ctrl+Shift+A, clic dreapta, Home › Same Type, Select › Select Similar): toate elementele vizibile cu acelasi tip (Revit Family and Type / IFC type), din toate modelele; si **Select Same Category**.
- **Export Web viewer** (Share › Web viewer): un singur fisier .html cu modelul (sau selectia) - vizualizare 3D WebGL, arbore model › tip, cautare, proprietati, ascundere / izolare, X-ray. Se deschide in orice browser, fara internet si fara instalare.
- **Biblioteca de materiale**: import de pachete PBR (.zip de pe freepbr.com, ambientCG, Poly Haven... sau folder dezarhivat) - imaginile se copiaza in `%APPDATA%\ManageBIM\Materials`, harta de culoare devine textura, rugozitatea da luciul, metallic da metalicitatea. Materialele raman disponibile in orice proiect; „Save selected material to my library” pentru materialele proprii.
- Corectat: in vederea **ortho**, orbitarea nu mai sare cu zoom-ul departe.

## 0.17.0 - 2026-11-19

- **Model Checker** (Coordination › ✔ Checker): verificarea datelor modelului dupa reguli.
  - Regula = „Applies to” (tip IFC / categorie, proprietati, atribute) + „Requirements” (proprietate prezenta, valoare exacta sau din lista, model /regex/, interval numeric 10..20, optional / interzis).
  - **Import buildingSMART IDS 1.0** (.ids): entity, attribute, property, classification, material; conditiile nesuportate (ex. partOf) sunt raportate.
  - Reguli de pornire (⭐), editor de reguli, salvare / incarcare .mbrules; regulile se salveaza si in proiect.
  - Rezultate pe regula (trec / nu trec / neaplicabile), lista elementelor gresite cu motivul; Select / Isolate / Focus.
  - Export **Excel** (Summary + Failures) si **BCF** (un issue pe regula, cu elementele selectate si camera pe ele).

## 0.16.0 - 2026-11-19
- **Aspect „studio”** (pentru toate temele) + tema nouă **Studio** (aproape negru, albastru electric):
  bara de unelte de jos, compactă, pe grupe (Select, Window, Hide, Isolate, Unhide, Focus, Orbit, Pan, Walk, Fit, Home,
  Measure, Section, Markup), **Properties pe carduri** cu filtru și butonul **Show in Model**, **căutare în model** și
  punct colorat pe fiecare model în arbore, **nivelul curent în bara de stare** (alegerea lui face secțiunea pe nivel).
- **Matricea de clash** (Clash Detective › ▦ Matrix): deschise / total pe fiecare pereche de modele, toate testele,
  export Excel.

## 0.15.1 - 2026-11-19
- **Plant 3D / AutoCAD**: comenzile de referință apar acum (înainte nu erau înregistrate) și au butoane în tab-ul
  ManageBIM › Reference: **Attach / Reload / Detach Reference**.
- **Revit Reference** compact și rapid: o plasă netedă pe culoare (fără muchiile triunghiurilor, fără grup), fixată (pin);
  Load / Reload / Unload ca înainte.
- Bara de navigare: **Select, Window, Hide, Hide Unselected, Unhide All**.
- **Section box ca în Revit**: săgeți pe fiecare față - tragi de ele și fața se mută; **Model box** (Viewpoint › Sectioning)
  pune cutia în jurul întregului model, **Fit Section** în jurul elementelor selectate.
- Bara de panouri de jos se trage liber în sus / în jos (mâner vizibil); font mai mare în ribbon și pe filele panourilor.

## 0.15.0 - 2026-11-19
- **Panouri profesionale**: fiecare zonă (stânga / dreapta / jos) are 📌 **auto-hide** - devine o bandă subțire cu
  file, panoul apare peste model când îl atingi și se ascunde singur; **View › Auto-hide** pentru toate, **Hide panels
  (F11)** pentru tot ecranul pe model.
- **Selecție cu fereastră** (Home › Window): stânga → dreapta = complet în interior, dreapta → stânga = atinge
  (ca AutoCAD), Ctrl adaugă.
- **Export IFC doar pentru selecție** (Output › IFC › Selection, sau click dreapta › Export selection to IFC).
- **Export IFC mai corect**: vârfuri sudate (0.1 mm), solide închise marcate Closed, o suprafață pe culoare,
  plasarea fiecărui element la colțul lui (precizie), geometria identică scrisă o dată (IfcMappedItem).
- **Relocare ca în Navisworks** (Review › Relocate): **Move** punct → punct, **Rotate** (pivot + direcții),
  **Align** fața unui model pe o fată țintă; pentru tot modelul sau doar mutarea selecției.
- **Secțiuni pe niveluri mai bune**: grup **Level** în Viewpoint (nivel, ▲ ▼, box, plan, doar nivelul), **PgUp / PgDn**,
  margini reglabile (placa de sub nivel / de deasupra), **Only this level** (ascunde restul).
- **Import axe din DWG / DXF** (Edit Grids): liniile stratului de axe + etichetele din bule; stratul se ghicește sau se alege.
- **Referință în Revit și AutoCAD / Plant 3D** (Output › Reference → .mbref):
  Revit - butonul **ManageBIM Reference** (Load / Reload / Unload, DirectShape Generic Models în grup fixat);
  AutoCAD / Plant 3D - **MBREFERENCE / MBREFRELOAD / MBREFUNLOAD** (plase 3D pe strat blocat).
- Stabilitate: exporturile / importurile din dialoguri rulează protejat (o eroare internă nu mai închide aplicația);
  DWG-urile cu solide ACIS se încarcă ~15% mai repede.

## 0.14.0 - 2026-11-19
- **.bimd** - echivalentul NWD: **Publish** (Home › Project, sau File) pune proiectul și toate modelele într-un singur
  fișier, care se deschide pe orice PC fără fișierele sursă; Save scrie din nou în .bimd. Se asociază cu ManageBIM.
- **Pagina de start ca în Navisworks**: Open… cu meniu (model / proiect), New, Recent ca **carduri sau listă**,
  **Sort by** (ultima deschidere, nume, dată, tip), **Search**; butonul **Start** o arată peste proiectul deschis, iar
  săgeata ⬅ (sau Esc) te întoarce la proiect. Când deschizi altceva, întreabă dacă salvezi.
- **New** intră direct într-un proiect gol, cu toată interfața și Save.
## 0.13.1 - 2026-11-19
- **New** (pagina de start și File) face un proiect nou gol, cu toată interfața și Save (Ctrl+S).
- **Secțiune pe niveluri** (Sectioning › By level): **Level box** (de la nivel la nivelul următor, ◀ ▶ între niveluri) sau
  **Plan cut** la o înălțime peste nivel; planul de secțiune se vede ca o **suprafață verde transparentă** (Show plane).
- **Axe create în ManageBIM** (View › Edit Grids): generator (număr de axe, distanțe, etichete 1,2,3 / A,B,C, rotație),
  **origine și direcție alese cu clic în model** (snap pe colțuri), mutare / rotire, editare axă cu axă, preluare din modele.
- Axele arată ca pe planșă: **linie de ax, bulă albă cu etichetă** în prelungirea liniei; mărimea bulelor și
  **prelungirea liniilor** reglabile - se văd și de la distanță.
- **Export axe DXF** (straturile GRID / GRID_TAGS, coordonate reale în metri) și **DWG** prin ODA File Converter, dacă e instalat.
## 0.13.0 - 2026-11-19
- **Axe și niveluri** (View › Grids & Levels): axele din IFC (IfcGrid) cu bulele etichetelor, desenate pe nivelul de sub
  cameră, și nivelurile cu nume și cotă; **Grids** și **Levels** se ascund separat.
- **Niveluri proprii** (View › Edit Levels): ±0.00 al proiectului, nume și cote editabile, preluate din modele cu un clic;
  salvate în proiect.
- **Locația clash-urilor** „C-4 / Level 2” în Clash Detective, în rapoartele HTML / CSV și ca grupare automată
  (Group by › Grid location).
- **Măsurători cu snap** pe colț, mijloc de muchie și muchie (simbol pătrat / triunghi / X); cotele se salvează în viewpoint.
- **Umbrire de contact (SSAO)** pentru adâncime (View › Lighting › Contact shadows) și **tăieturi pline** hașurate la
  secțiune (Sectioning › Filled cuts). Se aplică și imaginilor / video-urilor.
- **Nori de puncte LAS** (necomprimat, formatele 0-10); LAZ: mesaj cu conversia în LAS.
- **Salvare automată** a proiectului la 3 minute (în %LOCALAPPDATA%\ManageBIM\Autosave, fișierul tău nu e atins) și
  **recuperare** pe pagina de start după o închidere neașteptată.
- **Issue-uri partajate** (Review › Shared Issues): un folder comun (rețea, SharePoint / OneDrive sincronizat), un fișier
  pe issue, sincronizare automată, ștergeri propagate, conflictele păstrate ca „(my copy)”.
- Revit: textul markup-urilor cu un tip de text propriu „ManageBIM Markup …” - mare, bold, în culoarea markup-ului.
## 0.12.1 - 2026-11-18
- **Pagina de start** (ca Navisworks): la pornire doar ribbon-ul (inactiv) și ecranul cu **Open / Open Project / New** și
  fișierele **recente** ca niște carduri (tip, nume, data); click deschide, click dreapta: Show in folder / Remove from list.
  Panourile apar după ce se deschide un model sau un proiect.
- Tema **Tokyo Night** (View › Theme, sau butonul de temă din dreapta sus).
- **ViewCube mai mic** și interfață mai compactă (spații, butoane și panouri implicite mai înguste) - mai mult loc pentru model.
## 0.12.0 - 2026-11-18
- **Nori de puncte E57** (fără programe externe): fiecare scanare e un element în arbore (ascunde / izolează / section box),
  culori RGB sau intensitate. **View › Scene › ☁ Point Cloud**: densitatea (1-100 %) și mărimea punctelor; în mișcare se
  desenează automat mai puține puncte.
- **Export IFC4** (Output › IFC › Export IFC): selecția sau tot ce e vizibil, cu geometria, culorile, nivelurile,
  GUID-urile originale și **toate proprietățile** (un Property Set pe grup).
- **Lumina** (View › Scene › 💡 Lighting): luminozitate, lumină ambientală, intensitatea și poziția soarelui -
  se aplică și imaginilor / video-urilor (AVI cu materiale), salvată în proiect.
- **Materiale și profil de culori în Excel / CSV**: export, editare în Excel, import înapoi (pe lângă .bimmat / .bimprofile).
- **Scoate un model din proiect**: click dreapta pe model în Selection Tree › Remove model from project (fișierul rămâne pe disc).
- **Markup-uri în Revit și Navisworks** prin „ManageBIM Issues”: liniile, săgețile, dreptunghiurile, norii și textele
  apar în vederea creată (Revit: Model Lines colorate în grupul „ManageBIM markup”, texte în vederea 3D blocată;
  Navisworks: redlines pe Saved Viewpoint).
- **Installer grafic**: Bun venit › Componente (alegi add-in-urile Revit / AutoCAD-Plant 3D / Navisworks și scurtătura) ›
  Instalare cu progres › Finish. Arată când e o actualizare (versiunea veche › nouă). `/S` rămâne instalare fără întrebări.
## 0.11.0 - 2026-11-18
- **Import BCF** (2.1 / 3.0): topic-urile devin viewpoints cu cameră, status, prioritate, comentarii și elementele selectate (după GUID).
- **Clash-uri spre alte aplicații**: export BCF (Revit, ACC, Solibri, BIMcollab) și viewpoints Navisworks cu section box în jurul fiecărui clash.
- **ManageBIM Issues** în Revit și Navisworks: issue-urile / viewpoints din ManageBIM se partajează automat
  (`%LOCALAPPDATA%\ManageBIM\Outbox\issues.txt`). În Revit se creează o vedere 3D cu aceeași cameră și același section box,
  cu elementele selectate; în Navisworks intră ca Saved Viewpoints cu comentarii.
- **Afișare simplificată la distanță**: se desenează doar ce e în câmpul vizual (nu și elementele ascunse sau tăiate
  de section box); în timpul mișcării, elementele mai mici de ~2 pixeli se sar. Imaginea statică rămâne completă.
- **Materiale mai bune**: lumină ambientală cer / sol, lumină de umplere, luciu fin pe toate suprafețele, luminile puternice nu se mai ard.
- Stabilitate: o eroare internă în testul de clash oprește testul cu mesaj, nu mai rămâne blocat.
## 0.10.2 - 2026-11-18
- Markups exportate spre Navisworks: **norul** se trimite cu festoane (ca în ManageBIM), toate liniile continue
  (`pattern=65535`); section box-ul are și cele 6 planuri + rotația cutiei, ca exportul Navisworks.

## 0.10.1 - 2026-11-18
- **Viewpoints pentru Navisworks** se scriu în unitatea documentului Navisworks (ex. inch pentru modelele deschise
  din NWD în inch), ca să ajungă la locul lor la import. Auto din model, sau aleasă în View › Options.

## 0.10.0 - 2026-11-18
- **Navisworks XML**: import și export de viewpoints (cameră, section box, comentarii, markups), export de seturi
  (search sets și seturi explicite) și de teste de clash, în formatul oficial `nw-exchange-12.0.xsd`.
- **.ifczip** se deschide direct.
- **FBX** nativ (biblioteca ufbx, MIT): obiecte, nume, ierarhie, culori de material.
- **Gratare Plant 3D**: desenate ca gratar presat (ramă, bare portante, bare transversale); dimensiunile din baza de
  date a proiectului când există.
- **Unități de afișare** mm / m pentru măsurători, distanțe și coordonate (cantitățile rămân în m / m² / m³).
- **.bimc v3**: geometria identică (doar mutată) e salvată o singură dată; proprietățile, culorile și numele rămân per element.
  Cache-urile noi nu se pot deschide cu ManageBIM 0.9 (care se reface singur din fișierul sursă).
- Navigare **Pan**, **Fit Section** + **Follow** (section box pe selecție).
- Installer într-un singur fișier cu iconiță, `ManageBIM-Setup-<versiune>.exe`.

## 0.9.0 - 2026-11-17
- STEP / IGES / BREP / FCStd fără programe externe (OpenCascade), solide ACIS din DWG, obiecte și proprietăți Plant 3D.
- Send to ManageBIM din Revit / AutoCAD / Plant 3D / Navisworks, cu actualizare live.
- Mers lin + mers pe traseu, bara de navigare, teme Dark / Light / Nord, ribbon compact.
