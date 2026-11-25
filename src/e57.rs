//! Nori de puncte ASTM E57 (.e57), fara biblioteci externe: antetul, paginile de 1024 octeti (1020 date + CRC),
//! XML-ul cu descrierea scanarilor si vectorii comprimati (pachete de date, campuri impachetate pe biti).
//! Fiecare scanare (data3D) = un element ManageBIM fara triunghiuri; punctele merg in `PointCloud`.
//! Punctele fiecarei scanari sunt amestecate aleator: densitatea de afisare = primele N% din fiecare scanare.
use crate::model::{Prop, RawElement, RawModel};
use glam::{DQuat, DVec3};

/// Norul citit: coordonate relative la `base` (f32 = precizie buna langa base), culori RGBA.
pub struct RawCloud {
    pub base: DVec3,
    pub pos: Vec<[f32; 3]>,
    pub col: Vec<[u8; 4]>,
    /// (index element, primul punct, numar de puncte) pe scanare
    pub scans: Vec<(u32, u32, u32)>,
}

/// Cate puncte pastram cel mult (memorie video ~16 octeti / punct).
pub const MAX_POINTS: usize = 40_000_000;

// ------------------------------------------------------------------ XML minimal

#[derive(Default, Debug)]
pub(crate) struct Node {
    pub(crate) name: String,
    pub(crate) attrs: Vec<(String, String)>,
    pub(crate) text: String,
    pub(crate) kids: Vec<Node>,
}

impl Node {
    pub(crate) fn attr(&self, k: &str) -> Option<&str> {
        self.attrs.iter().find(|a| a.0 == k).map(|a| a.1.as_str())
    }
    fn kid(&self, k: &str) -> Option<&Node> {
        self.kids.iter().find(|n| n.name == k)
    }
    fn num(&self) -> Option<f64> {
        let t = self.text.trim();
        if t.is_empty() {
            // E57: element fara valoare = 0 (ex. <versionMinor type="Integer"/>)
            return Some(0.0);
        }
        t.parse().ok()
    }
    fn attr_num(&self, k: &str) -> Option<f64> {
        self.attr(k).and_then(|v| v.trim().parse().ok())
    }
}

fn unesc(s: &str) -> String {
    s.replace("&lt;", "<").replace("&gt;", ">").replace("&quot;", "\"").replace("&apos;", "'").replace("&amp;", "&")
}

pub(crate) fn parse_xml(s: &str) -> Result<Node, String> {
    let b = s.as_bytes();
    let mut stack: Vec<Node> = vec![Node::default()];
    let mut i = 0;
    while i < b.len() {
        if b[i] != b'<' {
            let j = s[i..].find('<').map(|j| i + j).unwrap_or(b.len());
            if let Some(top) = stack.last_mut() {
                top.text.push_str(&unesc(&s[i..j]));
            }
            i = j;
            continue;
        }
        if s[i..].starts_with("<![CDATA[") {
            let j = s[i..].find("]]>").ok_or("E57: unterminated CDATA")? + i;
            if let Some(top) = stack.last_mut() {
                top.text.push_str(&s[i + 9..j]);
            }
            i = j + 3;
        } else if s[i..].starts_with("<?") || s[i..].starts_with("<!") {
            i = s[i..].find('>').ok_or("E57: bad XML")? + i + 1;
        } else if s[i..].starts_with("</") {
            let j = s[i..].find('>').ok_or("E57: bad XML")? + i;
            let n = stack.pop().ok_or("E57: bad XML nesting")?;
            stack.last_mut().ok_or("E57: bad XML nesting")?.kids.push(n);
            i = j + 1;
        } else {
            let j = s[i..].find('>').ok_or("E57: bad XML")? + i;
            let mut tag = &s[i + 1..j];
            let selfc = tag.ends_with('/');
            if selfc {
                tag = &tag[..tag.len() - 1];
            }
            let name_end = tag.find(|c: char| c.is_whitespace()).unwrap_or(tag.len());
            let mut n = Node { name: tag[..name_end].to_string(), ..Default::default() };
            let mut rest = &tag[name_end..];
            while let Some(eq) = rest.find('=') {
                let key = rest[..eq].trim().to_string();
                let after = rest[eq + 1..].trim_start();
                let q = after.chars().next().unwrap_or('"');
                let Some(end) = after[1..].find(q) else { break };
                n.attrs.push((key, unesc(&after[1..1 + end])));
                rest = &after[end + 2..];
            }
            if selfc {
                stack.last_mut().ok_or("E57: bad XML")?.kids.push(n);
            } else {
                stack.push(n);
            }
            i = j + 1;
        }
    }
    let mut root = stack.into_iter().next().ok_or("E57: empty XML")?;
    root.kids.pop().ok_or_else(|| "E57: no XML root".to_string())
}

// ------------------------------------------------------------------ fisier: pagini si adrese logice

struct File {
    data: Vec<u8>,
    page: u64,
}

impl File {
    /// adresa fizica -> logica (fara cei 4 octeti de CRC de la finalul fiecarei pagini)
    fn logical(&self, phys: u64) -> u64 {
        phys / self.page * (self.page - 4) + phys % self.page
    }
    /// `len` octeti logici incepand de la adresa fizica `phys`
    fn read(&self, phys: u64, len: usize) -> Result<Vec<u8>, String> {
        let mut out = Vec::with_capacity(len);
        let mut pos = phys;
        let data_per_page = self.page - 4;
        while out.len() < len {
            let in_page = pos % self.page;
            if in_page >= data_per_page {
                pos += self.page - in_page;
                continue;
            }
            let n = ((data_per_page - in_page) as usize).min(len - out.len());
            let s = self.data.get(pos as usize..pos as usize + n).ok_or("E57: the file is truncated")?;
            out.extend_from_slice(s);
            pos += n as u64;
        }
        Ok(out)
    }
    /// adresa fizica dupa ce avansam `len` octeti logici de la `phys`
    fn advance(&self, phys: u64, len: u64) -> u64 {
        let l = self.logical(phys) + len;
        l / (self.page - 4) * self.page + l % (self.page - 4)
    }
}

