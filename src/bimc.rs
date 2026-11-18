//! ManageBIM Cache (.bimc) - native, compact model cache (similar in role to Navisworks NWC).
//!
//! Layout:  "BIMC" | u16 version | u16 flags | compressed payload
//!   flags 0: LZ4 block, size-prepended (written by ManageBIM)
//!   flags 1: u32 LE uncompressed size + raw DEFLATE (RFC 1951) - easy to write from .NET
//!            (System.IO.Compression.DeflateStream), used by the Revit add-in.
//! Payload (all integers LEB128 varints, signed values zig-zag encoded):
//!   string table (every name / type / property group / property name / value stored once)
//!   model info + warnings (string indices)
//!   per element: name, type, guid, level, flags, properties (group,name,value index triples),
//!                indexed geometry quantized to 0.1 mm, delta-coded; colour runs.
use crate::model::{Model, Prop, RawElement, RawModel};
use glam::DVec3;
use std::collections::HashMap;

pub const MAGIC: &[u8; 4] = b"BIMC";
/// versiunea scrisa de ManageBIM (cache-urile proprii). v3 = v1 + geometrie partajata: un element a carui
/// geometrie cuantizata e IDENTICA cu a unui element anterior (doar deplasata) trimite la ea, cu deplasarea.
/// Proprietatile, numele si culorile raman ale fiecarui element; la citire geometria se reface exact.
pub const VERSION: u16 = 3;
/// cea mai noua versiune pe care o putem citi. v2 = "pe bucati" (add-in-ul Navisworks, modele foarte mari):
///   "BIMC" u16 2 u16 2, apoi bucati: u8 tip | u32 marime necomprimata | u32 marime comprimata | DEFLATE
///   tip 1 siruri noi, 2 elemente (codare v1), 3 info + avertismente, 0 sfarsit
pub const MAX_READ_VERSION: u16 = 3;
/// quantisation: 1 unit = 0.1 mm
const Q: f64 = 10_000.0;

// ------------------------------------------------------------------ writer
struct W {
    buf: Vec<u8>,
}

impl W {
    fn u(&mut self, mut v: u64) {
        loop {
            let b = (v & 0x7f) as u8;
            v >>= 7;
            if v == 0 {
                self.buf.push(b);
                break;
            }
            self.buf.push(b | 0x80);
        }
    }
    fn i(&mut self, v: i64) {
        self.u(((v << 1) ^ (v >> 63)) as u64);
    }
}

struct Strings {
    map: HashMap<String, u32>,
    list: Vec<String>,
}

impl Strings {
    fn id(&mut self, s: &str) -> u32 {
        if let Some(i) = self.map.get(s) {
            return *i;
        }
        let i = self.list.len() as u32;
        self.map.insert(s.to_string(), i);
        self.list.push(s.to_string());
        i
    }
}

#[derive(Default)]
struct ElemScratch {
    vmap: HashMap<[i64; 3], u32>,
    verts: Vec<[i64; 3]>,
    idx: Vec<u32>,
}

/// Codarea unui element (comuna v1 / v2): metadate ca indici in tabela de siruri, geometrie indexata
/// cuantizata la 0.1 mm si codata delta, culori pe secvente.
fn write_element(body: &mut W, st: &mut Strings, e: &RawElement, sc: &mut ElemScratch) {
    for s in [&e.name, &e.ifc_type, &e.guid, &e.storey] {
        let a = st.id(s);
        body.u(a as u64);
    }
    body.u(e.default_hidden as u64);
    body.u(e.props.len() as u64);
    for p in &e.props {
        let (g, n, v) = (st.id(&p.group), st.id(&p.name), st.id(&p.value));
        body.u(g as u64);
        body.u(n as u64);
        body.u(v as u64);
    }
    sc.vmap.clear();
    sc.verts.clear();
    sc.idx.clear();
    for t in &e.tris {
        for p in t {
            let q = [(p.x * Q).round() as i64, (p.y * Q).round() as i64, (p.z * Q).round() as i64];
            let n = sc.verts.len() as u32;
            let verts = &mut sc.verts;
            let k = *sc.vmap.entry(q).or_insert_with(|| {
                verts.push(q);
                n
            });
            sc.idx.push(k);
        }
    }
    body.u(sc.verts.len() as u64);
    body.u(e.tris.len() as u64);
    let mut prev = [0i64; 3];
    for v in &sc.verts {
        for a in 0..3 {
            body.i(v[a] - prev[a]);
        }
        prev = *v;
    }
    let mut pa = 0i64;
    for t in sc.idx.chunks_exact(3) {
        let a = t[0] as i64;
        body.i(a - pa);
        body.i(t[1] as i64 - a);
        body.i(t[2] as i64 - a);
        pa = a;
    }
    let mut runs: Vec<([u8; 4], u32)> = Vec::new();
    for c in &e.colors {
        match runs.last_mut() {
            Some((rc, n)) if rc == c => *n += 1,
            _ => runs.push((*c, 1)),
        }
    }
    body.u(runs.len() as u64);
    for (c, n) in runs {
        body.u(u32::from_le_bytes(c) as u64);
        body.u(n as u64);
    }
}

