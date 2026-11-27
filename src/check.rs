//! Model Checker: data checking rules (properties, attributes, types) and import
//! buildingSMART IDS 1.0 (Information Delivery Specification).
//!
//! A rule = "applicability" (which elements it checks: IFC type / category, properties) +
//! "requirements" (what they must have: property present, value from a list, pattern, range).

use crate::model::{ElemId, Element, Model};
use serde::{Deserialize, Serialize};

/// The restriction of a value (type name, Pset, property or its value).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Default)]
pub enum Value {
    /// anything (or "must only exist")
    #[default]
    Any,
    /// one of the values (simpleValue / enumeration)
    OneOf(Vec<String>),
    /// XSD regular expression (anchored to the whole text)
    Pattern(String),
    /// numeric range (min / max inclusive)
    Range { min: Option<f64>, max: Option<f64> },
}

impl Value {
    pub fn one(s: &str) -> Value {
        if s.trim().is_empty() { Value::Any } else { Value::OneOf(vec![s.trim().to_string()]) }
    }
    pub fn is_any(&self) -> bool {
        matches!(self, Value::Any)
    }
    /// the text shown in reports
    pub fn describe(&self) -> String {
        match self {
            Value::Any => "any".into(),
            Value::OneOf(v) if v.len() == 1 => format!("'{}'", v[0]),
            Value::OneOf(v) => format!("one of {}", v.iter().map(|s| format!("'{s}'")).collect::<Vec<_>>().join(", ")),
            Value::Pattern(p) => format!("matching /{p}/"),
            Value::Range { min, max } => match (min, max) {
                (Some(a), Some(b)) => format!("between {a} and {b}"),
                (Some(a), None) => format!(">= {a}"),
                (None, Some(b)) => format!("<= {b}"),
                (None, None) => "a number".into(),
            },
        }
    }
    /// does the value `s` satisfy the restriction? Tolerant comparison: no leading/trailing spaces, no
    /// upper / lower case differences, equal numbers (12 = 12.0), the unit written after the number is ignored.
    pub fn matches(&self, s: &str) -> bool {
        let s = s.trim();
        match self {
            Value::Any => true,
            Value::OneOf(v) => v.iter().any(|x| same(x, s)),
            Value::Pattern(p) => Pattern::new(p).map(|re| re.is_match(s)).unwrap_or(false),
            Value::Range { min, max } => match num(s) {
                Some(x) => min.map(|a| x >= a - 1e-9).unwrap_or(true) && max.map(|b| x <= b + 1e-9).unwrap_or(true),
                None => false,
            },
        }
    }
}

fn same(a: &str, b: &str) -> bool {
    let a = a.trim();
    if a.eq_ignore_ascii_case(b) {
        return true;
    }
    let boolean = |s: &str| match s.to_ascii_uppercase().as_str() {
        "TRUE" | ".T." | "YES" | "DA" => Some(true),
        "FALSE" | ".F." | "NO" | "NU" => Some(false),
        _ => None,
    };
    if let (Some(x), Some(y)) = (boolean(a), boolean(b)) {
        return x == y;
    }
    match (num(a), num(b)) {
        (Some(x), Some(y)) => (x - y).abs() <= 1e-9 * x.abs().max(y.abs()).max(1.0),
        _ => false,
    }
}

/// the first number in the text ("12,5 m²" -> 12.5); None if the text does not start with a number
pub fn num(s: &str) -> Option<f64> {
    let s = s.trim();
    let end = s.find(|c: char| !(c.is_ascii_digit() || matches!(c, '.' | ',' | '-' | '+' | 'e' | 'E'))).unwrap_or(s.len());
    let t = s[..end].trim_end_matches(['e', 'E']).replace(',', ".");
    if t.is_empty() {
        return None;
    }
    t.parse().ok()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum Card {
    #[default]
    Required,
    Optional,
    Prohibited,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum Kind {
    /// the element type (IfcWall, or the Revit category / the type from other formats)
    #[default]
    Entity,
    /// attribute: Name, GlobalId, Description, ObjectType, Tag, PredefinedType...
    Attribute,
    /// a property from a set (Pset / parameter group)
    Property,
    /// classification (Uniclass, OmniClass...): `set` = the system, `value` = the code
    Classification,
    /// material
    Material,
}

impl Kind {
    pub const ALL: [Kind; 5] = [Kind::Entity, Kind::Attribute, Kind::Property, Kind::Classification, Kind::Material];
    pub fn label(self) -> &'static str {
        match self {
            Kind::Entity => "Type",
            Kind::Attribute => "Attribute",
            Kind::Property => "Property",
            Kind::Classification => "Classification",
            Kind::Material => "Material",
        }
    }
}

/// A condition (IDS "facet").
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Default)]
pub struct Facet {
    pub kind: Kind,
    /// Property: the set (Pset_WallCommon / the group); Classification: the system
    #[serde(default)]
    pub set: Value,
    /// Entity: the type name; Attribute / Property: the name
    #[serde(default)]
    pub name: Value,
    /// the required value (Any = must only exist); Entity: PredefinedType
    #[serde(default)]
    pub value: Value,
    #[serde(default)]
    pub card: Card,
    /// the value must be unique among the checked elements (e.g. Mark, GlobalId, door number)
    #[serde(default)]
    pub unique: bool,
}