fn u16le(b: &[u8], o: usize) -> u16 {
    u16::from_le_bytes([b[o], b[o + 1]])
}
fn u64le(b: &[u8], o: usize) -> u64 {
    u64::from_le_bytes(b[o..o + 8].try_into().unwrap())
}

// ------------------------------------------------------------------ campuri

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    F32,
    F64,
    /// intreg impachetat pe `bits` biti: valoare = (raw + min) * scale + offset
    Int { min: i64, bits: u32, scale: f64, offset: f64 },
}

struct Field {
    name: String,
    kind: Kind,
    /// limitele pentru normalizare (culori / intensitate)
    lo: f64,
    hi: f64,
}

fn field_of(n: &Node) -> Result<Field, String> {
    let ty = n.attr("type").unwrap_or("");
    let (lo, hi) = (n.attr_num("minimum").unwrap_or(0.0), n.attr_num("maximum").unwrap_or(0.0));
    let kind = match ty {
        "Float" => {
            if n.attr("precision") == Some("single") {
                Kind::F32
            } else {
                Kind::F64
            }
        }
        "Integer" | "ScaledInteger" => {
            let (min, max) = (n.attr("minimum").and_then(|v| v.parse::<i64>().ok()).unwrap_or(0), n.attr("maximum").and_then(|v| v.parse::<i64>().ok()).unwrap_or(0));
            let range = (max as i128 - min as i128).max(0) as u128;
            let bits = 128 - range.leading_zeros();
            if bits > 64 {
                return Err(format!("E57: field {} is too wide", n.name));
            }
            let (scale, offset) = if ty == "ScaledInteger" { (n.attr_num("scale").unwrap_or(1.0), n.attr_num("offset").unwrap_or(0.0)) } else { (1.0, 0.0) };
            Kind::Int { min, bits, scale, offset }
        }
        other => return Err(format!("E57: unsupported field type {other} ({})", n.name)),
    };
    // pentru Float, limitele normalizarii vin din colorLimits / intensityLimits (vezi mai jos)
    let (lo, hi) = match kind {
        Kind::Int { scale, offset, .. } => (lo * scale + offset, hi * scale + offset),
        _ => (lo, hi),
    };
    Ok(Field { name: n.name.clone(), kind, lo, hi })
}

/// Decodeaza `count` valori dintr-un flux de octeti (bitii in ordine little-endian).
fn decode(kind: Kind, bytes: &[u8], count: usize) -> Vec<f64> {
    match kind {
        Kind::F32 => bytes.chunks_exact(4).take(count).map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]) as f64).collect(),
        Kind::F64 => bytes.chunks_exact(8).take(count).map(|c| f64::from_le_bytes(c.try_into().unwrap())).collect(),
        Kind::Int { min, bits, scale, offset } => {
            let conv = |raw: u64| (raw as i128 + min as i128) as f64 * scale + offset;
            if bits == 0 {
                return vec![conv(0); count];
            }
            if bits == 8 {
                return bytes.iter().take(count).map(|&b| conv(b as u64)).collect();
            }
            let mut out = Vec::with_capacity(count);
            let mask: u128 = if bits >= 64 { u64::MAX as u128 } else { (1u128 << bits) - 1 };
            let mut acc: u128 = 0;
            let mut have = 0u32;
            let mut it = bytes.iter();
            while out.len() < count {
                while have < bits {
                    let Some(&b) = it.next() else { return out };
                    acc |= (b as u128) << have;
                    have += 8;
                }
                out.push(conv((acc & mask) as u64));
                acc >>= bits;
                have -= bits;
            }
            out
        }
    }
}

/// Citeste un CompressedVector: fluxurile de octeti ale fiecarui camp, concatenate peste toate pachetele.
fn read_streams(f: &File, file_offset: u64, nfields: usize) -> Result<Vec<Vec<u8>>, String> {
    let h = f.read(file_offset, 32)?;
    if h[0] != 1 {
        return Err("E57: compressed vector section expected".into());
    }
    let section_len = u64le(&h, 8);
    let data_phys = u64le(&h, 16);
    let section_end = f.logical(file_offset) + section_len;
    let mut streams = vec![Vec::new(); nfields];
    let mut pos = if data_phys == 0 { f.advance(file_offset, 32) } else { data_phys };
    while f.logical(pos) + 4 <= section_end {
        let ph = f.read(pos, 4)?;
        let ptype = ph[0];
        let plen = u16le(&ph, 2) as u64 + 1;
        if plen < 4 {
            break;
        }
        if ptype == 1 {
            let p = f.read(pos, plen as usize)?;
            // antetul pachetului: 4 octeti + numarul de campuri + lungimile campurilor
            if p.len() < 6 {
                return Err("E57: data packet is truncated".into());
            }
            let n = u16le(&p, 4) as usize;
            if n != nfields {
                return Err(format!("E57: data packet with {n} fields, expected {nfields}"));
            }
            if p.len() < 6 + 2 * n {
                return Err("E57: data packet is truncated".into());
            }
            let mut o = 6 + 2 * n;
            for (k, s) in streams.iter_mut().enumerate() {
                let len = u16le(&p, 6 + 2 * k) as usize;
                let part = p.get(o..o + len).ok_or("E57: data packet is truncated")?;
                s.extend_from_slice(part);
                o += len;
            }
        } else if ptype > 2 {
            return Err(format!("E57: unknown packet type {ptype}"));
        }
        pos = f.advance(pos, plen);
    }
    Ok(streams)
}