/// Geometria cuantizata a unui element: varfuri (0.1 mm) + indici, exact ca in codarea v1.
fn quantize(e: &RawElement) -> (Vec<[i64; 3]>, Vec<u32>) {
    let mut vmap: HashMap<[i64; 3], u32> = HashMap::new();
    let mut verts = Vec::new();
    let mut idx = Vec::with_capacity(e.tris.len() * 3);
    for t in &e.tris {
        for p in t {
            let q = [(p.x * Q).round() as i64, (p.y * Q).round() as i64, (p.z * Q).round() as i64];
            let n = verts.len() as u32;
            let k = *vmap.entry(q).or_insert_with(|| {
                verts.push(q);
                n
            });
            idx.push(k);
        }
    }
    (verts, idx)
}

/// Modul de scriere al geometriei unui element in v3.
enum Geo {
    /// geometrie proprie (tag 0), sau proprie si refolosita de elemente urmatoare (tag 2)
    Own { shared: bool },
    /// aceeasi geometrie ca geometria partajata `id`, deplasata cu `delta` (unitati de 0.1 mm)
    Ref { id: u32, delta: [i64; 3] },
}

fn write_geometry(body: &mut W, verts: &[[i64; 3]], idx: &[u32]) {
    body.u(verts.len() as u64);
    body.u((idx.len() / 3) as u64);
    let mut prev = [0i64; 3];
    for v in verts {
        for a in 0..3 {
            body.i(v[a] - prev[a]);
        }
        prev = *v;
    }
    let mut pa = 0i64;
    for t in idx.chunks_exact(3) {
        let a = t[0] as i64;
        body.i(a - pa);
        body.i(t[1] as i64 - a);
        body.i(t[2] as i64 - a);
        pa = a;
    }
}

fn write_colors(body: &mut W, colors: &[[u8; 4]]) {
    let mut runs: Vec<([u8; 4], u32)> = Vec::new();
    for c in colors {
        match runs.last_mut() {
            Some((rc, n)) if rc == c => *n += 1,
            _ => runs.push((*c, 1)),
        }
    }
    body.u(runs.len() as u64);
    for (c, n) in runs {
        body.u(u32::from_le_bytes(c) as u64);
        body.u(n as u64);
    }
}

