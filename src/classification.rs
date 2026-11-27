//! Local BIM classification (compatible with bSDD / Uniclass / OmniClass / MasterFormat).
//! It does not depend on the internet: the user can import a catalog from a JSON/CSV file,
//! search codes and assign them to the selected elements. The classifications are saved in the project
//! as "Classification" properties and are exported to IFC through IfcClassificationReference.
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ClassItem {
    /// the system: "Uniclass 2015", "OmniClass", "bSDD", "MasterFormat"...
    pub system: String,
    /// the code: "EF_20_10_20", "23-13 31 13", ...
    pub code: String,
    /// the title
    pub name: String,
    /// description (optional)
    #[serde(default)]
    pub description: String,
}

impl ClassItem {
    pub fn label(&self) -> String {
        format!("{} · {}", self.code, self.name)
    }
}

/// Local catalog, savable as JSON.
pub type Catalog = Vec<ClassItem>;

/// Default catalog with common classifications (Uniclass 2015, OmniClass) - offline, no license.
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
    // BOM: Excel opens the file as UTF-8 (the diacritics stay correct)
    let mut out = format!("\u{feff}{};{};{};{}\n", q("System"), q("Code"), q("Name"), q("Description"));
    for i in c {
        out.push_str(&format!("{};{};{};{}\n", q(&i.system), q(&i.code), q(&i.name), q(&i.description)));
    }
    out
}

pub fn from_csv(s: &str) -> Result<Catalog, String> {
    // the BOM added by Excel; the separator: ';' (Excel RO / EU) or ',' (Excel EN), by the header
    let s = s.trim_start_matches('\u{feff}');
    let header = s.lines().next().unwrap_or("");
    let sep = if header.matches(';').count() >= header.matches(',').count() { ';' } else { ',' };
    let mut out = Vec::new();
    for line in s.lines().skip(1) {
        if line.trim().is_empty() {
            continue;
        }
        let fields: Vec<String> = parse_csv_line(line, sep);
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

/// A CSV line (RFC 4180): fields in quotes, "" = literal quote, UTF-8 text
/// (it used to be read byte by byte and the diacritics / „—" came out broken).
fn parse_csv_line(s: &str, sep: char) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut quoted = false;
    let mut it = s.chars().peekable();
    while let Some(c) = it.next() {
        match c {
            '"' if quoted && it.peek() == Some(&'"') => {
                cur.push('"');
                it.next();
            }
            '"' => quoted = !quoted,
            c if c == sep && !quoted => out.push(std::mem::take(&mut cur)),
            _ => cur.push(c),
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
    fn csv_keeps_utf8_quotes_and_comma_separator() {
        let c = vec![ClassItem { system: "Uniclass 2015".into(), code: "EF_20".into(), name: "Pereți \"exteriori\" — fațadă".into(), description: "a;b".into() }];
        let back = from_csv(&format!("\u{feff}{}", to_csv(&c))).unwrap();
        assert_eq!(back, c);
        let comma = from_csv("System,Code,Name\nOmniClass,23-13 31 13,\"Walls, exterior\"\n").unwrap();
        assert_eq!(comma[0].name, "Walls, exterior");
    }

    #[test]
    fn empty_csv_rejected() {
        assert!(from_csv("System;Code;Name\n").is_err());
    }
}
