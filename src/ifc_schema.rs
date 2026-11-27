//! The IFC EXPRESS schemas (IFC2X3, IFC4, IFC4X3 - the official buildingSMART files, in `third_party/ifc-schema`):
//! the attribute order of each entity (with inheritance), their types and the inverse relations. Used to
//! turn ifcXML / ifcJSON (named attributes) back into IFC STEP (positional attributes), which is
//! then read by the usual IFC reader - so the same geometry and the same properties as an .ifc.
use std::collections::HashMap;
use std::sync::OnceLock;

/// The base type a TYPE / attribute reduces to.
#[derive(Clone, Debug, PartialEq)]
pub enum Base {
    Str,
    Real,
    Int,
    Bool,
    Logical,
    Binary,
    Enum,
    Select,
    Entity,
    /// LIST / SET / ARRAY / BAG OF <inner type>
    Agg(Box<Base>, String),
    Unknown,
}

#[derive(Default, Debug)]
pub struct Entity {
    pub parent: Option<String>,
    /// the own explicit attributes: (name, type expression)
    pub own: Vec<(String, String)>,
    /// inverse: (name, the relation type, the relation attribute that refers to us)
    pub inverse: Vec<(String, String, String)>,
}

#[derive(Default)]
pub struct Schema {
    pub name: String,
    /// the keys: upper-case names (IFCWALL)
    pub entities: HashMap<String, Entity>,
    /// TYPE: upper-case name -> the base expression
    pub types: HashMap<String, String>,
    /// the names with the original capitalisation (IFCWALL -> IfcWall)
    pub proper: HashMap<String, String>,
}

fn strip_comments(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find("(*") {
        out.push_str(&rest[..i]);
        match rest[i..].find("*)") {
            Some(j) => rest = &rest[i + j + 2..],
            None => return out,
        }
    }
    out.push_str(rest);
    out
}

impl Schema {
    pub fn parse(src: &str) -> Schema {
        let text = strip_comments(src);
        let mut s = Schema::default();
        if let Some(i) = text.find("SCHEMA ") {
            s.name = text[i + 7..].split(';').next().unwrap_or("").trim().to_string();
        }
        // TYPE X = ...; END_TYPE;
        let mut rest = text.as_str();
        while let Some(i) = rest.find("TYPE ") {
            let after = &rest[i + 5..];
            let Some(end) = after.find("END_TYPE") else { break };
            let body = &after[..end];
            if let Some((name, def)) = body.split_once('=') {
                let def = def.split(";\n").next().unwrap_or(def).split("WHERE").next().unwrap_or("").trim().trim_end_matches(';').trim();
                let n = name.trim().to_string();
                s.proper.insert(n.to_uppercase(), n.clone());
                s.types.insert(n.to_uppercase(), def.split_whitespace().collect::<Vec<_>>().join(" "));
            }
            rest = &after[end + 8..];
        }
        // ENTITY X ... END_ENTITY;
        let mut rest = text.as_str();
        while let Some(i) = rest.find("ENTITY ") {
            let after = &rest[i + 7..];
            let Some(end) = after.find("END_ENTITY") else { break };
            let body = &after[..end];
            rest = &after[end + 10..];
            let (head, attrs) = body.split_once(';').unwrap_or((body, ""));
            let name = head.split_whitespace().next().unwrap_or("").to_string();
            let mut e = Entity::default();
            if let Some(p) = head.find("SUBTYPE OF") {
                let inner = &head[p + 10..];
                e.parent = inner.split('(').nth(1).and_then(|x| x.split(')').next()).map(|x| x.trim().to_string());
            }
            // the sections: explicit until DERIVE / INVERSE / UNIQUE / WHERE
            let mut section = "explicit";
            for stmt in attrs.split(';') {
                let st = stmt.trim();
                if st.is_empty() {
                    continue;
                }
                let mut st = st;
                for kw in ["DERIVE", "INVERSE", "UNIQUE", "WHERE"] {
                    if let Some(r) = st.strip_prefix(kw) {
                        section = kw;
                        st = r.trim();
                    }
                }
                let Some((n, t)) = st.split_once(':') else { continue };
                let n = n.trim();
                if n.contains('\\') || n.contains(' ') && section != "UNIQUE" {
                    continue;
                }
                let t = t.split_whitespace().collect::<Vec<_>>().join(" ");
                match section {
                    "explicit" => e.own.push((n.to_string(), t)),
                    "INVERSE" => {
                        // SET [0:?] OF IfcRelDefinesByProperties FOR RelatedObjects
                        if let Some((ty, attr)) = t.rsplit_once(" FOR ") {
                            let rel = ty.split(" OF ").last().unwrap_or(ty).trim().to_string();
                            e.inverse.push((n.to_string(), rel, attr.trim().to_string()));
                        }
                    }
                    _ => {}
                }
            }
            s.proper.insert(name.to_uppercase(), name.clone());
            s.entities.insert(name.to_uppercase(), e);
        }
        s
    }

    /// all the explicit attributes in STEP order (the parents' first)
    pub fn attrs(&self, entity: &str) -> Vec<(String, String)> {
        let mut chain = Vec::new();
        let mut cur = Some(entity.to_uppercase());
        while let Some(c) = cur {
            let Some(e) = self.entities.get(&c) else { break };
            chain.push(e);
            cur = e.parent.as_ref().map(|p| p.to_uppercase());
            if chain.len() > 32 {
                break;
            }
        }
        chain.iter().rev().flat_map(|e| e.own.iter().cloned()).collect()
    }

