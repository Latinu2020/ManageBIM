//! Instalarea / dezinstalarea add-in-urilor ManageBIM pentru utilizatorul curent, fara
//! drepturi de administrator. Fisierele add-in-ului sunt incluse in binarul installerului
//! la compilare (build.rs le citeste din `installers\payload`); aici doar se copiaza in
//! versiunile de Revit / AutoCAD / Navisworks gasite pe calculator.
//!
//! Logica e separata de binare (`src/bin/addin/*.rs`) ca sa poata fi testata fara
//! registru si fara instalari reale: toate caile vin ca parametri.

use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Revit,
    Acad,
    Navisworks,
}

impl Kind {
    pub fn label(self) -> &'static str {
        match self {
            Kind::Revit => "Revit",
            Kind::Acad => "AutoCAD",
            Kind::Navisworks => "Navisworks",
        }
    }
}

/// O operatiune de instalare: ce add-in, cu ce fisiere, unde si in ce sens.
pub struct Job<'a> {
    pub kind: Kind,
    /// fisierele din pachet: nume relativ cu '/' (ex. "2025/ManageBIM.NwExporter.dll")
    pub files: &'a [(&'a str, &'a [u8])],
    /// %APPDATA% (sau folderul de test)
    pub appdata: &'a Path,
    /// %ProgramFiles% (sau folderul de test)
    pub program_files: &'a Path,
    pub uninstall: bool,
}

/// Ruleaza instalarea si intoarce liniile de raport (aceeasi forma ca inainte:
/// "Revit 2024: installed" / "AutoCAD: removed").
pub fn run(job: &Job) -> Vec<String> {
    let mut out = Vec::new();
    match job.kind {
        Kind::Revit => revit(job, &mut out),
        Kind::Acad => acad(job, &mut out),
        Kind::Navisworks => navisworks(job, &mut out),
    }
    out
}

// ------------------------------------------------------------------ utilitare

/// fisierul din pachet (numele e relativ, cu '/' ca separator)
fn payload<'a>(files: &'a [(&str, &[u8])], name: &str) -> Option<&'a [u8]> {
    files.iter().find(|(n, _)| n.eq_ignore_ascii_case(name)).map(|(_, b)| *b)
}

fn write(path: &Path, data: &[u8]) -> bool {
    if let Some(dir) = path.parent() {
        if std::fs::create_dir_all(dir).is_err() {
            return false;
        }
    }
    std::fs::write(path, data).is_ok()
}

fn remove(path: &Path) -> bool {
    if path.is_dir() {
        std::fs::remove_dir_all(path).is_ok()
    } else if path.is_file() {
        std::fs::remove_file(path).is_ok()
    } else {
        false
    }
}

/// Subfolderele `<prefix><an>` din %ProgramFiles%\Autodesk (ex. "Revit 2024").
fn autodesk_years(program_files: &Path, prefix: &str) -> Vec<u32> {
    let root = program_files.join("Autodesk");
    let Ok(rd) = std::fs::read_dir(root) else { return Vec::new() };
    let mut out: Vec<u32> = rd
        .flatten()
        .filter(|e| e.path().is_dir())
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().to_string();
            let rest = name.strip_prefix(prefix)?;
            rest.trim().parse().ok()
        })
        .collect();
    out.sort_unstable();
    out.dedup();
    out
}

// ------------------------------------------------------------------ Revit

fn revit(job: &Job, out: &mut Vec<String>) {
    let addins = job.appdata.join("Autodesk").join("Revit").join("Addins");
    let installed = autodesk_years(job.program_files, "Revit ");
    let years: Vec<u32> = if job.uninstall { (2023..=2027).collect() } else { installed.clone() };
    if !job.uninstall && installed.is_empty() {
        out.push("Revit: no installed version found".into());
        return;
    }
    for y in years {
        let dir = addins.join(y.to_string());
        let target = dir.join("ManageBIM");
        if job.uninstall {
            let a = remove(&target);
            let b = remove(&dir.join("ManageBIM.addin"));
            if a || b {
                out.push(format!("Revit {y}: removed"));
            }
            continue;
        }
        // Revit 2026 si mai nou folosesc API-ul / DLL-ul pentru 2026
        let dll_year = y.min(2026);
        let Some(dll) = payload(job.files, &format!("ManageBIM.RevitExporter.{dll_year}.dll")) else {
            out.push(format!("Revit {y}: no DLL for {dll_year} in this package"));
            continue;
        };
        let Some(addin) = payload(job.files, "ManageBIM.addin") else {
            out.push(format!("Revit {y}: ManageBIM.addin missing from this package"));
            continue;
        };
        let mut ok = write(&target.join("ManageBIM.RevitExporter.dll"), dll);
        ok &= write(&dir.join("ManageBIM.addin"), addin);
        for png in ["managebim16.png", "managebim32.png"] {
            if let Some(b) = payload(job.files, png) {
                ok &= write(&target.join(png), b);
            }
        }
        if ok {
            out.push(format!("Revit {y}: installed"));
        } else {
            out.push(format!("Revit {y}: could not write to {}", dir.display()));
        }
    }
}

