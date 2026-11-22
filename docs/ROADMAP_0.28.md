# Plan versiunea 0.28 — confirmat de utilizator (22.11.2026)

> 0.27.5 e în validare finală. Tot ce e aici intră în **0.28**.

## 🐞 Fixuri aduse din 0.27.5

- **DWG Overlay la locul tăieturii**: la import, desenul exportat prin „Cut to DWG" revine exact pe poziția tăieturii.
  - Metadatele planului (comentariu DXF `MB_CUT_PLANE` + sidecar `.mbplane`) se scriu și pentru tăieturile din **cub**, nu doar pentru vederile denumite.
  - Pentru tăieturi în plan (Z) → overlay-ul primește `z = valoare - origin.z` și cade exact la cota tăieturii; X/Y sunt deja aliniate.
  - Pentru elevații (Front/Back/Left/Right) → suport de **plan vertical** în `CadOverlay` (`vert: Option<u8>`: YZ / XZ), cu `mirror`, astfel încât desenul revine în planul vertical corect, nu pe orizontală.
  - Câmpurile noi au `#[serde(default)]` → proiectele vechi se încarcă fără modificări.

## 🟢 Sprint 1 — stabilizare (zero-risc, aditiv)

| # | Item | Detaliu |
|---|---|---|
| 1 | `cargo clippy -D warnings` + `cargo fmt --check` în CI | job separat, nu blochează build-ul de release dacă e pus `continue-on-error` la început; se strânge treptat datoriile (82 `unwrap()` în parser-e) |
| 2 | `cargo-deny` în CI | audit licențe (MIT / Apache-2.0 / OFL / LGPL OCCT) + advisory-uri |
| 3 | **Copy diagnostics** în Help › About | copiază în clipboard: versiune, OS, GPU/GL renderer, ultimele 30 linii `crash.log` — fără date personale |
| 4 | Scripturi exemplu 04–06 | `04_quantities_excel.rhai`, `05_hide_mep.rhai`, `06_active_clashes.rhai` |

## 🔶 Sprint 3 — BIM greu (diferențiatori)

| # | Item | Detaliu |
|---|---|---|
| 7 | **IFC 4.3 complet + validare buildingSMART** | IFC4X3_ADD2 la citire/scriere; validator de conformitate (GUID, Pset, structură spațială IFC) cu raport în UI |
| 8 | **Clasificări Uniclass / Omniclass / SFG20** | mapaj pe proprietăți, culori per diviziune, filtrare în Selection Tree |
| 9 | **Verificări de spațiu / accesibilitate** | arii `IfcSpace`, adiacențe, lățimi minime de coridor, rază de acces — reguli reale în Model Checker |

## 🔷 Sprint 4 — colaborare

| # | Item | Detaliu |
|---|---|---|
| 10 | **Review package** | `.bimr` read-only publicabil (model + issue-uri + viewpoint-uri), pe baza `.bimd` existent — „Navisworks Freedom" al ManageBIM |
| 11 | **Snapshot timeline** | versiuni etichetate, diff între revizii (pe baza `compare.rs`) + restore |
| 12 | **Conectori BIMcollab / ACC** | se pregătesc acum chiar dacă nu folosești cont: interfață de configurare + backend REST (token, proiect, sincronizare issue-uri), activabil ulterior |

## 🧠 buildingSMART Data Dictionary (bSDD) — funcții tip Bonsai

