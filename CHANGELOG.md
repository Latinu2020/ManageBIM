# ManageBIM - ce s-a schimbat

Versiunea apare în **View › Help › About** și în proprietățile fișierului `ManageBIM-Setup-<versiune>.exe`.
La fiecare livrare: crește versiunea în `Cargo.toml` (și `installers\setup\Cargo.toml`) și adaugă o secțiune aici.

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
