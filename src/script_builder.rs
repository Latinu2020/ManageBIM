//! Editor vizual de scripturi (fără cod): utilizatorul adaugă „pași" dintr-o listă,
//! completează parametri în GUI, iar aplicația generează cod Rhai și îl rulează.
//! Fiecare pas corespunde unei funcții înregistrate în `scripting.rs`.

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

/// Ce fel de valoare cere un câmp (decide editorul din GUI și verificarea).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum FieldKind {
    /// numele unei proprietăți din model (listă derulantă)
    Prop,
    /// o valoare a proprietății din câmpul anterior (listă derulantă)
    Value,
    /// text liber
    Text,
    /// număr zecimal
    Number,
    /// listă de ID-uri întregi separate prin virgulă
    Ids,
    /// cale de fișier de salvat, cu extensia dată
    Path(&'static str),
}

pub struct Field {
    pub label: &'static str,
    pub default: &'static str,
    pub kind: FieldKind,
    pub hint: &'static str,
}

const fn f(label: &'static str, default: &'static str, kind: FieldKind, hint: &'static str) -> Field {
    Field { label, default, kind, hint }
}

impl StepKind {
    pub const ALL: [StepKind; 9] = [
        StepKind::SelectWhere, StepKind::FindEmpty, StepKind::SetProp,
        StepKind::CreateClash, StepKind::Zoom, StepKind::SelectIds,
        StepKind::Log, StepKind::ExportBcf, StepKind::ExportExcel,
    ];

    /// Numele afișat în editor.
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

    /// Explicația pasului, pe înțelesul tuturor.
    pub fn explain(self) -> &'static str {
        match self {
            StepKind::SelectWhere => "Selectează toate elementele care au proprietatea aleasă egală cu valoarea aleasă (înlocuiește selecția).",
            StepKind::SelectIds => "Selectează elementele după numărul lor de ordine în model.",
            StepKind::FindEmpty => "Selectează elementele la care proprietatea lipsește sau e goală.",
            StepKind::SetProp => "Scrie proprietatea pe elementele selectate în acel moment.",
            StepKind::CreateClash => "Adaugă un test nou în Clash Detective (apoi apeși Run Test).",
            StepKind::Zoom => "Mută camera pe elementele selectate.",
            StepKind::Log => "Afișează un mesaj după rulare.",
            StepKind::ExportBcf => "Salvează issue-urile curente într-un fișier BCF.",
            StepKind::ExportExcel => "Salvează raportul într-un fișier Excel.",
        }
    }

    /// Câmpurile editabile ale pasului.
    pub fn fields(self) -> &'static [Field] {
        use FieldKind::*;
        const SELECT_WHERE: &[Field] = &[f("Property", "Category", Prop, "ex. Category, Level, Name"), f("Value", "", Value, "alege din listă")];
        const SELECT_IDS: &[Field] = &[f("IDs (comma separated)", "1,2,3", Ids, "numere întregi: 1, 2, 3")];
        const FIND_EMPTY: &[Field] = &[f("Property", "FireRating", Prop, "proprietatea care trebuie completată")];
        const SET_PROP: &[Field] = &[f("Property", "Review", Prop, "o proprietate existentă sau un nume nou"), f("Value", "yes", Text, "textul de scris")];
        const CLASH: &[Field] = &[f("Name", "Structure vs MEP", Text, "numele testului"), f("Tolerance (mm)", "10", Number, "ex. 10 sau 25.5")];
        const LOG: &[Field] = &[f("Message", "Done", Text, "")];
        const BCF: &[Field] = &[f("File path", "", Path("bcf"), "apasă 📁 ca să alegi unde se salvează")];
        const XLSX: &[Field] = &[f("File path", "", Path("xlsx"), "apasă 📁 ca să alegi unde se salvează")];
        match self {
            StepKind::SelectWhere => SELECT_WHERE,
            StepKind::SelectIds => SELECT_IDS,
            StepKind::FindEmpty => FIND_EMPTY,
            StepKind::SetProp => SET_PROP,
            StepKind::CreateClash => CLASH,
            StepKind::Zoom => &[],
            StepKind::Log => LOG,
            StepKind::ExportBcf => BCF,
            StepKind::ExportExcel => XLSX,
        }
    }
}

/// Un pas concret cu valorile completate de utilizator.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Step {
    pub kind: StepKind,
    pub values: Vec<String>,
}

