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
    let template = "// ManageBIM script (Rhai) - the scripting engine is enabled in 0.28\n// select_where(\"Category\", \"Walls\");\n// set_prop(\"FireRating\", \"2h\");\n// create_clash_test(\"Structure vs MEP\", 25.0);\n";
    std::fs::write(&p, template).map_err(|e| format!("Cannot write {}: {e}", p.display()))?;
    Ok(p)
}
