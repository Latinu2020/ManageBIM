//! Manual prietenos pentru scripturi Rhai (RO / EN), afișat în Script Library.
//! Fiecare funcție are: ce face, exemplu, și efectul în aplicație.

pub struct Entry {
    pub func: &'static str,
    pub ro: &'static str,
    pub en: &'static str,
    pub example: &'static str,
    pub effect_ro: &'static str,
}

pub const ENTRIES: &[Entry] = &[
    Entry {
        func: "select_where(\"Property\", \"Value\")",
        ro: "Selectează toate elementele care au proprietatea cu valoarea dată.",
        en: "Select every element whose property equals the given value.",
        example: "select_where(\"Category\", \"Walls\");",
        effect_ro: "Elementele devin selectate în arbore și în 3D; returnează numărul lor.",
    },
    Entry {
        func: "find_where_empty(\"Property\")",
        ro: "Găsește elementele care NU au proprietatea completată.",
        en: "Find elements that have the property empty or missing.",
        example: "for id in find_where_empty(\"FireRating\") { select_ids([id]); set_prop(\"NeedsReview\", \"yes\"); }",
        effect_ro: "Returnează o listă de ID-uri; combină cu select_ids / set_prop.",
    },
    Entry {
        func: "set_prop(\"Name\", \"Value\")",
        ro: "Scrie o proprietate pe elementele selectate.",
        en: "Write a property on the currently selected elements.",
        example: "set_prop(\"FireRating\", \"EI30\");",
        effect_ro: "Proprietatea apare în Properties și se salvează în proiect.",
    },
    Entry {
        func: "create_clash_test(\"Name\", tolerance)",
        ro: "Creează un test de coliziune (toleranță în mm).",
        en: "Create a clash test (tolerance in mm).",
        example: "create_clash_test(\"Structure vs MEP\", 10.0);",
        effect_ro: "Testul apare în Clash Detective; apasă Run Test pentru rezultate.",
    },
    Entry {
        func: "zoom_to_selection()",
        ro: "Mută camera pe elementele selectate.",
        en: "Move the camera to the selected elements.",
        example: "zoom_to_selection();",
        effect_ro: "3D-ul se apropie de selecție.",
    },
    Entry {
        func: "selected_count()",
        ro: "Returnează câte elemente sunt selectate acum.",
        en: "Return how many elements are selected right now.",
        example: "log(\"Selectate: \" + selected_count().to_string());",
        effect_ro: "Util în condiții: if selected_count() > 0 { ... }",
    },
    Entry {
        func: "log(\"Message\")",
        ro: "Scrie un mesaj în consola scriptului.",
        en: "Write a message to the script console.",
        example: "log(\"Am terminat!\");",
        effect_ro: "Textul apare în jos, în fereastra Script.",
    },
    Entry {
        func: "export_bcf(\"path\")",
        ro: "Exportă issue-urile curente în BCF.",
        en: "Export the current issues as BCF.",
        example: "export_bcf(\"C:/proiect/issues.bcf\");",
        effect_ro: "Fișierul BCF se scrie la calea dată.",
    },
    Entry {
        func: "export_excel(\"path\")",
        ro: "Exportă raportul în Excel.",
        en: "Export the report to Excel.",
        example: "export_excel(\"C:/proiect/raport.xlsx\");",
        effect_ro: "Fișierul .xlsx se scrie la calea dată.",
    },
];

/// Reguli de sintaxă, explicate simplu.
pub const RULES_RO: &[(&str, &str)] = &[
    ("let x = ...;", "Declară o variabilă. Ex: let n = selected_count();"),
    ("\"text\"", "Text mereu între ghilimele duble."),
    ("10.0", "Număr zecimal (cu punct, nu virgulă)."),
    ("if x > 5 { ... }", "Condiție: dacă e adevărată, rulează ce e între acolade."),
    ("for id in lista { ... }", "Buclă: rulează pentru fiecare element din listă."),
    (";", "Pune `;` la finalul fiecărei instrucțiuni."),
];
