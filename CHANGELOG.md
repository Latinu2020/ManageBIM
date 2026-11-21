# ManageBIM - ce s-a schimbat

Versiunea apare în **View › Help › About** și în proprietățile fișierului `ManageBIM-Setup-<versiune>.exe`.
La fiecare livrare: crește versiunea în `Cargo.toml` (și `installers\setup\Cargo.toml`) și adaugă o secțiune aici.

## 0.27.0-beta.1 - 2026-11-21 (test)
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
