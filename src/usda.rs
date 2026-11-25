//! USD ASCII (.usda): Pixar Universal Scene Description in forma text (Omniverse, Houdini, Blender, Revit
//! prin Omniverse Connector). Citim ierarhia de prim-uri, transformarile (xformOpOrder), plasele (Mesh) si
//! primitivele Cube / Sphere / Cylinder, culoarea (primvars:displayColor / displayOpacity), unitatile
//! (metersPerUnit, upAxis) si datele utilizatorului: customData, assetInfo, kind si atributele custom.
//! Referintele @fisier.usda@ se incarca si ele. Fara variante, fara .usdc / .usdz binare.
use crate::model::{Prop, RawElement, RawModel};
use glam::{DMat4, DQuat, DVec3};
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, PartialEq)]
enum Tok {
    Ident(String),
    Str(String),
    Num(f64),
    Path(String),
    Asset(String),
    P(char),
}

fn tokenize(s: &str) -> Vec<Tok> {
    let c: Vec<char> = s.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    // prima linie "#usda 1.0" e antetul
    while i < c.len() {
        let ch = c[i];
        if ch.is_whitespace() {
            i += 1;
        } else if ch == '#' {
            while i < c.len() && c[i] != '\n' {
                i += 1;
            }
        } else if ch == '"' || ch == '\'' {
            let triple = i + 2 < c.len() && c[i + 1] == ch && c[i + 2] == ch;
            let mut s = String::new();
            if triple {
                i += 3;
                while i + 2 < c.len() && !(c[i] == ch && c[i + 1] == ch && c[i + 2] == ch) {
                    s.push(c[i]);
                    i += 1;
                }
                i += 3;
            } else {
                i += 1;
                while i < c.len() && c[i] != ch {
                    if c[i] == '\\' && i + 1 < c.len() {
                        i += 1;
                        s.push(match c[i] {
                            'n' => '\n',
                            't' => '\t',
                            x => x,
                        });
                    } else {
                        s.push(c[i]);
                    }
                    i += 1;
                }
                i += 1;
            }
            out.push(Tok::Str(s));
        } else if ch == '<' {
            let j = (i + 1..c.len()).find(|&j| c[j] == '>').unwrap_or(c.len());
            out.push(Tok::Path(c[i + 1..j].iter().collect()));
            i = j + 1;
        } else if ch == '@' {
            let j = (i + 1..c.len()).find(|&j| c[j] == '@').unwrap_or(c.len());
            out.push(Tok::Asset(c[i + 1..j].iter().collect()));
            i = j + 1;
        } else if "()[]{}=,;".contains(ch) {
            out.push(Tok::P(ch));
            i += 1;
        } else if ch == '-' || ch == '+' || ch == '.' || ch.is_ascii_digit() {
            let j = (i + 1..c.len()).find(|&j| !(c[j].is_ascii_alphanumeric() || c[j] == '.' || c[j] == '-' || c[j] == '+')).unwrap_or(c.len());
            let t: String = c[i..j].iter().collect();
            match t.parse::<f64>() {
                Ok(v) => out.push(Tok::Num(v)),
                Err(_) => out.push(Tok::Ident(t)),
            }
            i = j;
        } else {
            let j = (i..c.len()).find(|&j| c[j].is_whitespace() || "()[]{}=,;<>@\"'#".contains(c[j])).unwrap_or(c.len());
            let j = j.max(i + 1);
            out.push(Tok::Ident(c[i..j].iter().collect()));
            i = j;
        }
    }
    out
}

#[derive(Clone, Debug)]
enum Val {
    Num(f64),
    Str(String),
    Ident(String),
    Path(String),
    Asset(String),
    List(Vec<Val>),
    Tuple(Vec<Val>),
    Dict(Vec<(String, Val)>),
    None,
}

impl Val {
    fn num(&self) -> Option<f64> {
        match self {
            Val::Num(v) => Some(*v),
            _ => None,
        }
    }
    fn nums(&self) -> Vec<f64> {
        match self {
            Val::Num(v) => vec![*v],
            Val::List(a) | Val::Tuple(a) => a.iter().flat_map(|x| x.nums()).collect(),
            _ => Vec::new(),
        }
    }
    fn text(&self) -> String {
        match self {
            Val::Num(v) => {
                if v.fract() == 0.0 && v.abs() < 1e15 {
                    format!("{}", *v as i64)
                } else {
                    format!("{v}")
                }
            }
            Val::Str(s) | Val::Ident(s) => s.clone(),
            Val::Path(p) => format!("<{p}>"),
            Val::Asset(a) => format!("@{a}@"),
            Val::List(a) => a.iter().map(|x| x.text()).collect::<Vec<_>>().join(", "),
            Val::Tuple(a) => format!("({})", a.iter().map(|x| x.text()).collect::<Vec<_>>().join(", ")),
            Val::Dict(_) => "{…}".into(),
            Val::None => String::new(),
        }
    }
}

