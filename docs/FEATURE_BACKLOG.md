# Feature backlog — confirmat de utilizator (23.11.2026)

> După 0.27.11. Se implementează **în etape**, cu `cargo check` + `cargo test --release`
> după fiecare etapă. Nu se publică până nu e totul verificat.

## Etapa A — fundație (început aici)

| # | Item | Detaliu | Stare |
|---|------|---------|-------|
| A1 | **OSNAP + unități reale (mm/m/in/ft)** | snap Vertex / Midpoint / Edge, rază reglabilă (6-24px), F3 on/off, etichetă lângă cursor; unități m/mm/in/ft + precizie în measure / Properties / status bar / QTO. | ✅ |
| A2 | **Split-view / comparație vizuală** | două jumătăți în aceeași fereastră, separator trasabil (stretch), cameră + secțiune proprii per jumătate, link/unlink camere, buton în View › Compare. | ✅ |

## Etapa B — quick wins

| # | Item | Detaliu | Stare |
|---|------|---------|-------|
| B1 | **Keybindings customizabile** | override-uri în `Settings::keybinds`, editor în Settings (click → taste → Enter), conflict detection, reset, aplicate în `shortcuts()` pentru open/find/palette. | ✅ |
| B2 | **Export în lot** | buton „Batch" în Viewpoints → alegi dosar → PDF sheets + HTML + Excel + BCF, secvențial prin `batch_queue`. | ✅ |
| B3 | **Prof de setări partajabil** | salvează / încarcă temă, panouri, filtre, scări, unități, keybindings ca fișier `.mbprofile`; buton Import/Export în Settings. | ✅ |

## Etapa C — performanță / robustețe

| # | Item | Detaliu | Stare |
|---|------|---------|-------|
| C1 | **Instancing + occlusion culling** | elemente repetitive (coloane, șuruburi) desenate instanțiat; culling pe AABB / ierarhie; contor FPS în status bar. | ⬜ |
| C2 | **Import IFC progresiv** | afișare pe măsură ce se încarcă, progress bar, cancel, fără blocarea UI-ului. | ⬜ |

## Etapa D — pe viitor (amânat)

| # | Item | Detaliu | Stare |
|---|------|---------|-------|
| D1 | **Space / Room checks** | arii `IfcSpace`, adiacențe, lățimi minime coridor, rază de acces — reguli în Model Checker. Vezi `ROADMAP_0.28.md` Sprint 3. | ⬜ (amânat) |
| D2 | **OpenPBR / MaterialX (subset)** | Adăugat la listă pe 23.11.2026, de reluat mâine. Varianta aleasă: **A** — parametri OpenPBR (`coat` / clearcoat, `sheen`, IOR / specular) în `.bimmat`, shader și path trace; import de parametri + căi de imagini din XML MaterialX (fără evaluarea grafului) și export simplu înapoi. Nu se leagă biblioteca C++ MaterialX (FFI) — risc mare, build complicat. Ambele standarde sunt Apache-2.0 (notă în `THIRD_PARTY.md`). | ⬜ (programat) |
| D3 | **Randare la nivel profesional (fără Eevee)** | Adăugat pe 23.11.2026. ManageBIM are deja motor propriu (glow + path trace CPU + OIDN), nu e nevoie de Eevee. Varianta aleasă: **A** — IBL/HDRI (mediu procedural + suport `.hdr`/`.exr`), BRDF GGX cu IOR/Fresnel, 3–4 bounces în path trace, tonemap ACES, denoise existent. Apoi opțional **B** — IBL și reflexii de mediu în viewport-ul 3D + contact shadows. Separat de D2: D2 aduce materiale, D3 crește calitatea randării. | ⬜ (programat) |
| D4 | **MCP server în ManageBIM** | Adăugat pe 24.11.2026, pentru versiunea finală. Un mic server MCP local expune operații (deschide model, selectează elemente, setează camera / viewpoint, adaugă / mută / șterge lumini, pornește path trace, exportă PNG) ca agentul să se poată conecta și testa aplicația în timp real, fără capturi de ecran. | ⬜ (versiunea finală) |
| D5 | **Cercuri de rază ca șine de rotație** | Adăugat pe 24.11.2026. Cercul de rotație separat e greu de prins; cercurile de acțiune XY / XZ / YZ sunt deja desenate în gizmo. Transformă-le în traseu: drag pe cerc = rotația direcției luminii în jurul axei perpendiculare pe planul cercului (XY → în jurul Z, XZ → în jurul Y, YZ → în jurul X). Crește intensitatea luminii în model (lux) pentru a fi vizibilă imediat. | ⬜ (0.27.28) |
| D6 | **bSDD + IDS + IFC + MVD (BIM)** | Adăugat pe 24.11.2026. bSDD: căutare dictionary / clasificare după GUID cu cache offline, atribuire de clasificări elementelor. IDS: import/export fișiere IDS, validare model cu reguli IDS în Model Checker. IFC: extindere validare / export (proprietăți, quantități, verificări de conformitate). **MVD (Model View Definition)**: opțiuni în dialogul de export IFC — Reference View, Design Transfer View, Coordination View 2.0, SV 2.0 — cu filtrarea proprietăților / geometriei cerută de fiecare view. Verificat: `ifc_export::write_ifc` scrie azi un singur format fix, fără MVD. | ⬜ (după D5) |

## Reguli de calitate

- Fiecare etapă: `cargo check --all-targets` curat (0 erori, 0 warning-uri) + `cargo test --release` (0 failed).
- Installer local după fiecare etapă verificată; publicare doar când utilizatorul cere.
- Păstrează comportamentul existent (panouri dock/float/stretch, add-in-uri, OCCT, vederi).
- Fără Tauri / rescriere: tot în Rust + egui + glow.
