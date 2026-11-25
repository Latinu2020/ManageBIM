//! Interfata in romana / engleza. Textele din cod sunt in engleza; `tr` intoarce traducerea romaneasca
//! cand limba aleasa e RO (ribbon: file, grupe, butoane, tooltip-uri; panouri; meniul File; bara de stare).
//! Un text fara traducere ramane in engleza.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::OnceLock;

static RO: AtomicBool = AtomicBool::new(false);

pub fn set_romanian(on: bool) {
    RO.store(on, Ordering::Relaxed);
}

pub fn is_romanian() -> bool {
    RO.load(Ordering::Relaxed)
}

fn table() -> &'static HashMap<&'static str, &'static str> {
    static T: OnceLock<HashMap<&'static str, &'static str>> = OnceLock::new();
    T.get_or_init(|| PAIRS.iter().copied().collect())
}

/// Textul in limba aleasa (engleza = textul primit).
pub fn tr(s: &str) -> &str {
    if !is_romanian() {
        return s;
    }
    table().get(s).copied().unwrap_or(s)
}

/// ca `tr`, pentru texte compuse: traduce si fiecare rand separat (tooltip-uri pe mai multe randuri).
pub fn tr_lines(s: &str) -> String {
    if !is_romanian() {
        return s.to_string();
    }
    if let Some(t) = table().get(s) {
        return t.to_string();
    }
    s.lines().map(tr).collect::<Vec<_>>().join("\n")
}

/// Cate texte au traducere (pentru teste / manual).
pub fn count() -> usize {
    PAIRS.len()
}