#[derive(Default, Debug)]
struct Prim {
    spec: String,
    ty: String,
    name: String,
    meta: Vec<(String, Val)>,
    attrs: Vec<(String, String, Val)>,
    kids: Vec<Prim>,
}

struct Parser {
    t: Vec<Tok>,
    i: usize,
}

impl Parser {
    fn peek(&self) -> Option<&Tok> {
        self.t.get(self.i)
    }
    fn next(&mut self) -> Option<Tok> {
        let t = self.t.get(self.i).cloned();
        self.i += 1;
        t
    }
    fn is(&self, c: char) -> bool {
        self.peek() == Some(&Tok::P(c))
    }
    fn eat(&mut self, c: char) -> bool {
        if self.is(c) {
            self.i += 1;
            true
        } else {
            false
        }
    }
    /// sare un grup echilibrat ( ) / [ ] / { } incepand cu deschiderea curenta
    fn skip_group(&mut self) {
        let mut depth = 0i32;
        while let Some(t) = self.next() {
            match t {
                Tok::P('(') | Tok::P('[') | Tok::P('{') => depth += 1,
                Tok::P(')') | Tok::P(']') | Tok::P('}') => {
                    depth -= 1;
                    if depth <= 0 {
                        return;
                    }
                }
                _ => {}
            }
        }
    }
    fn value(&mut self) -> Val {
        match self.next() {
            Some(Tok::Num(v)) => Val::Num(v),
            Some(Tok::Str(s)) => Val::Str(s),
            Some(Tok::Path(p)) => Val::Path(p),
            Some(Tok::Asset(a)) => Val::Asset(a),
            Some(Tok::Ident(s)) if s == "None" => Val::None,
            Some(Tok::Ident(s)) => Val::Ident(s),
            Some(Tok::P('[')) => {
                let mut v = Vec::new();
                while self.peek().is_some() && !self.eat(']') {
                    if self.eat(',') {
                        continue;
                    }
                    v.push(self.value());
                }
                Val::List(v)
            }
            Some(Tok::P('(')) => {
                let mut v = Vec::new();
                while self.peek().is_some() && !self.eat(')') {
                    if self.eat(',') {
                        continue;
                    }
                    v.push(self.value());
                }
                Val::Tuple(v)
            }
            Some(Tok::P('{')) => {
                // dictionar: [tip] nume = valoare
                self.i -= 1;
                Val::Dict(self.dict())
            }
            _ => Val::None,
        }
    }
    fn dict(&mut self) -> Vec<(String, Val)> {
        let mut out = Vec::new();
        if !self.eat('{') {
            return out;
        }
        while self.peek().is_some() && !self.eat('}') {
            if self.eat(',') || self.eat(';') {
                continue;
            }
            // "string ifcGuid = ..." / "dictionary sub = { }" / '"key": value' (timeSamples)
            let mut words: Vec<String> = Vec::new();
            loop {
                match self.peek() {
                    Some(Tok::Ident(w)) => {
                        words.push(w.clone());
                        self.i += 1;
                    }
                    Some(Tok::Str(w)) => {
                        words.push(w.clone());
                        self.i += 1;
                    }
                    Some(Tok::Num(n)) => {
                        words.push(n.to_string());
                        self.i += 1;
                    }
                    _ => break,
                }
            }
            if self.eat('=') {
                let v = self.value();
                if let Some(k) = words.last() {
                    out.push((k.trim_end_matches(':').to_string(), v));
                }
            } else if words.is_empty() {
                self.i += 1;
            }
        }
        out
    }
    /// metadatele ( ... ) ale unui prim / atribut: "kind = ..", "customData = {..}", "references = @..@"
    fn metadata(&mut self) -> Vec<(String, Val)> {
        let mut out = Vec::new();
        if !self.eat('(') {
            return out;
        }
        while self.peek().is_some() && !self.eat(')') {
            match self.next() {
                Some(Tok::Ident(k)) if ["prepend", "append", "add", "delete", "reorder"].contains(&k.as_str()) => {}
                Some(Tok::Ident(k)) => {
                    if self.eat('=') {
                        let v = self.value();
                        out.push((k, v));
                    }
                }
                Some(Tok::Str(doc)) => out.push(("doc".into(), Val::Str(doc))),
                _ => {}
            }
        }
        out
    }
    fn prim(&mut self, spec: String) -> Prim {
        let mut p = Prim { spec, ..Default::default() };
        if let Some(Tok::Ident(t)) = self.peek().cloned() {
            p.ty = t;
            self.i += 1;
        }
        if let Some(Tok::Str(n)) = self.peek().cloned() {
            p.name = n;
            self.i += 1;
        }
        if self.is('(') {
            p.meta = self.metadata();
        }
        if !self.eat('{') {
            return p;
        }
        while self.peek().is_some() && !self.eat('}') {
            match self.peek().cloned() {
                Some(Tok::Ident(w)) if w == "def" || w == "over" || w == "class" => {
                    self.i += 1;
                    let k = self.prim(w);
                    p.kids.push(k);
                }
                Some(Tok::Ident(w)) if w == "variantSet" || w == "variants" => {
                    // variantele: sarite (se foloseste ce e in afara lor)
                    while self.peek().is_some() && !self.is('{') {
                        self.i += 1;
                    }
                    self.skip_group();
                }
                Some(Tok::Ident(_)) => self.attribute(&mut p),
                Some(Tok::P('{')) | Some(Tok::P('(')) | Some(Tok::P('[')) => self.skip_group(),
                _ => self.i += 1,
            }
        }
        p
    }
    /// [custom] [uniform] [prepend] tip nume [.timeSamples / .connect] [= valoare] [( meta )]  sau  rel nume = <..>
    fn attribute(&mut self, p: &mut Prim) {
        let mut words: Vec<String> = Vec::new();
        while let Some(Tok::Ident(w)) = self.peek().cloned() {
            words.push(w);
            self.i += 1;
            if words.len() > 6 {
                break;
            }
        }
        let quals = ["custom", "uniform", "varying", "config", "prepend", "append", "add", "delete"];
        let words: Vec<String> = words.into_iter().filter(|w| !quals.contains(&w.as_str())).collect();
        let (ty, name) = match words.len() {
            0 => return,
            1 => (String::new(), words[0].clone()),
            _ => (words[words.len() - 2].clone(), words[words.len() - 1].clone()),
        };
        let mut val = Val::None;
        if self.eat('=') {
            if name.ends_with(".timeSamples") {
                // prima proba de timp
                let d = self.dict();
                val = d.into_iter().next().map(|x| x.1).unwrap_or(Val::None);
            } else {
                val = self.value();
            }
        }
        if self.is('(') {
            let _ = self.metadata();
        }
        let name = name.trim_end_matches(".timeSamples").to_string();
        p.attrs.push((ty, name, val));
    }
}

