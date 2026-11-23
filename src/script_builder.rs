//! Editor vizual de scripturi (fără cod): utilizatorul adaugă „pași" dintr-o listă,
//! completează parametri în GUI, iar aplicația generează cod Rhai și îl rulează.
//! Fiecare pas corespunde unei funcții înregistrate în `scripting.rs`.

// Editor vizual de scripturi - datele scenariului sunt publice (folosite și de App).

/// Tipul unui pas din scenariu.
#[derive(Clone, Copy, PartialEq, Eq, Debug, serde::Serialize, serde::Deserialize)]
pub enum StepKind {
    Log,
    SelectWhere,
    SelectIds,
    FindEmpty,
    SetProp,
    CreateClash,
    Zoom,
    ExportBcf,
    ExportExcel,
}

impl StepKind {
    pub const ALL: [StepKind; 9] = [
        StepKind::SelectWhere, StepKind::FindEmpty, StepKind::SetProp,
        StepKind::CreateClash, StepKind::Zoom, StepKind::SelectIds,
        StepKind::Log, StepKind::ExportBcf, StepKind::ExportExcel,
    ];

    /// Numele afișat în editor (RO / EN scurt, compatibil).
    pub fn label(self) -> &'static str {
        match self {
            StepKind::SelectWhere => "Select by property",
            StepKind::SelectIds => "Select by IDs",
            StepKind::FindEmpty => "Find missing data",
            StepKind::SetProp => "Set property",
            StepKind::CreateClash => "Create clash test",
            StepKind::Zoom => "Zoom to selection",
            StepKind::Log => "Write message",
            StepKind::ExportBcf => "Export BCF",
            StepKind::ExportExcel => "Export Excel",
        }
    }

    pub fn ro(self) -> &'static str {
        match self {
            StepKind::SelectWhere => "Selectează după proprietate",
            StepKind::SelectIds => "Selectează după ID",
            StepKind::FindEmpty => "Găsește date lipsă",
            StepKind::SetProp => "Scrie proprietate",
            StepKind::CreateClash => "Creează test clash",
            StepKind::Zoom => "Zoom la selecție",
            StepKind::Log => "Scrie mesaj",
            StepKind::ExportBcf => "Export BCF",
            StepKind::ExportExcel => "Export Excel",
        }
    }

    /// Câmpurile editabile ale pasului (etichetă, valoare inițială).
    pub fn fields(self) -> &'static [(&'static str, &'static str)] {
        match self {
            StepKind::SelectWhere => &[("Property", "Category"), ("Value", "Walls")],
            StepKind::SelectIds => &[("IDs (comma separated)", "1,2,3")],
            StepKind::FindEmpty => &[("Property", "FireRating")],
            StepKind::SetProp => &[("Property", "Review"), ("Value", "yes")],
            StepKind::CreateClash => &[("Name", "Structure vs MEP"), ("Tolerance (mm)", "10")],
            StepKind::Zoom => &[],
            StepKind::Log => &[("Message", "Done")],
            StepKind::ExportBcf => &[("File path", "issues.bcf")],
            StepKind::ExportExcel => &[("File path", "report.xlsx")],
        }
    }
}

/// Un pas concret cu valorile completate de utilizator.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Step {
    pub kind: StepKind,
    pub values: Vec<String>,
}

impl Step {
    pub fn new(kind: StepKind) -> Self {
        Self { kind, values: kind.fields().iter().map(|(_, d)| d.to_string()).collect() }
    }

    /// Generează linia de cod Rhai pentru acest pas.
    pub fn to_rhai(&self) -> String {
        let v = |i: usize| self.values.get(i).cloned().unwrap_or_default();
        let q = |s: &str| format!("\"{}\"", s.replace('\\', "\\\\").replace('\"', "\\\""));
        match self.kind {
            StepKind::Log => format!("log({});", q(&v(0))),
            StepKind::SelectWhere => format!("select_where({}, {});", q(&v(0)), q(&v(1))),
            StepKind::SelectIds => {
                let ids: Vec<String> = v(0).split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect();
                format!("select_ids([{}]);", ids.join(", "))
            }
            StepKind::FindEmpty => format!("let found = find_where_empty({}); select_ids(found); log(\"Găsite: \" + len(found).to_string());", q(&v(0))),
            StepKind::SetProp => format!("set_prop({}, {});", q(&v(0)), q(&v(1))),
            StepKind::CreateClash => format!("create_clash_test({}, {});", q(&v(0)), v(1).parse::<f64>().unwrap_or(10.0)),
            StepKind::Zoom => "zoom_to_selection();".to_string(),
            StepKind::ExportBcf => format!("export_bcf({});", q(&v(0))),
            StepKind::ExportExcel => format!("export_excel({});", q(&v(0))),
        }
    }
}

/// Scenariul complet (lista de pași) cu nume.
#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct Scenario {
    pub name: String,
    pub steps: Vec<Step>,
}

impl Scenario {
    pub fn to_rhai(&self) -> String {
        let mut s = format!("// {}", if self.name.is_empty() { "Script" } else { &self.name });
        s.push('\n');
        for st in &self.steps {
            s.push_str(&st.to_rhai());
            s.push('\n');
        }
        s
    }
}
