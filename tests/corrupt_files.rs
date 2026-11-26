//! Robustness: truncated files or files with damaged bytes must never stop the app (panic).
//! Formats: IFC, DXF, OBJ, PLY, FBX, STL, .bimc, LAS and glTF (.glb).
//! For every test file: cuts at several lengths and copies with changed bytes
//! (deterministic generator, the same variants on every run). Error = ok; panic = bug.
use bimrust::*;

/// simple pseudo-random generator (xorshift), so the results are reproducible
struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
}

fn variants(data: &[u8], seed: u64) -> Vec<(String, Vec<u8>)> {
    let mut out = Vec::new();
    for pct in [0, 1, 5, 10, 25, 50, 75, 90, 99] {
        let n = data.len() * pct / 100;
        out.push((format!("cut{pct}"), data[..n].to_vec()));
    }
    let mut r = Rng(seed | 1);
    // MB_FUZZ_N=500 cargo test --test corrupt_files: a longer search
    let n: usize = std::env::var("MB_FUZZ_N").ok().and_then(|v| v.parse().ok()).unwrap_or(24);
    for k in 0..n {
        let mut d = data.to_vec();
        if d.is_empty() {
            break;
        }
        let flips = 1 + (r.next() % 16) as usize;
        for _ in 0..flips {
            let i = (r.next() as usize) % d.len();
            d[i] = match r.next() % 4 {
                0 => 0,
                1 => 0xFF,
                2 => d[i] ^ (1 << (r.next() % 8)),
                _ => b"0123456789-.,;()'#=$*eE\n"[(r.next() % 24) as usize],
            };
        }
        out.push((format!("flip{k}"), d));
    }
    // huge numbers in place of a digit (indices, vertex counts, lengths)
    let mut d = data.to_vec();
    if let Some(i) = d.iter().position(|c| c.is_ascii_digit()) {
        d.splice(i..i + 1, b"99999999999".iter().copied());
        out.push(("bignum".into(), d));
    }
    out
}

/// A small LAS 1.2 point cloud (format 2: XYZ + RGB), 50 points.
fn las_sample() -> Vec<u8> {
    let mut b = vec![0u8; 227];
    b[..4].copy_from_slice(b"LASF");
    b[24] = 1;
    b[25] = 2;
    b[94..96].copy_from_slice(&227u16.to_le_bytes());
    b[96..100].copy_from_slice(&227u32.to_le_bytes());
    b[104] = 2;
    b[105..107].copy_from_slice(&26u16.to_le_bytes());
    b[107..111].copy_from_slice(&50u32.to_le_bytes());
    for (k, v) in [0.001f64, 0.001, 0.001, 500000.0, 4000000.0, 100.0].iter().enumerate() {
        b[131 + k * 8..139 + k * 8].copy_from_slice(&v.to_le_bytes());
    }
    for i in 0..50i32 {
        let mut r = vec![0u8; 26];
        r[0..4].copy_from_slice(&(i * 137).to_le_bytes());
        r[4..8].copy_from_slice(&(i * 211 % 3000).to_le_bytes());
        r[8..12].copy_from_slice(&(i * 53 % 900).to_le_bytes());
        r[20..22].copy_from_slice(&((i as u16) * 1000).to_le_bytes());
        b.extend_from_slice(&r);
    }
    b
}

/// A binary glTF (.glb) written by ManageBIM's own exporter from a small IFC.
fn glb_sample() -> Vec<u8> {
    let raw = load_any("tests/data/wall_opening.ifc", &|_, _| {}).expect("test IFC");
    let m = build_model(raw, "wall_opening.ifc", glam::DVec3::ZERO);
    let items: Vec<gltf_export::Item> = m
        .elements
        .iter()
        .filter_map(|e| {
            let (t0, t1) = (e.tri_start as usize, (e.tri_start + e.tri_count) as usize);
            let tris = m.pos.get(t0 * 3..t1 * 3)?;
            Some(gltf_export::Item {
                model: &m.name,
                level: &e.storey,
                name: &e.name,
                guid: &e.guid,
                ifc_type: &e.ifc_type,
                props: e.props.iter().map(|p| (p.group.as_str(), p.name.as_str(), p.value.as_str())).collect(),
                tris,
                gloss_metal: vec![[0, 0]; t1 - t0],
                colors: (t0..t1).map(|t| m.tri_color.get(t).copied().unwrap_or([200, 200, 200, 255])).collect(),
            })
        })
        .collect();
    gltf_export::write_glb(&items, "test").0
}