/// Rhai: număr zecimal cu punct (10 -> 10.0), altfel funcțiile cu f64 nu se potrivesc.
fn rhai_float(s: &str) -> String {
    let x = s.trim().replace(',', ".").parse::<f64>().unwrap_or(10.0);
    format!("{x:?}")
}

impl Step {
    pub fn new(kind: StepKind) -> Self {
        Self { kind, values: kind.fields().iter().map(|fl| fl.default.to_string()).collect() }
    }

    fn v(&self, i: usize) -> String {
        self.values.get(i).cloned().unwrap_or_default()
    }

    /// Problema unui câmp (None = în regulă). Afișată cu roșu în editor.
    pub fn field_error(&self, i: usize) -> Option<&'static str> {
        let fl = self.kind.fields().get(i)?;
        let v = self.v(i);
        let v = v.trim();
        match fl.kind {
            FieldKind::Prop if v.is_empty() => Some("Alege o proprietate"),
            FieldKind::Value if v.is_empty() => Some("Alege o valoare"),
            FieldKind::Number if v.replace(',', ".").parse::<f64>().map(|x| !x.is_finite() || x < 0.0).unwrap_or(true) => Some("Scrie un număr (ex. 10 sau 25.5)"),
            FieldKind::Ids if v.is_empty() || v.split(',').map(str::trim).filter(|s| !s.is_empty()).any(|s| s.parse::<i64>().is_err()) => Some("Doar numere întregi separate prin virgulă"),
            FieldKind::Path(_) if v.is_empty() => Some("Alege fișierul cu 📁"),
            _ => None,
        }
    }

    pub fn is_valid(&self) -> bool {
        (0..self.kind.fields().len()).all(|i| self.field_error(i).is_none())
    }

    /// Generează linia de cod Rhai pentru acest pas.
    pub fn to_rhai(&self) -> String {
        let v = |i: usize| self.v(i);
        let q = |s: &str| format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""));
        match self.kind {
            StepKind::Log => format!("log({});", q(&v(0))),
            StepKind::SelectWhere => format!("select_where({}, {});", q(&v(0)), q(&v(1))),
            StepKind::SelectIds => {
                let ids: Vec<String> = v(0).split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect();
                format!("select_ids([{}]);", ids.join(", "))
            }
            StepKind::FindEmpty => format!("let found = find_where_empty({}); select_ids(found); log(\"Găsite: \" + len(found).to_string());", q(&v(0))),
            StepKind::SetProp => format!("set_prop({}, {});", q(&v(0)), q(&v(1))),
            StepKind::CreateClash => format!("create_clash_test({}, {});", q(&v(0)), rhai_float(&v(1))),
            StepKind::Zoom => "zoom_to_selection();".to_string(),
            StepKind::ExportBcf => format!("export_bcf({});", q(&v(0))),
            StepKind::ExportExcel => format!("export_excel({});", q(&v(0))),
        }
    }
}

/// Proprietățile din modelele încărcate: pentru listele derulante și previzualizare.
#[derive(Clone, Debug, Default)]
pub struct PropIndex {
    /// numărul total de elemente
    pub total: usize,
    pub keys: std::collections::BTreeMap<String, KeyInfo>,
}

#[derive(Clone, Debug, Default)]
pub struct KeyInfo {
    /// câte elemente au proprietatea completată
    pub filled: usize,
    /// valoare -> câte elemente o au (cel mult `MAX_VALUES` valori distincte)
    pub values: std::collections::BTreeMap<String, usize>,
    /// au fost mai multe valori decât păstrăm (numărătorile pe valori pot lipsi)
    pub truncated: bool,
}

pub const MAX_VALUES: usize = 300;