fn pose_of(n: &Node) -> (DQuat, DVec3) {
    let Some(p) = n.kid("pose") else { return (DQuat::IDENTITY, DVec3::ZERO) };
    let r = p.kid("rotation").map(|r| {
        let g = |k: &str| r.kid(k).and_then(|x| x.num()).unwrap_or(0.0);
        let q = DQuat::from_xyzw(g("x"), g("y"), g("z"), g("w"));
        if q.length_squared() > 1e-12 { q.normalize() } else { DQuat::IDENTITY }
    });
    let t = p.kid("translation").map(|t| {
        let g = |k: &str| t.kid(k).and_then(|x| x.num()).unwrap_or(0.0);
        DVec3::new(g("x"), g("y"), g("z"))
    });
    (r.unwrap_or(DQuat::IDENTITY), t.unwrap_or(DVec3::ZERO))
}

/// Limite de normalizare din `colorLimits` / `intensityLimits`, daca exista.
fn limit(n: &Node, group: &str, lo: &str, hi: &str) -> Option<(f64, f64)> {
    let g = n.kid(group)?;
    let a = g.kid(lo).map(|x| scaled_value(x)).unwrap_or(0.0);
    let b = g.kid(hi).map(|x| scaled_value(x))?;
    (b > a).then_some((a, b))
}

fn scaled_value(n: &Node) -> f64 {
    let v = n.num().unwrap_or(0.0);
    if n.attr("type") == Some("ScaledInteger") { v * n.attr_num("scale").unwrap_or(1.0) + n.attr_num("offset").unwrap_or(0.0) } else { v }
}

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        // xorshift64*
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_f491_4f6c_dd1d)
    }
}

/// Citeste un fisier .e57: modelul (un element pe scanare, cu proprietati) + punctele.
pub fn read(path: &str, progress: &dyn Fn(f32, &str)) -> Result<(RawModel, RawCloud), String> {
    let data = std::fs::read(path).map_err(|e| format!("Cannot read the file: {e}"))?;
    parse(data, progress)
}