fn parse(text: &str) -> (Vec<(String, Val)>, Vec<Prim>) {
    let mut ps = Parser { t: tokenize(text), i: 0 };
    let layer = if ps.is('(') { ps.metadata() } else { Vec::new() };
    let mut prims = Vec::new();
    while let Some(t) = ps.peek().cloned() {
        match t {
            Tok::Ident(w) if w == "def" || w == "over" || w == "class" => {
                ps.i += 1;
                prims.push(ps.prim(w));
            }
            _ => ps.i += 1,
        }
    }
    (layer, prims)
}

fn get<'a>(p: &'a Prim, name: &str) -> Option<&'a Val> {
    p.attrs.iter().find(|a| a.1 == name).map(|a| &a.2)
}

/// transformarea locala a prim-ului din xformOpOrder (primul op = cel exterior)
fn local_xf(p: &Prim) -> DMat4 {
    let order: Vec<String> = match get(p, "xformOpOrder") {
        Some(Val::List(a)) => a.iter().map(|x| x.text()).collect(),
        _ => return DMat4::IDENTITY,
    };
    let mut m = DMat4::IDENTITY;
    for op in order {
        let (inv, op) = match op.strip_prefix("!invert!") {
            Some(o) => (true, o.to_string()),
            None => (false, op),
        };
        if op == "!resetXformStack!" {
            continue;
        }
        let v = get(p, &op).map(|v| v.nums()).unwrap_or_default();
        let kind = op.trim_start_matches("xformOp:").split(':').next().unwrap_or("").to_string();
        let v3 = |d: f64| if v.len() >= 3 { DVec3::new(v[0], v[1], v[2]) } else { DVec3::splat(d) };
        let r = |a: f64| a.to_radians();
        let mut o = match kind.as_str() {
            "translate" => DMat4::from_translation(v3(0.0)),
            "scale" => DMat4::from_scale(if v.len() == 1 { DVec3::splat(v[0]) } else { v3(1.0) }),
            "rotateX" => DMat4::from_rotation_x(r(v.first().copied().unwrap_or(0.0))),
            "rotateY" => DMat4::from_rotation_y(r(v.first().copied().unwrap_or(0.0))),
            "rotateZ" => DMat4::from_rotation_z(r(v.first().copied().unwrap_or(0.0))),
            // rotateXYZ: intai X, apoi Y, apoi Z (pe puncte) => Rz * Ry * Rx
            "rotateXYZ" => {
                let a = v3(0.0);
                DMat4::from_rotation_z(r(a.z)) * DMat4::from_rotation_y(r(a.y)) * DMat4::from_rotation_x(r(a.x))
            }
            "rotateZYX" => {
                let a = v3(0.0);
                DMat4::from_rotation_x(r(a.x)) * DMat4::from_rotation_y(r(a.y)) * DMat4::from_rotation_z(r(a.z))
            }
            "rotateXZY" => {
                let a = v3(0.0);
                DMat4::from_rotation_y(r(a.y)) * DMat4::from_rotation_z(r(a.z)) * DMat4::from_rotation_x(r(a.x))
            }
            "rotateYXZ" => {
                let a = v3(0.0);
                DMat4::from_rotation_z(r(a.z)) * DMat4::from_rotation_x(r(a.x)) * DMat4::from_rotation_y(r(a.y))
            }
            "rotateYZX" => {
                let a = v3(0.0);
                DMat4::from_rotation_x(r(a.x)) * DMat4::from_rotation_z(r(a.z)) * DMat4::from_rotation_y(r(a.y))
            }
            "rotateZXY" => {
                let a = v3(0.0);
                DMat4::from_rotation_y(r(a.y)) * DMat4::from_rotation_x(r(a.x)) * DMat4::from_rotation_z(r(a.z))
            }
            // quatf / quatd: (w, x, y, z)
            "orient" if v.len() == 4 => DMat4::from_quat(DQuat::from_xyzw(v[1], v[2], v[3], v[0]).normalize()),
            // matrix4d e pe randuri, cu translatia pe ultimul rand (vectori-rand) => transpusa in glam
            "transform" if v.len() == 16 => DMat4::from_cols_array(&v[..16].try_into().unwrap()),
            _ => DMat4::IDENTITY,
        };
        if inv {
            o = o.inverse();
        }
        m = m * o;
    }
    m
}

