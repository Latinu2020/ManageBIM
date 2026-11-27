//! ifcJSON (buildingSMART) and ifcXML (IFC2X3 / IFC4) -> IFC STEP, then read by the usual IFC reader:
//! the same geometry, the same properties, the same levels as with an .ifc.
//! The entities have their attributes by NAME; the EXPRESS schema (`crate::ifc_schema`) gives their order and type in STEP.
//! Relationships written "inverted" inside the object (e.g. `isDefinedBy`, `containedInStructure`) become
//! complete relationship entities (the object is added to RelatedObjects / RelatingObject).
use crate::e57::{parse_xml, Node};
use crate::ifc_schema::{Base, Schema};
use crate::model::RawModel;
use std::collections::HashMap;

#[derive(Clone, Debug)]
enum Dv {
    Null,
    Str(String),
    Num(f64, bool),
    Bool(bool),
    /// reference by id / GlobalId
    Ref(String),
    /// instance (inline or resolved), its index
    Inst(usize),
    List(Vec<Dv>),
    /// value with an explicit type (select): IFCLABEL('x')
    Typed(String, Box<Dv>),
}

#[derive(Debug)]
struct Inst {
    ty: String,
    attrs: HashMap<String, Dv>,
}

struct Conv<'a> {
    s: &'a Schema,
    insts: Vec<Inst>,
    keys: HashMap<String, usize>,
    /// (the relationship, the relationship attribute, the object to add)
    inverse: Vec<(Dv, String, usize)>,
}

impl<'a> Conv<'a> {
    fn new(s: &'a Schema) -> Self {
        Conv { s, insts: Vec::new(), keys: HashMap::new(), inverse: Vec::new() }
    }

    fn add(&mut self, ty: &str) -> usize {
        self.insts.push(Inst { ty: ty.to_uppercase(), attrs: HashMap::new() });
        self.insts.len() - 1
    }

    fn set(&mut self, i: usize, name: &str, v: Dv) {
        let ty = self.insts[i].ty.clone();
        // inverse relationship: the object is added to the relationship, not as an attribute
        if let Some((_, rel_attr)) = self.s.inverse(&ty, name) {
            let items = match v {
                Dv::List(a) => a,
                other => vec![other],
            };
            for it in items {
                self.inverse.push((it, rel_attr.clone(), i));
            }
            return;
        }
        self.insts[i].attrs.insert(name.to_ascii_lowercase(), v);
    }

    // ---------------------------------------------------------------- JSON
    fn json_val(&mut self, v: &serde_json::Value) -> Dv {
        use serde_json::Value;
        match v {
            Value::Null => Dv::Null,
            Value::Bool(b) => Dv::Bool(*b),
            Value::Number(n) => Dv::Num(n.as_f64().unwrap_or(0.0), n.is_i64() || n.is_u64()),
            Value::String(s) => Dv::Str(s.clone()),
            Value::Array(a) => Dv::List(a.iter().map(|x| self.json_val(x)).collect()),
            Value::Object(o) => {
                let ty = o.get("type").and_then(|t| t.as_str()).unwrap_or("");
                if self.s.is_entity(ty) {
                    let only_ref = o.contains_key("ref") && o.keys().all(|k| k == "type" || k == "ref");
                    if only_ref {
                        return Dv::Ref(o["ref"].as_str().unwrap_or("").to_string());
                    }
                    return Dv::Inst(self.json_inst(v));
                }
                if let Some(r) = o.get("ref").and_then(|r| r.as_str()) {
                    return Dv::Ref(r.to_string());
                }
                if !ty.is_empty() {
                    let inner = o.get("value").map(|x| self.json_val(x)).unwrap_or(Dv::Null);
                    return Dv::Typed(ty.to_string(), Box::new(inner));
                }
                Dv::Null
            }
        }
    }

    fn json_inst(&mut self, v: &serde_json::Value) -> usize {
        let ty = v.get("type").and_then(|t| t.as_str()).unwrap_or("");
        let i = self.add(ty);
        for key in ["id", "globalId", "GlobalId"] {
            if let Some(k) = v.get(key).and_then(|x| x.as_str()) {
                self.keys.entry(k.to_string()).or_insert(i);
            }
        }
        if let Some(o) = v.as_object() {
            for (k, x) in o {
                if k == "type" || k == "id" || k == "ref" {
                    continue;
                }
                let d = self.json_val(x);
                self.set(i, k, d);
            }
        }
        i
    }