/// Pentru fiecare element: geometria proprie sau trimiterea la o geometrie identica (doar deplasata).
/// Comparatia e exacta pe intregii cuantizati, deci geometria refacuta la citire e identica cu v1.
fn plan_shared(raw: &RawModel) -> (Vec<Geo>, usize) {
    use std::hash::{Hash, Hasher};
    let q: Vec<(Vec<[i64; 3]>, Vec<u32>)> = raw.elements.iter().map(quantize).collect();
    let rel = |(v, i): &(Vec<[i64; 3]>, Vec<u32>)| -> (u64, [i64; 3]) {
        let o = v.first().copied().unwrap_or([0; 3]);
        let mut h = std::collections::hash_map::DefaultHasher::new();
        for p in v {
            [p[0] - o[0], p[1] - o[1], p[2] - o[2]].hash(&mut h);
        }
        i.hash(&mut h);
        (h.finish(), o)
    };
    let same = |a: &(Vec<[i64; 3]>, Vec<u32>), b: &(Vec<[i64; 3]>, Vec<u32>)| -> bool {
        if a.0.len() != b.0.len() || a.1 != b.1 || a.0.is_empty() {
            return false;
        }
        let (oa, ob) = (a.0[0], b.0[0]);
        a.0.iter().zip(&b.0).all(|(p, r)| p[0] - oa[0] == r[0] - ob[0] && p[1] - oa[1] == r[1] - ob[1] && p[2] - oa[2] == r[2] - ob[2])
    };
    // hash -> elementele "reprezentant" (prima aparitie a fiecarei geometrii)
    let mut reps: HashMap<u64, Vec<usize>> = HashMap::new();
    let mut ref_of: Vec<Option<usize>> = vec![None; q.len()];
    for i in 0..q.len() {
        // geometriile mici (< 4 triunghiuri) nu merita trimiteri
        if q[i].1.len() < 12 {
            continue;
        }
        let (h, _) = rel(&q[i]);
        let list = reps.entry(h).or_default();
        match list.iter().copied().find(|&r| same(&q[i], &q[r])) {
            Some(r) => ref_of[i] = Some(r),
            None => list.push(i),
        }
    }
    let mut used = vec![false; q.len()];
    for r in ref_of.iter().flatten() {
        used[*r] = true;
    }
    let mut shared_id: HashMap<usize, u32> = HashMap::new();
    let mut plan = Vec::with_capacity(q.len());
    let mut nref = 0;
    for i in 0..q.len() {
        plan.push(match ref_of[i] {
            Some(r) => {
                nref += 1;
                let (a, b) = (q[i].0[0], q[r].0[0]);
                Geo::Ref { id: shared_id[&r], delta: [a[0] - b[0], a[1] - b[1], a[2] - b[2]] }
            }
            None => {
                if used[i] {
                    let id = shared_id.len() as u32;
                    shared_id.insert(i, id);
                }
                Geo::Own { shared: used[i] }
            }
        });
    }
    (plan, nref)
}

pub fn encode(raw: &RawModel) -> Vec<u8> {
    let mut st = Strings { map: HashMap::new(), list: Vec::new() };
    let mut body = W { buf: Vec::with_capacity(1 << 20) };
    let (plan, _) = plan_shared(raw);
    // info / warnings
    body.u(raw.info.len() as u64);
    for (k, v) in &raw.info {
        let (a, b) = (st.id(k), st.id(v));
        body.u(a as u64);
        body.u(b as u64);
    }
    body.u(raw.warnings.len() as u64);
    for w in &raw.warnings {
        let a = st.id(w);
        body.u(a as u64);
    }
    body.u(raw.elements.len() as u64);
    for (e, g) in raw.elements.iter().zip(&plan) {
        // metadate: identice cu v1 (fiecare element isi pastreaza numele, tipul, proprietatile)
        for s in [&e.name, &e.ifc_type, &e.guid, &e.storey] {
            let a = st.id(s);
            body.u(a as u64);
        }
        body.u(e.default_hidden as u64);
        body.u(e.props.len() as u64);
        for p in &e.props {
            let (g, n, v) = (st.id(&p.group), st.id(&p.name), st.id(&p.value));
            body.u(g as u64);
            body.u(n as u64);
            body.u(v as u64);
        }
        match g {
            Geo::Own { shared } => {
                body.u(if *shared { 2 } else { 0 });
                let (v, i) = quantize(e);
                write_geometry(&mut body, &v, &i);
            }
            Geo::Ref { id, delta } => {
                body.u(1);
                body.u(*id as u64);
                for d in delta {
                    body.i(*d);
                }
            }
        }
        // culorile raman ale fiecarui element (aceeasi geometrie poate avea alt material)
        write_colors(&mut body, &e.colors);
    }
    // payload = string table + body
    let mut head = W { buf: Vec::with_capacity(st.list.iter().map(|s| s.len() + 2).sum()) };
    head.u(st.list.len() as u64);
    for s in &st.list {
        head.u(s.len() as u64);
        head.buf.extend_from_slice(s.as_bytes());
    }
    head.buf.extend_from_slice(&body.buf);
    let comp = lz4_flex::compress_prepend_size(&head.buf);
    let mut out = Vec::with_capacity(comp.len() + 8);
    out.extend_from_slice(MAGIC);
    out.extend_from_slice(&VERSION.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out.extend_from_slice(&comp);
    out
}

// ------------------------------------------------------------------ reader
struct R<'a> {
    b: &'a [u8],
    p: usize,
}