#[test]
fn corrupt_files_never_panic() {
    let files = [
        "tests/fixtures/demo_arhitectura.ifc",
        "tests/fixtures/demo_instalatii.ifc",
        "tests/data/wall_opening.ifc",
        "tests/fixtures/fmt/attribs.dxf",
        "tests/fixtures/fmt/from_blender.obj",
        "tests/fixtures/fmt/test.obj",
        "tests/fixtures/fmt/segmented.ply",
        "tests/fixtures/fmt/test.ply",
        "tests/fixtures/fmt/test.fbx",
        "tests/fixtures/fmt/test_ascii.stl",
        "tests/fixtures/fmt/test_bin.stl",
        "samples/Block_A_MEP.ifc.bimc",
        "samples/Block_A_Architecture_Structure.ifc.bimc",
    ];
    let dir = std::env::temp_dir().join(format!("mb_corrupt_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    // panic messages do not flood the test output; they are collected in the list
    std::panic::set_hook(Box::new(|_| {}));
    let mut panics = Vec::new();
    let mut runs = 0;
    // files on disk plus generated ones: a LAS point cloud and a glTF written by the exporter
    let mut inputs: Vec<(String, Vec<u8>, String)> = files
        .iter()
        .filter_map(|f| Some((f.to_string(), std::fs::read(f).ok()?, std::path::Path::new(f).extension()?.to_string_lossy().to_string())))
        .collect();
    inputs.push(("generated.las".into(), las_sample(), "las".into()));
    inputs.push(("generated.glb".into(), glb_sample(), "glb".into()));
    for (fi, (f, data, ext)) in inputs.iter().enumerate() {
        for (name, bytes) in variants(data, 0x9E37_79B9_7F4A_7C15 ^ fi as u64) {
            let p = dir.join(format!("{fi}_{name}.{ext}"));
            std::fs::write(&p, &bytes).unwrap();
            let path = p.to_string_lossy().to_string();
            runs += 1;
            let r = std::panic::catch_unwind(|| {
                // the .bimc cache has its own reader; a model that was read must also build (indices, colours)
                // point clouds have their own reader (they do not go through load_any)
                if path.ends_with(".las") {
                    let _ = e57::read_las(&path, &|_, _| {});
                    return;
                }
                let raw = if path.ends_with(".bimc") { bimc::read(&path) } else { load_any(&path, &|_, _| {}) };
                if let Ok(raw) = raw {
                    let _ = build_model(raw, &path, glam::DVec3::ZERO);
                }
            });
            if let Err(e) = r {
                let msg = e.downcast_ref::<String>().cloned().or_else(|| e.downcast_ref::<&str>().map(|s| s.to_string())).unwrap_or_default();
                panics.push(format!("{f} [{name}]: {msg}"));
            } else {
                let _ = std::fs::remove_file(&p);
            }
        }
    }
    let _ = std::panic::take_hook();
    assert!(runs > 100, "{runs}");
    assert!(panics.is_empty(), "{} of {runs} corrupt files panicked (kept in {}):\n{}", panics.len(), dir.display(), panics.join("\n"));
    let _ = std::fs::remove_dir_all(&dir);
}

/// Reads one file given in MB_ONE (to get a backtrace of a failure found above).
#[test]
#[ignore]
fn one_file() {
    let Some(p) = std::env::var_os("MB_ONE") else { return };
    let p = p.to_string_lossy().to_string();
    let _ = load_any(&p, &|_, _| {}).map(|raw| build_model(raw, &p, glam::DVec3::ZERO));
}
