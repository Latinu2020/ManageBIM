# ManageBIM pentru Navisworks

NWD / NWC / NWF sunt formate închise Autodesk: nu se pot citi direct. Add-in-ul folosește **API-ul oficial
Navisworks** ca să exporte modelul deschis în `.bimc` (formatul nativ ManageBIM): geometria triangulată,
culorile afișate, **toate proprietățile** (pe categorii, ca în Properties din Navisworks), GUID și nivel.

## Instalare
* **Compilat deja** (pachetul ManageBIM, `AddIns\Navisworks\`): dublu-click pe `install.cmd`.
  Se instalează în toate versiunile Navisworks Manage / Simulate găsite, doar pentru utilizatorul curent.
  `install.cmd /u` îl dezinstalează. Închide Navisworks înainte.
* **Din sursă**: `powershell -ExecutionPolicy Bypass -File navis-addin\build.ps1`
  (necesită Navisworks + Visual Studio / Build Tools 2019+; `-Version 2024` pentru altă versiune).

## Folosire
1. **În Navisworks**: deschide NWD / NWC / NWF (sau RVT, DWG, IFC...) → tab **Tool add-ins** →
   **Export to ManageBIM (.bimc)** → alegi fișierul. Deschizi `.bimc`-ul în ManageBIM.
2. **Direct din ManageBIM**: File › Open pe un `.nwd` / `.nwc` / `.nwf` / `.rvt`. ManageBIM pornește Navisworks
   în fundal (fără fereastră), convertește și încarcă modelul. A doua deschidere e instantă (cache `.bimc`).
   Necesită Navisworks instalat și licențiat pe acel PC și `nwd2bimc.exe` în `AddIns\Navisworks\`.
3. **Linie de comandă / loturi**: `nwd2bimc.exe model.nwd [iesire.bimc]`
   (ex. toate fișierele dintr-un folder: `for %f in (*.nwd) do nwd2bimc "%f"`).

## Ce se exportă
| ManageBIM | din Navisworks |
|---|---|
| un element | fiecare **obiect** Navisworks (nivelul selectat implicit, ex. un element Revit), cu toată geometria lui |
| GUID | IFC GlobalId / IfcGUID dacă există (stabil între versiuni → Compare, Refresh), altfel InstanceGuid Navisworks |
| Level | proprietatea Level / Base Constraint / Reference Level, altfel stratul (layer) |
| Type | Category (Revit), altfel clasa obiectului Navisworks |
| proprietăți | toate categoriile și valorile, cu unități (ex. „Element / Volume: 1.234 m³”) |
| culoare | culoarea și transparența afișate în Navisworks (inclusiv override-uri) |
| ascuns | elementele ascunse în Navisworks pornesc ascunse |

Coordonatele sunt cele din Navisworks (shared / project), convertite în metri.
Jurnalul exportului: `<fișier>.bimc.log` lângă rezultat (timpi pe etape, câte obiecte au fost împărțite).

## Modele mari
* Exportul scrie `.bimc` **versiunea 2, pe bucăți**: fiecare element e scris imediat și eliberat din memorie,
  deci memoria rămâne constantă oricât de mare e modelul (fără limita de 4 GB).
* „Obiectele” care sunt de fapt grupuri mari (peste 50 de noduri de geometrie - modele din 3ds / FBX,
  ansambluri) sunt împărțite în elemente separate, fiecare cu proprietățile obiectului părinte.
* ManageBIM arată progresul real al conversiei (element X din Y, triunghiuri) cât timp Navisworks lucrează.
* DWG: dacă Navisworks e instalat, ManageBIM deschide DWG-urile prin el (solide 3D, Plant 3D,
  Architecture / MEP); altfel prin ODA File Converter. Navisworks salvează un `.nwc` lângă DWG (cache-ul lui).
* Viteza e limitată de API-ul Navisworks (geometria vine vârf cu vârf prin COM): ~100 000–150 000
  triunghiuri / secundă. Pentru multe fișiere folosește `nwd2bimc <folder>` - Navisworks pornește o singură dată.