    // ---------------------------------------------------------------- XML
    fn local(n: &str) -> &str {
        n.rsplit(':').next().unwrap_or(n)
    }

    /// the content of an attribute element: entity / reference / list / typed value / text
    fn xml_val(&mut self, n: &Node) -> Dv {
        let kids: Vec<&Node> = n.kids.iter().collect();
        if kids.is_empty() {
            let t = n.text.trim();
            return if t.is_empty() { Dv::Null } else { Dv::Str(t.to_string()) };
        }
        let mut items: Vec<Dv> = Vec::new();
        for k in kids {
            let name = Self::local(&k.name);
            let ty = name.trim_end_matches("-wrapper");
            if self.s.is_entity(ty) {
                items.push(self.xml_entity(k));
            } else if ty != name || self.s.types.contains_key(&ty.to_uppercase()) {
                // <IfcLabel-wrapper>x</IfcLabel-wrapper> or <IfcLengthMeasure>3</IfcLengthMeasure>
                items.push(Dv::Typed(ty.to_string(), Box::new(Dv::Str(k.text.trim().to_string()))));
            } else {
                items.push(self.xml_val(k));
            }
        }
        let is_list = n.attrs.iter().any(|(k, v)| Self::local(k) == "cType" && v != "") || items.len() > 1;
        if is_list {
            Dv::List(items)
        } else {
            items.into_iter().next().unwrap_or(Dv::Null)
        }
    }

    fn xml_entity(&mut self, n: &Node) -> Dv {
        let attr = |k: &str| n.attrs.iter().find(|(a, _)| Self::local(a) == k).map(|(_, v)| v.clone());
        // empty reference: <IfcLocalPlacement ref="i12" xsi:nil="true"/> (or href)
        if let Some(r) = attr("ref").or_else(|| attr("href").map(|h| h.trim_start_matches('#').to_string())) {
            if n.kids.is_empty() && n.attrs.iter().all(|(a, _)| ["ref", "href", "nil", "type"].contains(&Self::local(a))) {
                return Dv::Ref(r);
            }
        }
        let ty = Self::local(&n.name).to_string();
        let i = self.add(&ty);
        if let Some(id) = attr("id") {
            self.keys.entry(id).or_insert(i);
        }
        for (k, v) in &n.attrs {
            let k = Self::local(k);
            if ["id", "ref", "href", "nil", "type", "schemaLocation", "pos", "arraySize", "cType", "itemType"].contains(&k) {
                continue;
            }
            if k == "GlobalId" {
                self.keys.entry(v.clone()).or_insert(i);
            }
            self.set(i, k, Dv::Str(v.clone()));
        }
        for k in &n.kids {
            let name = Self::local(&k.name).to_string();
            let d = self.xml_val(k);
            self.set(i, &name, d);
        }
        Dv::Inst(i)
    }

    fn xml_container(&mut self, n: &Node) {
        for k in &n.kids {
            if self.s.is_entity(Self::local(&k.name)) {
                self.xml_entity(k);
            } else {
                self.xml_container(k);
            }
        }
    }

    // ---------------------------------------------------------------- inverse + STEP
    fn resolve(&self, d: &Dv) -> Option<usize> {
        match d {
            Dv::Inst(i) => Some(*i),
            Dv::Ref(k) => self.keys.get(k).copied(),
            _ => None,
        }
    }

    fn apply_inverse(&mut self) {
        let inv = std::mem::take(&mut self.inverse);
        for (rel, attr, owner) in inv {
            let Some(r) = self.resolve(&rel) else { continue };
            let rty = self.insts[r].ty.clone();
            let decl = self.s.attrs(&rty).into_iter().find(|a| a.0.eq_ignore_ascii_case(&attr)).map(|a| a.1);
            let agg = decl.map(|t| matches!(self.s.base(&t), Base::Agg(..))).unwrap_or(false);
            let key = attr.to_ascii_lowercase();
            let cur = self.insts[r].attrs.remove(&key).unwrap_or(Dv::Null);
            let new = if agg {
                let mut items = match cur {
                    Dv::List(a) => a,
                    Dv::Null => Vec::new(),
                    other => vec![other],
                };
                if !items.iter().any(|x| self.resolve(x) == Some(owner)) {
                    items.push(Dv::Inst(owner));
                }
                Dv::List(items)
            } else {
                Dv::Inst(owner)
            };
            self.insts[r].attrs.insert(key, new);
        }
    }