pub fn parse(data: Vec<u8>, progress: &dyn Fn(f32, &str)) -> Result<(RawModel, RawCloud), String> {
    if data.len() < 48 || &data[..8] != b"ASTM-E57" {
        return Err("Not an E57 file (ASTM-E57 signature missing).".into());
    }
    let xml_off = u64le(&data, 24);
    let xml_len = u64le(&data, 32) as usize;
    let page = u64le(&data, 40);
    if page < 64 || page > 1 << 20 {
        return Err("E57: invalid page size".into());
    }
    let f = File { data, page };
    let xml = String::from_utf8_lossy(&f.read(xml_off, xml_len)?).into_owned();
    let root = parse_xml(&xml)?;
    let scans_xml: Vec<&Node> = root.kid("data3D").map(|d| d.kids.iter().filter(|k| k.name == "vectorChild").collect()).unwrap_or_default();
    if scans_xml.is_empty() {
        return Err("E57: the file has no 3D scans (data3D is empty).".into());
    }
    let total: u64 = scans_xml.iter().filter_map(|s| s.kid("points").and_then(|p| p.attr_num("recordCount"))).map(|v| v as u64).sum();
    // peste limita: pastram un punct din `stride`
    let stride = ((total as f64 / MAX_POINTS as f64).ceil() as usize).max(1);
    let mut cloud = RawCloud { base: DVec3::ZERO, pos: Vec::new(), col: Vec::new(), scans: Vec::new() };
    let mut raw = RawModel { elements: Vec::new(), info: Vec::new(), warnings: Vec::new() };
    let mut base_set = false;
    let mut done: u64 = 0;
    for (si, s) in scans_xml.iter().enumerate() {
        let name = s.kid("name").map(|n| n.text.trim().to_string()).filter(|n| !n.is_empty()).unwrap_or_else(|| format!("Scan {}", si + 1));
        progress(0.05 + 0.85 * done as f32 / total.max(1) as f32, &format!("Reading {name}"));
        let Some(pts) = s.kid("points") else { continue };
        let count = pts.attr_num("recordCount").unwrap_or(0.0) as usize;
        let off = pts.attr_num("fileOffset").unwrap_or(0.0) as u64;
        let proto = pts.kid("prototype").ok_or("E57: scan without a point prototype")?;
        let fields: Vec<Field> = proto.kids.iter().map(field_of).collect::<Result<_, _>>()?;
        let streams = read_streams(&f, off, fields.len())?;
        let get = |n: &str| fields.iter().position(|x| x.name == n).map(|i| decode(fields[i].kind, &streams[i], count));
        let (xs, ys, zs) = match (get("cartesianX"), get("cartesianY"), get("cartesianZ")) {
            (Some(x), Some(y), Some(z)) => (x, y, z),
            _ => match (get("sphericalRange"), get("sphericalAzimuth"), get("sphericalElevation")) {
                (Some(r), Some(a), Some(e)) => {
                    let n = r.len().min(a.len()).min(e.len());
                    let mut x = Vec::with_capacity(n);
                    let mut y = Vec::with_capacity(n);
                    let mut z = Vec::with_capacity(n);
                    for i in 0..n {
                        x.push(r[i] * e[i].cos() * a[i].cos());
                        y.push(r[i] * e[i].cos() * a[i].sin());
                        z.push(r[i] * e[i].sin());
                    }
                    (x, y, z)
                }
                _ => {
                    raw.warnings.push(format!("{name}: no cartesian or spherical coordinates - skipped"));
                    continue;
                }
            },
        };
        let invalid = get("cartesianInvalidState").or_else(|| get("sphericalInvalidState"));
        let norm = |field: &str, limits: Option<(f64, f64)>| -> Option<Vec<u8>> {
            let i = fields.iter().position(|x| x.name == field)?;
            let v = decode(fields[i].kind, &streams[i], count);
            let (lo, hi) = limits.or_else(|| (fields[i].hi > fields[i].lo).then_some((fields[i].lo, fields[i].hi))).unwrap_or((0.0, 255.0));
            Some(v.iter().map(|x| ((x - lo) / (hi - lo) * 255.0).clamp(0.0, 255.0) as u8).collect())
        };
        let r = norm("colorRed", limit(s, "colorLimits", "colorRedMinimum", "colorRedMaximum"));
        let g = norm("colorGreen", limit(s, "colorLimits", "colorGreenMinimum", "colorGreenMaximum"));
        let b = norm("colorBlue", limit(s, "colorLimits", "colorBlueMinimum", "colorBlueMaximum"));
        let inten = norm("intensity", limit(s, "intensityLimits", "intensityMinimum", "intensityMaximum"));
        let (rot, tr) = pose_of(s);
        if !base_set {
            cloud.base = tr;
            base_set = true;
        }
        let n = xs.len().min(ys.len()).min(zs.len());
        let start = cloud.pos.len();
        let mut lo = DVec3::splat(f64::INFINITY);
        let mut hi = DVec3::splat(f64::NEG_INFINITY);
        let mut i = 0;
        while i < n {
            if invalid.as_ref().map_or(false, |v| v.get(i).copied().unwrap_or(0.0) != 0.0) {
                i += stride;
                continue;
            }
            let p = rot * DVec3::new(xs[i], ys[i], zs[i]) + tr;
            if !p.is_finite() {
                i += stride;
                continue;
            }
            lo = lo.min(p);
            hi = hi.max(p);
            let l = p - cloud.base;
            cloud.pos.push([l.x as f32, l.y as f32, l.z as f32]);
            // fluxurile de culoare pot fi mai scurte decat XYZ (scanere / exporturi incomplete): gri, nu eroare
            let c = match (&r, &g, &b, &inten) {
                (Some(r), Some(g), Some(b), _) => match (r.get(i), g.get(i), b.get(i)) {
                    (Some(r), Some(g), Some(b)) => [*r, *g, *b, 255],
                    _ => [190, 190, 190, 255],
                },
                (_, _, _, Some(t)) => t.get(i).map_or([190, 190, 190, 255], |t| [*t, *t, *t, 255]),
                _ => [190, 190, 190, 255],
            };
            cloud.col.push(c);
            i += stride;
        }
        let kept = cloud.pos.len() - start;
        // amestec aleator (Fisher-Yates) -> densitatea = primele N% puncte ale scanarii
        let mut rng = Rng(0x9e37_79b9_7f4a_7c15 ^ (si as u64 + 1));
        for k in (1..kept).rev() {
            let j = (rng.next() % (k as u64 + 1)) as usize;
            cloud.pos.swap(start + k, start + j);
            cloud.col.swap(start + k, start + j);
        }
        let elem = raw.elements.len() as u32;
        cloud.scans.push((elem, start as u32, kept as u32));
        let guid = s.kid("guid").map(|n| n.text.trim().to_string()).unwrap_or_default();
        let mut props = vec![
            Prop { group: "Point cloud".into(), name: "Points".into(), value: kept.to_string() },
            Prop { group: "Point cloud".into(), name: "Points in file".into(), value: count.to_string() },
            Prop { group: "Point cloud".into(), name: "Colour".into(), value: if r.is_some() { "RGB".into() } else if inten.is_some() { "Intensity".into() } else { "None".into() } },
            Prop { group: "Point cloud".into(), name: "Scanner position".into(), value: format!("{:.3}, {:.3}, {:.3}", tr.x, tr.y, tr.z) },
        ];
        if lo.x.is_finite() {
            props.push(Prop { group: "Point cloud".into(), name: "Size X × Y × Z".into(), value: format!("{:.2} × {:.2} × {:.2} m", hi.x - lo.x, hi.y - lo.y, hi.z - lo.z) });
        }
        for k in ["sensorVendor", "sensorModel", "sensorSerialNumber", "description"] {
            if let Some(v) = s.kid(k).map(|n| n.text.trim().to_string()).filter(|v| !v.is_empty()) {
                props.push(Prop { group: "Point cloud".into(), name: k.into(), value: v });
            }
        }
        raw.elements.push(RawElement { name, ifc_type: "PointCloudScan".into(), guid, storey: String::new(), props, tris: Vec::new(), colors: Vec::new(), default_hidden: false });
        done += count as u64;
    }
    if cloud.pos.is_empty() {
        return Err("E57: no points could be read.".into());
    }
    raw.info.push(("Format".into(), "ASTM E57 point cloud".into()));
    raw.info.push(("Scans".into(), cloud.scans.len().to_string()));
    raw.info.push(("Points".into(), cloud.pos.len().to_string()));
    if stride > 1 {
        raw.warnings.push(format!("{total} points in the file: one point in {stride} kept ({} shown).", cloud.pos.len()));
    }
    if let Some(v) = root.kid("e57LibraryVersion").map(|n| n.text.trim().to_string()).filter(|v| !v.is_empty()) {
        raw.info.push(("Written by".into(), v));
    }
    progress(0.95, "Point cloud read");
    Ok((raw, cloud))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bitpacked_integers_cross_byte_boundaries() {
        // valori 0..=1023 pe 10 biti, impachetate little-endian
        let vals: Vec<u64> = (0..50).map(|i| (i * 37) % 1024).collect();
        let mut bytes = Vec::new();
        let (mut acc, mut have) = (0u128, 0u32);
        for v in &vals {
            acc |= (*v as u128) << have;
            have += 10;
            while have >= 8 {
                bytes.push(acc as u8);
                acc >>= 8;
                have -= 8;
            }
        }
        if have > 0 {
            bytes.push(acc as u8);
        }
        let k = Kind::Int { min: 0, bits: 10, scale: 1.0, offset: 0.0 };
        let d = decode(k, &bytes, vals.len());
        assert_eq!(d, vals.iter().map(|v| *v as f64).collect::<Vec<_>>());
        let s = decode(Kind::Int { min: -5, bits: 4, scale: 0.5, offset: 1.0 }, &[0x21], 2);
        assert_eq!(s, vec![(1.0 - 5.0) * 0.5 + 1.0, (2.0 - 5.0) * 0.5 + 1.0]);
    }

    #[test]
    fn xml_parser_reads_attributes_cdata_and_empty_values() {
        let n = parse_xml("<?xml version=\"1.0\"?><r a=\"1\"><x type=\"Float\">2.5</x><e type=\"Integer\"/><s><![CDATA[Scan <1>]]></s></r>").unwrap();
        assert_eq!(n.attr("a"), Some("1"));
        assert_eq!(n.kid("x").unwrap().num(), Some(2.5));
        assert_eq!(n.kid("e").unwrap().num(), Some(0.0));
        assert_eq!(n.kid("s").unwrap().text, "Scan <1>");
    }

    #[test]
    fn page_addresses_skip_crc() {
        let f = File { data: vec![0; 4096], page: 1024 };
        assert_eq!(f.logical(1024), 1020);
        assert_eq!(f.advance(1000, 30), 1024 + 10);
        assert_eq!(f.advance(48, 0), 48);
    }
}

