# ManageBIM

Aplicație desktop în Rust pentru revizuirea și coordonarea modelelor BIM federate (IFC / glTF):
vizualizare 3D, arbore de selecție, proprietăți, căutare și seturi, Clash Detective, TimeLiner 4D,
secționare, măsurători, viewpoints, proiecte `.bimr`.

## Compilare pe Windows
1. Instalează Rust: https://rustup.rs (varianta MSVC, implicită).
2. `cargo build --release`
3. Executabil: `target\release\ManageBIM.exe`

### Release semnat (altfel Smart App Control blochează exe-ul)
1. Instalează Windows SDK (componenta „Signing Tools for Desktop Apps”) pentru `signtool.exe`.
2. Setează certificatul (o singură variantă): `MANAGEBIM_SIGN_THUMBPRINT`, sau `MANAGEBIM_PFX` + `MANAGEBIM_PFX_PASSWORD`, sau `MANAGEBIM_TS_DLIB` + `MANAGEBIM_TS_METADATA` (Azure Trusted Signing). Detalii în antetul scriptului.
3. `powershell -ExecutionPolicy Bypass -File tools\build-release.ps1 -RequireSign -SignPayload`
4. Rezultat: `dist\ManageBIM-Windows-x64\` + `.zip`, cu toate exe-urile semnate și verificate.

Un certificat self-signed NU trece de Smart App Control; e nevoie de unul emis de o autoritate de încredere.

## Appearance Profiler (Home › Display sau Coordination › Models)
Colorează modelul după orice proprietate: **By value** (o culoare pe valoare - Level, IFC Type, System, FireRating...)
sau **By range** (intervale numerice cu gradient - volume, arii, cote). Culorile se pot edita, regulile se pot ascunde,
click pe o regulă selectează elementele. Elementele fără regulă: păstrează culoarea / fantomă / ascunse.
Prioritate: override manual > profil > material. Profilul se salvează în proiectul `.bimr`.
**Export… / Import…** (`.bimprofile`): păstrează proprietatea, regulile și culorile tale, ca să le aplici pe alte modele.
După import, **Add missing values** adaugă culori doar pentru valorile noi, fără să le atingă pe ale tale.
Regulile se editează direct: valoarea (By value), limitele intervalelor (By range), ➕ Rule, 🗑.

## Compare (Coordination › Models)
Deschide versiunea veche, apoi ➕ Append versiunea nouă, apoi ⚖ Compare. Elementele se potrivesc după GlobalId:
**roșu** șters, **verde** adăugat, **portocaliu** geometrie schimbată (mutat / redimensionat, toleranță în mm),
**albastru** proprietăți schimbate, restul fantomă. Lista „Changes” arată ce s-a schimbat (ex. „Moved 250 mm”,
„Pset_WallCommon / FireRating: 60 → 90”); fiecare categorie se poate selecta sau salva ca set.
**Save… / Open…** (`.bimcmp`): salvează comparația și o redeschide mai târziu pe aceleași două versiuni
(modelele se găsesc după nume, elementele după GlobalId). **CSV report…**: toate schimbările, pentru Excel.

## Selection Tree personalizat
Bara „Group by” de deasupra arborelui (⚙): alegi nivelurile de grupare - orice proprietate, în orice ordine
(ex. IFC Type › Level › Type Name, sau System › Level). Presets, ⬆ ⬇ 🗑, **Export… / Import…** (`.bimtree`).
Gruparea se păstrează între sesiuni.

## Quantification (Coordination › Models › Quantities)
Cantități grupate după orice proprietăți (ex. IFC Type › Level): Count, Volume, Surface area, Plan area, Length,
Height (din geometrie, pentru orice format) + coloane „Sum” din orice proprietate numerică (ex. Qto_*BaseQuantities /
NetVolume - convertite automat în m / m² / m³ din unitățile IFC). Domeniu: tot / vizibil / selecția.
Click pe un rând selectează elementele. **Excel…** (foi Summary + Items, cu totaluri), **CSV…**,
**Template** (`.bimqto`) pentru refolosire; configurația se salvează în proiect.

## Navisworks: NWD / NWC / NWF (și RVT)
Formate închise - se deschid prin Navisworks Manage / Simulate instalat, cu add-in-ul ManageBIM (`navis-addin/`):
în Navisworks **Tool add-ins › Export to ManageBIM (.bimc)**, sau direct **File › Open** în ManageBIM
(conversie în fundal, apoi cache `.bimc`), sau `nwd2bimc.exe model.nwd`. Detalii: `navis-addin/README.md`.

## CAD: STEP / IGES / BREP fără programe externe (OpenCascade)
ManageBIM deschide nativ `.step` / `.stp`, `.iges` / `.igs` și `.brep` / `.brp` prin **OpenCascade (OCCT 8.0.1)**,
și `.FCStd` (FreeCAD), fără programe externe. Din STEP / IGES se citesc (XCAF): structura de ansamblu, numele pieselor și ale instanțelor,
culorile (pe piesă, pe instanță și pe fiecare față), unitățile fișierului (convertite în metri).
- Fiecare piesă frunză din ansamblu = un element. Proprietăți (grupul **CAD**): *Assembly path*
  (ex. `Bracket assembly / Cone sub-assembly`), *Part name*, *Instance name*, *Shape type*, *Solids*, *Faces*.
  „Level” din arbore = sub-ansamblul părinte, deci piesele se grupează pe sub-ansambluri.
- Triangularea (`BRepMesh`) e fină pentru piese mici și rezonabilă pentru cele mari (deflexie relativă la
  mărimea piesei, 20° unghiular); piesele instanțiate de mai multe ori se triangulează o singură dată.
- **FreeCAD nu mai e necesar deloc.** Fișierele `.FCStd` se citesc direct (`src/fcstd.rs`): arhiva ZIP, `Document.xml`
  (obiectele vizibile și forma lor BREP) și `GuiDocument.xml` (culoarea / transparența), triangulate prin OpenCascade.
  Un obiect FreeCAD = un element (Label, tip, culoare). Se afișează doar ce e vizibil în FreeCAD; schițele, axele,
  containerele (App::Part, grupuri, BuildingPart Arch) și operațiile PartDesign din interiorul unui Body nu dublează geometria.
  Limitare: formele salvate ca BREP binar sunt sărite (cu avertisment), iar culorile pe fețe individuale nu se citesc.

## Mers prin model (Viewpoint › Navigation Mode / bara de navigare de jos)
- **Mers lin:** accelerare și frânare progresive, indiferent de FPS; viteza implicită 1,4 m/s (pas de om).
  Trasul cu mouse-ul are zonă moartă și răspuns progresiv (mișcare mică = mers încet), plafonat la 2× viteza;
  Shift (taste) / Ctrl (mouse) = mai repede. Presetări: Slow 0,7 · Normal 1,4 · Fast 4 m/s.
- **Mers pe traseu (Walk Path):** ✏ Draw → click pe podea pentru puncte (Backspace = anulează ultimul, dublu-click /
  Enter = gata) → ▶ Play (Space). Camera merge pe traseul netezit la înălțimea ochilor, cu viteza de mers, privind
  înainte și rotindu-se lin în curbe; Space = pauză, Esc = stop. **🎞 To Animation** salvează traseul ca animație în
  Animator (se poate reda și exporta video). Traseul se salvează în proiectul `.bimr`.

## Installer într-un singur fișier: ManageBIM-Setup-<versiune>.exe
`powershell -ExecutionPolicy Bypass -File tools\make-setup.ps1` (după `cargo build --release`) face un singur .exe, cu
iconița programului, care conține aplicația, OpenCascade, exemplele și add-in-urile. Pe alt calculator: dublu-click →
instalare **per utilizator, fără admin** în `%LOCALAPPDATA%\Programs\ManageBIM`, scurtături Start Menu + Desktop,
tipurile .bimc / .bimr, intrare în Settings › Apps (cu Uninstall) și add-in-urile pentru Revit / AutoCAD / Plant 3D /
Navisworks găsite pe acel PC. Rulat din nou = actualizare. Opțiuni: `/S` (fără întrebări), `/NoAddins`.
Jurnal: `%LOCALAPPDATA%\ManageBIM-setup.log`. `tools\make-backup.ps1` îl pune automat și în folderul de backup.
Exe-ul nu e semnat digital: pe un alt PC cu Smart App Control, Windows poate cere confirmare sau îl poate bloca.

## Viewpoints și markups pentru Navisworks (XML)
Review › Viewpoints & Issues › **🧭 Navisworks** (sau „Navisworks XML” în panoul Issues) scrie `viewpoints.xml` în
formatul oficial Navisworks (`nw-exchange-12.0.xsd`): camera (perspectivă / ortografic), section box-ul, comentariile
(descrierea, starea issue-ului, comentariile) și markup-urile (linie, săgeată, elipsă, nor, dreptunghi, desen liber,
text). În Navisworks: **Saved Viewpoints › click dreapta › Import Viewpoints**. Coordonate în metri, în sistemul modelului.

**Import din Navisworks:** Output › Navisworks (XML) › **Import VP** (sau „Import Navisworks XML” în panoul Issues)
citește viewpoints exportate din Navisworks (Saved Viewpoints › click dreapta › Export Viewpoints): cameră, section box,
comentarii, redlines. **Sets:** search sets (condiții Item / Name, Type, GUID… sau categorie / proprietate) și seturile
explicite (după GUID) → Navisworks: Sets › Import Search Sets. **Clash Tests:** testele (tip, toleranță, selecțiile A / B)
plus seturile folosite → Navisworks: Clash Detective › Import / Export › Import Clash Tests. Toate fișierele trec
validarea cu schema oficială `nw-exchange-12.0.xsd`.

## BCF, clash-uri și issue-uri în Revit / Navisworks
- **Import BCF** (panoul Issues › 📥 Import BCF…, sau Output › Navisworks (XML) › Import BCF): BCF 2.1 / 3.0 din Revit,
  ACC, Solibri, BIMcollab → viewpoints cu cameră, status, comentarii și elementele selectate (după IFC GUID / UniqueId).
- **Clash-uri** (Clash Detective › 📤 BCF / 🧭 Navisworks): fiecare clash deschis devine un topic BCF sau un viewpoint
  Navisworks cu section box în jurul clash-ului și cele două elemente.
- **ManageBIM Issues** (butonul din tab-ul ManageBIM în Revit, Add-ins în Navisworks): viewpoints / issue-urile din
  ManageBIM se partajează singure în `%LOCALAPPDATA%\ManageBIM\Outbox\issues.txt`. Revit: alegi un issue → vedere 3D
  „ManageBIM - <issue>” cu aceeași cameră și același section box, elementele selectate. Navisworks: alegi unul sau mai multe
  → Saved Viewpoints cu comentarii (stare, prioritate, descriere). Coordonatele sunt cele partajate (shared), în metri.

## Afișare și materiale
- Se desenează doar ce e în câmpul vizual, fără elementele ascunse sau din afara section box-ului; în timpul mișcării
  elementele sub ~2 pixeli se sar (imaginea se completează la oprire). Capturile și randările desenează tot.
- Umbrire: lumină ambientală cer / sol, soare cald, lumină de umplere, luciu fin, fără zone arse.

## Nori de puncte, export IFC, lumină
- **E57** (File › Open): fiecare scanare = un element (proprietăți: număr de puncte, poziția scanerului). View › Scene ›
  **☁ Point Cloud**: densitate și mărimea punctelor. Punctele sunt amestecate la citire, așa că 10 % înseamnă o
  zecime uniformă din nor, nu o bucată. Peste 40 milioane de puncte se păstrează unul din N.
- **Export IFC4** (Output › IFC): geometrie triangulată (IfcTriangulatedFaceSet), culori, niveluri (IfcBuildingStorey),
  GUID-urile originale, toate proprietățile; clasele IFC cunoscute se păstrează, restul devin IfcBuildingElementProxy
  (tipul original în ObjectType și în setul „ManageBIM”).
- **Lumina** (View › Scene › 💡 Lighting): luminozitate, ambient, soare (intensitate, azimut, înălțime).
- **Materiale / profil de culori în Excel**: în dialogul Export alege „Excel workbook” sau „CSV”; Import citește .xlsx / .csv
  (coloane: Name, Colour #RRGGBB, Transparency %, Gloss, Metallic, Texture, Texture size m, Image /
  Value, From, To, Colour, Opacity %, Visible).
## Formate noi
- **FBX** (binar / ASCII) nativ, prin ufbx (MIT, `third_party\ufbx`): fiecare nod cu geometrie = un element, cu numele
  nodului, ierarhia (proprietatea „Hierarchy”, grupat pe nodul părinte) și culorile materialelor; metri, Z în sus.
- **.ifczip** se deschide direct (IFC-ul din arhivă).
- **.bimc v3**: geometria identică doar mutată se salvează o dată (detalii în `BIMC_FORMAT.md`).

## Unități
Lungimile din măsurători, distanța minimă și ΔX / ΔY / ΔZ se afișează în **m** sau **mm**: butonul din bara de stare
(dreapta) sau View › Options. Cantitățile (Quantities) rămân în m / m² / m³.

## Navigare și secțiune
- **Pan** (Viewpoint › Navigation Mode sau ✋ în bara de navigare): click stânga + trage mută modelul lateral.
- **Fit Section** (Viewpoint › Sectioning sau ⬚ în bara de navigare): section box în jurul elementelor selectate +
  zoom pe ele. **Follow**: cutia urmărește automat selecția (camera rămâne pe loc). Click dreapta › „Section box
  around selection” rămâne disponibil.

## Gratare Plant 3D
Plant 3D salvează gratarul doar ca un contur plat; ManageBIM îl desenează ca un gratar presat: ramă pe contur, bare
portante la 30 mm pe deschiderea scurtă și bare transversale la 100 mm, sub suprafața de călcare. **Grosimea** se ia din
baza de date a proiectului (`Piping.dcf`, dimensiunea „L x W x grosime”, unitatea dedusă automat din contur); fără ea,
30 mm. Proprietatea „Grating” arată dimensiunile și sursa lor.

## Send to ManageBIM (model trimis direct, actualizare live)
În Revit (tab **ManageBIM › Send to ManageBIM**), AutoCAD / Plant 3D (tab **ManageBIM**, comanda `BIMCSEND`) și
Navisworks (**Tool add-ins › Send to ManageBIM**): modelul se exportă fără dialog în
`%LOCALAPPDATA%\ManageBIM\Inbox\<aplicație> - <fișier>.bimc` și apare singur în ManageBIM (pornește ManageBIM dacă nu
e deschis). **Trimis din nou = actualizat pe loc** în ManageBIM, păstrând selecția, elementele ascunse, culorile și
camera (potrivire pe GUID). Fișierul se scrie atomic (`.tmp` + redenumire), deci ManageBIM nu citește niciodată
un model pe jumătate. Add-in-urile se recompilează fără .NET SDK: `revit-addin\build.ps1`, `acad-addin\build.ps1`,
`navis-addin\build.ps1` (rulate automat de `tools\make-backup.ps1`).

## Interfață
- **Teme:** Dark, Light și **Nord** (paleta nordtheme.com) - butonul din dreapta tab-urilor le schimbă pe rând,
  sau View › Theme. Ribbon-ul e mai compact (butoane mai mici, mai puțină înălțime).
- **Bara de navigare** (jos, în vizualizarea 3D): Orbit, Walk, desenează / rulează traseul, viteza de mers, Zoom All,
  Home, Focus pe selecție, Unhide all, perspectivă / ortografic. Se ascunde din butonul 🧭 din bara de stare.
- **Ribbon restrâns:** butonul ⏶ din dreapta tab-urilor (sau dublu-click pe un tab) lasă doar rândul de tab-uri;
  click pe un tab îl deschide din nou.
- Bara de stare arată doar ce contează (modele, elemente, selecție, ascunse); texte de ajutor mai scurte.

**Cum e legat (LGPL):** dinamic. `mbocct.dll` (shim C, `src/occt/mbocct.cpp`) + DLL-urile OCCT necesare stau în
subfolderul `occt\` de lângă `ManageBIM.exe` și se încarcă doar când deschizi un fișier CAD; aplicația pornește și
fără ele. Pachetul conține licența (`occt\LICENSE_LGPL_21.txt`, `occt\OCCT_LGPL_EXCEPTION.txt`) și versiunea folosită;
sursa OCCT: https://github.com/Open-Cascade-SAS/OCCT (tag `V8.0.1`). DLL-urile OCCT se pot înlocui cu o altă
compilare compatibilă (același API C al shim-ului).

**Compilare OCCT (o singură dată, fără admin, ~10-15 min):**
`powershell -ExecutionPolicy Bypass -File tools\build-occt.ps1` – descarcă sursa în `%LOCALAPPDATA%\ManageBIM-dev`,
compilează cu Visual Studio Build Tools (CMake + Ninja incluse) doar toolkit-urile necesare și instalează în
`third_party\occt\`. Apoi `cargo build --release` compilează shim-ul (`build.rs`) și copiază în `target\release\occt\`
exact DLL-urile de care are nevoie (inclusiv runtime-ul C++ MSVC, app-local). Fără `third_party\occt` build-ul merge
în continuare (doar fără cititorul nativ). Alt folder OCCT: variabila `OCCT_DIR`.

Fișierele de test (`tests/fixtures/cad/`) se regenerează cu `tools/occt-fixtures.cpp`.

## Performanță
- Geometria IFC se generează pe toate nucleele (rezultat identic cu varianta serială, aceeași ordine a elementelor).
- Selecția / ascunderea / culorile trimit la GPU doar elementele schimbate (`glBufferSubData`), nu tot modelul.

## Refresh model
Butonul **⟳ Refresh** (Home › Project) sau **F5** reîncarcă modelele ale căror fișiere s-au modificat pe disc.
Butonul se aprinde singur și arată câte fișiere s-au schimbat. **Shift+click / Ctrl+F5** reîncarcă toate modelele.
Selecția, elementele ascunse, culorile, seturile explicite și rezultatele de clash se păstrează (potrivire după GUID).

Teste: `cargo test --release`. Modele demo: `samples/` (generate cu `tools/gen_demo.py`).

## Structură
- `src/step.rs` – parser STEP (ISO 10303-21)
- `src/ifc.rs` – geometrie IFC (extrudări, revoluții, B-rep, mapped items, tessellation, swept disk, clipping), stiluri, Pset-uri, structură spațială
- `src/gltf_load.rs` – încărcare glTF/GLB
- `src/occt.rs`, `src/occt/mbocct.cpp`, `build.rs` – STEP / IGES / BREP prin OpenCascade (legare dinamică)
- `src/model.rs` – model federat, BVH per element
- `src/clash.rs` – Hard / Clearance / Duplicate (BVH-BVH, triunghi-triunghi)
- `src/timeline.rs` – TimeLiner 4D
- `src/render.rs` – randare OpenGL
- `src/app.rs`, `src/ui.rs` – stare și interfață (egui)
