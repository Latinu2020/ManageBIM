//! Descarca biblioteca profesionala de materiale (ambientCG, CC0) intr-un folder care se include
//! in installer ca biblioteca implicita a aplicatiei (`<app>\materials\`).
//!
//! Ruleaza: cargo run --release --bin fetch_pro_library [-- <folder> <1K|2K>]
//! Implicit: third_party\pbr-library, 1K. Scrie `library.bimmat` cu cai RELATIVE la folder,
//! ca biblioteca sa functioneze oriunde e instalata aplicatia.
use bimrust::pro_materials::{install_one, PRO_LIBRARY};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let root = std::path::PathBuf::from(args.first().cloned().unwrap_or_else(|| "third_party/pbr-library".into()));
    let res = args.get(1).cloned().unwrap_or_else(|| "1K".into());
    std::fs::create_dir_all(&root).expect("create folder");
    let root = root.canonicalize().expect("folder");
    let mut out = Vec::new();
    let mut failed = 0;
    for (i, (name, asset, scale)) in PRO_LIBRARY.iter().enumerate() {
        print!("[{}/{}] {name} ({asset}) ... ", i + 1, PRO_LIBRARY.len());
        match install_one(name, asset, *scale, &res, &root) {
            Ok(mut m) => {
                // cai relative la folderul bibliotecii (separator '/')
                let rel = |p: &mut String| {
                    if p.is_empty() {
                        return;
                    }
                    let pb = std::path::PathBuf::from(&*p);
                    let pb = pb.canonicalize().unwrap_or(pb);
                    if let Ok(r) = pb.strip_prefix(&root) {
                        *p = r.to_string_lossy().replace('\\', "/");
                    }
                };
                for p in [&mut m.texture, &mut m.normal_map, &mut m.roughness_map, &mut m.metallic_map, &mut m.ao_map, &mut m.height_map] {
                    rel(p);
                }
                println!("ok");
                out.push(m);
            }
            Err(e) => {
                println!("FAILED: {e}");
                failed += 1;
            }
        }
    }
    let _ = std::fs::remove_dir_all(root.join("_downloads"));
    // fara hartile DirectX si previzualizari: nu sunt folosite (installer mai mic)
    if let Ok(dirs) = std::fs::read_dir(root.join("Pro Library")) {
        for d in dirs.flatten() {
            if let Ok(files) = std::fs::read_dir(d.path()) {
                for f in files.flatten() {
                    let n = f.file_name().to_string_lossy().to_ascii_lowercase();
                    if n.contains("normaldx") || n.ends_with(".png") {
                        let _ = std::fs::remove_file(f.path());
                    }
                }
            }
        }
    }
    std::fs::write(root.join("library.bimmat"), serde_json::to_string_pretty(&out).unwrap()).expect("write library.bimmat");
    std::fs::write(root.join("LICENSE.txt"), "PBR textures from ambientCG (https://ambientcg.com), licensed CC0 1.0 Universal (public domain).\n").ok();
    println!("{} materials in {} ({failed} failed)", out.len(), root.display());
    if failed > 0 {
        std::process::exit(1);
    }
}