impl Facet {
    pub fn entity(types: &[&str]) -> Facet {
        Facet { kind: Kind::Entity, name: Value::OneOf(types.iter().map(|s| s.to_string()).collect()), ..Default::default() }
    }
    pub fn property(set: &str, name: &str, value: Value) -> Facet {
        Facet { kind: Kind::Property, set: Value::one(set), name: Value::one(name), value, card: Card::Required, unique: false }
    }
    pub fn attribute(name: &str, value: Value) -> Facet {
        Facet { kind: Kind::Attribute, name: Value::one(name), value, card: Card::Required, ..Default::default() }
    }
    /// the condition text, for lists and reports
    pub fn describe(&self) -> String {
        let nm = match &self.name {
            Value::OneOf(v) => v.join(" / "),
            Value::Any => "*".into(),
            other => other.describe(),
        };
        let st = match &self.set {
            Value::OneOf(v) => v.join(" / "),
            Value::Any => String::new(),
            other => other.describe(),
        };
        let val = if self.value.is_any() { String::new() } else { format!(" = {}", self.value.describe()) };
        let card = match self.card {
            Card::Required => "",
            Card::Optional => " (optional)",
            Card::Prohibited => " (must NOT exist)",
        };
        let card = if self.unique { format!("{card} (unique)") } else { card.to_string() };
        match self.kind {
            Kind::Entity => format!("type {nm}{}", if self.value.is_any() { String::new() } else { format!(" ({})", self.value.describe()) }),
            Kind::Attribute => format!("attribute {nm}{val}{card}"),
            Kind::Property => format!("{}{nm}{val}{card}", if st.is_empty() { String::new() } else { format!("{st}.") }),
            Kind::Classification => format!("classification{}{val}{card}", if st.is_empty() { String::new() } else { format!(" {st}") }),
            Kind::Material => format!("material{val}{card}"),
        }
    }

    /// the values found on the element for this condition (without checking the value)
    fn found<'a>(&self, e: &'a Element) -> Vec<&'a str> {
        match self.kind {
            Kind::Entity => vec![e.ifc_type.as_str()],
            Kind::Attribute => {
                let mut out: Vec<&str> = Vec::new();
                for (attr, v) in [("Name", e.name.as_str()), ("GlobalId", e.guid.as_str())] {
                    if self.name.matches(attr) && !v.is_empty() {
                        out.push(v);
                    }
                }
                for p in &e.props {
                    let g = p.group.to_ascii_lowercase();
                    if (g == "element" || g == "attributes" || g == "ifc" || g == "identity data") && self.name.matches(&p.name) && !p.value.trim().is_empty() && !matches!(p.name.as_str(), "Name" | "GlobalId") {
                        out.push(p.value.as_str());
                    }
                }
                out
            }
            Kind::Property => e.props.iter().filter(|p| (self.set.is_any() || self.set.matches(&p.group)) && self.name.matches(&p.name) && !p.value.trim().is_empty()).map(|p| p.value.as_str()).collect(),
            Kind::Classification => e
                .props
                .iter()
                .filter(|p| {
                    let g = p.group.to_ascii_lowercase();
                    let n = p.name.to_ascii_lowercase();
                    (g.contains("classification") || n.contains("classification") || n.contains("uniclass") || n.contains("omniclass") || n == "assembly code")
                        && (self.set.is_any() || self.set.matches(&p.group) || self.set.matches(&p.name))
                        && !p.value.trim().is_empty()
                })
                .map(|p| p.value.as_str())
                .collect(),
            Kind::Material => e.props.iter().filter(|p| p.name.to_ascii_lowercase().contains("material") && !p.value.trim().is_empty()).map(|p| p.value.as_str()).collect(),
        }
    }

    /// the element meets the condition (for applicability: the type / property exists with the required value)
    pub fn holds(&self, e: &Element) -> bool {
        if self.kind == Kind::Entity {
            return entity_matches(&self.name, &e.ifc_type) && (self.value.is_any() || e.props.iter().any(|p| matches!(p.name.as_str(), "PredefinedType" | "ObjectType") && self.value.matches(&p.value)));
        }
        self.found(e).iter().any(|v| self.value.matches(v))
    }

    /// checks a requirement; Some(reason) if the element does NOT satisfy it
    pub fn fails(&self, e: &Element) -> Option<String> {
        if self.kind == Kind::Entity {
            let ok = self.holds(e);
            return match (self.card, ok) {
                (Card::Prohibited, true) => Some(format!("type {} is not allowed", e.ifc_type)),
                (Card::Prohibited, false) | (_, true) => None,
                (Card::Optional, false) => None,
                (Card::Required, false) => Some(format!("type is {} - expected {}", e.ifc_type, self.describe())),
            };
        }
        let vals = self.found(e);
        let ok = vals.iter().any(|v| self.value.matches(v));
        match self.card {
            Card::Prohibited => {
                if ok {
                    Some(format!("{} must not exist (found '{}')", self.describe_short(), vals.iter().find(|v| self.value.matches(v)).copied().unwrap_or("")))
                } else {
                    None
                }
            }
            Card::Required if vals.is_empty() => Some(format!("{} is missing", self.describe_short())),
            Card::Optional if vals.is_empty() => None,
            _ if ok => None,
            _ => Some(format!("{} is '{}' - expected {}", self.describe_short(), vals[0], self.value.describe())),
        }
    }

    fn describe_short(&self) -> String {
        let f = Facet { value: Value::Any, card: Card::Required, ..self.clone() };
        f.describe()
    }
}