// ------------------------------------------------------------------ AutoCAD / Plant 3D

fn acad(job: &Job, out: &mut Vec<String>) {
    let bundle = job.appdata.join("Autodesk").join("ApplicationPlugins").join("ManageBIM.bundle");
    if job.uninstall {
        let _ = acad_trust(&bundle, job, false);
        if remove(&bundle) {
            out.push("AutoCAD: removed".into());
        }
        return;
    }
    // Nu lasam un bundle pe jumatate: fara macar un DLL nu are ce incarca AutoCAD-ul.
    let has_dll = ["ManageBIM.AcadExporter.R24.dll", "ManageBIM.AcadExporter.R25.dll"]
        .iter()
        .any(|d| payload(job.files, d).is_some());
    if !has_dll {
        out.push("AutoCAD: no add-in DLL in this package".into());
        return;
    }
    let mut ok = false;
    if let Some(xml) = payload(job.files, "PackageContents.xml") {
        ok |= write(&bundle.join("PackageContents.xml"), xml);
    }
    // PackageContents.xml incarca ./Contents/R24/ManageBIM.AcadExporter.dll (si R25)
    for (r, dll) in [("R24", "ManageBIM.AcadExporter.R24.dll"), ("R25", "ManageBIM.AcadExporter.R25.dll")] {
        if let Some(b) = payload(job.files, dll) {
            ok |= write(&bundle.join("Contents").join(r).join("ManageBIM.AcadExporter.dll"), b);
        }
    }
    if ok {
        out.push("AutoCAD: installed".into());
        // fara dialogul "Unsigned Executable File - Always Load" la fiecare pornire: folderul devine de incredere
        if let Some(n) = acad_trust(&bundle, job, true) {
            if n > 0 {
                out.push(format!("AutoCAD: add-in folder trusted in {n} profile(s)"));
            }
        }
    } else {
        out.push("AutoCAD: no add-in DLL in this package".into());
    }
}

/// Adauga (sau scoate) folderul bundle-ului in TRUSTEDPATHS din fiecare profil AutoCAD al utilizatorului
/// (HKCU\Software\Autodesk\AutoCAD\R*\ACAD-*\Profiles\*\Variables). Doar pentru %APPDATA% real (nu in teste).
/// AutoCAD trebuie sa fie inchis: la iesire isi rescrie profilul.
fn acad_trust(bundle: &Path, job: &Job, add: bool) -> Option<usize> {
    let real = std::env::var_os("APPDATA").map(PathBuf::from)?;
    if real != job.appdata {
        return None;
    }
    if !cfg!(windows) {
        return None;
    }
    let entry = format!("{}\\...", bundle.display());
    let q = std::process::Command::new("reg").args(["query", "HKCU\\Software\\Autodesk\\AutoCAD", "/s", "/f", "Variables", "/k"]).output().ok()?;
    let keys: Vec<String> = String::from_utf8_lossy(&q.stdout)
        .lines()
        .map(|l| l.trim().to_string())
        .filter(|l| l.starts_with("HKEY_CURRENT_USER") && l.contains("\\Profiles\\") && l.ends_with("\\Variables"))
        .collect();
    let mut n = 0;
    for k in keys {
        let cur = std::process::Command::new("reg").args(["query", &k, "/v", "TRUSTEDPATHS"]).output().ok();
        let value = cur
            .and_then(|o| String::from_utf8_lossy(&o.stdout).lines().find(|l| l.contains("TRUSTEDPATHS")).and_then(|l| l.split("REG_SZ").nth(1)).map(|v| v.trim().to_string()))
            .unwrap_or_default();
        let mut parts: Vec<String> = value.split(';').filter(|p| !p.trim().is_empty()).map(|p| p.trim().to_string()).collect();
        let has = parts.iter().any(|p| p.eq_ignore_ascii_case(&entry));
        if add == has {
            continue;
        }
        if add {
            parts.push(entry.clone());
        } else {
            parts.retain(|p| !p.eq_ignore_ascii_case(&entry));
        }
        let ok = std::process::Command::new("reg").args(["add", &k, "/v", "TRUSTEDPATHS", "/t", "REG_SZ", "/d", &parts.join(";"), "/f"]).output().map(|o| o.status.success()).unwrap_or(false);
        if ok {
            n += 1;
        }
    }
    Some(n)
}