fn color_of(p: &Prim, inherited: [u8; 4]) -> [u8; 4] {
    let c = get(p, "primvars:displayColor").map(|v| v.nums()).unwrap_or_default();
    let a = get(p, "primvars:displayOpacity").map(|v| v.nums()).unwrap_or_default();
    let to8 = |x: f64| (x.clamp(0.0, 1.0).powf(1.0 / 2.2) * 255.0) as u8;
    let mut col = if c.len() >= 3 { [to8(c[0]), to8(c[1]), to8(c[2]), 255] } else { inherited };
    if let Some(o) = a.first() {
        col[3] = (o.clamp(0.15, 1.0) * 255.0) as u8;
    }
    col
}

fn mesh_tris(p: &Prim, m: &DMat4) -> Vec<[DVec3; 3]> {
    let mut out = Vec::new();
    let tf = |q: DVec3| m.transform_point3(q);
    match p.ty.as_str() {
        "Mesh" => {
            let pts: Vec<DVec3> = get(p, "points").map(|v| v.nums()).unwrap_or_default().chunks_exact(3).map(|c| DVec3::new(c[0], c[1], c[2])).collect();
            let counts: Vec<usize> = get(p, "faceVertexCounts").map(|v| v.nums()).unwrap_or_default().iter().map(|x| *x as usize).collect();
            let idx: Vec<usize> = get(p, "faceVertexIndices").map(|v| v.nums()).unwrap_or_default().iter().map(|x| *x as usize).collect();
            let left = get(p, "orientation").map(|v| v.text() == "leftHanded").unwrap_or(false);
            let mut k = 0;
            for n in counts {
                if k + n > idx.len() {
                    break;
                }
                let poly: Vec<DVec3> = idx[k..k + n].iter().filter_map(|i| pts.get(*i)).map(|q| tf(*q)).collect();
                k += n;
                if poly.len() < 3 {
                    continue;
                }
                let tris = if poly.len() <= 4 {
                    (1..poly.len() - 1).map(|j| [poly[0], poly[j], poly[j + 1]]).collect()
                } else {
                    crate::open_formats::poly_tris(&poly, &[])
                };
                for t in tris {
                    out.push(if left { [t[0], t[2], t[1]] } else { t });
                }
            }
        }
        "Cube" => {
            let h = get(p, "size").and_then(|v| v.num()).unwrap_or(2.0) / 2.0;
            let c = |x: f64, y: f64, z: f64| tf(DVec3::new(x * h, y * h, z * h));
            let v = [c(-1., -1., -1.), c(1., -1., -1.), c(1., 1., -1.), c(-1., 1., -1.), c(-1., -1., 1.), c(1., -1., 1.), c(1., 1., 1.), c(-1., 1., 1.)];
            for f in [[0, 3, 2, 1], [4, 5, 6, 7], [0, 1, 5, 4], [1, 2, 6, 5], [2, 3, 7, 6], [3, 0, 4, 7]] {
                out.push([v[f[0]], v[f[1]], v[f[2]]]);
                out.push([v[f[0]], v[f[2]], v[f[3]]]);
            }
        }
        "Sphere" => {
            let r = get(p, "radius").and_then(|v| v.num()).unwrap_or(1.0);
            let (nu, nv) = (24, 12);
            let pt = |i: usize, j: usize| {
                let (u, v) = (std::f64::consts::TAU * i as f64 / nu as f64, std::f64::consts::PI * j as f64 / nv as f64);
                tf(DVec3::new(r * v.sin() * u.cos(), r * v.sin() * u.sin(), r * v.cos()))
            };
            for i in 0..nu {
                for j in 0..nv {
                    let (a, b, c, d) = (pt(i, j), pt(i + 1, j), pt(i + 1, j + 1), pt(i, j + 1));
                    out.push([a, c, b]);
                    out.push([a, d, c]);
                }
            }
        }
        "Cylinder" | "Cone" => {
            let r = get(p, "radius").and_then(|v| v.num()).unwrap_or(1.0);
            let h = get(p, "height").and_then(|v| v.num()).unwrap_or(2.0) / 2.0;
            let axis = get(p, "axis").map(|v| v.text()).unwrap_or_else(|| "Z".into());
            let cone = p.ty == "Cone";
            let ax = |x: f64, y: f64, z: f64| match axis.as_str() {
                "X" => tf(DVec3::new(z, x, y)),
                "Y" => tf(DVec3::new(y, z, x)),
                _ => tf(DVec3::new(x, y, z)),
            };
            let n = 24;
            for i in 0..n {
                let (a0, a1) = (std::f64::consts::TAU * i as f64 / n as f64, std::f64::consts::TAU * (i + 1) as f64 / n as f64);
                let (b0, b1) = (ax(r * a0.cos(), r * a0.sin(), -h), ax(r * a1.cos(), r * a1.sin(), -h));
                let (t0, t1) = if cone { (ax(0.0, 0.0, h), ax(0.0, 0.0, h)) } else { (ax(r * a0.cos(), r * a0.sin(), h), ax(r * a1.cos(), r * a1.sin(), h)) };
                out.push([b0, b1, t1]);
                if !cone {
                    out.push([b0, t1, t0]);
                    out.push([ax(0.0, 0.0, h), t0, t1]);
                }
                out.push([ax(0.0, 0.0, -h), b1, b0]);
            }
        }
        _ => {}
    }
    out
}