/// IFCWALL = IfcWall; the names in IDS are uppercase, the ones in the model "pretty" or Revit categories
fn entity_matches(v: &Value, ty: &str) -> bool {
    match v {
        Value::Any => true,
        Value::OneOf(list) => list.iter().any(|x| x.trim().eq_ignore_ascii_case(ty.trim())),
        Value::Pattern(_) => v.matches(ty) || v.matches(&ty.to_ascii_uppercase()),
        Value::Range { .. } => false,
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Rule {
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default = "yes")]
    pub enabled: bool,
    /// all must be met for the element to be checked (empty = all elements)
    #[serde(default)]
    pub applicability: Vec<Facet>,
    /// all must be satisfied
    #[serde(default)]
    pub requirements: Vec<Facet>,
    /// where it comes from (the IDS file), for display
    #[serde(default)]
    pub source: String,
}

fn yes() -> bool {
    true
}

impl Rule {
    pub fn new(name: &str) -> Rule {
        Rule { name: name.into(), description: String::new(), enabled: true, applicability: Vec::new(), requirements: Vec::new(), source: String::new() }
    }
    pub fn applies(&self, e: &Element) -> bool {
        self.applicability.iter().all(|f| f.holds(e))
    }
    /// the reasons the element does not satisfy the rule (empty = OK)
    pub fn check(&self, e: &Element) -> Vec<String> {
        self.requirements.iter().filter_map(|f| f.fails(e)).collect()
    }
    pub fn applies_text(&self) -> String {
        if self.applicability.is_empty() { "all elements".into() } else { self.applicability.iter().map(|f| f.describe()).collect::<Vec<_>>().join(" and ") }
    }
}

#[derive(Clone, Debug, Default)]
pub struct RuleResult {
    pub checked: usize,
    pub failed: Vec<(ElemId, String)>,
}

impl RuleResult {
    pub fn passed(&self) -> usize {
        self.checked - self.failed.len()
    }
}

/// Runs the active rules on the given models. The result has one entry for each rule
/// (and for the inactive ones: empty).
pub fn run(rules: &[Rule], models: &[(usize, &Model)]) -> Vec<RuleResult> {
    use rayon::prelude::*;
    rules
        .par_iter()
        .map(|r| {
            let mut res = RuleResult::default();
            if !r.enabled {
                return res;
            }
            // the "unique" requirements: value -> the elements that have it (second pass, after we know them all)
            let uniq: Vec<&Facet> = r.requirements.iter().filter(|f| f.unique && f.kind != Kind::Entity).collect();
            let mut seen: Vec<std::collections::HashMap<String, Vec<ElemId>>> = vec![Default::default(); uniq.len()];
            let mut fails: std::collections::BTreeMap<ElemId, Vec<String>> = Default::default();
            for (mi, m) in models {
                for (ei, e) in m.elements.iter().enumerate() {
                    // point clouds / elements without data are not checked
                    if e.tri_count == 0 && e.props.is_empty() {
                        continue;
                    }
                    if !r.applies(e) {
                        continue;
                    }
                    let id = ElemId { m: *mi as u32, e: ei as u32 };
                    res.checked += 1;
                    let why = r.check(e);
                    if !why.is_empty() {
                        fails.entry(id).or_default().extend(why);
                    }
                    for (k, f) in uniq.iter().enumerate() {
                        if let Some(v) = f.found(e).first() {
                            seen[k].entry(v.trim().to_string()).or_default().push(id);
                        }
                    }
                }
            }
            for (k, f) in uniq.iter().enumerate() {
                for (v, ids) in &seen[k] {
                    if ids.len() > 1 {
                        for id in ids {
                            fails.entry(*id).or_default().push(format!("{} '{v}' is not unique ({} elements have it)", f.describe_short(), ids.len()));
                        }
                    }
                }
            }
            res.failed = fails.into_iter().map(|(id, w)| (id, w.join("; "))).collect();
            res
        })
        .collect()
}