impl<'a> R<'a> {
    fn u(&mut self) -> Result<u64, String> {
        let mut v = 0u64;
        let mut sh = 0;
        loop {
            let c = *self.b.get(self.p).ok_or("Unexpected end of .bimc data")?;
            self.p += 1;
            v |= ((c & 0x7f) as u64) << sh;
            if c & 0x80 == 0 {
                return Ok(v);
            }
            sh += 7;
            if sh > 63 {
                return Err("Corrupt varint in .bimc".into());
            }
        }
    }
    fn i(&mut self) -> Result<i64, String> {
        let v = self.u()?;
        Ok(((v >> 1) as i64) ^ -((v & 1) as i64))
    }
    fn n(&mut self, max: usize) -> Result<usize, String> {
        let v = self.u()? as usize;
        if v > max {
            return Err("Corrupt count in .bimc".into());
        }
        Ok(v)
    }
}

/// Un element (aceeasi codare in v1 si in bucatile ELEMENTS din v2).
fn read_element(r: &mut R, strs: &[String], lim: usize, verts: &mut Vec<DVec3>) -> Result<RawElement, String> {
    let s = |i: u64| -> Result<String, String> { strs.get(i as usize).cloned().ok_or_else(|| "Bad string index".to_string()) };
    let name = s(r.u()?)?;
    let ifc_type = s(r.u()?)?;
    let guid = s(r.u()?)?;
    let storey = s(r.u()?)?;
    let default_hidden = r.u()? != 0;
    let np = r.n(lim)?;
    let mut props = Vec::with_capacity(np);
    for _ in 0..np {
        props.push(Prop { group: s(r.u()?)?, name: s(r.u()?)?, value: s(r.u()?)? });
    }
    let nv = r.n(lim)?;
    let nt = r.n(lim)?;
    verts.clear();
    let mut prev = [0i64; 3];
    for _ in 0..nv {
        for a in prev.iter_mut() {
            *a += r.i()?;
        }
        verts.push(DVec3::new(prev[0] as f64 / Q, prev[1] as f64 / Q, prev[2] as f64 / Q));
    }
    let mut tris = Vec::with_capacity(nt);
    let mut pa = 0i64;
    let g = |i: i64| -> Result<DVec3, String> { verts.get(i as usize).copied().ok_or_else(|| "Bad vertex index".to_string()) };
    for _ in 0..nt {
        let a = pa + r.i()?;
        let b = a + r.i()?;
        let c = a + r.i()?;
        tris.push([g(a)?, g(b)?, g(c)?]);
        pa = a;
    }
    let nr = r.n(lim)?;
    let mut colors = Vec::with_capacity(nt);
    for _ in 0..nr {
        let c = (r.u()? as u32).to_le_bytes();
        let n = r.n(nt)?;
        colors.extend(std::iter::repeat(c).take(n));
    }
    colors.resize(nt, [200, 200, 200, 255]);
    Ok(RawElement { name, ifc_type, guid, storey, props, tris, colors, default_hidden })
}