// ------------------------------------------------------------------ Navisworks

/// (edition, year) pentru fiecare "Navisworks <Manage|Simulate> <an>" instalat.
fn navisworks_installed(program_files: &Path) -> Vec<(String, u32)> {
    let root = program_files.join("Autodesk");
    let Ok(rd) = std::fs::read_dir(root) else { return Vec::new() };
    let mut out: Vec<(String, u32)> = rd
        .flatten()
        .filter(|e| e.path().is_dir())
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().to_string();
            let rest = name.strip_prefix("Navisworks ")?;
            let (edition, year) = rest.rsplit_once(' ')?;
            Some((edition.to_string(), year.trim().parse().ok()?))
        })
        .collect();
    out.sort_by(|a, b| a.1.cmp(&b.1).then(a.0.cmp(&b.0)));
    out.dedup();
    out
}

/// Toate folderele de add-in-uri Navisworks din %APPDATA% (pentru dezinstalare).
fn navisworks_roots(appdata: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    if let Ok(rd) = std::fs::read_dir(appdata.join("Autodesk")) {
        for e in rd.flatten() {
            let n = e.file_name().to_string_lossy().to_string();
            if n.starts_with("Navisworks ") && e.path().is_dir() {
                out.push(e.path());
            }
        }
    }
    if let Ok(rd) = std::fs::read_dir(appdata) {
        for e in rd.flatten() {
            let n = e.file_name().to_string_lossy().to_string();
            if n.starts_with("Autodesk Navisworks ") && e.path().is_dir() {
                out.push(e.path());
            }
        }
    }
    out
}