/// Ready-made rules, useful on any model.
pub fn starter_rules() -> Vec<Rule> {
    let mut a = Rule::new("Every element has a name");
    a.requirements.push(Facet::attribute("Name", Value::Any));
    let mut b = Rule::new("Walls have a fire rating");
    b.applicability.push(Facet::entity(&["IfcWall", "IfcWallStandardCase", "Walls"]));
    b.requirements.push(Facet { kind: Kind::Property, set: Value::Any, name: Value::OneOf(vec!["FireRating".into(), "Fire Rating".into()]), value: Value::Any, card: Card::Required, unique: false });
    let mut c = Rule::new("Doors have IsExternal");
    c.applicability.push(Facet::entity(&["IfcDoor", "Doors"]));
    c.requirements.push(Facet::property("Pset_DoorCommon", "IsExternal", Value::OneOf(vec!["TRUE".into(), "FALSE".into()])));
    vec![a, b, c]
}

// ===================================================================== IDS 1.0

fn local(n: &str) -> &str {
    n.rsplit(':').next().unwrap_or(n)
}

fn kid<'a>(n: &'a crate::e57::Node, name: &str) -> Option<&'a crate::e57::Node> {
    n.kids.iter().find(|k| local(&k.name) == name)
}

/// <ids:simpleValue> or <xs:restriction> (enumeration / pattern / min / max)
fn ids_value(n: Option<&crate::e57::Node>) -> Value {
    let Some(n) = n else { return Value::Any };
    if let Some(s) = kid(n, "simpleValue") {
        return Value::OneOf(vec![s.text.trim().to_string()]);
    }
    if let Some(r) = kid(n, "restriction") {
        let attr = |k: &crate::e57::Node| k.attr("value").map(|s| s.to_string());
        let enums: Vec<String> = r.kids.iter().filter(|k| local(&k.name) == "enumeration").filter_map(attr).collect();
        if !enums.is_empty() {
            return Value::OneOf(enums);
        }
        if let Some(p) = r.kids.iter().find(|k| local(&k.name) == "pattern").and_then(attr) {
            // ".*" = any value
            return if p == ".*" { Value::Any } else { Value::Pattern(p) };
        }
        let get = |names: &[&str]| r.kids.iter().find(|k| names.contains(&local(&k.name))).and_then(attr).and_then(|s| s.trim().parse::<f64>().ok());
        let (min, max) = (get(&["minInclusive", "minExclusive"]), get(&["maxInclusive", "maxExclusive"]));
        if min.is_some() || max.is_some() {
            return Value::Range { min, max };
        }
    }
    let t = n.text.trim();
    if t.is_empty() { Value::Any } else { Value::OneOf(vec![t.to_string()]) }
}

fn ids_card(n: &crate::e57::Node) -> Card {
    match n.attr("cardinality").map(|s| s.trim().to_ascii_lowercase()) {
        Some(c) if c == "optional" => Card::Optional,
        Some(c) if c == "prohibited" => Card::Prohibited,
        Some(_) => Card::Required,
        // IDS 0.9: minOccurs / maxOccurs
        None => match (n.attr("minOccurs"), n.attr("maxOccurs")) {
            (_, Some("0")) => Card::Prohibited,
            (Some("0"), _) => Card::Optional,
            _ => Card::Required,
        },
    }
}

fn ids_facet(n: &crate::e57::Node) -> Option<Facet> {
    let card = ids_card(n);
    Some(match local(&n.name) {
        "entity" => Facet { kind: Kind::Entity, name: ids_value(kid(n, "name")), value: ids_value(kid(n, "predefinedType")), set: Value::Any, card, unique: false },
        "attribute" => Facet { kind: Kind::Attribute, name: ids_value(kid(n, "name")), value: ids_value(kid(n, "value")), set: Value::Any, card, unique: false },
        "property" => Facet { kind: Kind::Property, set: ids_value(kid(n, "propertySet")), name: ids_value(kid(n, "baseName").or_else(|| kid(n, "name"))), value: ids_value(kid(n, "value")), card, unique: false },
        "classification" => Facet { kind: Kind::Classification, set: ids_value(kid(n, "system")), name: Value::Any, value: ids_value(kid(n, "value")), card, unique: false },
        "material" => Facet { kind: Kind::Material, set: Value::Any, name: Value::Any, value: ids_value(kid(n, "value")), card, unique: false },
        _ => return None,
    })
}

