//! Mapare categorie sursa -> clasa IFC (ca in Revit: "Revit Category" / "IFC Class Name" / "IFC Type").
//! Pentru geometriile care NU vin din IFC (STEP, OBJ, glTF, DWG, blocuri CAD etc.),ManageBIM foloseste
//! IfcBuildingElementProxy. Aici utilizatorul poate alege clasa corecta inainte de export.
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MapRule {
    /// categoria sursa (ex. "Block", "Mesh", "glTF Mesh", numele stratului DWG)
    pub source: String,
    /// clasa IFC (ex. "IfcWall"); gol sau "Not Exported" = elementul se sarteaza
    pub class: String,
    /// valoare PredefinedType / ObjectType (optional)
    pub ifc_type: String,
    #[serde(default = "yes")]
    pub enabled: bool,
}

fn yes() -> bool { true }

impl Default for MapRule {
    fn default() -> Self {
        Self { source: String::new(), class: "IfcBuildingElementProxy".into(), ifc_type: String::new(), enabled: true }
    }
}

/// Clase IFC uzuale acceptate de exportator (cele cu lungimea corecta a listei de argumente).
pub const CLASSES: &[(&str, &str)] = &[
    ("IfcBuildingElementProxy", "Proxy / generic element"),
    ("IfcWall", "Wall"),
    ("IfcWallStandardCase", "Wall standard case"),
    ("IfcSlab", "Slab"),
    ("IfcRoof", "Roof"),
    ("IfcColumn", "Column"),
    ("IfcBeam", "Beam"),
    ("IfcDoor", "Door"),
    ("IfcWindow", "Window"),
    ("IfcPlate", "Plate"),
    ("IfcCovering", "Covering"),
    ("IfcStair", "Stair"),
    ("IfcRailing", "Railing"),
    ("IfcFurnishingElement", "Furnishing"),
    ("IfcPipeSegment", "Pipe segment"),
    ("IfcPipeFitting", "Pipe fitting"),
    ("IfcDuctSegment", "Duct segment"),
    ("IfcDuctFitting", "Duct fitting"),
    ("IfcCableCarrierSegment", "Cable carrier"),
    ("IfcFlowTerminal", "Terminal / grille"),
    ("IfcSpace", "Space"),
    ("IfcBuilding", "Building"),
    ("Not Exported", "Not Exported"),
];

/// reguli implicite pentru geometriile non-IFC intalnite in ManageBIM
pub fn default_rules() -> Vec<MapRule> {
    [
        ("Block", "IfcBuildingElementProxy"),
        ("Mesh", "IfcBuildingElementProxy"),
        ("glTF Mesh", "IfcBuildingElementProxy"),
        ("Import", "IfcBuildingElementProxy"),
        ("Solid", "IfcBuildingElementProxy"),
        ("Point cloud", "IfcBuildingElementProxy"),
    ]
    .into_iter()
    .map(|(source, class)| MapRule { source: source.into(), class: class.into(), ifc_type: String::new(), enabled: true })
    .collect()
}

/// rezolva clasa pentru o categorie sursa: regula exacta, apoi prefix ignorand cazul.
pub fn resolve<'a>(rules: &'a [MapRule], source: &str) -> Option<&'a MapRule> {
    rules.iter().find(|r| r.enabled && r.source.eq_ignore_ascii_case(source))
        .or_else(|| rules.iter().find(|r| {
            r.enabled && !r.source.is_empty() && (source.to_ascii_lowercase().starts_with(&r.source.to_ascii_lowercase()) || r.source.to_ascii_lowercase().starts_with(&source.to_ascii_lowercase()))
        }))
}

pub fn to_json(rules: &[MapRule]) -> String {
    serde_json::to_string_pretty(rules).unwrap_or_default()
}

pub fn from_json(s: &str) -> Result<Vec<MapRule>, String> {
    serde_json::from_str(s).map_err(|e| e.to_string())
}

/// CSV ca in Revit: prima coloana = categorie sursa, a doua = clasa IFC, a treia = IFC Type.
/// Separator ';' (compatibil cu Excel romanesc).
pub fn to_csv(rules: &[MapRule]) -> String {
    let q = |s: &str| format!("\"{}\"", s.replace('"', "\"\""));
    let mut out = format!("{};{};{};{}\n", q("Source Category"), q("IFC Class Name"), q("IFC Type"), q("Enabled"));
    for r in rules {
        out.push_str(&format!("{};{};{};{}\n", q(&r.source), q(&r.class), q(&r.ifc_type), if r.enabled { "yes" } else { "no" }));
    }
    out
}

/// parser CSV simplu, cu ghilimele duble (fara virgule in campuri)
fn parse_csv(s: &str, sep: u8) -> Vec<Vec<String>> {
    let mut rows = Vec::new();
    let mut row = Vec::new();
    let mut cur = String::new();
    let mut quoted = false;
    for &b in s.as_bytes() {
        match b {
            b'"' => quoted = !quoted,
            b if b == sep && !quoted => {
                row.push(std::mem::take(&mut cur));
            }
            b'\n' if !quoted => {
                row.push(std::mem::take(&mut cur));
                if row.iter().any(|c| !c.trim().is_empty()) {
                    rows.push(std::mem::take(&mut row));
                }
            }
            _ => cur.push(b as char),
        }
    }
    if !cur.is_empty() {
        row.push(cur);
        rows.push(row);
    }
    rows
}

pub fn from_csv(s: &str) -> Result<Vec<MapRule>, String> {
    let rows = parse_csv(s, b';');
    if rows.len() < 2 {
        return Err("CSV gol sau fara randuri de mapare.".into());
    }
    let mut out = Vec::new();
    for row in rows.into_iter().skip(1) {
        if row.len() < 2 {
            continue;
        }
        let source = row[0].trim().trim_matches('"').to_string();
        if source.is_empty() {
            continue;
        }
        let class = row[1].trim().trim_matches('"').to_string();
        let ifc_type = row.get(2).map(|s| s.trim().trim_matches('"').to_string()).unwrap_or_default();
        let enabled = row.get(3).map(|s| !matches!(s.trim().to_ascii_lowercase().as_str(), "no" | "0" | "false")).unwrap_or(true);
        out.push(MapRule { source, class, ifc_type, enabled });
    }
    if out.is_empty() {
        return Err("Nicio mapare valida in CSV.".into());
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_exact_and_prefix() {
        let rules = vec![
            MapRule { source: "Block".into(), class: "IfcWall".into(), ..Default::default() },
            MapRule { source: "IfcSlab".into(), class: "IfcSlab".into(), ..Default::default() },
        ];
        assert_eq!(resolve(&rules, "Block").unwrap().class, "IfcWall");
        assert_eq!(resolve(&rules, "Block_A").unwrap().class, "IfcWall");
        assert!(resolve(&rules, "Unknown").is_none());
    }

    #[test]
    fn csv_roundtrip() {
        let r = vec![MapRule { source: "Block".into(), class: "IfcWall".into(), ifc_type: "STANDARD".into(), enabled: true }];
        let csv = to_csv(&r);
        assert!(csv.contains("Source Category"));
        let back = from_csv(&csv).unwrap();
        assert_eq!(back.len(), 1);
        assert_eq!(back[0].source, "Block");
        assert_eq!(back[0].class, "IfcWall");
        assert_eq!(back[0].ifc_type, "STANDARD");
    }

    #[test]
    fn json_roundtrip() {
        let r = default_rules();
        let back = from_json(&to_json(&r)).unwrap();
        assert_eq!(back.len(), r.len());
        assert_eq!(back[0].class, r[0].class);
    }
}
