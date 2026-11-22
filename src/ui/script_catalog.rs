//! Catalogul grafic de scripturi: definitii cu nume, descriere, categorie si
//! parametri editabili in UI. Sursa e generata inlocuind `{{cheie}}` cu valorile
//! din setari, apoi executata de motorul Rhai.

pub struct Param {
    pub key: &'static str,
    pub label: &'static str,
    pub default: &'static str,
}

pub struct Spec {
    pub id: &'static str,
    pub name: &'static str,
    pub desc: &'static str,
    pub category: &'static str,
    pub params: &'static [Param],
    pub source: &'static str,
}

pub const CATALOG: &[Spec] = &[
    Spec {
        id: "select_category",
        name: "Select by category",
        desc: "Selects every element of a category and zooms to it",
        category: "Selection",
        params: &[Param { key: "category", label: "Category", default: "Walls" }],
        source: r#"
let n = select_where("Category", "{{category}}");
log("Selected " + n.to_string() + " x {{category}}");
zoom_to_selection();
"#,
    },
    Spec {
        id: "select_level",
        name: "Select level",
        desc: "Finds all elements on a given level",
        category: "Selection",
        params: &[Param { key: "level", label: "Level", default: "Level 1" }],
        source: r#"
let n = select_where("Level", "{{level}}");
log("Level {{level}}: " + n.to_string() + " elemente");
zoom_to_selection();
"#,
    },
    Spec {
        id: "mark_missing",
        name: "Mark missing data",
        desc: "Flags elements with an empty property for review",
        category: "Data quality",
        params: &[
            Param { key: "property", label: "Property", default: "Classification" },
            Param { key: "mark", label: "Mark as", default: "NeedsReview" },
        ],
        source: r#"
let ids = find_where_empty("{{property}}");
for id in ids { select_ids([id]); set_prop("{{mark}}", "yes"); }
let n = select_where("{{mark}}", "yes");
log("Marked " + n.to_string() + " elements without {{property}}");
zoom_to_selection();
"#,
    },
    Spec {
        id: "prop_value",
        name: "Set property on selection",
        desc: "Writes a property/value on the current selection",
        category: "Data quality",
        params: &[
            Param { key: "property", label: "Property", default: "FireRating" },
            Param { key: "value", label: "Value", default: "EI30" },
        ],
        source: r#"
let n = selected_count();
if n > 0 { set_prop("{{property}}", "{{value}}"); log("Set {{property}}={{value}} on " + n.to_string() + " elemente"); }
else { log("Nothing selected."); }
"#,
    },
    Spec {
        id: "clash_pair",
        name: "Clash test pair",
        desc: "Creates a clash test between two categories with a tolerance",
        category: "Coordination",
        params: &[
            Param { key: "name", label: "Test name", default: "Structure vs MEP" },
            Param { key: "tolerance", label: "Tolerance (mm)", default: "25" },
        ],
        source: r#"
create_clash_test("{{name}}", {{tolerance}});
log("Created clash test: {{name}} ({{tolerance}} mm)");
"#,
    },
    Spec {
        id: "clash_suite",
        name: "Standard clash suite",
        desc: "Four typical discipline clash tests (Arch vs Struct, Arch vs HVAC, Struct vs Piping, Ceiling vs Sprinklers)",
        category: "Coordination",
        params: &[],
        source: r#"
let pairs = [["Arch vs Struct", "Walls", "Columns"], ["Arch vs HVAC", "Walls", "Ducts"],
             ["Struct vs Piping", "Beams", "Pipes"], ["Ceiling vs Sprinklers", "Coverings", "Sprinklers"]];
for p in pairs { create_clash_test(p[0], 25.0); log("Test: " + p[0]); }
"#,
    },
    Spec {
        id: "hide_mep",
        name: "Find MEP for hide",
        desc: "Selects pipes, ducts and cable trays so you can hide them with I / Shift+I",
        category: "Review",
        params: &[],
        source: r#"
let mep = ["IfcPipeSegment", "IfcDuctSegment", "IfcCableCarrierSegment", "Pipes", "Ducts"];
let mut total = 0;
for k in mep { total = total + select_where("Category", k); }
log("Instalatii selectate: " + total.to_string() + " - apasa I pentru Hide Unselected");
zoom_to_selection();
"#,
    },
    Spec {
        id: "quantities",
        name: "Count by category",
        desc: "Logs the number of elements for the main IFC categories",
        category: "Review",
        params: &[],
        source: r#"
let cats = ["IfcWall", "IfcColumn", "IfcBeam", "IfcSlab", "IfcDoor", "IfcWindow", "IfcPipeSegment", "IfcDuctSegment"];
for c in cats { let n = select_where("Category", c); if n > 0 { log(c + ": " + n.to_string()); } }
select_ids([]);
"#,
    },
    Spec {
        id: "export_bcf",
        name: "Export BCF",
        desc: "Exports the current issues to a BCF file (path editable)",
        category: "Output",
        params: &[Param { key: "path", label: "File", default: "issues.bcf" }],
        source: r#"
export_bcf("{{path}}");
log("BCF export requested: {{path}}");
"#,
    },
];

/// Genereaza sursa Rhai inlocuind placeholderii cu valorile din UI.
pub fn source(spec: &Spec, values: &std::collections::BTreeMap<String, String>) -> String {
    let mut src = spec.source.to_string();
    for p in spec.params {
        let value = values.get(p.key).map(|s| s.as_str()).unwrap_or(p.default);
        src = src.replace(&format!("{{{{{}}}}}", p.key), value);
    }
    src
}