/// Reads an IDS file. Returns the rules and the warnings (unsupported conditions, e.g. partOf).
pub fn parse_ids(xml: &str, source: &str) -> Result<(Vec<Rule>, Vec<String>), String> {
    let root = crate::e57::parse_xml(xml.trim_start_matches('\u{feff}')).map_err(|_| "This is not a valid IDS (XML) file.".to_string())?;
    if local(&root.name) != "ids" {
        return Err(format!("This is not an IDS file (root element <{}>).", root.name));
    }
    let specs = kid(&root, "specifications").ok_or("The IDS file has no <specifications>.")?;
    let mut rules = Vec::new();
    let mut warn = Vec::new();
    for (i, s) in specs.kids.iter().filter(|k| local(&k.name) == "specification").enumerate() {
        let mut r = Rule::new(s.attr("name").filter(|n| !n.trim().is_empty()).unwrap_or(&format!("Specification {}", i + 1)));
        r.description = s.attr("description").or(s.attr("instructions")).unwrap_or("").to_string();
        r.source = source.to_string();
        for (part, dst) in [("applicability", 0), ("requirements", 1)] {
            let Some(p) = kid(s, part) else { continue };
            for f in &p.kids {
                match ids_facet(f) {
                    Some(fc) => {
                        if dst == 0 {
                            r.applicability.push(fc);
                        } else {
                            r.requirements.push(fc);
                        }
                    }
                    None => warn.push(format!("{}: <{}> is not checked yet (ignored)", r.name, local(&f.name))),
                }
            }
        }
        // "any IFC entity" (this is how we write the rules without conditions) = no conditions
        if r.applicability.len() == 1 && r.applicability[0].kind == Kind::Entity && r.applicability[0].name == Value::Pattern("IFC.*".into()) && r.applicability[0].value.is_any() {
            r.applicability.clear();
        }
        rules.push(r);
    }
    if rules.is_empty() {
        return Err("The IDS file contains no specifications.".into());
    }
    Ok((rules, warn))
}

// ===================================================================== XSD regular expressions (subset)

/// Minimal XSD regular expression (no dependencies): letters, `.`, classes `[a-z]` / `[^..]`, `\d \w \s \D \W \S`,
/// groups `( )`, alternatives `|`, quantifiers `* + ? {n} {n,} {n,m}`. Anchored to the whole text (as in XSD).
pub struct Pattern {
    node: Re,
}

#[derive(Debug, Clone)]
enum Re {
    Char(char),
    Any,
    Class(Vec<(char, char)>, bool),
    Seq(Vec<Re>),
    Alt(Vec<Re>),
    Rep(Box<Re>, usize, usize),
}

impl Pattern {
    pub fn new(p: &str) -> Result<Pattern, String> {
        let cs: Vec<char> = p.chars().collect();
        let mut i = 0;
        let node = parse_alt(&cs, &mut i)?;
        if i != cs.len() {
            return Err(format!("bad pattern near position {i}"));
        }
        Ok(Pattern { node })
    }
    pub fn is_match(&self, s: &str) -> bool {
        let cs: Vec<char> = s.chars().collect();
        let steps = std::cell::Cell::new(0usize);
        m(&self.node, &cs, 0, &mut |j| j == cs.len(), &steps)
    }
}

fn parse_alt(c: &[char], i: &mut usize) -> Result<Re, String> {
    let mut alts = vec![parse_seq(c, i)?];
    while *i < c.len() && c[*i] == '|' {
        *i += 1;
        alts.push(parse_seq(c, i)?);
    }
    Ok(if alts.len() == 1 { alts.pop().unwrap() } else { Re::Alt(alts) })
}

fn parse_seq(c: &[char], i: &mut usize) -> Result<Re, String> {
    let mut v = Vec::new();
    while *i < c.len() && c[*i] != '|' && c[*i] != ')' {
        let mut atom = match c[*i] {
            '(' => {
                *i += 1;
                if c.get(*i) == Some(&'?') && c.get(*i + 1) == Some(&':') {
                    *i += 2;
                }
                let a = parse_alt(c, i)?;
                if c.get(*i) != Some(&')') {
                    return Err("missing )".into());
                }
                *i += 1;
                a
            }
            '[' => parse_class(c, i)?,
            '.' => {
                *i += 1;
                Re::Any
            }
            '\\' => {
                *i += 1;
                let e = *c.get(*i).ok_or("pattern ends with \\")?;
                *i += 1;
                escape(e)
            }
            ch => {
                *i += 1;
                Re::Char(ch)
            }
        };
        // quantifiers
        while *i < c.len() {
            let (lo, hi) = match c[*i] {
                '*' => (0, usize::MAX),
                '+' => (1, usize::MAX),
                '?' => (0, 1),
                '{' => {
                    let end = c[*i..].iter().position(|&x| x == '}').ok_or("missing }")? + *i;
                    let body: String = c[*i + 1..end].iter().collect();
                    let mut parts = body.splitn(2, ',');
                    let lo: usize = parts.next().unwrap_or("").trim().parse().map_err(|_| "bad {n,m}")?;
                    let hi = match parts.next() {
                        None => lo,
                        Some(t) if t.trim().is_empty() => usize::MAX,
                        Some(t) => t.trim().parse().map_err(|_| "bad {n,m}")?,
                    };
                    *i = end;
                    (lo, hi)
                }
                _ => break,
            };
            *i += 1;
            atom = Re::Rep(Box::new(atom), lo, hi);
        }
        v.push(atom);
    }
    Ok(if v.len() == 1 { v.pop().unwrap() } else { Re::Seq(v) })
}