Sursă de inspirație: video-ul Bonsai/bSDD (https://www.youtube.com/watch?v=Eqtqun9JrWM).

| # | Funcție | Detaliu |
|---|---|---|
| 13 | **Căutare în bSDD** | API public bSDD 4.0 (`Search`, `Classes`, `Properties`), cu filtru după standard/domeniu (IFC 4.3, domeniu, versiune) |
| 14 | **Atribuire clasificare bSDD pe elemente** | alegi o clasă din dicționar → se scrie în `IfcClassification` / proprietate ManageBIM |
| 15 | **Import proprietăți din bSDD** | property sets (Pset_) cu valori/tipuri/unități, compatibile cu Model Checker și IDS |
| 16 | **Cache local + mod offline** | dicționarul descărcat se păstrează în `%APPDATA%\ManageBIM\bsdd\` și poate fi folosit fără cont |
| 17 | **Export IDS/COBie augmentat** | clasificările bSDD apar în rapoartele deja existente |

> Notă: bSDD-ul public nu cere cont pentru citire; contul e necesar doar pentru scrierea în dicționar (nu o implementăm în 0.28).

## Ordine de execuție recomandată

1. Sprint 1 (aditiv) → confirmare CI verde.
2. Fix overlay DWG (închide subiectul din 0.27.5).
3. bSDD (căutare + atribuire + cache) — cel mai mare câștig BIM, se integrează cu Search/Properties/Model Checker existente.
4. Sprint 3 (IFC 4.3, clasificări, spații).
5. Sprint 4 (review package, snapshot timeline, conectori cu interfață pregătită).

## 📐 Măsurători — snapuri tip AutoCAD (OSNAP) și unități (0.28)

Starea curentă: snapuri la **Vertex / Midpoint / Edge / Surface**, rază fixă mică, afișare doar în metri (cu `units_mm` simplu).

| # | Îmbunătățire | Detaliu |
|---|---|---|
| 1 | **Rază de snap mai mare + reglabilă** | setare în Settings (px, ex. 6–24, implicit ~12); markerul vizual se mărește odată cu raza, ca să vezi ce ai prins |
| 2 | **Snapuri exacte pe geometrie** | se calculează pe coordonatele reale ale triunghiurilor (nu aproximare pe ecran): capăt de muchie, mijloc, centru de cerc/arc, intersecție de muchii, picior de perpendiculară pe muchie, punctul cel mai apropiat pe suprafață |
| 3 | **Tipuri de snap selectabile (ca OSNAP)** | dialog în Settings cu checkboxuri + glifele din AutoCAD: **Endpoint** (─), **Midpoint** (△), **Center** (○), **Geometric Center**, **Node**, **Quadrant** (◇), **Intersection** (X), **Extension**, **Insertion**, **Perpendicular** (⊥), **Tangent**, **Nearest**, **Apparent intersection**, **Parallel** (//); butoane **Select All / Clear All**; **F3** = Object Snap On/Off |
| 4 | **Marcaj + etichetă pe ecran** | ca în captura ta: la punctul prins apare **glifa + numele tipului** („Vertex", „Midpoint"…) lângă cursor, nu doar un punct |
| 5 | **Snap tracking** | opțional 0.28: pauză peste punct → vector de tracking (F11 în AutoCAD); **atenție: F11 e deja „hide all panels" în ManageBIM → Model focus se mută pe altă tastă** |
| 6 | **Prioritate / ciclare** | când mai multe snapuri cad în rază, se arată cel mai apropiat și se poate cicla cu Tab între candidați |
| 7 | **Unități: mm / m / in / ft** | setare în Settings (înlocuiește `units_mm`, cu migrare la mm); distanțele, ariile și coordonatele se afișează cu sufixul unității; precizie (număr de zecimale) configurabilă |
| 8 | **Afișare consecventă** | aceeași unitate în measure, Properties, status bar, QTO și în etichetele de pe imagine/video |

> Sursă vizuală: capturile trimise cu dialogul AutoCAD Object Snap (lista completă de moduri) și marcajul „Vertex" lângă cursor. Compatibilitate: `units_mm` existent se migrează la sistemul metric mm; proiectele vechi își păstrează unitatea afișată.

## 🎨 Restilizarea interfeței existente (prototip aprobat)

Sursă: `docs/prototype/design-preview-actual.html` (prototipul cu **taburile și butoanele reale** ale aplicației). Se implementează **fără Tauri**, direct în egui:

1. **Design tokens în `theme.rs`** — `RADIUS_SM/MD`, `GAP`, culori `panel2`/`border` per temă; înlocuiesc cele 7 valori de rounding hardcodate cu 3 constante.
2. **`rbtn()` restilizat o singură dată** — card button: fundal `panel2`, border 1px pe temă, iconiță 15px + etichetă 9px sub, hover cu border accent, activ cu fundal accent 18%, înălțime fixă 42px → **toate butoanele din ribbon se schimbă instant**.
3. **Carduri în panouri** — `Frame::fill(faint_bg).rounding(9).stroke(...)` pentru Clash Detective, Properties, Script Library (deja început în 0.27.8).
4. **Temele existente rămân** — se completează doar câmpurile lipsă din `Palette`.
5. **Borduri subtile pe panouri** deja făcute; se extind la carduri și grupuri.

> Regulă: 80% din aspectul prototipului cu modificări în 3-4 fișiere, fără rescriere, fără risc funcțional.

## 🖼️ Branding pentru versiunea finală (iconiță + installer)

Starea curentă: iconița e desenată procedural (3 cuburi colorate) în `main.rs::icon()` și `installers/setup/main.rs::icon()`; installerul e egui cu tema Nord, 720×560, fără logo.

| # | Item | Detaliu |
|---|---|---|
| 1 | **Iconiță nouă, profesională** | înlocuiește cuburile procedurale cu un design real: fundal navy închis, cub isometric BIM cu accente albastru (#3b82f6) / portocaliu (#eb8730) / verde (#46be64) — culorile brandului existent; livrat ca `.ico` multi-dimensiune (**16, 32, 48, 64, 128, 256 px**) pentru Windows + `.png` 512 pentru Linux/FreeBSD |
| 2 | **Aceeași iconiță peste tot** | `ManageBIM.exe`, `ManageBIM-Setup.exe`, installerul `install.ps1` (Start Menu / Desktop shortcut), web viewer (favicon), documentație README |
| 3 | **Installer mai profesionist** | header cu **logo + nume + versiune** (în loc de text simplu), bară de progres cu pași evidențiați, panou lateral cu „Ce se instalează" (app + OCCT + add-in-uri detectate), pagină Finish cu „Ce e nou în 0.28" și buton „Open ManageBIM" / „View release notes" |
| 4 | **Coerență vizuală** | aceleași culori Nord/Studio, spațieri și glife Phosphor ca în aplicație; fonturile incluse (Inter/Lato) folosite și în installer |
| 5 | **Detalii de încredere** | badge „No admin required", „MIT License", dimensiunea pachetului și lista componentelor afișate înainte de Install |

> Regulă pentru 0.28: branding-ul e **parte din criteriile de calitate** — iconița și installerul se verifică odată cu funcțiile, nu la final.

## 🎨 Icoane distincte în ribbon (duplicatelor actuale li se dau semne proprii)

Surse de confuzie identificate (din `rbtn`): ⬚ ×4, ✏ ×5, ◎ ×4, 📄 ×6, 📥 ×5, 📤 ×3, 📦 ×3, 🏗 ×3, 👁 ×3, 🎞 ×3, 📷 ×3, ➕ ×3, 🎯 ×3, 🔗 ×3.

Atribuiri noi (se adaugă și mapări Phosphor noi unde e nevoie):

| Buton | Icoana veche | Icoana nouă | Motiv |
|---|---|---|---|
| Edit Grids | ✏ | **▦** (GRID_FOUR) | grilă, nu creion |
| Edit Levels | ✏ | **📏** (RULER) | cote/niveluri |
| Title Block | ✏ | ✏ (rămâne singurul creion din tab) | editare |
| Draw (walk path) | ✏ | **🛤** (PATH — mapare Phosphor nouă) ✅ aplicat | traseu, nu creion |
| Hide Unselected | ◎ | **🚫** (EYE_SLASH) | ascundere |
| Field of view | ◎ | **📷** (CAMERA) | cameră |
| Relocate Selection/Model | ◎ | **↔** (ARROWS_OUT_CARDINAL) | mutare |
| Path trace | ◎ | **✨** (SPARKLE — mapare Phosphor nouă) | randare progresivă |
| Select All | ⬚ | **⊞** (SQUARES_FOUR — mapare nouă) | tot |
| Orthographic | ⬚ | **🔲** (SQUARE) | proiecție paralelă |
| Fit Section | ⬚ | **⛶** (CORNERS_OUT) | potrivire |
| Import Properties vs alte 📥 | 📥 | **⇩** (mapare nouă DOWNLOAD) | importuri diferențiate pe grup |

Regula de implementare: **într-un tab, nicio două butoane cu aceeași iconiță**; între taburi se admite numai dacă acțiunile sunt înrudite.



**Definiția de „gata" pentru fiecare funcție în parte:**
1. **Teste scrise** — fiecare funcție nouă vine cu teste (unitare sau integrare); fără teste, funcția nu intră în release.
2. **Verificare în aplicație** — fiecare funcție e testată manual pe proiectul demo (Help › Demo): se deschide, se folosește, se exportă.
3. **Fără regresii** — suita completă (152+ teste) trece la FIECARE commit de feature; orice eșec se corectează înainte de a continua.
4. **`cargo check --all-targets` + `cargo clippy`** fără erori; avertismentele noi nu se acceptă.
5. **Compatibilitate** — proiectele și setările vechi (0.27.x) se încarcă fără pierderi (serde defaults pe toate câmpurile noi).
6. **Performanță** — modelele mari se deschid în timp comparabil cu 0.27.5 (fără regresii >10%).

**Definiția de „gata" pentru release-ul 0.28:**
- Toate itemele din Sprint 1/3/4 + bSDD implementate și testate.
- Suita completă verde (0 eșuate) pe Windows.
- CI verde pe toate cele 3 platforme (Windows / Linux / FreeBSD).
- Installer construit local, verificat (versiune, hash, conținut OCCT + add-in-uri).
- `docs/ROADMAP_0.28.md` actualizat cu starea finală; CHANGELOG complet.
- Push cu tag `v0.28.0` → CI publică automat Windows + Linux + FreeBSD.

> Regula de aur: **nimic nu se livrează „aproape gata"** — dacă o funcție nu trece de toate cele 6 criterii, rămâne pe roadmap, nu în release.

