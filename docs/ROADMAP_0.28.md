# Plan 0.28 (nu intrăm încă în 0.28 — seria rămâne 0.27.x până la stabilizare)

> Notat la cerere. Elementele de mai jos NU sunt implementate în 0.27.1.

## 🏗️ BIM / interoperabilitate

1. **Export COBie** (xlsx) — pachet de predare către facility management; reutilizează `qto.rs` / `tables.rs`.
2. **IFC 4.3 (IFC4X3_ADD2) + validare buildingSMART** — readerul e declarat IFC2x3/IFC4; exportul IDS menționează deja IFC4X3_ADD2. Validator de conformitate (GUID-uri, Pset-uri, structură spațială).
3. **BCF 3.0 export** — importul suportă 2.1 + 3.0 (`markup.rs:1063`), exportul scrie doar `VersionId="2.1"` (`markup.rs:403`). Quick-win.
4. **Conectori cloud** — Speckle / BIMcollab / ACC, peste sincronizarea prin folder partajat din `shared.rs`.
5. **Sisteme de clasificare** — Uniclass / Omniclass / SFG20, mapaj pe proprietăți + culori per diviziune.
6. **Verificări de spațiu și accesibilitate** — arii `IfcSpace`, adiacențe, lățimi minime de coridor, accesibilitate; reguli BIM reale în Model Checker.
7. **5D / cost codes** — cost code per element + raport cost pe fază/disciplină, pe lângă qto + 4D existente.

## 🧩 Funcționalitate

1. **Export BCF 3.0** (vezi mai sus, pct. 3).
2. **Motorul Rhai propriu-zis** — consola, folderul și 3 exemple există deja; se activează API-ul `select_where`, `set_prop`, `create_clash_test`, `export_bcf`, `log` etc.
3. **Review package** — fișier `.bimr` read-only, publicabil (model + issue-uri + viewpoint-uri), deschis fără surse; se bazează pe `.bimd` existent.
4. **Editor de scurtături** — rebind + detectare de conflicte (shortcut-urile sunt hardcodate în `shortcuts()`).
5. **Teme pentru daltonism / contrast mare** — 2 variante noi, palettele sunt deja centralizate în `theme::Palette`.
6. **Timeline de snapshot-uri** — versiuni etichetate, diff între revizii și restore (pe baza `compare.rs`).

## ⚡ Performanță (fără instancing — exclus la cerere)

1. **Path trace pe mai multe thread-uri** — înlocuit bucla secvențială din `pathtrace.rs` cu `rayon` (`par_chunks_mut`); câștig ~8× pe 8 core. `rayon` e deja dependență.
2. **Cache BVH** — `Scene::build` reconstruiește BVH-ul la fiecare randare; ținut în `Model` / `OnceLock`, invalidat doar la schimbarea geometriei.
3. **Clash incremental cu broad-phase** — folosit `Bvh` din `model.rs`; se re-testează doar elementele al căror AABB s-a schimbat (`run_key` / `last_run` există deja în `ClashTest`).

## 🐞 Debugging în curs (seria 0.27.x)

- Path trace nu se oprește după finish (buclă de repaint).
- Planurile Left/Right și Front/Back nu se schimbă la export DWG.
- Overlay-ul DWG nu se vede la import.
- Ecranul clipeste la schimbarea font/UI size.
- Scripturile exemplu nu rulează (motorul nu e încă activ).