fn escape(e: char) -> Re {
    let d = vec![('0', '9')];
    let w = vec![('a', 'z'), ('A', 'Z'), ('0', '9'), ('_', '_')];
    let s = vec![(' ', ' '), ('\t', '\t'), ('\n', '\n'), ('\r', '\r')];
    match e {
        'd' => Re::Class(d, false),
        'D' => Re::Class(d, true),
        'w' => Re::Class(w, false),
        'W' => Re::Class(w, true),
        's' => Re::Class(s, false),
        'S' => Re::Class(s, true),
        'n' => Re::Char('\n'),
        't' => Re::Char('\t'),
        other => Re::Char(other),
    }
}

fn parse_class(c: &[char], i: &mut usize) -> Result<Re, String> {
    *i += 1;
    let neg = c.get(*i) == Some(&'^');
    if neg {
        *i += 1;
    }
    let mut ranges = Vec::new();
    let mut first = true;
    while *i < c.len() && (c[*i] != ']' || first) {
        first = false;
        let mut a = c[*i];
        *i += 1;
        if a == '\\' {
            let e = *c.get(*i).ok_or("bad class")?;
            *i += 1;
            match escape(e) {
                Re::Class(r, false) => {
                    ranges.extend(r);
                    continue;
                }
                Re::Char(ch) => a = ch,
                _ => continue,
            }
        }
        if c.get(*i) == Some(&'-') && c.get(*i + 1).map(|&x| x != ']').unwrap_or(false) {
            let b = c[*i + 1];
            *i += 2;
            ranges.push((a, b));
        } else {
            ranges.push((a, a));
        }
    }
    if c.get(*i) != Some(&']') {
        return Err("missing ]".into());
    }
    *i += 1;
    Ok(Re::Class(ranges, neg))
}

/// matching with backtracking in the "continuation" style; `steps` stops the pathological expressions
fn m(re: &Re, s: &[char], i: usize, k: &mut dyn FnMut(usize) -> bool, steps: &std::cell::Cell<usize>) -> bool {
    steps.set(steps.get() + 1);
    if steps.get() > 200_000 {
        return false;
    }
    match re {
        Re::Char(c) => i < s.len() && s[i] == *c && k(i + 1),
        Re::Any => i < s.len() && s[i] != '\n' && k(i + 1),
        Re::Class(r, neg) => i < s.len() && (r.iter().any(|(a, b)| s[i] >= *a && s[i] <= *b) != *neg) && k(i + 1),
        Re::Seq(v) => seq(v, s, i, k, steps),
        Re::Alt(v) => v.iter().any(|a| m(a, s, i, k, steps)),
        Re::Rep(a, lo, hi) => rep(a, *lo, *hi, s, i, k, steps),
    }
}

fn seq(v: &[Re], s: &[char], i: usize, k: &mut dyn FnMut(usize) -> bool, steps: &std::cell::Cell<usize>) -> bool {
    match v.split_first() {
        None => k(i),
        Some((h, t)) => m(h, s, i, &mut |j| seq(t, s, j, k, steps), steps),
    }
}

fn rep(a: &Re, lo: usize, hi: usize, s: &[char], i: usize, k: &mut dyn FnMut(usize) -> bool, steps: &std::cell::Cell<usize>) -> bool {
    // greedy: we try one more repetition (that consumes something), then the continuation
    if hi > 0 {
        let nhi = if hi == usize::MAX { hi } else { hi - 1 };
        if m(a, s, i, &mut |j| j != i && rep(a, lo.saturating_sub(1), nhi, s, j, k, steps), steps) {
            return true;
        }
    }
    lo == 0 && k(i)
}

// ===================================================================== IDS 1.0: export