struct Walk<'a> {
    out: Vec<RawElement>,
    warnings: Vec<String>,
    dir: PathBuf,
    depth: usize,
    progress: &'a dyn Fn(f32, &str),
}

const GEOM_ATTRS: [&str; 14] = ["points", "faceVertexCounts", "faceVertexIndices", "normals", "extent", "xformOpOrder", "primvars:displayColor", "primvars:displayOpacity", "primvars:st", "primvars:normals", "subdivisionScheme", "orientation", "doubleSided", "visibility"];

fn walk(w: &mut Walk, p: &Prim, parent: DMat4, path: &str, group: &str, col: [u8; 4], top: bool) {
    if p.spec == "class" || w.out.len() > 200_000 {
        return;
    }
    if get(p, "visibility").map(|v| v.text() == "invisible").unwrap_or(false) {
        return;
    }
    let path = format!("{path}/{}", p.name);
    let mut m = parent * local_xf(p);
    let col = color_of(p, col);
    let group = if top || group.is_empty() { p.name.clone() } else { group.to_string() };
    // referintele externe (@fisier.usda@</prim>): continutul lor, sub acest prim
    for (k, v) in &p.meta {
        if k != "references" && k != "payload" {
            continue;
        }
        let refs: Vec<&Val> = match v {
            Val::List(a) => a.iter().collect(),
            other => vec![other],
        };
        for r in refs {
            let Val::Asset(a) = r else { continue };
            if w.depth > 6 {
                continue;
            }
            let f = w.dir.join(a.trim_start_matches("./"));
            let ext = f.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
            if ext != "usda" && ext != "usd" {
                w.warnings.push(format!("Reference {a}: only .usda text references are read."));
                continue;
            }
            match std::fs::read(&f).map(|b| String::from_utf8_lossy(&b).to_string()) {
                Ok(t) if t.starts_with("#usda") => {
                    let (layer, prims) = parse(&t);
                    let def = layer.iter().find(|(k, _)| k == "defaultPrim").map(|(_, v)| v.text());
                    let sub_dir = f.parent().map(|d| d.to_path_buf()).unwrap_or_default();
                    let old = std::mem::replace(&mut w.dir, sub_dir);
                    w.depth += 1;
                    for sp in prims.iter().filter(|sp| def.as_ref().map_or(true, |d| *d == sp.name)) {
                        // prim-ul referit inlocuieste numele, dar pastreaza transformarea locala
                        for k in &sp.kids {
                            walk(w, k, m * local_xf(sp), &path, &group, color_of(sp, col), false);
                        }
                        emit(w, sp, m * local_xf(sp), &path, &group, color_of(sp, col), Some(p));
                    }
                    w.depth -= 1;
                    w.dir = old;
                }
                _ => w.warnings.push(format!("Reference {a}: not found or not a .usda text file.")),
            }
        }
    }
    if get(p, "xformOpOrder").map(|v| v.text().contains("!resetXformStack!")).unwrap_or(false) {
        m = local_xf(p);
    }
    emit(w, p, m, &path, &group, col, None);
    for k in &p.kids {
        walk(w, k, m, &path, &group, col, false);
    }
    if w.out.len() % 500 == 0 {
        (w.progress)(0.5, "Reading USD prims");
    }
}