/// v3: metadate ca in v1, apoi tag geometrie (0 proprie, 2 proprie + partajata, 1 trimitere), apoi culorile.
fn read_element_v3(r: &mut R, strs: &[String], lim: usize, shared: &mut Vec<(Vec<[i64; 3]>, Vec<u32>)>) -> Result<RawElement, String> {
    let s = |i: u64| -> Result<String, String> { strs.get(i as usize).cloned().ok_or_else(|| "Bad string index".to_string()) };
    let name = s(r.u()?)?;
    let ifc_type = s(r.u()?)?;
    let guid = s(r.u()?)?;
    let storey = s(r.u()?)?;
    let default_hidden = r.u()? != 0;
    let np = r.n(lim)?;
    let mut props = Vec::with_capacity(np);
    for _ in 0..np {
        props.push(Prop { group: s(r.u()?)?, name: s(r.u()?)?, value: s(r.u()?)? });
    }
    let tag = r.u()?;
    let to_tris = |v: &[[i64; 3]], idx: &[u32], d: [i64; 3]| -> Result<Vec<[DVec3; 3]>, String> {
        let p = |k: u32| -> Result<DVec3, String> {
            let q = v.get(k as usize).ok_or("Bad vertex index")?;
            Ok(DVec3::new((q[0] + d[0]) as f64 / Q, (q[1] + d[1]) as f64 / Q, (q[2] + d[2]) as f64 / Q))
        };
        idx.chunks_exact(3).map(|t| Ok([p(t[0])?, p(t[1])?, p(t[2])?])).collect()
    };
    let tris = match tag {
        0 | 2 => {
            let nv = r.n(lim)?;
            let nt = r.n(lim)?;
            let mut v = Vec::with_capacity(nv);
            let mut prev = [0i64; 3];
            for _ in 0..nv {
                for a in prev.iter_mut() {
                    *a += r.i()?;
                }
                v.push(prev);
            }
            let mut idx = Vec::with_capacity(nt * 3);
            let mut pa = 0i64;
            for _ in 0..nt {
                let a = pa + r.i()?;
                let b = a + r.i()?;
                let c = a + r.i()?;
                if a < 0 || b < 0 || c < 0 {
                    return Err("Bad vertex index".into());
                }
                idx.extend_from_slice(&[a as u32, b as u32, c as u32]);
                pa = a;
            }
            let t = to_tris(&v, &idx, [0; 3])?;
            if tag == 2 {
                shared.push((v, idx));
            }
            t
        }
        1 => {
            let id = r.n(shared.len().saturating_sub(1))?;
            let d = [r.i()?, r.i()?, r.i()?];
            let (v, idx) = shared.get(id).ok_or("Bad shared geometry index")?;
            to_tris(v, idx, d)?
        }
        t => return Err(format!("Unknown geometry tag {t} in .bimc")),
    };
    let nt = tris.len();
    let nr = r.n(lim)?;
    let mut colors = Vec::with_capacity(nt);
    for _ in 0..nr {
        let c = (r.u()? as u32).to_le_bytes();
        let n = r.n(nt)?;
        colors.extend(std::iter::repeat(c).take(n));
    }
    colors.resize(nt, [200, 200, 200, 255]);
    Ok(RawElement { name, ifc_type, guid, storey, props, tris, colors, default_hidden })
}

/// v2: bucati independente, fiecare comprimata separat (scrise pe masura exportului).
fn decode_v2(mut b: &[u8]) -> Result<RawModel, String> {
    let mut strs: Vec<String> = Vec::new();
    let mut elements = Vec::new();
    let mut info = Vec::new();
    let mut warnings = Vec::new();
    let mut verts: Vec<DVec3> = Vec::new();
    loop {
        if b.len() < 9 {
            return Err("Incomplete .bimc (the export was interrupted or the file is truncated).".into());
        }
        let kind = b[0];
        let raw = u32::from_le_bytes([b[1], b[2], b[3], b[4]]) as usize;
        let comp = u32::from_le_bytes([b[5], b[6], b[7], b[8]]) as usize;
        let body = b.get(9..9 + comp).ok_or("Incomplete .bimc chunk")?;
        b = &b[9 + comp..];
        if kind == 0 {
            break;
        }
        let data = if raw == 0 { Vec::new() } else { miniz_oxide::inflate::decompress_to_vec_with_limit(body, raw).map_err(|e| format!("Corrupt .bimc chunk (deflate): {e:?}"))? };
        let lim = data.len();
        let mut r = R { b: &data, p: 0 };
        match kind {
            1 => {
                let n = r.n(lim)?;
                strs.reserve(n);
                for _ in 0..n {
                    let l = r.n(lim)?;
                    let s = data.get(r.p..r.p + l).ok_or("Corrupt string table")?;
                    strs.push(String::from_utf8_lossy(s).into_owned());
                    r.p += l;
                }
            }
            2 => {
                let n = r.n(lim)?;
                elements.reserve(n);
                for _ in 0..n {
                    elements.push(read_element(&mut r, &strs, lim, &mut verts)?);
                }
            }
            3 => {
                let s = |i: u64| -> Result<String, String> { strs.get(i as usize).cloned().ok_or_else(|| "Bad string index".to_string()) };
                let ni = r.n(lim)?;
                for _ in 0..ni {
                    let k = s(r.u()?)?;
                    let v = s(r.u()?)?;
                    info.push((k, v));
                }
                let nw = r.n(lim)?;
                for _ in 0..nw {
                    warnings.push(s(r.u()?)?);
                }
            }
            _ => {} // tipuri viitoare: ignorate
        }
    }
    Ok(RawModel { elements, info, warnings })
}