impl PropIndex {
    /// `elements` = pentru fiecare element, perechile (proprietate, valoare).
    pub fn build<'a>(elements: impl Iterator<Item = Vec<(&'a str, &'a str)>>) -> Self {
        let mut ix = PropIndex::default();
        let mut seen: std::collections::HashSet<(&str, &str)> = std::collections::HashSet::new();
        for props in elements {
            ix.total += 1;
            seen.clear();
            for (k, v) in props {
                if k.is_empty() || !seen.insert((k, v.trim())) {
                    continue;
                }
                let v = v.trim();
                let info = ix.keys.entry(k.to_string()).or_default();
                if v.is_empty() {
                    continue;
                }
                // „filled” o singură dată pe element, chiar dacă proprietatea apare în mai multe grupuri
                if !seen.contains(&(k, "\u{0}")) {
                    seen.insert((k, "\u{0}"));
                    info.filled += 1;
                }
                if let Some(n) = info.values.get_mut(v) {
                    *n += 1;
                } else if info.values.len() < MAX_VALUES {
                    info.values.insert(v.to_string(), 1);
                } else {
                    info.truncated = true;
                }
            }
        }
        ix
    }

    fn key(&self, key: &str) -> Option<&KeyInfo> {
        self.keys.get(key).or_else(|| self.keys.iter().find(|(k, _)| k.eq_ignore_ascii_case(key)).map(|(_, v)| v))
    }

    /// Câte elemente ar selecta `select_where(key, value)` (None = nu știm sigur).
    pub fn count_where(&self, key: &str, value: &str) -> Option<usize> {
        let info = self.key(key)?;
        let n: usize = info.values.iter().filter(|(v, _)| v.eq_ignore_ascii_case(value.trim())).map(|(_, n)| *n).sum();
        if n == 0 && info.truncated {
            None
        } else {
            Some(n)
        }
    }

    /// Câte elemente ar găsi `find_where_empty(key)`.
    pub fn count_empty(&self, key: &str) -> usize {
        self.total - self.key(key).map_or(0, |i| i.filled).min(self.total)
    }

    /// Valorile unei proprietăți, cele mai frecvente primele.
    pub fn values_of(&self, key: &str) -> Vec<(String, usize)> {
        let mut v: Vec<(String, usize)> = self.key(key).map(|i| i.values.iter().map(|(k, n)| (k.clone(), *n)).collect()).unwrap_or_default();
        v.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
        v
    }
}

/// Scenariul complet (lista de pași) cu nume.
#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct Scenario {
    pub name: String,
    pub steps: Vec<Step>,
}