fn emit(w: &mut Walk, p: &Prim, m: DMat4, path: &str, group: &str, col: [u8; 4], referrer: Option<&Prim>) {
    let tris: Vec<[DVec3; 3]> = mesh_tris(p, &m).into_iter().filter(|t| t.iter().all(|q| q.is_finite()) && (t[1] - t[0]).cross(t[2] - t[0]).length_squared() > 1e-18).collect();
    if tris.is_empty() {
        return;
    }
    let name = referrer.map(|r| r.name.clone()).unwrap_or_else(|| p.name.clone());
    let mut props = vec![Prop { group: "Element".into(), name: "Name".into(), value: name.clone() }, Prop { group: "USD".into(), name: "Prim path".into(), value: path.to_string() }, Prop { group: "USD".into(), name: "Prim type".into(), value: p.ty.clone() }];
    let mut guid = path.to_string();
    let mut ifc_type = format!("USD {}", p.ty);
    let mut push_meta = |meta: &[(String, Val)], props: &mut Vec<Prop>| {
        for (k, v) in meta {
            match (k.as_str(), v) {
                ("customData", Val::Dict(d)) | ("assetInfo", Val::Dict(d)) => {
                    let g = if k == "customData" { "Custom data" } else { "Asset info" };
                    let mut stack: Vec<(String, &Val)> = d.iter().map(|(n, v)| (n.clone(), v)).collect();
                    while let Some((n, v)) = stack.pop() {
                        if let Val::Dict(dd) = v {
                            stack.extend(dd.iter().map(|(n2, v2)| (format!("{n}.{n2}"), v2)));
                            continue;
                        }
                        let low = n.to_ascii_lowercase();
                        if low.ends_with("guid") || low.ends_with("globalid") {
                            guid = v.text();
                        }
                        if low == "ifcclass" || low == "ifctype" || low.ends_with(".ifcclass") {
                            ifc_type = v.text();
                        }
                        props.push(Prop { group: g.into(), name: n, value: v.text() });
                    }
                }
                ("kind", v) | ("doc", v) | ("instanceable", v) => props.push(Prop { group: "USD".into(), name: k.clone(), value: v.text() }),
                _ => {}
            }
        }
    };
    push_meta(&p.meta, &mut props);
    if let Some(r) = referrer {
        push_meta(&r.meta, &mut props);
    }
    // atributele care nu sunt geometrie: proprietati (namespace-ul = grupul, ex. "userProperties:FireRating")
    for (ty, n, v) in p.attrs.iter().chain(referrer.map(|r| r.attrs.as_slice()).unwrap_or(&[]).iter()) {
        if GEOM_ATTRS.contains(&n.as_str()) || n.starts_with("xformOp") || n.starts_with("primvars:st") || n.starts_with("material:") || matches!(v, Val::None) || ty.ends_with("[]") && v.nums().len() > 16 {
            continue;
        }
        if ["size", "radius", "height", "axis"].contains(&n.as_str()) {
            props.push(Prop { group: "Geometry".into(), name: n.clone(), value: v.text() });
            continue;
        }
        let (g, nn) = n.rsplit_once(':').map(|(g, n)| (g.replace(':', " "), n.to_string())).unwrap_or(("Attributes".into(), n.clone()));
        let low = nn.to_ascii_lowercase();
        if low == "globalid" || low == "guid" {
            guid = v.text();
        }
        if low == "ifcclass" || low == "class" && g.to_ascii_lowercase().contains("ifc") {
            ifc_type = v.text();
        }
        props.push(Prop { group: g, name: nn, value: v.text() });
    }
    let n = tris.len();
    w.out.push(RawElement { name, ifc_type, guid, storey: group.to_string(), props, tris, colors: vec![col; n], default_hidden: false });
}