fn navisworks(job: &Job, out: &mut Vec<String>) {
    if job.uninstall {
        let mut any = false;
        for root in navisworks_roots(job.appdata) {
            any |= remove(&root.join("Plugins").join("ManageBIM.NwExporter"));
        }
        if any {
            out.push("Navisworks: removed".into());
        }
        return;
    }
    let installed = navisworks_installed(job.program_files);
    if installed.is_empty() {
        out.push("Navisworks: no installed version found".into());
        return;
    }
    for (edition, year) in installed {
        let Some(dll) = payload(job.files, &format!("{year}/ManageBIM.NwExporter.dll")) else {
            out.push(format!("Navisworks {edition} {year}: no DLL for {year} in this package"));
            continue;
        };
        // 2024+ scrie in "Autodesk\Navisworks ..."; versiunile vechi in "Autodesk Navisworks ..."
        let new = job.appdata.join("Autodesk").join(format!("Navisworks {edition} {year}"));
        let old = job.appdata.join(format!("Autodesk Navisworks {edition} {year}"));
        let base = if year >= 2024 || new.is_dir() { new } else { old };
        let dir = base.join("Plugins").join("ManageBIM.NwExporter");
        if write(&dir.join("ManageBIM.NwExporter.dll"), dll) {
            out.push(format!("Navisworks {edition} {year}: installed"));
        } else {
            out.push(format!("Navisworks {edition} {year}: could not write to {}", dir.display()));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("managebim_addin_{name}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    const REVIT: &[(&str, &[u8])] = &[
        ("ManageBIM.addin", b"<RevitAddIns/>"),
        ("managebim16.png", b"png16"),
        ("managebim32.png", b"png32"),
        ("ManageBIM.RevitExporter.2024.dll", b"dll2024"),
        ("ManageBIM.RevitExporter.2026.dll", b"dll2026"),
    ];

    #[test]
    fn revit_installs_only_in_installed_versions_and_reuses_2026_for_2027() {
        let root = tmp("revit");
        let appdata = root.join("appdata");
        let pf = root.join("pf");
        std::fs::create_dir_all(pf.join("Autodesk/Revit 2024")).unwrap();
        std::fs::create_dir_all(pf.join("Autodesk/Revit 2027")).unwrap();
        let msgs = run(&Job { kind: Kind::Revit, files: REVIT, appdata: &appdata, program_files: &pf, uninstall: false });
        assert_eq!(msgs, vec!["Revit 2024: installed", "Revit 2027: installed"]);
        let d24 = appdata.join("Autodesk/Revit/Addins/2024");
        assert_eq!(std::fs::read(d24.join("ManageBIM/ManageBIM.RevitExporter.dll")).unwrap(), b"dll2024");
        assert!(d24.join("ManageBIM.addin").is_file());
        assert!(d24.join("ManageBIM/managebim16.png").is_file());
        // 2027 foloseste DLL-ul de 2026
        let d27 = appdata.join("Autodesk/Revit/Addins/2027");
        assert_eq!(std::fs::read(d27.join("ManageBIM/ManageBIM.RevitExporter.dll")).unwrap(), b"dll2026");
        // dezinstalare: dispare tot, in toate versiunile
        let msgs = run(&Job { kind: Kind::Revit, files: REVIT, appdata: &appdata, program_files: &pf, uninstall: true });
        assert!(msgs.contains(&"Revit 2024: removed".to_string()));
        assert!(!d24.join("ManageBIM").exists() && !d24.join("ManageBIM.addin").exists());
        assert!(!d27.join("ManageBIM").exists() && !d27.join("ManageBIM.addin").exists());
    }

    #[test]
    fn revit_without_versions_says_so() {
        let root = tmp("revit_none");
        let pf = root.join("pf");
        std::fs::create_dir_all(&pf).unwrap();
        let msgs = run(&Job { kind: Kind::Revit, files: REVIT, appdata: &root.join("a"), program_files: &pf, uninstall: false });
        assert_eq!(msgs, vec!["Revit: no installed version found"]);
    }

    #[test]
    fn acad_writes_the_paths_from_package_contents() {
        let root = tmp("acad");
        let files: &[(&str, &[u8])] = &[
            ("PackageContents.xml", b"<ApplicationPackage/>"),
            ("ManageBIM.AcadExporter.R24.dll", b"r24"),
            ("ManageBIM.AcadExporter.R25.dll", b"r25"),
        ];
        let msgs = run(&Job { kind: Kind::Acad, files, appdata: &root, program_files: &root, uninstall: false });
        assert_eq!(msgs, vec!["AutoCAD: installed"]);
        let b = root.join("Autodesk/ApplicationPlugins/ManageBIM.bundle");
        assert!(b.join("PackageContents.xml").is_file());
        assert_eq!(std::fs::read(b.join("Contents/R24/ManageBIM.AcadExporter.dll")).unwrap(), b"r24");
        assert_eq!(std::fs::read(b.join("Contents/R25/ManageBIM.AcadExporter.dll")).unwrap(), b"r25");
        assert_eq!(run(&Job { kind: Kind::Acad, files, appdata: &root, program_files: &root, uninstall: true }), vec!["AutoCAD: removed"]);
        assert!(!b.exists());
    }

    #[test]
    fn navisworks_uses_the_legacy_folder_for_old_versions() {
        let root = tmp("navis");
        let pf = root.join("pf");
        std::fs::create_dir_all(pf.join("Autodesk/Navisworks Manage 2023")).unwrap();
        std::fs::create_dir_all(pf.join("Autodesk/Navisworks Simulate 2025")).unwrap();
        let files: &[(&str, &[u8])] = &[("2023/ManageBIM.NwExporter.dll", b"n23"), ("2025/ManageBIM.NwExporter.dll", b"n25")];
        let msgs = run(&Job { kind: Kind::Navisworks, files, appdata: &root, program_files: &pf, uninstall: false });
        assert_eq!(msgs, vec!["Navisworks Manage 2023: installed", "Navisworks Simulate 2025: installed"]);
        // 2023 -> folderul vechi, fara "Autodesk\"
        assert_eq!(
            std::fs::read(root.join("Autodesk Navisworks Manage 2023/Plugins/ManageBIM.NwExporter/ManageBIM.NwExporter.dll")).unwrap(),
            b"n23"
        );
        // 2025 -> folderul nou
        assert_eq!(
            std::fs::read(root.join("Autodesk/Navisworks Simulate 2025/Plugins/ManageBIM.NwExporter/ManageBIM.NwExporter.dll")).unwrap(),
            b"n25"
        );
        assert_eq!(run(&Job { kind: Kind::Navisworks, files, appdata: &root, program_files: &pf, uninstall: true }), vec!["Navisworks: removed"]);
        assert!(!root.join("Autodesk Navisworks Manage 2023/Plugins/ManageBIM.NwExporter").exists());
    }

    #[test]
    fn missing_payload_is_reported_not_written() {
        let root = tmp("empty");
        let pf = root.join("pf");
        std::fs::create_dir_all(pf.join("Autodesk/Revit 2024")).unwrap();
        let msgs = run(&Job { kind: Kind::Revit, files: &[], appdata: &root, program_files: &pf, uninstall: false });
        assert_eq!(msgs, vec!["Revit 2024: no DLL for 2024 in this package"]);
        // AutoCAD: doar PackageContents.xml, fara DLL -> nu se creeaza bundle-ul
        let files: &[(&str, &[u8])] = &[("PackageContents.xml", b"<ApplicationPackage/>")];
        let msgs = run(&Job { kind: Kind::Acad, files, appdata: &root, program_files: &pf, uninstall: false });
        assert_eq!(msgs, vec!["AutoCAD: no add-in DLL in this package"]);
        assert!(!root.join("Autodesk/ApplicationPlugins/ManageBIM.bundle").exists());
    }
}