// ------------------------------------------------------------------ LAS (nori de puncte, necomprimat)

/// Citeste un fisier .las (formatele de punct 0-10): coordonate (scara + offset din antet), culoare RGB sau intensitate.
/// .laz (comprimat) nu e suportat: se converteste in .las (ex. CloudCompare, LAStools).
pub fn read_las(path: &str, progress: &dyn Fn(f32, &str)) -> Result<(RawModel, RawCloud), String> {
    let data = std::fs::read(path).map_err(|e| format!("Cannot read the file: {e}"))?;
    if data.len() > 104 && &data[..4] == b"LASF" && data[104] & 0x80 != 0 {
        progress(0.02, "Decompressing LAZ");
        let plain = laz_to_las(&data)?;
        return parse_las(&plain, progress).map(|(mut m, c)| {
            m.info.insert(0, ("Compression".into(), "LAZ (LASzip), decompressed on open".into()));
            (m, c)
        });
    }
    parse_las(&data, progress)
}

/// LAZ (LASzip) -> LAS necomprimat in memorie: antetul si VLR-urile raman, punctele se decomprima
/// (crate-ul `laz`, Rust pur). Bitul de compresie din formatul punctului se sterge.
pub fn laz_to_las(b: &[u8]) -> Result<Vec<u8>, String> {
    let u16_ = |o: usize| u16::from_le_bytes([b[o], b[o + 1]]) as usize;
    let u32_ = |o: usize| u32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]]) as usize;
    if b.len() < 227 {
        return Err("LAZ: file too short.".into());
    }
    let (maj, min) = (b[24], b[25]);
    let header = u16_(94);
    let offset = u32_(96);
    let nvlr = u32_(100);
    let rec = u16_(105);
    let mut count = u32_(107);
    if count == 0 && maj == 1 && min >= 4 && b.len() >= 255 {
        count = u64::from_le_bytes(b[247..255].try_into().unwrap()) as usize;
    }
    // VLR-ul LASzip: user id "laszip encoded", record 22204
    let mut p = header;
    let mut vlr: Option<laz::LazVlr> = None;
    for _ in 0..nvlr {
        if p + 54 > b.len() {
            break;
        }
        let user = String::from_utf8_lossy(&b[p + 2..p + 18]).trim_end_matches('\0').to_string();
        let id = u16_(p + 18);
        let len = u16_(p + 20);
        let data = &b[p + 54..(p + 54 + len).min(b.len())];
        if user == "laszip encoded" && id == 22204 {
            vlr = Some(laz::LazVlr::from_buffer(data).map_err(|e| format!("LAZ: invalid LASzip record ({e})."))?);
        }
        p += 54 + len;
    }
    let vlr = vlr.ok_or("LAZ: the LASzip record is missing - the file is damaged or not LASzip-compressed.")?;
    if offset >= b.len() || rec == 0 {
        return Err("LAZ: no point data.".into());
    }
    let mut out = Vec::with_capacity(offset + count * rec);
    out.extend_from_slice(&b[..offset]);
    out[104] &= 0x7f;
    let start = out.len();
    out.resize(start + count * rec, 0);
    let mut cur = std::io::Cursor::new(b);
    cur.set_position(offset as u64);
    let mut dec = laz::LasZipDecompressor::new(cur, vlr).map_err(|e| format!("LAZ: {e}"))?;
    dec.decompress_many(&mut out[start..]).map_err(|e| format!("LAZ: decompression failed ({e}).")).map(|_| out)
}