pub fn load_usda(path: &str, progress: &dyn Fn(f32, &str)) -> Result<RawModel, String> {
    progress(0.05, "Reading USD");
    let bytes = std::fs::read(path).map_err(|e| format!("Cannot read the file: {e}"))?;
    if bytes.starts_with(b"PXR-USDC") {
        return Err("This is a binary USD file (.usdc crate). Save it as text (.usda) - in Omniverse / Blender / usdcat: usdcat in.usd -o out.usda - or export glTF.".into());
    }
    if bytes.starts_with(b"PK") {
        return Err("This is a .usdz package. Unzip it (it is a ZIP) and open the .usda inside, or export glTF.".into());
    }
    let text = String::from_utf8_lossy(&bytes);
    if !text.starts_with("#usda") {
        return Err("Not a USD text file (missing the #usda header).".into());
    }
    progress(0.2, "Parsing USD");
    let (layer, prims) = parse(&text);
    let meta = |k: &str| layer.iter().find(|(n, _)| n == k).map(|(_, v)| v.clone());
    // unitatile: implicit centimetri (metersPerUnit = 0.01), axa sus implicit Y
    let mpu = meta("metersPerUnit").and_then(|v| v.num()).unwrap_or(0.01);
    let up = meta("upAxis").map(|v| v.text()).unwrap_or_else(|| "Y".into());
    let to_z = if up == "Y" { DMat4::from_rotation_x(std::f64::consts::FRAC_PI_2) } else { DMat4::IDENTITY };
    let root = to_z * DMat4::from_scale(DVec3::splat(mpu));
    let dir = Path::new(path).parent().map(|p| p.to_path_buf()).unwrap_or_default();
    let mut w = Walk { out: Vec::new(), warnings: Vec::new(), dir, depth: 0, progress };
    for p in &prims {
        if p.spec == "def" {
            // copiii prim-ului radacina dau grupurile din arbore
            let m = root * local_xf(p);
            let col = color_of(p, [200, 200, 200, 255]);
            emit(&mut w, p, m, "", &p.name, col, None);
            for k in &p.kids {
                walk(&mut w, k, m, &format!("/{}", p.name), "", col, true);
            }
        }
    }
    if w.out.is_empty() {
        return Err(format!("{}: no Mesh / Cube / Sphere / Cylinder prims with geometry.", crate::file_name(path)));
    }
    let mut info = vec![("Format".to_string(), "USD (ASCII)".to_string()), ("Units".into(), format!("{mpu} m per unit, {up} up"))];
    if let Some(d) = meta("defaultPrim") {
        info.push(("Default prim".into(), d.text()));
    }
    if let Some(Val::Dict(d)) = meta("customLayerData") {
        for (k, v) in d.iter().take(10) {
            info.push((format!("Layer {k}"), v.text()));
        }
    }
    w.warnings.sort();
    w.warnings.dedup();
    Ok(RawModel { elements: w.out, info, warnings: w.warnings })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn usda_mesh_cube_transforms_and_custom_data() {
        let src = r#"#usda 1.0
(
    defaultPrim = "Building"
    metersPerUnit = 1
    upAxis = "Z"
)

def Xform "Building" (
    kind = "assembly"
)
{
    def Xform "Level1"
    {
        double3 xformOp:translate = (0, 0, 3)
        uniform token[] xformOpOrder = ["xformOp:translate"]

        def Mesh "Wall_01" (
            customData = {
                string ifcGuid = "2O2Fr$t4X7Zf8NOew3FLOH"
                dictionary pset = {
                    string FireRating = "REI 60"
                }
            }
        )
        {
            int[] faceVertexCounts = [4]
            int[] faceVertexIndices = [0, 1, 2, 3]
            point3f[] points = [(0, 0, 0), (4, 0, 0), (4, 0, 3), (0, 0, 3)]
            color3f[] primvars:displayColor = [(1, 0, 0)]
            custom string userProperties:Material = "Concrete"
            string ifc:class = "IfcWall"
        }

        def Cube "Column" {
            double size = 0.4
            double3 xformOp:translate = (10, 0, 0)
            float3 xformOp:rotateXYZ = (0, 0, 90)
            uniform token[] xformOpOrder = ["xformOp:translate", "xformOp:rotateXYZ"]
        }
    }
}
"#;
        let dir = std::env::temp_dir().join(format!("mb_usda_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("b.usda");
        std::fs::write(&p, src).unwrap();
        let m = load_usda(&p.to_string_lossy(), &|_, _| {}).unwrap();
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(m.elements.len(), 2);
        let wall = m.elements.iter().find(|e| e.name == "Wall_01").unwrap();
        assert_eq!(wall.guid, "2O2Fr$t4X7Zf8NOew3FLOH");
        assert_eq!(wall.ifc_type, "IfcWall");
        assert_eq!(wall.storey, "Level1");
        assert_eq!(wall.colors[0], [255, 0, 0, 255]);
        assert!(wall.props.iter().any(|p| p.name == "pset.FireRating" && p.value == "REI 60"), "{:?}", wall.props);
        assert!(wall.props.iter().any(|p| p.group == "userProperties" && p.name == "Material" && p.value == "Concrete"));
        let zmin = wall.tris.iter().flat_map(|t| t.iter()).map(|p| p.z).fold(f64::INFINITY, f64::min);
        assert!((zmin - 3.0).abs() < 1e-9, "parent translate");
        let col = m.elements.iter().find(|e| e.name == "Column").unwrap();
        assert_eq!(col.tris.len(), 12);
        let xs: Vec<f64> = col.tris.iter().flat_map(|t| t.iter()).map(|p| p.x).collect();
        assert!((xs.iter().cloned().fold(f64::INFINITY, f64::min) - 9.8).abs() < 1e-9);
    }

    #[test]
    fn usda_default_units_and_y_up() {
        // implicit: centimetri, Y sus -> 100 unitati pe Y = 1 m pe Z
        let src = "#usda 1.0\ndef Mesh \"M\" {\n int[] faceVertexCounts = [3]\n int[] faceVertexIndices = [0,1,2]\n point3f[] points = [(0,0,0),(100,0,0),(0,100,0)]\n}\n";
        let dir = std::env::temp_dir().join(format!("mb_usda2_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("m.usda");
        std::fs::write(&p, src).unwrap();
        let m = load_usda(&p.to_string_lossy(), &|_, _| {}).unwrap();
        let _ = std::fs::remove_dir_all(&dir);
        let zmax = m.elements[0].tris.iter().flat_map(|t| t.iter()).map(|p| p.z).fold(f64::MIN, f64::max);
        assert!((zmax - 1.0).abs() < 1e-9, "cm + Y up: {zmax}");
    }
}
