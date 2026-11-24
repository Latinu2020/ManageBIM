//! Clasificare BIM locală (compatibilă cu bSDD / Uniclass / OmniClass / MasterFormat).
//! Nu depinde de internet: utilizatorul poate importa un catalog dintr-un fisier JSON/CSV,
//! cauta coduri si le atribuie elementelor selectate. Clasificarile se salveaza in proiect
//! ca proprietati "Classification" si se exporta in IFC prin IfcClassificationReference.
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ClassItem {
    /// sistemul: "Uniclass 2015", "OmniClass", "bSDD", "MasterFormat"...
    pub system: String,
    /// codul: "EF_20_10_20", "23-13 31 13", ...
    pub code: String,
    /// denumirea
    pub name: String,
    /// descriere (optional)
    #[serde(default)]
    pub description: String,
}

impl ClassItem {
    pub fn label(&self) -> String {
        format!("{} · {}", self.code, self.name)
    }
}

/// Catalog local, salvabil ca JSON.
pub type Catalog = Vec<ClassItem>;

/// Catalog implicit cu clasificari uzuale (Uniclass 2015, OmniClass) - offline, fara licenta.
pub fn builtin() -> Catalog {
    let u = |code: &str, name: &str| ClassItem { system: "Uniclass 2015".into(), code: code.into(), name: name.into(), description: String::new() };
    let o = |code: &str, name: &str| ClassItem { system: "OmniClass".into(), code: code.into(), name: name.into(), description: String::new() };
    vec![
        u("EF_20_10_20", "Walls"),
        u("EF_20_10_40", "Floors"),
        u("EF_20_10_60", "Roofs"),
        u("EF_20_10_75", "Columns"),
        u("EF_20_10_80", "Beams"),
        u("EF_20_10_35", "Doors"),
        u("EF_20_10_30", "Windows"),
        u("SS_20_10_20", "Structural walls"),
        u("SS_20_10_75", "Structural columns"),
        u("SS_25_10_20", "HVAC distribution"),
        u("SS_25_10_25", "Piping"),
        o("23-13 31 13", "Walls"),
        o("23-13 31 17", "Doors"),
        o("23-13 31 19", "Windows"),
        o("23-17 11 13", "Structural columns"),
    ]
}

pub fn to_json(c: &Catalog) -> String {
    serde_json::to_string_pretty(c).unwrap_or_default()
}

pub fn from_json(s: &str) -> Result<Catalog, String> {
    serde_json::from_str(s).map_err(|e| e.to_string())
}

pub fn to_csv(c: &Catalog) -> String {
    let q = |s: &str| format!("\"{}\"", s.replace('"', "\"\""));
    let mut out = format!("{};{};{};{}\n", q("System"), q("Code"), q("Name"), q("Description"));
    for i in c {
        out.push_str(&format!("{};{};{};{}\n", q(&i.system), q(&i.code), q(&i.name), q(&i.description)));
    }
    out
}

pub fn from_csv(s: &str) -> Result<Catalog, String> {
    let mut out = Vec::new();
    for line in s.lines().skip(1) {
        let fields: Vec<String> = parse_csv_line(line, b';');
        if fields.len() < 3 {
            continue;
        }
        let clean = |x: &str| x.trim().trim_matches('"').to_string();
        let system = clean(&fields[0]);
        let code = clean(&fields[1]);
        let name = clean(&fields[2]);
        if system.is_empty() || code.is_empty() {
            continue;
        }
        out.push(ClassItem { system, code, name, description: fields.get(3).map(|x| clean(x)).unwrap_or_default() });
    }
    if out.is_empty() {
        return Err("Nicio clasificare valida in CSV.".into());
    }
    Ok(out)
}

fn parse_csv_line(s: &str, sep: u8) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut quoted = false;
    for &b in s.as_bytes() {
        match b {
            b'"' => quoted = !quoted,
            b if b == sep && !quoted => out.push(std::mem::take(&mut cur)),
            _ => cur.push(b as char),
        }
    }
    out.push(cur);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_roundtrip() {
        let c = builtin();
        assert_eq!(from_json(&to_json(&c)).unwrap().len(), c.len());
        let csv = to_csv(&c);
        let back = from_csv(&csv).unwrap();
        assert_eq!(back.len(), c.len());
        assert_eq!(back[0].code, c[0].code);
    }

    #[test]
    fn empty_csv_rejected() {
        assert!(from_csv("System;Code;Name\n").is_err());
    }
}