/// v2 din Rust (aceeasi structura ca add-in-ul Navisworks) - folosit la teste si pentru modele foarte mari.
pub fn encode_v2(raw: &RawModel, chunk_elements: usize) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(MAGIC);
    out.extend_from_slice(&2u16.to_le_bytes());
    out.extend_from_slice(&2u16.to_le_bytes());
    let chunk = |out: &mut Vec<u8>, kind: u8, data: &[u8]| {
        let comp = miniz_oxide::deflate::compress_to_vec(data, 1);
        out.push(kind);
        out.extend_from_slice(&(data.len() as u32).to_le_bytes());
        out.extend_from_slice(&(comp.len() as u32).to_le_bytes());
        out.extend_from_slice(&comp);
    };
    let mut st = Strings { map: HashMap::new(), list: Vec::new() };
    let mut written = 0usize;
    let flush_strings = |out: &mut Vec<u8>, st: &Strings, written: &mut usize| {
        if *written == st.list.len() {
            return;
        }
        let mut w = W { buf: Vec::new() };
        w.u((st.list.len() - *written) as u64);
        for s in &st.list[*written..] {
            w.u(s.len() as u64);
            w.buf.extend_from_slice(s.as_bytes());
        }
        chunk(out, 1, &w.buf);
        *written = st.list.len();
    };
    let mut sc = ElemScratch::default();
    for part in raw.elements.chunks(chunk_elements.max(1)) {
        let mut w = W { buf: Vec::new() };
        w.u(part.len() as u64);
        for e in part {
            write_element(&mut w, &mut st, e, &mut sc);
        }
        // sirurile noi inaintea elementelor care le folosesc
        flush_strings(&mut out, &st, &mut written);
        chunk(&mut out, 2, &w.buf);
    }
    let mut w = W { buf: Vec::new() };
    let ids: Vec<(u32, u32)> = raw.info.iter().map(|(k, v)| (st.id(k), st.id(v))).collect();
    let wids: Vec<u32> = raw.warnings.iter().map(|x| st.id(x)).collect();
    flush_strings(&mut out, &st, &mut written);
    w.u(ids.len() as u64);
    for (a, b) in ids {
        w.u(a as u64);
        w.u(b as u64);
    }
    w.u(wids.len() as u64);
    for a in wids {
        w.u(a as u64);
    }
    chunk(&mut out, 3, &w.buf);
    chunk(&mut out, 0, &[]);
    out
}