pub fn parse_las(b: &[u8], progress: &dyn Fn(f32, &str)) -> Result<(RawModel, RawCloud), String> {
    if b.len() < 227 || &b[..4] != b"LASF" {
        return Err("Not a LAS file (LASF signature missing).".into());
    }
    let u16_ = |o: usize| u16::from_le_bytes([b[o], b[o + 1]]) as usize;
    let u32_ = |o: usize| u32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]]) as usize;
    let f64_ = |o: usize| f64::from_le_bytes(b[o..o + 8].try_into().unwrap());
    let (maj, min) = (b[24], b[25]);
    let offset = u32_(96);
    let fmt = b[104] & 0x3f;
    if b[104] & 0x80 != 0 {
        return Err("This LAS file is compressed (LAZ). Convert it to plain .las (e.g. with CloudCompare) and open that.".into());
    }
    let rec = u16_(105);
    let mut count = u32_(107);
    if count == 0 && maj == 1 && min >= 4 && b.len() >= 255 {
        count = u64::from_le_bytes(b[247..255].try_into().unwrap()) as usize;
    }
    let scale = DVec3::new(f64_(131), f64_(139), f64_(147));
    let off = DVec3::new(f64_(155), f64_(163), f64_(171));
    let rgb_at: Option<usize> = match fmt {
        2 => Some(20),
        3 | 5 => Some(28),
        7 | 8 | 10 => Some(30),
        _ => None,
    };
    let min_rec = match rgb_at {
        Some(o) => o + 6,
        None => 14,
    };
    if rec < min_rec || offset >= b.len() {
        return Err(format!("LAS: unsupported point record (format {fmt}, {rec} bytes)."));
    }
    let avail = (b.len() - offset) / rec;
    let count = count.min(avail);
    if count == 0 {
        return Err("LAS: the file has no points.".into());
    }
    let stride = ((count as f64 / MAX_POINTS as f64).ceil() as usize).max(1);
    // culorile pe 16 biti (ca in standard) sau pe 8 biti (unele programe): detectate din primele puncte
    let wide = rgb_at.map_or(false, |o| (0..count.min(2000)).any(|i| {
        let p = offset + i * rec + o;
        u16_(p) > 255 || u16_(p + 2) > 255 || u16_(p + 4) > 255
    }));
    let max_int = (0..count.min(5000)).map(|i| u16_(offset + i * rec + 12)).max().unwrap_or(0).max(1) as f32;
    let base = DVec3::new(i32::from_le_bytes(b[offset..offset + 4].try_into().unwrap()) as f64, i32::from_le_bytes(b[offset + 4..offset + 8].try_into().unwrap()) as f64, i32::from_le_bytes(b[offset + 8..offset + 12].try_into().unwrap()) as f64) * scale + off;
    let mut cloud = RawCloud { base, pos: Vec::with_capacity(count / stride + 1), col: Vec::with_capacity(count / stride + 1), scans: Vec::new() };
    let (mut lo, mut hi) = (DVec3::splat(f64::INFINITY), DVec3::splat(f64::NEG_INFINITY));
    // atributele fiecarui punct (formatele 0-5 vs 6-10): clasificare, intensitate, intoarceri, timp GPS
    let new_fmt = fmt >= 6;
    let class_at = if new_fmt { 16 } else { 15 };
    let gps_at: Option<usize> = match fmt {
        1 | 3 | 4 | 5 => Some(20),
        6..=10 => Some(22),
        _ => None,
    };
    let mut classes: Vec<u8> = Vec::with_capacity(count / stride + 1);
    let mut stats: std::collections::BTreeMap<u8, ClassStats> = std::collections::BTreeMap::new();
    let mut i = 0;
    while i < count {
        if i % 2_000_000 == 0 {
            progress(0.05 + 0.85 * i as f32 / count as f32, "Reading points");
        }
        let p = offset + i * rec;
        let xyz = DVec3::new(
            i32::from_le_bytes(b[p..p + 4].try_into().unwrap()) as f64,
            i32::from_le_bytes(b[p + 4..p + 8].try_into().unwrap()) as f64,
            i32::from_le_bytes(b[p + 8..p + 12].try_into().unwrap()) as f64,
        ) * scale
            + off;
        if xyz.is_finite() {
            lo = lo.min(xyz);
            hi = hi.max(xyz);
            let l = xyz - base;
            cloud.pos.push([l.x as f32, l.y as f32, l.z as f32]);
            let c = match rgb_at {
                Some(o) => {
                    let k = |v: usize| if wide { (v >> 8) as u8 } else { v.min(255) as u8 };
                    [k(u16_(p + o)), k(u16_(p + o + 2)), k(u16_(p + o + 4)), 255]
                }
                None => {
                    let g = (u16_(p + 12) as f32 / max_int * 230.0 + 25.0).min(255.0) as u8;
                    [g, g, g, 255]
                }
            };
            cloud.col.push(c);
            let cls = if new_fmt { b[p + class_at] } else { b[p + class_at] & 0x1f };
            let ret = b[p + 14];
            let (rn, nr) = if new_fmt { (ret & 0x0f, ret >> 4) } else { (ret & 0x07, (ret >> 3) & 0x07) };
            let gps = gps_at.map(|o| f64::from_le_bytes(b[p + o..p + o + 8].try_into().unwrap()));
            stats.entry(cls).or_default().add(u16_(p + 12) as u32, rn, nr, gps, xyz.z);
            classes.push(cls);
        }
        i += stride;
    }
    let n = cloud.pos.len();
    // punctele grupate pe clasificare (un element in arbore pe clasa: Ground, Building, Vegetation...),
    // amestecate in interiorul fiecarei clase (densitatea = primele N% din fiecare)
    let mut order: Vec<usize> = (0..n).collect();
    order.sort_by_key(|&k| classes[k]);
    let mut rng = Rng(0x2545_f491_4f6c_dd1d);
    let mut groups: Vec<(u8, usize, usize)> = Vec::new();
    let mut s = 0;
    while s < n {
        let c = classes[order[s]];
        let mut e = s;
        while e < n && classes[order[e]] == c {
            e += 1;
        }
        for k in (s + 1..e).rev() {
            let j = s + (rng.next() % ((k - s) as u64 + 1)) as usize;
            order.swap(k, j);
        }
        groups.push((c, s, e - s));
        s = e;
    }
    cloud.pos = order.iter().map(|&k| cloud.pos[k]).collect();
    cloud.col = order.iter().map(|&k| cloud.col[k]).collect();
    let mut raw = RawModel { elements: Vec::new(), info: Vec::new(), warnings: Vec::new() };
    let pc = |n: &str, v: String| Prop { group: "Point cloud".into(), name: n.into(), value: v };
    for (gi, (cls, first, cnt)) in groups.iter().enumerate() {
        let st = stats.get(cls).cloned().unwrap_or_default();
        let mut props = vec![
            pc("Classification", format!("{cls} - {}", asprs_class(*cls))),
            pc("Points", cnt.to_string()),
            pc("LAS version", format!("{maj}.{min}")),
            pc("Point format", fmt.to_string()),
            pc("Colour", if rgb_at.is_some() { "RGB".into() } else { "Intensity".into() }),
            pc("Elevation min / max", format!("{:.2} / {:.2} m", st.zmin, st.zmax)),
        ];
        if st.n > 0 {
            props.push(pc("Intensity min / mean / max", format!("{} / {:.0} / {}", st.imin, st.isum as f64 / st.n as f64, st.imax)));
            props.push(pc("Single returns", format!("{:.1} %", 100.0 * st.single as f64 / st.n as f64)));
            props.push(pc("First / last returns", format!("{} / {}", st.first, st.last)));
            props.push(pc("Max returns per pulse", st.max_returns.to_string()));
        }
        if st.gps_min.is_finite() {
            props.push(pc("GPS time min / max", format!("{:.3} / {:.3}", st.gps_min, st.gps_max)));
        }
        if gi == 0 {
            props.push(pc("Points in file", count.to_string()));
            props.push(pc("Size X × Y × Z", format!("{:.2} × {:.2} × {:.2} m", hi.x - lo.x, hi.y - lo.y, hi.z - lo.z)));
        }
        let name = if groups.len() == 1 && *cls <= 1 { "Points".to_string() } else { asprs_class(*cls).to_string() };
        raw.elements.push(RawElement { name, ifc_type: "PointCloudScan".into(), guid: String::new(), storey: String::new(), props, tris: Vec::new(), colors: Vec::new(), default_hidden: false });
        cloud.scans.push((gi as u32, *first as u32, *cnt as u32));
    }
    if groups.len() > 1 {
        raw.info.push(("Classes".into(), groups.iter().map(|(c, _, k)| format!("{} {}", k, asprs_class(*c))).collect::<Vec<_>>().join(", ")));
    }
    raw.info.push(("Format".into(), format!("LAS {maj}.{min} point cloud (format {fmt})")));
    raw.info.push(("Points".into(), n.to_string()));
    if stride > 1 {
        raw.warnings.push(format!("{count} points in the file: one point in {stride} kept ({n} shown)."));
    }
    progress(0.95, "Point cloud read");
    Ok((raw, cloud))
}