/// Primul rând al unui .rhai salvat din editor: pașii în JSON, ca să poată fi redeschiși.
pub const HEADER: &str = "// @managebim-scenario ";

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

    /// Codul de salvat: antetul cu pașii + codul generat.
    pub fn to_file(&self) -> String {
        let json = serde_json::to_string(self).unwrap_or_default();
        format!("{HEADER}{json}\n{}", self.to_rhai())
    }

    /// Pașii dintr-un fișier salvat de editor (None = script scris de mână).
    pub fn from_file(src: &str) -> Option<Scenario> {
        let first = src.lines().next()?;
        let json = first.strip_prefix(HEADER)?;
        let mut s: Scenario = serde_json::from_str(json).ok()?;
        // pași dintr-o versiune mai veche: completăm câmpurile lipsă cu valorile implicite
        for st in &mut s.steps {
            let n = st.kind.fields().len();
            while st.values.len() < n {
                st.values.push(st.kind.fields()[st.values.len()].default.to_string());
            }
        }
        Some(s)
    }

    pub fn is_valid(&self) -> bool {
        !self.steps.is_empty() && self.steps.iter().all(Step::is_valid)
    }

    /// Scenarii gata făcute (nume, descriere, pași).
    pub fn templates() -> Vec<(&'static str, &'static str, Scenario)> {
        let st = |k: StepKind, vals: &[&str]| Step { kind: k, values: vals.iter().map(|s| s.to_string()).collect() };
        vec![
            (
                "Find elements without FireRating",
                "Selectează elementele fără FireRating, le marchează și face zoom.",
                Scenario { name: "Missing FireRating".into(), steps: vec![st(StepKind::FindEmpty, &["FireRating"]), st(StepKind::SetProp, &["NeedsReview", "yes"]), st(StepKind::Zoom, &[])] },
            ),
            (
                "Select all walls and zoom",
                "Selectează toți pereții (IfcWall) și mută camera pe ei.",
                Scenario { name: "Walls".into(), steps: vec![st(StepKind::SelectWhere, &["Category", "IfcWall"]), st(StepKind::Zoom, &[])] },
            ),
            (
                "Mark doors for review",
                "Selectează ușile și le scrie proprietatea Review = yes.",
                Scenario { name: "Doors review".into(), steps: vec![st(StepKind::SelectWhere, &["Category", "IfcDoor"]), st(StepKind::SetProp, &["Review", "yes"]), st(StepKind::Log, &["Ușile au fost marcate."])] },
            ),
            (
                "Create standard clash tests",
                "Adaugă două teste uzuale în Clash Detective.",
                Scenario { name: "Clash tests".into(), steps: vec![st(StepKind::CreateClash, &["Structure vs MEP", "10"]), st(StepKind::CreateClash, &["Architecture vs MEP", "25"])] },
            ),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clash_tolerance_is_float() {
        let s = Step { kind: StepKind::CreateClash, values: vec!["A".into(), "10".into()] };
        assert_eq!(s.to_rhai(), "create_clash_test(\"A\", 10.0);");
        let s = Step { kind: StepKind::CreateClash, values: vec!["A".into(), "25,5".into()] };
        assert_eq!(s.to_rhai(), "create_clash_test(\"A\", 25.5);");
    }

    #[test]
    fn validation() {
        assert!(Step { kind: StepKind::SelectIds, values: vec!["1, 2,3".into()] }.is_valid());
        assert!(!Step { kind: StepKind::SelectIds, values: vec!["1,a".into()] }.is_valid());
        assert!(!Step { kind: StepKind::CreateClash, values: vec!["A".into(), "x".into()] }.is_valid());
        assert!(!Step::new(StepKind::ExportBcf).is_valid());
    }

    #[test]
    fn file_roundtrip() {
        let (_, _, s) = Scenario::templates().remove(0);
        let back = Scenario::from_file(&s.to_file()).expect("header");
        assert_eq!(back.steps.len(), s.steps.len());
        assert_eq!(back.to_rhai(), s.to_rhai());
        assert!(Scenario::from_file("// handwritten\nlog(\"x\");").is_none());
    }

    /// Toate scenariile gata făcute rulează în motorul Rhai fără erori.
    #[test]
    fn templates_run() {
        let input = crate::scripting::Input {
            elements: vec![(0, vec![("Category".into(), "IfcWall".into())]), (1, vec![("Category".into(), "IfcDoor".into())])],
            selected: vec![],
        };
        for (name, _, s) in Scenario::templates() {
            crate::scripting::run(&s.to_rhai(), &input).unwrap_or_else(|e| panic!("{name}: {e}"));
        }
    }

    #[test]
    fn prop_index_counts() {
        let els = vec![
            vec![("Category", "IfcWall"), ("FireRating", "EI30"), ("FireRating", "EI30")],
            vec![("Category", "IfcWall"), ("FireRating", "")],
            vec![("Category", "IfcDoor")],
        ];
        let ix = PropIndex::build(els.into_iter());
        assert_eq!(ix.total, 3);
        assert_eq!(ix.count_where("category", "ifcwall"), Some(2));
        assert_eq!(ix.count_where("Category", "IfcSlab"), Some(0));
        assert_eq!(ix.count_empty("FireRating"), 2);
        assert_eq!(ix.count_empty("Missing"), 3);
        assert_eq!(ix.values_of("Category")[0], ("IfcWall".to_string(), 2));
    }

    /// select_ids cu numere scrise de utilizator (i64 in Rhai) chiar selecteaza.
    #[test]
    fn select_ids_literals() {
        let input = crate::scripting::Input { elements: vec![(0, vec![]), (1, vec![]), (2, vec![])], selected: vec![] };
        let s = Scenario { name: "x".into(), steps: vec![Step { kind: StepKind::SelectIds, values: vec!["0, 2".into()] }] };
        let out = crate::scripting::run(&s.to_rhai(), &input).expect("run");
        assert!(matches!(out.actions.first(), Some(crate::scripting::Action::SelectIds(v)) if v == &vec![0, 2]), "{:?}", out.actions);
    }

    /// Find missing data: selecteaza toate elementele gasite, nu doar ultimul.
    #[test]
    fn find_empty_selects_all() {
        let input = crate::scripting::Input {
            elements: vec![(0, vec![("FireRating".into(), "EI30".into())]), (1, vec![]), (2, vec![("FireRating".into(), " ".into())])],
            selected: vec![],
        };
        let s = Scenario { name: "x".into(), steps: vec![Step::new(StepKind::FindEmpty)] };
        let out = crate::scripting::run(&s.to_rhai(), &input).expect("run");
        assert!(matches!(out.actions.first(), Some(crate::scripting::Action::SelectIds(v)) if v == &vec![1, 2]), "{:?}", out.actions);
    }
}