pub fn decode(data: &[u8]) -> Result<RawModel, String> {
    if data.len() < 8 || &data[0..4] != MAGIC {
        return Err("Not a ManageBIM cache file (.bimc).".into());
    }
    let ver = u16::from_le_bytes([data[4], data[5]]);
    if ver > MAX_READ_VERSION {
        return Err(format!("This .bimc was written by a newer ManageBIM (format v{ver})."));
    }
    let flags = u16::from_le_bytes([data[6], data[7]]);
    if ver == 2 {
        if flags & 0xff != 2 {
            return Err(format!("Unknown .bimc v2 layout ({flags})"));
        }
        return decode_v2(&data[8..]);
    }
    let payload = match flags & 0xff {
        0 => lz4_flex::decompress_size_prepended(&data[8..]).map_err(|e| format!("Corrupt .bimc: {e}"))?,
        1 => {
            if data.len() < 12 {
                return Err("Corrupt .bimc header".into());
            }
            let size = u32::from_le_bytes([data[8], data[9], data[10], data[11]]) as usize;
            let out = miniz_oxide::inflate::decompress_to_vec_with_limit(&data[12..], size.max(1)).map_err(|e| format!("Corrupt .bimc (deflate): {e:?}"))?;
            out
        }
        c => return Err(format!("Unknown .bimc compression ({c})")),
    };
    let mut r = R { b: &payload, p: 0 };
    let ns = r.n(payload.len())?;
    let mut strs: Vec<String> = Vec::with_capacity(ns);
    for _ in 0..ns {
        let l = r.n(payload.len())?;
        let s = payload.get(r.p..r.p + l).ok_or("Corrupt string table")?;
        strs.push(String::from_utf8_lossy(s).into_owned());
        r.p += l;
    }
    let s = |i: u64| -> Result<String, String> { strs.get(i as usize).cloned().ok_or_else(|| "Bad string index".to_string()) };
    let lim = payload.len();
    let ni = r.n(lim)?;
    let mut info = Vec::with_capacity(ni);
    for _ in 0..ni {
        let k = s(r.u()?)?;
        let v = s(r.u()?)?;
        info.push((k, v));
    }
    let nw = r.n(lim)?;
    let mut warnings = Vec::with_capacity(nw);
    for _ in 0..nw {
        warnings.push(s(r.u()?)?);
    }
    let ne = r.n(lim)?;
    let mut elements = Vec::with_capacity(ne);
    let mut verts: Vec<DVec3> = Vec::new();
    // v3: geometriile partajate (intregi cuantizati), in ordinea aparitiei
    let mut shared: Vec<(Vec<[i64; 3]>, Vec<u32>)> = Vec::new();
    for _ in 0..ne {
        if ver >= 3 {
            elements.push(read_element_v3(&mut r, &strs, lim, &mut shared)?);
        } else {
            elements.push(read_element(&mut r, &strs, lim, &mut verts)?);
        }
    }
    Ok(RawModel { elements, info, warnings })
}

/// Same payload, DEFLATE-compressed (flags = 1). Used to test the .NET writer format.
pub fn encode_deflate(raw: &RawModel) -> Vec<u8> {
    let lz = encode(raw);
    let payload = lz4_flex::decompress_size_prepended(&lz[8..]).expect("own data");
    let comp = miniz_oxide::deflate::compress_to_vec(&payload, 6);
    let mut out = Vec::with_capacity(comp.len() + 12);
    out.extend_from_slice(MAGIC);
    out.extend_from_slice(&VERSION.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    out.extend_from_slice(&comp);
    out
}

pub fn write(raw: &RawModel, path: &str) -> Result<usize, String> {
    let bytes = encode(raw);
    let tmp = format!("{path}.tmp");
    std::fs::write(&tmp, &bytes).map_err(|e| format!("Cannot write {path}: {e}"))?;
    std::fs::rename(&tmp, path).map_err(|e| format!("Cannot write {path}: {e}"))?;
    Ok(bytes.len())
}

pub fn read(path: &str) -> Result<RawModel, String> {
    let data = std::fs::read(path).map_err(|e| format!("Cannot read file: {e}"))?;
    decode(&data)
}

/// Cache path next to a source model: `model.ifc` -> `model.ifc.bimc`
pub fn cache_path(src: &str) -> String {
    format!("{src}.bimc")
}

/// Returns the cache path if it exists and is newer than the source file.
pub fn fresh_cache(src: &str) -> Option<String> {
    let c = cache_path(src);
    let ms = std::fs::metadata(src).ok()?.modified().ok()?;
    let mc = std::fs::metadata(&c).ok()?.modified().ok()?;
    if mc >= ms {
        Some(c)
    } else {
        None
    }
}

/// Rebuild an exportable model from a loaded one (world coordinates).
pub fn from_model(m: &Model, origin: DVec3) -> RawModel {
    let elements = m
        .elements
        .iter()
        .map(|e| {
            let t = m.tris(e);
            let tris = t.chunks_exact(3).map(|c| [c[0].as_dvec3() + origin, c[1].as_dvec3() + origin, c[2].as_dvec3() + origin]).collect();
            let colors = m.tri_color[e.tri_start as usize..(e.tri_start + e.tri_count) as usize].to_vec();
            RawElement {
                name: e.name.clone(),
                ifc_type: e.ifc_type.clone(),
                guid: e.guid.clone(),
                storey: e.storey.clone(),
                props: e.props.clone(),
                tris,
                colors,
                default_hidden: e.default_hidden,
            }
        })
        .collect();
    RawModel { elements, info: m.info.clone(), warnings: m.warnings.clone() }
}
