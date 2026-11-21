//! Compileaza shim-ul OpenCascade (src/occt/mbocct.cpp -> mbocct.dll) si pune DLL-urile OCCT
//! necesare in target\<profil>\occt\ (langa ManageBIM.exe). Legare DINAMICA (LGPL-2.1):
//! ManageBIM.exe nu depinde de OCCT la pornire - incarca mbocct.dll la rulare, cand deschizi
//! un STEP / IGES / BREP.
//!
//! OCCT se cauta in: %OCCT_DIR% sau third_party\occt (vezi tools\build-occt.ps1). Daca lipseste,
//! build-ul merge mai departe (cititoarele STEP / IGES / BREP / FCStd raporteaza "OCCT runtime not found").
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

include!("tools/icon_gen.rs");

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=tools/icon_gen.rs");
    // data build-ului (afisata in About)
    let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0);
    let z = secs.div_euclid(86400) + 719468;
    let era = z.div_euclid(146097);
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let (d, m) = (doy - (153 * mp + 2) / 5 + 1, if mp < 10 { mp + 3 } else { mp - 9 });
    let y = yoe + era * 400 + if m <= 2 { 1 } else { 0 };
    println!("cargo:rustc-env=MB_BUILD_DATE={y:04}-{m:02}-{d:02}");
    // iconita programului in toate exe-urile (Explorer, scurtaturi, bara de activitati)
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let out = PathBuf::from(std::env::var("OUT_DIR").unwrap());
        match compile_icon_res(&out, "") {
            Some(res) => println!("cargo:rustc-link-arg-bins={}", res.display()),
            None => println!("cargo:warning=rc.exe (Windows SDK) not found - exes built without an icon"),
        }
    }
    // FBX: ufbx (MIT, third_party/ufbx) + stratul C src/fbx/mbfbx.c, legate static
    println!("cargo:rerun-if-changed=src/fbx/mbfbx.c");
    println!("cargo:rerun-if-changed=third_party/ufbx/ufbx.c");
    cc::Build::new().file("third_party/ufbx/ufbx.c").file("src/fbx/mbfbx.c").opt_level(2).warnings(false).compile("mbfbx");
    occt();
    addin_payloads();
}

/// Instalerele add-in-urilor (binarele cu --features addins) includ fisierele din
/// installers\payload\<add-in> direct in .exe. Payload-ul e produs de scripturile C#
/// (revit-addin\build.ps1 etc.) si nu e in repo; daca lipseste, binarele se compileaza
/// in continuare si spun la rulare ce script trebuie rulat.
fn addin_payloads() {
    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let out = PathBuf::from(std::env::var("OUT_DIR").unwrap());
    for (kind, dir) in [("revit", "revit"), ("acad", "acad"), ("navisworks", "navisworks")] {
        let root = manifest.join("installers").join("payload").join(dir);
        println!("cargo:rerun-if-changed={}", root.display());
        let mut files = Vec::new();
        collect_payload(&root, &root, &mut files);
        files.sort_by(|a, b| a.0.cmp(&b.0));
        let mut src = String::from("// generat de build.rs din installers/payload\npub static FILES: &[(&str, &[u8])] = &[\n");
        for (rel, abs) in files {
            src.push_str(&format!("    ({:?}, include_bytes!({:?})),\n", rel, abs.display().to_string()));
        }
        src.push_str("];\n");
        std::fs::write(out.join(format!("addin_payload_{kind}.rs")), src).unwrap();
    }
}

fn collect_payload(root: &Path, dir: &Path, out: &mut Vec<(String, PathBuf)>) {
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        let p = e.path();
        if p.is_dir() {
            collect_payload(root, &p, out);
        } else if p.is_file() {
            if let Ok(rel) = p.strip_prefix(root) {
                out.push((rel.to_string_lossy().replace('\\', "/"), p));
            }
        }
    }
}

