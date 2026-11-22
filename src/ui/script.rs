//! Consola de scripturi: dosarul cu scripturi, lista de fisiere si sablonul de pornire.
//!
//! Interfata e gata din timp: motorul de executie (Rhai) se activeaza in 0.28,
//! iar scripturile puse in acest dosar vor aparea automat aici, fara alta schimbare.

use std::path::PathBuf;

/// Dosarul per-utilizator cu scripturi: %APPDATA%\ManageBIM\Scripts (Linux: ~/.config/...).
pub fn dir() -> PathBuf {
    let base = std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
        .unwrap_or_else(std::env::temp_dir);
    base.join("ManageBIM").join("Scripts")
}

/// Fisierele .rhai din dosar, sortate dupa nume.
pub fn list() -> Vec<PathBuf> {
    let Ok(rd) = std::fs::read_dir(dir()) else { return Vec::new() };
    let mut v: Vec<PathBuf> = rd
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|x| x.to_str()).map(|x| x.eq_ignore_ascii_case("rhai")).unwrap_or(false))
        .collect();
    v.sort();
    v
}

/// Creeaza un script nou cu un sablon scurt, fara sa suprascrie nimic.
pub fn create() -> Result<PathBuf, String> {
    let d = dir();
    std::fs::create_dir_all(&d).map_err(|e| format!("Cannot create {}: {e}", d.display()))?;
    let mut p = d.join("script.rhai");
    let mut n = 1;
    while p.exists() {
        n += 1;
        p = d.join(format!("script_{n}.rhai"));
    }
    let template = "// ManageBIM script (Rhai) - the scripting engine is enabled in 0.28\n// select_where(\"Category\", \"Walls\");\n// set_prop(\"FireRating\", \"2h\");\n// create_clash_test(\"Structure vs MEP\", 25.0);\n";
    std::fs::write(&p, template).map_err(|e| format!("Cannot write {}: {e}", p.display()))?;
    Ok(p)
}
