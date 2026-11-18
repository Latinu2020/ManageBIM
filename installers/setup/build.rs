//! Impacheteaza folderul pachetului (MB_PACKAGE_DIR: install.ps1 + ManageBIM\...) intr-un singur blob
//! comprimat (deflate) inclus in ManageBIM-Setup.exe, si pune iconita programului pe exe.
//! Format: "MBPK1", u32 numar fisiere, apoi per fisier: u16 lungime nume, nume UTF-8 (cale relativa cu '/'),
//! u64 marime originala, u64 marime comprimata, datele.
use std::path::{Path, PathBuf};

include!("../../tools/icon_gen.rs");

fn walk(dir: &Path, base: &Path, out: &mut Vec<(String, PathBuf)>) {
    let mut entries: Vec<PathBuf> = std::fs::read_dir(dir).expect("package dir").flatten().map(|e| e.path()).collect();
    entries.sort();
    for p in entries {
        if p.is_dir() {
            walk(&p, base, out);
        } else {
            let rel = p.strip_prefix(base).unwrap().to_string_lossy().replace('\\', "/");
            out.push((rel, p));
        }
    }
}

fn main() {
    println!("cargo:rerun-if-env-changed=MB_PACKAGE_DIR");
    println!("cargo:rerun-if-changed=build.rs");
    let pkg = PathBuf::from(std::env::var("MB_PACKAGE_DIR").unwrap_or_else(|_| "../../target/setup-package".into()));
    let out = PathBuf::from(std::env::var("OUT_DIR").unwrap());
    let mut files = Vec::new();
    if pkg.join("install.ps1").is_file() {
        walk(&pkg, &pkg, &mut files);
        println!("cargo:rerun-if-changed={}", pkg.display());
        for (_, p) in &files {
            println!("cargo:rerun-if-changed={}", p.display());
        }
    } else {
        println!("cargo:warning=MB_PACKAGE_DIR ({}) has no install.ps1 - building an empty installer (run tools\\make-setup.ps1)", pkg.display());
    }
    let mut blob = b"MBPK1".to_vec();
    blob.extend_from_slice(&(files.len() as u32).to_le_bytes());
    for (rel, p) in &files {
        let data = std::fs::read(p).unwrap();
        let comp = miniz_oxide::deflate::compress_to_vec(&data, 9);
        blob.extend_from_slice(&(rel.len() as u16).to_le_bytes());
        blob.extend_from_slice(rel.as_bytes());
        blob.extend_from_slice(&(data.len() as u64).to_le_bytes());
        blob.extend_from_slice(&(comp.len() as u64).to_le_bytes());
        blob.extend_from_slice(&comp);
    }
    std::fs::write(out.join("payload.bin"), blob).unwrap();
    // iconita + informatiile de versiune afisate de Windows (Properties > Details), din Cargo.toml
    let v = std::env::var("CARGO_PKG_VERSION").unwrap_or_else(|_| "0.0.0".into());
    let num = v.split('.').map(|x| x.parse::<u32>().unwrap_or(0)).chain(std::iter::repeat(0)).take(4).map(|x| x.to_string()).collect::<Vec<_>>().join(",");
    let ver = format!(
        r#"
1 VERSIONINFO
FILEVERSION {num}
PRODUCTVERSION {num}
BEGIN
  BLOCK "StringFileInfo"
  BEGIN
    BLOCK "040904b0"
    BEGIN
      VALUE "FileDescription", "ManageBIM Setup"
      VALUE "ProductName", "ManageBIM"
      VALUE "FileVersion", "{v}"
      VALUE "ProductVersion", "{v}"
      VALUE "OriginalFilename", "ManageBIM-Setup.exe"
    END
  END
  BLOCK "VarFileInfo"
  BEGIN
    VALUE "Translation", 0x409, 1200
  END
END
"#
    );
    let ver = ver.as_str();
    match compile_icon_res(&out, ver) {
        Some(res) => println!("cargo:rustc-link-arg-bins={}", res.display()),
        None => println!("cargo:warning=rc.exe not found - setup built without an icon"),
    }
}