    fn real(v: f64) -> String {
        if !v.is_finite() {
            return "0.".into();
        }
        let s = format!("{v:?}").replace('e', "E");
        if s.contains('.') || s.contains('E') {
            if s.contains('E') && !s.contains('.') {
                s.replacen('E', ".E", 1)
            } else {
                s
            }
        } else {
            format!("{s}.")
        }
    }

    fn string(s: &str) -> String {
        let mut o = String::from("'");
        for c in s.chars() {
            match c {
                '\'' => o.push_str("''"),
                '\\' => o.push_str("\\\\"),
                c if (c as u32) < 0x20 || (c as u32) > 0x7e => {
                    let mut buf = [0u16; 2];
                    o.push_str("\\X2\\");
                    for u in c.encode_utf16(&mut buf) {
                        o.push_str(&format!("{u:04X}"));
                    }
                    o.push_str("\\X0\\");
                }
                c => o.push(c),
            }
        }
        o.push('\'');
        o
    }

    fn bool_of(s: &str) -> Option<bool> {
        match s.trim().trim_matches('.').to_ascii_lowercase().as_str() {
            "true" | "t" => Some(true),
            "false" | "f" => Some(false),
            _ => None,
        }
    }

    /// the value `d` for an attribute of type `expr`
    fn write(&self, d: &Dv, expr: &str) -> String {
        let base = self.s.base(expr);
        // a single element where the schema asks for a list (e.g. <Items> with a single object in ifcXML)
        if let Base::Agg(_, inner) = &base {
            if matches!(d, Dv::Inst(_) | Dv::Ref(_) | Dv::Bool(_)) || matches!(d, Dv::Typed(..)) {
                return format!("({})", self.write(d, inner));
            }
        }
        match d {
            Dv::Null => "$".into(),
            Dv::Inst(_) | Dv::Ref(_) => self.resolve(d).map(|i| format!("#{}", i + 1)).unwrap_or_else(|| "$".into()),
            Dv::Typed(t, inner) => {
                let v = self.write(inner, t);
                if base == Base::Select || matches!(base, Base::Agg(..)) {
                    format!("{}({v})", t.to_uppercase())
                } else {
                    v
                }
            }
            Dv::List(items) => match &base {
                Base::Agg(_, inner) => format!("({})", items.iter().map(|x| self.write(x, inner)).collect::<Vec<_>>().join(",")),
                _ => items.first().map(|x| self.write(x, expr)).unwrap_or_else(|| "$".into()),
            },
            Dv::Bool(b) => match base {
                Base::Select => format!("IFCBOOLEAN({})", if *b { ".T." } else { ".F." }),
                _ => (if *b { ".T." } else { ".F." }).into(),
            },
            Dv::Num(v, int) => match &base {
                Base::Int => format!("{}", *v as i64),
                Base::Real => Self::real(*v),
                Base::Str => Self::string(&v.to_string()),
                Base::Select => {
                    if *int {
                        format!("IFCINTEGER({})", *v as i64)
                    } else {
                        format!("IFCREAL({})", Self::real(*v))
                    }
                }
                Base::Agg(_, inner) => format!("({})", self.write(d, inner)),
                _ => Self::real(*v),
            },
            Dv::Str(s) => match &base {
                Base::Str => Self::string(s),
                Base::Real => s.trim().parse::<f64>().map(Self::real).unwrap_or_else(|_| "$".into()),
                Base::Int => s.trim().parse::<f64>().map(|v| format!("{}", v as i64)).unwrap_or_else(|_| "$".into()),
                Base::Bool => Self::bool_of(s).map(|b| if b { ".T." } else { ".F." }.to_string()).unwrap_or_else(|| "$".into()),
                Base::Logical => Self::bool_of(s).map(|b| if b { ".T." } else { ".F." }.to_string()).unwrap_or_else(|| ".U.".into()),
                Base::Enum => format!(".{}.", s.trim().trim_matches('.').to_uppercase()),
                Base::Binary => format!("\"{}\"", s.trim()),
                Base::Entity => self.keys.get(s.trim()).map(|i| format!("#{}", i + 1)).unwrap_or_else(|| "$".into()),
                // list from XML: "0 0 0" / "i1 i2"
                Base::Agg(_, inner) => format!("({})", s.split_whitespace().map(|p| self.write(&Dv::Str(p.to_string()), inner)).collect::<Vec<_>>().join(",")),
                Base::Select => format!("IFCLABEL({})", Self::string(s)),
                Base::Unknown => Self::string(s),
            },
        }
    }

