# Plan versiunea finală (0.28) — actualizat după 0.27.5

> Starea la 22.11.2026. Marcate cu ✅ cele livrate deja în 0.27.5.

## 🏗️ BIM / interoperabilitate

| # | Funcționalitate | Stare | Efort | Prioritate |
|---|---|---|---|---|
| 1 | Export COBie 2.4 (Contact, Facility, Floor, Type, Component, System) | ✅ gata | — | — |
| 2 | Export BCF 2.1 / 3.0 (comentarii în `<Topic>` la 3.0) | ✅ gata | — | — |
| 3 | **IFC 4.3 (IFC4X3_ADD2) complet** + validare buildingSMART (GUID, Pset, structură spațială) | de făcut | mare | 🔶 medie |
| 4 | **Clasificări Uniclass / Omniclass / SFG20** — mapaj pe proprietăți, culori per diviziune | de făcut | mediu | 🔶 medie |
| 5 | **Verificări de spațiu/accesibilitate** — arii IfcSpace, adiacențe, lățimi minime | de făcut | mediu-mare | 🔶 medie |
| 6 | **5D cost codes** — cost code per element + raport pe fază/disciplină | de făcut | mediu | 🔷 mică |
| 7 | **Conectori cloud** Speckle / BIMcollab / ACC (peste folderul partajat) | de făcut | mare | 🔷 mică |

## 🧩 Funcționalitate

| # | Funcționalitate | Stare | Efort |
|---|---|---|---|
| 1 | Motor Rhai + consola + 3 exemple + folder configurabil | ✅ gata | — |
| 2 | Fonturi (Inter/Lato/sistem) + înălțime text 10–26 px | ✅ gata | — |
| 3 | Tăietura aliniată cu grila/nivelurile + highlight în Section Box | ✅ gata | — |
| 4 | **Review package** — `.bimr` read-only publicabil (pe baza `.bimd`) | de făcut | mediu |
| 5 | **Editor de scurtături** — rebind + detectare conflicte | de făcut | mediu |
| 6 | **Teme noi**: Solarized, Dracula, High Contrast (+ daltonism) | de făcut | mic |
| 7 | **Timeline de snapshot-uri** — versiuni etichetate, diff, restore | de făcut | mare |
| 8 | **„Copy diagnostics"** în Help › About (versiune, GPU, crash.log) | de făcut | mic |

## ⚡ Performanță

| # | Item | Stare | Efort |
|---|---|---|---|
| 1 | Path trace multi-core (rayon) + rezoluții până la 4096 | ✅ gata | — |
| 2 | **Cache BVH în `Model`** (invalidat la schimbarea geometriei) — „Update All" ~2× mai rapid | de făcut | mic-mediu |
| 3 | **Clash incremental cu broad-phase** — doar elementele mutate (`run_key`/`last_run` există) | de făcut | mediu |
| 4 | Instancing | ❌ exclus la cerere | — |

## 🧪 Calitate (zero-risc, aditive)

| # | Item | Efort |
|---|---|---|
| 1 | `cargo clippy -D warnings` + `cargo fmt --check` în CI (prinde regresii devreme) | 10 min |
| 2 | `cargo-deny` în CI — audit automat de licențe | 15 min |
| 3 | Scripturi exemplu 04–06 (cantități, hide MEP, clash-uri active) | 30 min |
| 4 | Cheatsheet complet de scurtături în Help | 1 h |
| 5 | Traduceri DE/FR în `i18n.rs` (tabelul e aditiv) | 2-4 h |
| 6 | Icoană de aplicație desenată (înlocuiește cuburile procedurale) | 1 h |

## 🧹 Igienă repo (după publicarea 0.27.5, nu în timpul unui release)

- **Istoric git: 582 MB** instalere vechi în `download/` → `git filter-repo --path download --invert-paths` + **force-push** + re-clone de către colaboratori. Numai după ce release-ul 0.27.5 e publicat.
- Șterge tag-urile locale inutile dacă nu mai trebuie.

## Ordinea recomandată pentru versiunea finală

1. **Sprint 1 (stabilizare):** clippy+fmt+deny în CI, diagnostics, scripturi 04–06 — tot aditiv.
2. **Sprint 2 (valoare):** teme noi, editor de scurtături, cache BVH, clash incremental.
3. **Sprint 3 (BIM greu):** IFC 4.3 + validare, clasificări, verificări de spațiu.
4. **Sprint 4 (colaborare):** review package, snapshot timeline, conectori cloud.
5. **La final:** curățarea istoricului git + versiunea finală 0.28.