/// Statistica atributelor LAS pe o clasa.
#[derive(Clone)]
struct ClassStats {
    n: u64,
    isum: u64,
    imin: u32,
    imax: u32,
    single: u64,
    first: u64,
    last: u64,
    max_returns: u8,
    gps_min: f64,
    gps_max: f64,
    zmin: f64,
    zmax: f64,
}

impl Default for ClassStats {
    fn default() -> Self {
        ClassStats { n: 0, isum: 0, imin: u32::MAX, imax: 0, single: 0, first: 0, last: 0, max_returns: 0, gps_min: f64::INFINITY, gps_max: f64::NEG_INFINITY, zmin: f64::INFINITY, zmax: f64::NEG_INFINITY }
    }
}

impl ClassStats {
    fn add(&mut self, intensity: u32, rn: u8, nr: u8, gps: Option<f64>, z: f64) {
        self.n += 1;
        self.isum += intensity as u64;
        self.imin = self.imin.min(intensity);
        self.imax = self.imax.max(intensity);
        if nr <= 1 {
            self.single += 1;
        }
        if rn <= 1 {
            self.first += 1;
        }
        if rn >= nr && nr > 0 {
            self.last += 1;
        }
        self.max_returns = self.max_returns.max(nr);
        if let Some(t) = gps.filter(|t| t.is_finite()) {
            self.gps_min = self.gps_min.min(t);
            self.gps_max = self.gps_max.max(t);
        }
        self.zmin = self.zmin.min(z);
        self.zmax = self.zmax.max(z);
    }
}

/// Numele clasei ASPRS (LAS 1.4, R15).
pub fn asprs_class(c: u8) -> &'static str {
    match c {
        0 => "Never classified",
        1 => "Unclassified",
        2 => "Ground",
        3 => "Low vegetation",
        4 => "Medium vegetation",
        5 => "High vegetation",
        6 => "Building",
        7 => "Low noise",
        8 => "Model key-point",
        9 => "Water",
        10 => "Rail",
        11 => "Road surface",
        12 => "Overlap",
        13 => "Wire - guard",
        14 => "Wire - conductor",
        15 => "Transmission tower",
        16 => "Wire - connector",
        17 => "Bridge deck",
        18 => "High noise",
        19 => "Overhead structure",
        20 => "Ignored ground",
        21 => "Snow",
        22 => "Temporal exclusion",
        _ => "User class",
    }
}

#[cfg(test)]
mod las_tests {
    use super::*;