    /// the inverse relation `name` of the entity (also searched in the parents): (the relation type, its attribute)
    pub fn inverse(&self, entity: &str, name: &str) -> Option<(String, String)> {
        let mut cur = Some(entity.to_uppercase());
        let mut n = 0;
        while let Some(c) = cur {
            let e = self.entities.get(&c)?;
            if let Some(x) = e.inverse.iter().find(|x| x.0.eq_ignore_ascii_case(name)) {
                return Some((x.1.clone(), x.2.clone()));
            }
            cur = e.parent.as_ref().map(|p| p.to_uppercase());
            n += 1;
            if n > 32 {
                break;
            }
        }
        None
    }

    pub fn is_entity(&self, name: &str) -> bool {
        self.entities.contains_key(&name.to_uppercase())
    }

    pub fn is_subtype(&self, entity: &str, of: &str) -> bool {
        let mut cur = Some(entity.to_uppercase());
        let of = of.to_uppercase();
        let mut n = 0;
        while let Some(c) = cur {
            if c == of {
                return true;
            }
            cur = self.entities.get(&c).and_then(|e| e.parent.as_ref()).map(|p| p.to_uppercase());
            n += 1;
            if n > 32 {
                break;
            }
        }
        false
    }

    /// the base type of a type expression ("OPTIONAL IfcLabel", "LIST [1:?] OF IfcLengthMeasure"...)
    pub fn base(&self, expr: &str) -> Base {
        let e = expr.trim().trim_start_matches("OPTIONAL").trim().trim_start_matches("UNIQUE").trim();
        for agg in ["LIST", "SET", "ARRAY", "BAG"] {
            if e.starts_with(agg) {
                let inner = e.split_once(" OF ").map(|x| x.1).unwrap_or("");
                let inner = inner.trim().trim_start_matches("UNIQUE").trim();
                return Base::Agg(Box::new(self.base(inner)), inner.to_string());
            }
        }
        match e {
            "STRING" => return Base::Str,
            "REAL" | "NUMBER" => return Base::Real,
            "INTEGER" => return Base::Int,
            "BOOLEAN" => return Base::Bool,
            "LOGICAL" => return Base::Logical,
            "BINARY" => return Base::Binary,
            _ => {}
        }
        if e.starts_with("STRING") {
            return Base::Str;
        }
        if e.starts_with("BINARY") {
            return Base::Binary;
        }
        if e.starts_with("ENUMERATION") {
            return Base::Enum;
        }
        if e.starts_with("SELECT") {
            return Base::Select;
        }
        let key = e.split_whitespace().next().unwrap_or("").to_uppercase();
        if self.entities.contains_key(&key) {
            return Base::Entity;
        }
        match self.types.get(&key) {
            Some(def) if def != e => self.base(def),
            _ => Base::Unknown,
        }
    }

    pub fn proper_name(&self, upper: &str) -> String {
        self.proper.get(&upper.to_uppercase()).cloned().unwrap_or_else(|| upper.to_string())
    }
}

static IFC2X3: OnceLock<Schema> = OnceLock::new();
static IFC4: OnceLock<Schema> = OnceLock::new();
static IFC4X3: OnceLock<Schema> = OnceLock::new();

/// The schema by the name in the file (IFC2X3 / IFC4 / IFC4X3...); IFC4 by default.
pub fn schema(name: &str) -> &'static Schema {
    let n = name.to_uppercase();
    if n.contains("2X3") {
        IFC2X3.get_or_init(|| Schema::parse(include_str!("../third_party/ifc-schema/IFC2X3.exp")))
    } else if n.contains("4X3") || n.contains("4.3") {
        IFC4X3.get_or_init(|| Schema::parse(include_str!("../third_party/ifc-schema/IFC4X3.exp")))
    } else {
        IFC4.get_or_init(|| Schema::parse(include_str!("../third_party/ifc-schema/IFC4.exp")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ifc4_wall_attribute_order_and_inverse() {
        let s = schema("IFC4");
        let a: Vec<String> = s.attrs("IfcWall").into_iter().map(|x| x.0).collect();
        assert_eq!(&a[..8], &["GlobalId", "OwnerHistory", "Name", "Description", "ObjectType", "ObjectPlacement", "Representation", "Tag"]);
        assert_eq!(a.last().map(|s| s.as_str()), Some("PredefinedType"));
        assert_eq!(s.inverse("IfcWall", "IsDefinedBy"), Some(("IfcRelDefinesByProperties".into(), "RelatedObjects".into())));
        assert_eq!(s.base("OPTIONAL IfcLabel"), Base::Str);
        assert_eq!(s.base("IfcLengthMeasure"), Base::Real);
        assert_eq!(s.base("IfcWallTypeEnum"), Base::Enum);
        assert_eq!(s.base("IfcValue"), Base::Select);
        assert!(matches!(s.base("LIST [3:3] OF IfcLengthMeasure"), Base::Agg(_, _)));
        let p: Vec<String> = s.attrs("IfcCartesianPoint").into_iter().map(|x| x.0).collect();
        assert_eq!(p, vec!["Coordinates".to_string()]);
        let s2 = schema("IFC2X3");
        assert_eq!(s2.attrs("IfcWallStandardCase").len(), 8);
    }
}