fn x(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

/// <ids:simpleValue> or <xs:restriction> for a restriction; `upper` = IFC entity names (uppercase)
fn ids_value_xml(v: &Value, upper: bool) -> String {
    let t = |s: &str| if upper { x(&s.trim().to_ascii_uppercase()) } else { x(s.trim()) };
    match v {
        Value::Any => "<xs:restriction base=\"xs:string\"><xs:pattern value=\".*\"/></xs:restriction>".into(),
        Value::OneOf(l) if l.len() == 1 => format!("<ids:simpleValue>{}</ids:simpleValue>", t(&l[0])),
        Value::OneOf(l) => format!("<xs:restriction base=\"xs:string\">{}</xs:restriction>", l.iter().map(|s| format!("<xs:enumeration value=\"{}\"/>", t(s))).collect::<String>()),
        Value::Pattern(p) => format!("<xs:restriction base=\"xs:string\"><xs:pattern value=\"{}\"/></xs:restriction>", x(p)),
        Value::Range { min, max } => {
            let mut s = String::from("<xs:restriction base=\"xs:double\">");
            if let Some(a) = min {
                s.push_str(&format!("<xs:minInclusive value=\"{a}\"/>"));
            }
            if let Some(b) = max {
                s.push_str(&format!("<xs:maxInclusive value=\"{b}\"/>"));
            }
            s.push_str("</xs:restriction>");
            s
        }
    }
}

fn ids_facet_xml(f: &Facet, requirement: bool) -> String {
    let card = if requirement && f.kind != Kind::Entity {
        format!(
            " cardinality=\"{}\"",
            match f.card {
                Card::Required => "required",
                Card::Optional => "optional",
                Card::Prohibited => "prohibited",
            }
        )
    } else {
        String::new()
    };
    let val = |tag: &str, v: &Value, upper: bool| if v.is_any() { String::new() } else { format!("<ids:{tag}>{}</ids:{tag}>", ids_value_xml(v, upper)) };
    match f.kind {
        Kind::Entity => format!("<ids:entity><ids:name>{}</ids:name>{}</ids:entity>", ids_value_xml(&f.name, true), val("predefinedType", &f.value, true)),
        Kind::Attribute => format!("<ids:attribute{card}><ids:name>{}</ids:name>{}</ids:attribute>", ids_value_xml(&f.name, false), val("value", &f.value, false)),
        Kind::Property => format!(
            "<ids:property{card}><ids:propertySet>{}</ids:propertySet><ids:baseName>{}</ids:baseName>{}</ids:property>",
            ids_value_xml(&f.set, false),
            ids_value_xml(&f.name, false),
            val("value", &f.value, false)
        ),
        Kind::Classification => format!("<ids:classification{card}>{}{}</ids:classification>", val("value", &f.value, false), val("system", &f.set, false)),
        Kind::Material => format!("<ids:material{card}>{}</ids:material>", val("value", &f.value, false)),
    }
}

/// The rules as a buildingSMART IDS 1.0 file (opens in Solibri, BIMcollab Zoom, usBIM, ifctester...).
/// The "unique" requirements do not exist in IDS: they are noted in the specification description.
pub fn write_ids(rules: &[Rule], title: &str, date: &str) -> String {
    let mut s = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<ids:ids xmlns:ids=\"http://standards.buildingsmart.org/IDS\" xmlns:xs=\"http://www.w3.org/2001/XMLSchema\" xmlns:xsi=\"http://www.w3.org/2001/XMLSchema-instance\" xsi:schemaLocation=\"http://standards.buildingsmart.org/IDS http://standards.buildingsmart.org/IDS/1.0/ids.xsd\">\n");
    s.push_str(&format!("  <ids:info>\n    <ids:title>{}</ids:title>\n    <ids:date>{}</ids:date>\n    <ids:description>Exported from ManageBIM Model Checker</ids:description>\n  </ids:info>\n  <ids:specifications>\n", x(title), x(date.get(..10).unwrap_or(date))));
    for r in rules {
        let mut desc = r.description.clone();
        let uniq: Vec<String> = r.requirements.iter().filter(|f| f.unique).map(|f| f.describe()).collect();
        if !uniq.is_empty() {
            if !desc.is_empty() {
                desc.push_str(" | ");
            }
            desc.push_str(&format!("Unique values (ManageBIM): {}", uniq.join("; ")));
        }
        s.push_str(&format!("    <ids:specification name=\"{}\" ifcVersion=\"IFC2X3 IFC4 IFC4X3_ADD2\"{}>\n", x(&r.name), if desc.is_empty() { String::new() } else { format!(" description=\"{}\"", x(&desc)) }));
        s.push_str("      <ids:applicability minOccurs=\"0\" maxOccurs=\"unbounded\">\n");
        if r.applicability.is_empty() {
            // IDS requires at least one condition: "any element" = any entity
            s.push_str("        <ids:entity><ids:name><xs:restriction base=\"xs:string\"><xs:pattern value=\"IFC.*\"/></xs:restriction></ids:name></ids:entity>\n");
        }
        for f in &r.applicability {
            s.push_str(&format!("        {}\n", ids_facet_xml(f, false)));
        }
        s.push_str("      </ids:applicability>\n      <ids:requirements>\n");
        for f in &r.requirements {
            s.push_str(&format!("        {}\n", ids_facet_xml(f, true)));
        }
        s.push_str("      </ids:requirements>\n    </ids:specification>\n");
    }
    s.push_str("  </ids:specifications>\n</ids:ids>\n");
    s
}

// ====================================================================== quick checks (without IDS)

/// A quick check of the model quality, without rules: name, description, result.
pub struct QuickCheck {
    pub name: &'static str,
    pub about: &'static str,
    pub result: RuleResult,
}

/// the spatial / organizational types do not need a level, material, geometry
fn is_spatial(t: &str) -> bool {
    let t = t.to_lowercase();
    ["ifcproject", "ifcsite", "ifcbuilding", "ifcbuildingstorey", "ifcspace", "ifczone", "ifcgroup", "ifcsystem", "ifcopening", "ifcannotation", "ifcgrid", "ifcvirtual"].iter().any(|k| t.starts_with(k))
}