fn occt() {
    println!("cargo:rerun-if-changed=src/occt/mbocct.cpp");
    println!("cargo:rerun-if-env-changed=OCCT_DIR");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let occt = std::env::var_os("OCCT_DIR").map(PathBuf::from).unwrap_or_else(|| manifest.join("third_party").join("occt"));
    println!("cargo:rerun-if-changed={}", occt.join("VERSION.txt").display());
    let Some((inc, lib, bin)) = locate(&occt) else {
        println!("cargo:warning=OpenCascade not found in {} - native STEP / IGES / BREP disabled. Run tools\\build-occt.ps1.", occt.display());
        return;
    };
    let out = PathBuf::from(std::env::var("OUT_DIR").unwrap());
    // target\<profil>\build\<pachet>-<hash>\out (sau, in cargo nou, build\<pachet>\<hash>\out) -> target\<profil>\
    let profile_dir = out.ancestors().find(|a| a.file_name().map(|n| n == "build").unwrap_or(false)).and_then(|a| a.parent()).unwrap_or_else(|| out.ancestors().nth(3).unwrap()).to_path_buf();
    let dest = profile_dir.join("occt");
    std::fs::create_dir_all(&dest).unwrap();

    // ------------------------------------------------ 1. mbocct.dll (MSVC, din mediul gasit de crate-ul cc)
    let src = manifest.join("src").join("occt").join("mbocct.cpp");
    let dll = out.join("mbocct.dll");
    let libs: Vec<String> = std::fs::read_dir(&lib).unwrap().flatten().map(|e| e.file_name().to_string_lossy().to_string()).filter(|n| n.starts_with("TK") && n.ends_with(".lib")).collect();
    let compiler = cc::Build::new().cpp(true).target("x86_64-pc-windows-msvc").host("x86_64-pc-windows-msvc").opt_level(2).get_compiler();
    let mut c = compiler.to_command();
    // argumentele implicite ale cc (ex. /c) nu le vrem: comanda curata cu mediul MSVC
    c.args(["/nologo", "/O2", "/EHsc", "/MD", "/std:c++17", "/permissive-", "/W3", "/wd4996", "/DNDEBUG", "/LD", "/utf-8"]);
    c.arg(format!("/I{}", inc.display()));
    c.arg(&src);
    c.arg(format!("/Fe{}", dll.display()));
    c.arg(format!("/Fo{}\\", out.display()));
    c.arg("/link").arg(format!("/LIBPATH:{}", lib.display()));
    c.args(&libs);
    let o = c.output().expect("cl.exe could not be started");
    if !o.status.success() {
        let log = format!("{}{}", String::from_utf8_lossy(&o.stdout), String::from_utf8_lossy(&o.stderr));
        for l in log.lines().filter(|l| l.contains("error")).take(30) {
            println!("cargo:warning={l}");
        }
        panic!("mbocct.cpp failed to compile:\n{log}");
    }
    copy_if_newer(&dll, &dest.join("mbocct.dll"));

    // ------------------------------------------------ 2. inchiderea dependentelor (tabela de import PE)
    // doar DLL-urile OCCT de care chiar e nevoie + runtime-ul C++ MSVC (app-local, redistribuibil)
    let crt = crt_dir(&compiler);
    let mut need: BTreeSet<String> = BTreeSet::new();
    let mut stack = vec![dll.clone()];
    while let Some(p) = stack.pop() {
        for imp in pe_imports(&p) {
            let key = imp.to_lowercase();
            if need.contains(&key) {
                continue;
            }
            let cand = [Some(bin.join(&imp)), crt.as_ref().map(|d| d.join(&imp))];
            if let Some(f) = cand.into_iter().flatten().find(|f| f.is_file()) {
                need.insert(key);
                copy_if_newer(&f, &dest.join(f.file_name().unwrap()));
                stack.push(f);
            }
        }
    }
    for f in ["LICENSE_LGPL_21.txt", "OCCT_LGPL_EXCEPTION.txt", "VERSION.txt"] {
        let p = occt.join(f);
        if p.is_file() {
            copy_if_newer(&p, &dest.join(f));
        }
    }
    copy_if_newer(&manifest.join("src").join("occt").join("NOTICE.txt"), &dest.join("NOTICE.txt"));
    println!("cargo:rerun-if-changed=src/occt/NOTICE.txt");
    println!("cargo:rustc-env=MBOCCT_BUILD_DIR={}", dest.display());
}

/// (inc, lib, bin) din instalarea OCCT (layout Windows: inc\, win64\vc14\lib, win64\vc14\bin; sau Unix: include\opencascade, lib, bin)
fn locate(root: &Path) -> Option<(PathBuf, PathBuf, PathBuf)> {
    let inc = [root.join("inc"), root.join("include").join("opencascade")].into_iter().find(|p| p.join("Standard.hxx").is_file())?;
    let lib = find_file(root, "TKernel.lib", 4)?.parent()?.to_path_buf();
    let bin = find_file(root, "TKernel.dll", 4)?.parent()?.to_path_buf();
    Some((inc, lib, bin))
}

