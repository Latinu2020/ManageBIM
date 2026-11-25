//! Consola de scripturi: dosarul cu scripturi, lista de fisiere si sablonul de pornire.
//!
//! Interfata e gata din timp: motorul de executie (Rhai) se activeaza in 0.28,
//! iar scripturile puse in acest dosar vor aparea automat aici, fara alta schimbare.

use std::path::PathBuf;

/// Dosarul cu scripturi. Daca utilizatorul a ales unul in Settings, il foloseste
/// pe acela; altfel foloseste %APPDATA%\ManageBIM\Scripts (Linux: ~/.config/...).
pub fn dir(override_dir: &str) -> PathBuf {
    if !override_dir.trim().is_empty() {
        return PathBuf::from(override_dir.trim());
    }
    let base = std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
        .unwrap_or_else(std::env::temp_dir);
    base.join("ManageBIM").join("Scripts")
}

/// Fisierele .rhai din dosar, sortate dupa nume.
pub fn list(override_dir: &str) -> Vec<PathBuf> {
    let Ok(rd) = std::fs::read_dir(dir(override_dir)) else { return Vec::new() };
    let mut v: Vec<PathBuf> = rd
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|x| x.to_str()).map(|x| x.eq_ignore_ascii_case("rhai")).unwrap_or(false))
        .collect();
    v.sort();
    v
}

/// Copiaza scripturile exemplu din pachetul aplicatiei in folderul de scripturi.
/// Intoarce numarul de fisiere copiate.
pub fn install_examples(override_dir: &str) -> Result<usize, String> {
    let dest = dir(override_dir);
    std::fs::create_dir_all(&dest).map_err(|e| e.to_string())?;
    let candidates = [
        std::env::current_exe().ok().and_then(|p| p.parent().map(|p| p.join("examples").join("scripts"))),
        Some(std::path::PathBuf::from("examples/scripts")),
    ];
    let src = candidates.into_iter().flatten().find(|p| p.is_dir()).ok_or("Examples folder not found (examples/scripts).")?;
    let mut n = 0;
    for e in std::fs::read_dir(&src).map_err(|e| e.to_string())?.flatten() {
        let p = e.path();
        if p.extension().and_then(|x| x.to_str()).map(|x| x.eq_ignore_ascii_case("rhai")).unwrap_or(false) {
            std::fs::copy(&p, dest.join(p.file_name().unwrap_or_default())).map_err(|e| e.to_string())?;
            n += 1;
        }
    }
    Ok(n)
}

/// Creeaza un script nou cu un sablon scurt, fara sa suprascrie nimic.
pub fn create(override_dir: &str) -> Result<PathBuf, String> {
    let d = dir(override_dir);
    std::fs::create_dir_all(&d).map_err(|e| format!("Cannot create {}: {e}", d.display()))?;
    let mut p = d.join("script.rhai");
    let mut n = 1;
    while p.exists() {
        n += 1;
        p = d.join(format!("script_{n}.rhai"));
    }
    let template = "// ManageBIM script (Rhai)\n// select_where(\"Category\", \"Walls\");\n// set_prop(\"FireRating\", \"2h\");\n// create_clash_test(\"Structure vs MEP\", 25.0);\n";
    std::fs::write(&p, template).map_err(|e| format!("Cannot write {}: {e}", p.display()))?;
    Ok(p)
}

/// Salvează un script generat de editorul vizual cu un nume ales de utilizator.
pub fn save_named(override_dir: &str, name: &str, source: &str) -> Result<PathBuf, String> {
    let d = dir(override_dir);
    std::fs::create_dir_all(&d).map_err(|e| format!("Cannot create {}: {e}", d.display()))?;
    let base: String = name.chars().map(|c| if c.is_alphanumeric() || c == '_' || c == '-' { c } else { '_' }).collect();
    let base = if base.trim().is_empty() { "script".to_string() } else { base };
    let mut p = d.join(format!("{base}.rhai"));
    let mut n = 1;
    while p.exists() { n += 1; p = d.join(format!("{base}_{n}.rhai")); }
    std::fs::write(&p, source).map_err(|e| format!("Cannot write {}: {e}", p.display()))?;
    Ok(p)
}

// ---------------------------------------------------------------- motorul de executie

use crate::app::App;
use crate::model::ElemId;
use crate::scripting::Action as ScriptAction;