/// Quality checks without IDS, on all models (not applied to point clouds):
/// no level, no name, no classification, no material, no geometry, degenerate geometry,
/// duplicates (same type and same geometry in the same place), duplicated GUID.
pub fn quick_checks(models: &[(usize, &Model)]) -> Vec<QuickCheck> {
    use std::collections::HashMap;
    let mut no_level = RuleResult::default();
    let mut no_name = RuleResult::default();
    let mut no_class = RuleResult::default();
    let mut no_mat = RuleResult::default();
    let mut no_geom = RuleResult::default();
    let mut tiny = RuleResult::default();
    let mut dups = RuleResult::default();
    let mut guid = RuleResult::default();
    for (mi, m) in models {
        if m.cloud.is_some() {
            continue;
        }
        let mut by_guid: HashMap<&str, Vec<u32>> = HashMap::new();
        // the duplicate key: type + triangle count + aabb rounded to mm
        let mut by_shape: HashMap<(String, u32, [i64; 6]), Vec<u32>> = HashMap::new();
        for (ei, e) in m.elements.iter().enumerate() {
            let id = ElemId { m: *mi as u32, e: ei as u32 };
            if !e.guid.is_empty() {
                by_guid.entry(e.guid.as_str()).or_default().push(ei as u32);
            }
            if is_spatial(&e.ifc_type) {
                continue;
            }
            let has = |f: &dyn Fn(&crate::model::Prop) -> bool| e.props.iter().any(|p| f(p) && !p.value.trim().is_empty());
            no_level.checked += 1;
            if e.storey.trim().is_empty() && !has(&|p| p.name.eq_ignore_ascii_case("Level") || p.name.eq_ignore_ascii_case("Storey")) {
                no_level.failed.push((id, "No level / storey".into()));
            }
            no_name.checked += 1;
            let n = e.name.trim();
            if n.is_empty() || n.eq_ignore_ascii_case(&e.ifc_type) || n.chars().all(|c| c.is_ascii_digit()) {
                no_name.failed.push((id, if n.is_empty() { "Empty name".into() } else { format!("Generic name \"{n}\"") }));
            }
            no_class.checked += 1;
            if !has(&|p| p.group.to_lowercase().contains("classification") || p.name.to_lowercase().contains("classification") || p.name.eq_ignore_ascii_case("Uniclass") || p.name.eq_ignore_ascii_case("OmniClass") || p.name.eq_ignore_ascii_case("Assembly Code")) {
                no_class.failed.push((id, "No classification".into()));
            }
            no_geom.checked += 1;
            if e.tri_count == 0 {
                no_geom.failed.push((id, "No geometry".into()));
                continue;
            }
            no_mat.checked += 1;
            if !has(&|p| p.group.to_lowercase().contains("material") || p.name.to_lowercase().contains("material")) {
                no_mat.failed.push((id, "No material".into()));
            }
            tiny.checked += 1;
            let s = e.aabb.size();
            if s.max_element() < 0.002 {
                tiny.failed.push((id, format!("Degenerate geometry ({:.1} mm)", s.max_element() * 1000.0)));
            }
            dups.checked += 1;
            let r = |v: f32| (v as f64 * 1000.0).round() as i64;
            let k = (e.ifc_type.clone(), e.tri_count, [r(e.aabb.min.x), r(e.aabb.min.y), r(e.aabb.min.z), r(e.aabb.max.x), r(e.aabb.max.y), r(e.aabb.max.z)]);
            by_shape.entry(k).or_default().push(ei as u32);
        }
        for v in by_shape.values().filter(|v| v.len() > 1) {
            for &e in v {
                dups.failed.push((ElemId { m: *mi as u32, e }, format!("Duplicate: {} identical elements in the same place", v.len())));
            }
        }
        guid.checked += m.elements.len();
        for (g, v) in by_guid.iter().filter(|(_, v)| v.len() > 1) {
            for &e in v {
                guid.failed.push((ElemId { m: *mi as u32, e }, format!("GUID {g} used {} times", v.len())));
            }
        }
    }
    for r in [&mut dups, &mut guid] {
        r.failed.sort_by_key(|f| f.0);
    }
    vec![
        QuickCheck { name: "Without level", about: "Elements not assigned to a level / storey", result: no_level },
        QuickCheck { name: "Without name", about: "Empty, numeric-only or generic names (same as the IFC type)", result: no_name },
        QuickCheck { name: "Without classification", about: "No Uniclass / OmniClass / other classification", result: no_class },
        QuickCheck { name: "Without material", about: "Elements with geometry but no material", result: no_mat },
        QuickCheck { name: "Without geometry", about: "Building elements with no 3D geometry", result: no_geom },
        QuickCheck { name: "Degenerate geometry", about: "Elements smaller than 2 mm", result: tiny },
        QuickCheck { name: "Duplicates", about: "Same type and identical geometry in the same place (copied twice)", result: dups },
        QuickCheck { name: "Duplicate GUID", about: "The same IFC GlobalId on more than one element in a model", result: guid },
    ]
}