    fn to_step(&self, schema_name: &str) -> String {
        let mut out = String::new();
        out.push_str("ISO-10303-21;\nHEADER;\nFILE_DESCRIPTION(('ViewDefinition [CoordinationView]'),'2;1');\nFILE_NAME('converted','',(''),(''),'ManageBIM','ManageBIM','');\n");
        out.push_str(&format!("FILE_SCHEMA(('{schema_name}'));\nENDSEC;\nDATA;\n"));
        for (i, inst) in self.insts.iter().enumerate() {
            if !self.s.is_entity(&inst.ty) {
                continue;
            }
            let args: Vec<String> = self.s.attrs(&inst.ty).iter().map(|(n, t)| inst.attrs.get(&n.to_ascii_lowercase()).map(|d| self.write(d, t)).unwrap_or_else(|| "$".into())).collect();
            out.push_str(&format!("#{}={}({});\n", i + 1, inst.ty, args.join(",")));
        }
        out.push_str("ENDSEC;\nEND-ISO-10303-21;\n");
        out
    }
}

/// the schema name to write in FILE_SCHEMA (IFC2X3 / IFC4 / IFC4X3)
fn schema_label(s: &str) -> &'static str {
    let u = s.to_uppercase();
    if u.contains("2X3") {
        "IFC2X3"
    } else if u.contains("4X3") || u.contains("4.3") {
        "IFC4X3"
    } else {
        "IFC4"
    }
}

pub fn ifcjson_to_step(v: &serde_json::Value) -> Result<(String, usize), String> {
    let sname = schema_label(v.get("schemaIdentifier").or_else(|| v.get("schema")).and_then(|s| s.as_str()).unwrap_or("IFC4"));
    let s = crate::ifc_schema::schema(sname);
    let mut c = Conv::new(s);
    let data = v.get("data").and_then(|d| d.as_array()).ok_or("ifcJSON: no \"data\" array.")?;
    for d in data {
        if d.get("type").and_then(|t| t.as_str()).is_some_and(|t| s.is_entity(t)) {
            c.json_inst(d);
        }
    }
    c.apply_inverse();
    Ok((c.to_step(sname), c.insts.len()))
}

pub fn ifcxml_to_step(text: &str) -> Result<(String, usize), String> {
    let root = parse_xml(text).map_err(|e| format!("ifcXML: {}", e.replace("E57: ", "")))?;
    // schema: from the namespace / schemaLocation (ifcXML4: .../IFC4/...; ifcXML2x3: IFC2X3)
    let ns = format!("{} {}", root.attrs.iter().map(|(_, v)| v.as_str()).collect::<Vec<_>>().join(" "), root.name);
    let sname = if ns.to_uppercase().contains("2X3") { "IFC2X3" } else if ns.to_uppercase().contains("4X3") { "IFC4X3" } else { "IFC4" };
    let s = crate::ifc_schema::schema(sname);
    let mut c = Conv::new(s);
    if s.is_entity(Conv::local(&root.name)) {
        c.xml_entity(&root);
    } else {
        c.xml_container(&root);
    }
    c.apply_inverse();
    if c.insts.is_empty() {
        return Err("ifcXML: no IFC entities found.".into());
    }
    Ok((c.to_step(sname), c.insts.len()))
}

fn finish(step: String, n: usize, fmt: &str, progress: &dyn Fn(f32, &str)) -> Result<RawModel, String> {
    let mut m = crate::ifc::load_ifc_bytes(step.into_bytes(), progress)?;
    m.info.insert(0, ("Format".into(), format!("{fmt} (converted to IFC STEP, {n} entities)")));
    Ok(m)
}

pub fn load_ifcjson(v: &serde_json::Value, progress: &dyn Fn(f32, &str)) -> Result<RawModel, String> {
    progress(0.05, "Converting ifcJSON");
    let (step, n) = ifcjson_to_step(v)?;
    finish(step, n, "ifcJSON", progress)
}

pub fn load_ifcxml(path: &str, progress: &dyn Fn(f32, &str)) -> Result<RawModel, String> {
    progress(0.05, "Converting ifcXML");
    let b = std::fs::read(path).map_err(|e| format!("Cannot read the file: {e}"))?;
    let (step, n) = ifcxml_to_step(&String::from_utf8_lossy(&b))?;
    finish(step, n, "ifcXML", progress)
}
