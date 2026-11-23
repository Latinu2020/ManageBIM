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

## Reguli de calitate

- Fiecare etapă: `cargo check --all-targets` curat (0 erori, 0 warning-uri) + `cargo test --release` (0 failed).
- Installer local după fiecare etapă verificată; publicare doar când utilizatorul cere.
- Păstrează comportamentul existent (panouri dock/float/stretch, add-in-uri, OCCT, vederi).
- Fără Tauri / rescriere: tot în Rust + egui + glow.