/// (engleza, romana)
pub const PAIRS: &[(&str, &str)] = &[
    // Visual script builder (0.28: texte in engleza, romana doar aici)
    ("1. Pick a ready-made scenario or add steps   2. Choose the values from the lists   3. ▶ Run", "1. Alege un scenariu gata făcut sau adaugă pași   2. Alege valorile din liste   3. ▶ Run"),
    ("ℹ Load a model to see the property lists and the preview.", "ℹ Încarcă un model ca să apară listele de proprietăți și previzualizarea."),
    ("⚠ Nothing will be selected: no match in the model", "⚠ Nu se selectează nimic: nicio potrivire în model"),
    ("👁 Unknown count (property with very many values)", "👁 Număr necunoscut (proprietate cu foarte multe valori)"),
    ("✔ Every element has this property filled in", "✔ Toate elementele au proprietatea completată"),
    ("👁 Written on the elements selected by the steps above", "👁 Se scrie pe elementele selectate de pașii de mai sus"),
    ("No steps yet. Press ➕ Add step or 📋 Templates.", "Niciun pas încă. Apasă ➕ Add step sau 📋 Templates."),
    // ---- file (tab-uri)
    ("Home", "Acasă"),
    ("Select", "Selecție"),
    ("Viewpoint", "Vederi"),
    ("Review", "Revizie"),
    ("Sections", "Secțiuni"),
    ("Coordination", "Coordonare"),
    ("Analyze", "Analiză"),
    ("Render", "Randare"),
    ("Export", "Export"),
    ("Share", "Partajare"),
    ("View", "Afișare"),
    ("File ", "Fișier "),
    // ---- grupe si butoane
    ("About", "Despre"),
    ("Animation", "Animație"),
    ("Append", "Adaugă"),
    ("Auto-hide", "Ascundere auto"),
    ("Camera", "Cameră"),
    ("Checker", "Verificare"),
    ("Clash", "Coliziuni"),
    ("Clash Reports", "Rapoarte coliziuni"),
    ("Clash Tests", "Teste coliziuni"),
    ("Clear", "Golește"),
    ("Commands", "Comenzi"),
    ("Compare", "Comparare"),
    ("Controls", "Comenzi"),
    ("Coordinates", "Coordonate"),
    ("Cut on Sheet", "Tăietură pe planșă"),
    ("Cut to DWG", "Tăietură în DWG"),
    ("DWG Overlay", "Suprapunere DWG"),
    ("Dark", "Întunecat"),
    ("Data", "Date"),
    ("Display", "Afișare"),
    ("Draw", "Desenează"),
    ("Edit Grids", "Editare axe"),
    ("Edit Levels", "Editare niveluri"),
    ("Enable", "Activează"),
    ("Erase", "Șterge"),
    ("Export .bimc", "Export .bimc"),
    ("Export IFC", "Export IFC"),
    ("Export Lib", "Export bibliotecă"),
    ("Find Items", "Căutare elemente"),
    ("Fit", "Încadrare"),
    ("Fit Section", "Încadrare secțiune"),
    ("Focus", "Focalizare"),
    ("Follow", "Urmărește"),
    ("Grids", "Axe"),
    ("Grids & Levels", "Axe și niveluri"),
    ("Help", "Ajutor"),
    ("Hide", "Ascunde"),
    ("Hide Unsel.", "Ascunde neselect."),
    ("Hide panels", "Ascunde panourile"),
    ("Image", "Imagine"),
    ("Images", "Imagini"),
    ("Import BCF", "Import BCF"),
    ("Import Lib", "Import bibliotecă"),
    ("Import VP", "Import vederi"),
    ("Isolate", "Izolează"),
    ("Issues", "Probleme"),
    ("Level", "Nivel"),
    ("Levels", "Niveluri"),
    ("Light", "Luminos"),
    ("Lighting", "Iluminare"),
    ("List", "Listă"),
    ("Live Link", "Legătură live"),
    ("Manual", "Manual"),
    ("Markup", "Adnotări"),
    ("Materials", "Materiale"),
    ("Measure", "Măsurare"),
    ("Model box", "Cutia modelului"),
    ("Models", "Modele"),
    ("Navigate", "Navigare"),
    ("Navigation Mode", "Mod navigare"),
    ("Open", "Deschide"),
    ("Options", "Opțiuni"),
    ("Options ⏷", "Opțiuni ⏷"),
    ("Orbit", "Orbită"),
    ("Orthographic", "Ortografic"),
    ("Output", "Ieșire"),
    ("PDF Sheets", "Planșe PDF"),
    ("Pan", "Deplasare"),
    ("Path", "Traseu"),
    ("Perspective", "Perspectivă"),
    ("Planes", "Planuri"),
    ("Point Cloud", "Nor de puncte"),
    ("Presentation", "Prezentare"),
    ("Profiler", "Profilare"),
    ("Project", "Proiect"),
    ("Projection", "Proiecție"),
    ("Prop Tree", "Arbore propr."),
    ("Properties", "Proprietăți"),
    ("Publish", "Publicare"),
    ("Quantities", "Cantități"),
    ("Redline", "Adnotare"),
    ("Redo", "Refă"),
    ("Reference", "Referință"),
    ("Relocate", "Relocare"),
    ("Reset", "Resetare"),
    ("Same Category", "Aceeași categorie"),
    ("Same Type", "Același tip"),
    ("Save", "Salvează"),
    ("Save View", "Salvează vederea"),
    ("Saved Viewpoints", "Vederi salvate"),
    ("Scan", "Scanare"),
    ("Scan vs Model", "Scanare vs model"),
    ("Scene", "Scenă"),
    ("Schedule", "Planificare"),
    ("Section", "Secțiune"),
    ("Sectioning", "Secționare"),
    ("Select All", "Selectează tot"),
    ("Select Similar", "Selectează similare"),
    ("Select & Search", "Selecție și căutare"),
    ("Selection", "Selecție"),
    ("Set Home", "Setează Acasă"),
    ("Sets", "Seturi"),
    ("Shared Issues", "Probleme partajate"),
    ("Show Mat.", "Arată mat."),
    ("Standard Views", "Vederi standard"),
    ("Start", "Start"),
    ("Refresh", "Reîncarcă"),
    ("Stop", "Oprește"),
    ("Sync", "Sincronizare"),
    ("Sync Selection", "Sincronizare selecție"),
    ("Textures", "Texturi"),
    ("Theme", "Temă"),
    ("Title Block", "Cartuș"),
    ("To Animation", "În animație"),
    ("Tree", "Arbore"),
    ("Undo", "Anulează"),
    ("Unhide", "Reafișează"),
    ("Unhide All", "Reafișează tot"),
    ("Viewpoints", "Vederi"),
    ("Viewpoints & Issues", "Vederi și probleme"),
    ("Visibility", "Vizibilitate"),
    ("Visual Style", "Stil vizual"),
    ("Visuals", "Aspect"),
    ("Walk", "Mers"),
    ("Walk Path", "Traseu de mers"),
    ("Web viewer", "Vizualizator web"),
    ("AI connection", "Conexiune AI"),
    ("AI connection (MCP)", "Conexiune AI (MCP)"),
    ("Enable MCP server", "Pornește serverul MCP"),
    ("Allow changes", "Permite modificări"),
    ("Port", "Port"),
    ("Token", "Token"),
    ("New token", "Token nou"),
    ("Copy", "Copiază"),
    ("Restart", "Repornește"),
    ("Run in a terminal (once):", "Rulează o dată într-un terminal:"),
    ("Off = read-only: properties, selection queries and model info only", "Oprit = doar citire: proprietăți, interogări și informații despre model"),
    ("Clients using the old token must be updated", "Clienții care folosesc tokenul vechi trebuie actualizați"),
    ("Window", "Fereastră"),
    ("Windows", "Ferestre"),
    ("Zoom All", "Zoom tot"),
    ("Zoom to It", "Zoom pe el"),
    ("Language", "Limbă"),
    // ---- panouri (titlu cu iconita si nume scurt)
    ("🌲 Selection Tree", "🌲 Arbore selecție"),
    ("🔎 Property Tree", "🔎 Arbore proprietăți"),
    ("📋 Sets", "📋 Seturi"),
    ("ℹ Properties", "ℹ Proprietăți"),
    ("🔍 Find Items", "🔍 Căutare elemente"),
    ("⚠ Clash Detective", "⚠ Detectare coliziuni"),
    ("📅 TimeLiner", "📅 TimeLiner (4D)"),
    ("📷 Viewpoints", "📷 Vederi"),
    ("✂ Sectioning", "✂ Secționare"),
    ("📏 Measure", "📏 Măsurare"),
    ("📐 Coordinates", "📐 Coordonate"),
    ("🎬 Render", "🎬 Randare"),
    ("📁 Project", "📁 Proiect"),
    ("🎨 Appearance Profiler", "🎨 Profilare aspect"),
    ("⚖ Compare", "⚖ Comparare"),
    ("📊 Quantification", "📊 Cantități"),
    ("✔ Model Checker", "✔ Verificare model"),
    ("Find", "Căutare"),
    // ---- meniul File si bara de stare
    ("New project", "Proiect nou"),
    ("Open…", "Deschide…"),
    ("Save project", "Salvează proiectul"),
    ("Save project as…", "Salvează proiectul ca…"),
    ("Recent", "Recente"),
    ("Settings", "Setări"),
    ("Start page", "Pagina de start"),
    ("Exit", "Ieșire"),
    ("Ready", "Gata"),
    ("Selected", "Selectate"),
    ("Hidden", "Ascunse"),
    ("Elements", "Elemente"),
    // ---- tooltip-uri frecvente
    ("Open a model (replaces the scene)  Ctrl+O", "Deschide un model (înlocuiește scena)  Ctrl+O"),
    ("Append a model to the federated scene", "Adaugă un model în scena federată"),
    ("Go to the Home view (Home key)", "Mergi la vederea Acasă (tasta Home)"),
    ("Light theme", "Temă luminoasă"),
    ("Dark theme", "Temă întunecată"),
    ("Edit the title block, the sheet format (A3 - A0) and the scale", "Editează cartușul, formatul planșei (A3 - A0) și scara"),
    ("Lay a 2D drawing (DWG / DXF) over the model at a level: compare the plan with the model", "Așază un desen 2D (DWG / DXF) peste model la un nivel: compari planul cu modelul"),
    ("(not available now)", "(indisponibil acum)"),
    ("📂 Open Model…  (Ctrl+O)", "📂 Deschide model…  (Ctrl+O)"),
    ("➕ Append Model…", "➕ Adaugă model…"),
    ("⟳ Refresh Changed Models  (F5)", "⟳ Reîncarcă modelele modificate  (F5)"),
    ("⟳ Reload All Models  (Ctrl+F5)", "⟳ Reîncarcă toate modelele  (Ctrl+F5)"),
    ("📁 Open Project (.bimr)…", "📁 Deschide proiect (.bimr)…"),
    ("💾 Save Project  (Ctrl+S)", "💾 Salvează proiectul  (Ctrl+S)"),
    ("Save Project As…", "Salvează proiectul ca…"),
    ("Clear list", "Golește lista"),
    ("📦 Export Model Cache (.bimc)", "📦 Export cache model (.bimc)"),
    ("New Project (close all)", "Proiect nou (închide tot)"),
    ("📦 Publish .bimd…  (project + models in one file)", "📦 Publică .bimd…  (proiect + modele într-un fișier)"),
    ("🏠 Start Page", "🏠 Pagina de start"),
    ("🌐 Română / English", "🌐 English / Română"),
    ("Interface language: English / Română", "Limba interfeței: English / Română"),
    // ---- path trace (progres / ETA / finalizare)
    ("Path trace FINISHED", "Path trace GATA"),
    ("Export PNG is ready", "Export PNG este disponibil"),
    ("samples", "mostre"),
    ("Preparing scene / first sample", "Pregătire scenă / prima mostră"),
    ("Finished", "Terminat"),
    ("left", "rămase"),
    ("Ready - press Export PNG to save the image.", "Gata — apasă Export PNG pentru a salva imaginea."),
    ("Cut View", "Vedere tăietură"),
    ("Highlight plane", "Evidențiază planul"),
    // ---- sectiuni / alte functii noi
    ("Current section", "Secțiunea curentă"),
    ("Front", "Față"),
    ("Back", "Spate"),
    ("Left", "Stânga"),
    ("Right", "Dreapta"),
    ("Top", "Sus"),
    ("Bottom", "Jos"),
];
