//! Robustness: truncated files or files with damaged bytes must never stop the app (panic).
//! Formats: IFC, DXF, OBJ, PLY, FBX, STL, .bimc, LAS, E57 and glTF (.glb).
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

/// A small ASTM E57 point cloud: one scan of 40 points (cartesianX / Y / Z as doubles) in one compressed-vector
/// section, 1024-byte pages (1020 data + 4 CRC bytes, left 0: the reader does not check them).
fn e57_sample() -> Vec<u8> {
    const N: usize = 40;
    let phys = |l: usize| l / 1020 * 1024 + l % 1020;
    // logical layout: header (48) | binary section at 64 | XML after it
    let mut streams: Vec<Vec<u8>> = vec![Vec::new(); 3];
    for i in 0..N {
        let v = [i as f64 * 0.25, (i % 7) as f64 * 0.5, (i % 3) as f64 * 0.1];
        for (k, s) in streams.iter_mut().enumerate() {
            s.extend_from_slice(&v[k].to_le_bytes());
        }
    }
    let mut packet = vec![1u8, 0, 0, 0];
    packet.extend_from_slice(&3u16.to_le_bytes());
    for st in &streams {
        packet.extend_from_slice(&(st.len() as u16).to_le_bytes());
    }
    for st in &streams {
        packet.extend_from_slice(st);
    }
    let plen = packet.len() as u16 - 1;
    packet[2..4].copy_from_slice(&plen.to_le_bytes());
    let sec_logical = 64usize;
    let mut section = vec![0u8; 32];
    section[0] = 1;
    let sec_len = (32 + packet.len()) as u64;
    section[8..16].copy_from_slice(&sec_len.to_le_bytes());
    section.extend_from_slice(&packet);
    let xml_logical = sec_logical + section.len();
    let xml = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?><e57Root type="Structure" xmlns="http://www.astm.org/COMMIT/E57/2010-e57-v1.0"><formatName type="String"><![CDATA[ASTM E57 3D Imaging Data File]]></formatName><data3D type="Vector" allowHeterogeneousChildren="1"><vectorChild type="Structure"><name type="String"><![CDATA[Scan 1]]></name><points type="CompressedVector" fileOffset="{}" recordCount="{N}"><prototype type="Structure"><cartesianX type="Float"/><cartesianY type="Float"/><cartesianZ type="Float"/></prototype><codecs type="Vector" allowHeterogeneousChildren="1"></codecs></points></vectorChild></data3D></e57Root>"#,
        phys(sec_logical)
    );
    let mut logical = vec![0u8; sec_logical];
    logical.extend_from_slice(&section);
    logical.extend_from_slice(xml.as_bytes());
    logical[..8].copy_from_slice(b"ASTM-E57");
    logical[8..12].copy_from_slice(&1u32.to_le_bytes());
    let total_phys = phys(logical.len()).div_ceil(1024) * 1024;
    logical[16..24].copy_from_slice(&(total_phys as u64).to_le_bytes());
    logical[24..32].copy_from_slice(&(phys(xml_logical) as u64).to_le_bytes());
    logical[32..40].copy_from_slice(&(xml.len() as u64).to_le_bytes());
    logical[40..48].copy_from_slice(&1024u64.to_le_bytes());
    let mut out = Vec::with_capacity(total_phys);
    for chunk in logical.chunks(1020) {
        out.extend_from_slice(chunk);
        out.resize(out.len() + (1020 - chunk.len()), 0);
        out.extend_from_slice(&[0; 4]);
    }
    out
}

#[test]
fn e57_sample_reads_back() {
    let (_, cloud) = e57::parse(e57_sample(), &|_, _| {}).expect("generated E57");
    assert_eq!(cloud.pos.len(), 40);
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
    inputs.push(("generated.e57".into(), e57_sample(), "e57".into()));
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
                if path.ends_with(".e57") {
                    let _ = e57::read(&path, &|_, _| {});
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
    if p.ends_with(".las") || p.ends_with(".laz") {
        let _ = e57::read_las(&p, &|_, _| {});
        return;
    }
    if p.ends_with(".e57") {
        let _ = e57::read(&p, &|_, _| {});
        return;
    }
    let _ = load_any(&p, &|_, _| {}).map(|raw| build_model(raw, &p, glam::DVec3::ZERO));
}
