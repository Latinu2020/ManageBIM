//! Biblioteca profesionala de materiale PBR: texturi scanate / fotogrammetrice de pe ambientCG
//! (licenta CC0 - domeniu public, fara atribuire obligatorie, utilizabile comercial).
//! Fiecare material inlocuieste materialul inclus cu acelasi nume (ex. "Brick red"), deci regulile
//! "Auto by IFC type" folosesc automat varianta profesionala. Descarcarea se face cu `curl`
//! (inclus in Windows 10 / 11) pe un fir separat; hartile (culoare, normala OpenGL, rugozitate,
//! metalicitate, AO, inaltime) sunt importate de `materials::import_pbr`.
use crate::materials::Material;
use std::sync::{Arc, Mutex};

/// (numele din biblioteca ManageBIM, id-ul ambientCG, marimea texturii in metri pe suprafata)
pub const PRO_LIBRARY: &[(&str, &str, f32)] = &[
    ("Concrete", "Concrete042A", 2.0),
    ("Exposed concrete", "Concrete031", 2.4),
    ("Smooth concrete panel surface boracity", "Concrete034", 2.0),
    ("Brick red", "Bricks104", 1.2),
    ("Brick yellow", "Bricks105", 1.2),
    ("Plaster white", "Plaster001", 1.5),
    ("Gypsum board", "PaintedPlaster017", 2.0),
    ("Steel", "Metal055A", 1.0),
    ("Aluminium", "Metal009", 1.0),
    ("Galvanized steel", "Metal041A", 1.0),
    ("Stainless steel", "Metal012", 1.0),
    ("Copper", "Metal035", 1.0),
    ("Wood oak", "Wood049", 1.5),
    ("Wood dark", "Wood066", 1.5),
    ("Ceramic tiles", "Tiles107", 1.0),
    ("Roof tiles", "RoofingTiles012A", 1.5),
    ("Asphalt", "Asphalt033", 2.5),
    ("Grass", "Grass004", 2.0),
    ("Fabric upholstery", "Fabric061", 0.8),
    ("Marble white", "Marble012", 1.5),
    ("Granite grey", "Granite002A", 1.2),
    ("Sandstone", "Travertine009", 1.5),
    ("Terracotta", "Clay005", 1.2),
    ("Carpet grey", "Carpet016", 1.0),
    // materiale noi (nu exista in biblioteca inclusa)
    ("Parquet oak", "WoodFloor051", 1.5),
    ("Paving stones", "PavingStones153", 1.5),
    ("Porcelain white", "Porcelain001", 1.0),
    ("Corrugated steel", "CorrugatedSteel009", 1.5),
];

/// Rezolutia pachetelor: 1K (~5 MB / material) sau 2K (~15-20 MB / material).
pub fn download_url(asset: &str, res: &str) -> String {
    format!("https://ambientcg.com/get?file={asset}_{res}-JPG.zip")
}

#[derive(Default)]
pub struct Progress {
    pub total: usize,
    pub done: usize,
    pub current: String,
    pub installed: Vec<Material>,
    pub errors: Vec<String>,
    pub finished: bool,
    pub cancel: bool,
}

fn curl_download(url: &str, dest: &std::path::Path) -> Result<(), String> {
    let mut cmd = std::process::Command::new("curl");
    cmd.args(["-s", "-S", "-f", "-L", "--retry", "2", "-m", "300", "-o"]).arg(dest).arg(url);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000);
    }
    let out = cmd.output().map_err(|e| format!("curl is not available ({e})"))?;
    if !out.status.success() {
        let e = String::from_utf8_lossy(&out.stderr).trim().to_string();
        return Err(if e.is_empty() { "download failed - check the internet connection".into() } else { e });
    }
    Ok(())
}

/// Descarca si instaleaza un material; intoarce materialul gata de folosit (cu numele din biblioteca).
pub fn install_one(name: &str, asset: &str, scale: f32, res: &str, root: &std::path::Path) -> Result<Material, String> {
    let dl = root.join("_downloads");
    std::fs::create_dir_all(&dl).map_err(|e| e.to_string())?;
    let zip = dl.join(format!("{asset}_{res}-JPG.zip"));
    curl_download(&download_url(asset, res), &zip)?;
    let r = crate::materials::import_pbr(&zip.to_string_lossy(), &root.join("Pro Library"));
    let _ = std::fs::remove_file(&zip);
    let mut m = r?;
    m.name = name.to_string();
    m.scale = scale.clamp(0.05, 4.05);
    Ok(m)
}

/// Porneste instalarea intregii biblioteci pe un fir separat.
pub fn spawn_install(res: &'static str) -> Result<Arc<Mutex<Progress>>, String> {
    let root = crate::materials::user_library_dir().ok_or("APPDATA is not set")?;
    let p = Arc::new(Mutex::new(Progress { total: PRO_LIBRARY.len(), ..Default::default() }));
    let job = p.clone();
    std::thread::spawn(move || {
        for (name, asset, scale) in PRO_LIBRARY {
            {
                let mut g = job.lock().unwrap_or_else(|e| e.into_inner());
                if g.cancel {
                    break;
                }
                g.current = name.to_string();
            }
            let r = install_one(name, asset, *scale, res, &root);
            let mut g = job.lock().unwrap_or_else(|e| e.into_inner());
            match r {
                Ok(m) => g.installed.push(m),
                Err(e) => g.errors.push(format!("{name} ({asset}): {e}")),
            }
            g.done += 1;
        }
        let mut g = job.lock().unwrap_or_else(|e| e.into_inner());
        g.finished = true;
        g.current.clear();
    });
    Ok(p)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn library_is_consistent() {
        let mut names = std::collections::HashSet::new();
        for (n, a, s) in PRO_LIBRARY {
            assert!(names.insert(*n), "duplicate {n}");
            assert!(*s > 0.05 && *s <= 4.0, "{n} scale");
            assert!(a.chars().all(|c| c.is_ascii_alphanumeric()), "{a}");
        }
        assert_eq!(download_url("Bricks104", "1K"), "https://ambientcg.com/get?file=Bricks104_1K-JPG.zip");
    }

    /// Test de retea (nu ruleaza implicit): `MB_PRO_TEST_DIR=<folder> cargo test download_one -- --ignored`
    #[test]
    #[ignore]
    fn download_one() {
        let dir = std::path::PathBuf::from(std::env::var("MB_PRO_TEST_DIR").expect("MB_PRO_TEST_DIR"));
        let m = install_one("Brick red", "Bricks104", 1.2, "1K", &dir).expect("install");
        assert_eq!(m.name, "Brick red");
        assert!(m.is_pbr(), "{m:?}");
        for p in [&m.texture, &m.normal_map, &m.roughness_map] {
            assert!(std::path::Path::new(p).exists(), "missing {p}");
        }
        eprintln!("{m:#?}");
    }

    #[test]
    fn pro_materials_replace_auto_rule_targets() {
        // fiecare material folosit de "Auto by IFC type" care are textura are o varianta profesionala
        for (_, m) in crate::materials::AUTO_RULES {
            if m.starts_with("Glass") {
                continue;
            }
            assert!(PRO_LIBRARY.iter().any(|(n, ..)| *n == m), "{m} has no pro material");
        }
    }
}