impl App {
    /// Fotografia modelului vazuta de script: proprietatile elementelor si
    /// indexii globali ai selectiei curente.
    pub(crate) fn script_input(&self) -> crate::scripting::Input {
        let mut elements = Vec::new();
        let mut selected = Vec::new();
        let mut idx = 0i32;
        for (mi, m) in self.loaded_models() {
            for (ei, e) in m.elements.iter().enumerate() {
                let mut props = vec![
                    ("Category".to_string(), e.ifc_type.clone()),
                    ("Name".to_string(), e.name.clone()),
                    ("Level".to_string(), e.storey.clone()),
                    ("GlobalId".to_string(), e.guid.clone()),
                ];
                for p in &e.props {
                    props.push((p.name.clone(), p.value.clone()));
                    props.push((format!("{}|{}", p.group, p.name), p.value.clone()));
                }
                elements.push((idx, props));
                if self.selected.contains(&ElemId { m: mi as u32, e: ei as u32 }) {
                    selected.push(idx);
                }
                idx += 1;
            }
        }
        crate::scripting::Input { elements, selected }
    }

    /// index global -> ElemId (acelasi parcurs ca in script_input)
    fn script_ids(&self, idx: &[i32]) -> Vec<ElemId> {
        let mut out = Vec::new();
        let mut i = 0i32;
        // set, nu cautare in lista: 100k din 300k elemente ar insemna minute de interfata blocata
        let want: std::collections::HashSet<i32> = idx.iter().copied().collect();
        for (mi, m) in self.loaded_models() {
            for ei in 0..m.elements.len() {
                if want.contains(&i) {
                    out.push(ElemId { m: mi as u32, e: ei as u32 });
                }
                i += 1;
            }
        }
        out
    }

    pub(super) fn run_script(&mut self, path: &str) {
        let source = match std::fs::read_to_string(path) {
            Ok(s) => s,
            Err(e) => return self.error(format!("Cannot read script: {e}")),
        };
        let name = crate::file_name(path);
        self.run_source(&name, &source);
    }

    /// Executa o sursa Rhai (din fisier sau generata de catalogul grafic).
    pub(crate) fn run_source(&mut self, name: &str, source: &str) {
        let input = self.script_input();
        let result = match crate::scripting::run(source, &input) {
            Ok(o) => o,
            Err(e) => return self.error(format!("{name}: {e}")),
        };
        for l in &result.logs {
            self.toast(l.clone());
        }
        let n = result.actions.len();
        for action in result.actions {
            match action {
                ScriptAction::SelectIds(idx) => {
                    let ids = self.script_ids(&idx);
                    self.selected = ids.into_iter().collect();
                    self.colors_dirty = true;
                }
                ScriptAction::SelectWhere(k, v) => {
                    let idx: Vec<i32> = input
                        .elements
                        .iter()
                        .filter(|(_, props)| props.iter().any(|(pk, pv)| pk.eq_ignore_ascii_case(&k) && pv.eq_ignore_ascii_case(&v)))
                        .map(|(i, _)| *i)
                        .collect();
                    let ids = self.script_ids(&idx);
                    self.selected = ids.into_iter().collect();
                    self.colors_dirty = true;
                }
                ScriptAction::SetProp(name, value) => {
                    let ids: Vec<ElemId> = self.selected.iter().copied().collect();
                    let changed = self.set_user_param(&ids, &name, &value);
                    self.toast(format!("Set {name} on {changed} elements."));
                }
                ScriptAction::CreateClash(name, tol) => {
                    self.clash_tests.push(crate::app::ClashTest::new(name));
                    if let Some(t) = self.clash_tests.last_mut() {
                        t.tol_mm = tol as f32;
                    }
                }
                ScriptAction::ZoomToSelection => {
                    if !self.selected.is_empty() {
                        let ids: Vec<ElemId> = self.selected.iter().copied().collect();
                        let bb = self.ids_aabb(&ids);
                        if bb.is_valid() {
                            self.cam.fit(&bb);
                        }
                    }
                }
                ScriptAction::ExportBcf(p) | ScriptAction::ExportExcel(p) => {
                    self.toast(format!("Script asked for export to {p} — use the Export menu to save the file."));
                }
            }
        }
        self.toast(format!("Script finished: {n} action(s)."));
    }
}