    /// LAZ: comprimat cu LASzip (crate `laz`), apoi deschis ca LAS - aceleasi puncte si culori.
    #[test]
    fn laz_roundtrip() {
        use laz::{LasZipCompressor, LazItemRecordBuilder, LazVlr};
        let n = 500u32;
        let mut pts = Vec::new();
        for i in 0..n as i32 {
            let mut r = vec![0u8; 26];
            r[0..4].copy_from_slice(&(i * 10).to_le_bytes());
            r[4..8].copy_from_slice(&(i * 20).to_le_bytes());
            r[8..12].copy_from_slice(&(i * 5).to_le_bytes());
            r[12..14].copy_from_slice(&(i as u16 * 3).to_le_bytes());
            r[15] = (i % 7) as u8; // clasificare
            r[20..22].copy_from_slice(&65535u16.to_le_bytes());
            pts.extend_from_slice(&r);
        }
        let vlr = LazVlr::from_laz_items(LazItemRecordBuilder::default_for_point_format_id(2, 0).unwrap());
        let mut vlr_data = Vec::new();
        vlr.write_to(&mut vlr_data).unwrap();
        // ca un scriitor LAZ real: compresorul porneste de la pozitia datelor din fisier (tabela de chunk-uri e absoluta)
        let pre = 227 + 54 + vlr_data.len();
        let mut cur = std::io::Cursor::new(vec![0u8; pre]);
        cur.set_position(pre as u64);
        let mut comp = LasZipCompressor::new(cur, vlr.clone()).unwrap();
        comp.compress_many(&pts).unwrap();
        comp.done().unwrap();
        let data = comp.into_inner().into_inner()[pre..].to_vec();
        let mut b = vec![0u8; 227];
        b[..4].copy_from_slice(b"LASF");
        b[24] = 1;
        b[25] = 2;
        b[94..96].copy_from_slice(&227u16.to_le_bytes());
        let offset = 227 + 54 + vlr_data.len();
        b[96..100].copy_from_slice(&(offset as u32).to_le_bytes());
        b[100..104].copy_from_slice(&1u32.to_le_bytes());
        b[104] = 2 | 0x80;
        b[105..107].copy_from_slice(&26u16.to_le_bytes());
        b[107..111].copy_from_slice(&n.to_le_bytes());
        for (k, v) in [0.001f64, 0.001, 0.001, 0.0, 0.0, 0.0].iter().enumerate() {
            b[131 + k * 8..139 + k * 8].copy_from_slice(&v.to_le_bytes());
        }
        let mut h = vec![0u8; 54];
        h[2..16].copy_from_slice(b"laszip encoded");
        h[18..20].copy_from_slice(&22204u16.to_le_bytes());
        h[20..22].copy_from_slice(&(vlr_data.len() as u16).to_le_bytes());
        b.extend_from_slice(&h);
        b.extend_from_slice(&vlr_data);
        b.extend_from_slice(&data);
        let plain = laz_to_las(&b).unwrap();
        assert_eq!(plain[104], 2);
        assert_eq!(&plain[offset..offset + pts.len()], &pts[..]);
        let (m, c) = parse_las(&plain, &|_, _| {}).unwrap();
        assert_eq!(c.pos.len(), n as usize);
        // clasa i % 7 -> 7 elemente (Never classified .. Low noise), cate unul pe clasa
        assert_eq!(m.elements.len(), 7);
        assert_eq!(c.scans.len(), 7);
        assert_eq!(m.elements[2].name, "Ground");
        assert!(m.elements[2].props.iter().any(|p| p.name == "Intensity min / mean / max"));
        assert_eq!(c.scans.iter().map(|s| s.2).sum::<u32>(), n);
    }

    #[test]
    fn las_points_with_scale_offset_and_rgb() {
        // antet LAS 1.2, format 2 (XYZ + intensitate + ... + RGB), 3 puncte
        let mut b = vec![0u8; 227];
        b[..4].copy_from_slice(b"LASF");
        b[24] = 1;
        b[25] = 2;
        b[94..96].copy_from_slice(&227u16.to_le_bytes());
        b[96..100].copy_from_slice(&227u32.to_le_bytes());
        b[104] = 2;
        b[105..107].copy_from_slice(&26u16.to_le_bytes());
        b[107..111].copy_from_slice(&3u32.to_le_bytes());
        for (k, v) in [0.001f64, 0.001, 0.001, 500000.0, 4000000.0, 100.0].iter().enumerate() {
            b[131 + k * 8..139 + k * 8].copy_from_slice(&v.to_le_bytes());
        }
        for i in 0..3i32 {
            let mut r = vec![0u8; 26];
            r[0..4].copy_from_slice(&(i * 1000).to_le_bytes());
            r[4..8].copy_from_slice(&(i * 2000).to_le_bytes());
            r[8..12].copy_from_slice(&(i * 500).to_le_bytes());
            r[20..22].copy_from_slice(&65535u16.to_le_bytes());
            b.extend_from_slice(&r);
        }
        let (raw, c) = parse_las(&b, &|_, _| {}).unwrap();
        assert_eq!(raw.elements.len(), 1);
        assert_eq!(c.pos.len(), 3);
        assert_eq!(c.base, DVec3::new(500000.0, 4000000.0, 100.0));
        // punctele (amestecate) acopera 0..2 m pe X, 0..4 m pe Y
        let maxx = c.pos.iter().map(|p| p[0]).fold(0.0f32, f32::max);
        let maxy = c.pos.iter().map(|p| p[1]).fold(0.0f32, f32::max);
        assert!((maxx - 2.0).abs() < 1e-4 && (maxy - 4.0).abs() < 1e-4);
        assert!(c.col.iter().all(|k| k[0] == 255 && k[1] == 0));
        b[104] |= 0x80;
        assert!(parse_las(&b, &|_, _| {}).err().unwrap_or_default().contains("LAZ"));
    }
}