fn find_file(dir: &Path, name: &str, depth: u32) -> Option<PathBuf> {
    let p = dir.join(name);
    if p.is_file() {
        return Some(p);
    }
    if depth == 0 {
        return None;
    }
    let mut subs: Vec<PathBuf> = std::fs::read_dir(dir).ok()?.flatten().map(|e| e.path()).filter(|p| p.is_dir()).collect();
    subs.sort();
    subs.iter().find_map(|d| find_file(d, name, depth - 1))
}

/// Folderul Microsoft.VC14x.CRT (x64) al Visual Studio, pentru msvcp140.dll / vcruntime140*.dll
fn crt_dir(compiler: &cc::Tool) -> Option<PathBuf> {
    // ...\VC\Tools\MSVC\<ver>\bin\Hostx64\x64\cl.exe -> ...\VC\Redist\MSVC\<ver>\x64\Microsoft.VC14x.CRT
    let vc = compiler.path().ancestors().find(|p| p.file_name().map(|n| n == "VC").unwrap_or(false))?.to_path_buf();
    let redist = vc.join("Redist").join("MSVC");
    let mut vers: Vec<PathBuf> = std::fs::read_dir(&redist).ok()?.flatten().map(|e| e.path()).filter(|p| p.join("x64").is_dir()).collect();
    vers.sort();
    for v in vers.iter().rev() {
        if let Ok(rd) = std::fs::read_dir(v.join("x64")) {
            if let Some(d) = rd.flatten().map(|e| e.path()).find(|p| p.file_name().map(|n| n.to_string_lossy().ends_with(".CRT") && n.to_string_lossy().starts_with("Microsoft.VC")).unwrap_or(false)) {
                return Some(d);
            }
        }
    }
    None
}

fn copy_if_newer(from: &Path, to: &Path) {
    let newer = match (std::fs::metadata(from), std::fs::metadata(to)) {
        (Ok(a), Ok(b)) => a.len() != b.len() || a.modified().ok() > b.modified().ok(),
        _ => true,
    };
    if newer {
        std::fs::copy(from, to).unwrap_or_else(|e| panic!("copy {} -> {}: {e}", from.display(), to.display()));
    }
}

/// Numele DLL-urilor importate de un fisier PE (x64). Cititor minimal: antete + tabela de import.
fn pe_imports(path: &Path) -> Vec<String> {
    let Ok(b) = std::fs::read(path) else { return Vec::new() };
    let u16_at = |o: usize| b.get(o..o + 2).map(|s| u16::from_le_bytes([s[0], s[1]]) as usize);
    let u32_at = |o: usize| b.get(o..o + 4).map(|s| u32::from_le_bytes([s[0], s[1], s[2], s[3]]) as usize);
    let parse = || -> Option<Vec<String>> {
        let pe = u32_at(0x3c)?;
        if b.get(pe..pe + 4)? != b"PE\0\0" {
            return None;
        }
        let coff = pe + 4;
        let nsec = u16_at(coff + 2)?;
        let opt_size = u16_at(coff + 16)?;
        let opt = coff + 20;
        let magic = u16_at(opt)?;
        let dd = opt + if magic == 0x20b { 112 } else { 96 };
        let imp_rva = u32_at(dd + 8)?;
        let secs = opt + opt_size;
        let rva2off = |rva: usize| -> Option<usize> {
            (0..nsec).find_map(|i| {
                let s = secs + i * 40;
                let va = u32_at(s + 12)?;
                let size = u32_at(s + 8)?.max(u32_at(s + 16)?);
                let raw = u32_at(s + 20)?;
                (rva >= va && rva < va + size).then(|| rva - va + raw)
            })
        };
        let mut out = Vec::new();
        let mut d = rva2off(imp_rva)?;
        loop {
            let name_rva = u32_at(d + 12)?;
            if name_rva == 0 {
                break;
            }
            let o = rva2off(name_rva)?;
            let end = b[o..].iter().position(|&c| c == 0)? + o;
            out.push(String::from_utf8_lossy(&b[o..end]).to_string());
            d += 20;
        }
        Some(out)
    };
    parse().unwrap_or_default()
}
