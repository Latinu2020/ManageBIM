//! Search rules, the property index behind Find Items / Smart sets, and selection sets.
use super::*;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum SearchOp {
    Contains,
    Equals,
    NotEquals,
    Defined,
    NotContains,
    Greater,
    Less,
    Undefined,
}

impl SearchOp {
    pub const ALL: [SearchOp; 8] = [
        SearchOp::Equals,
        SearchOp::NotEquals,
        SearchOp::Contains,
        SearchOp::NotContains,
        SearchOp::Greater,
        SearchOp::Less,
        SearchOp::Defined,
        SearchOp::Undefined,
    ];
    pub fn needs_value(&self) -> bool {
        !matches!(self, SearchOp::Defined | SearchOp::Undefined)
    }
    pub fn label(&self) -> &'static str {
        match self {
            SearchOp::Contains => "contains",
            SearchOp::Equals => "=",
            SearchOp::NotEquals => "≠",
            SearchOp::Defined => "defined",
            SearchOp::NotContains => "does not contain",
            SearchOp::Greater => ">",
            SearchOp::Less => "<",
            SearchOp::Undefined => "undefined",
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SearchRule {
    pub field: String,
    pub op: SearchOp,
    pub value: String,
    /// this row starts a new OR block (Navisworks "OR condition")
    #[serde(default)]
    pub or: bool,
    /// property category (property set / group). Empty = legacy field match by name only.
    #[serde(default)]
    pub group: String,
}

impl SearchRule {
    pub fn new(group: &str, field: &str, op: SearchOp, value: &str, or: bool) -> SearchRule {
        SearchRule { field: field.into(), op, value: value.into(), or, group: group.into() }
    }
}

/// Synthetic category with the basic item data (like Navisworks "Item").
pub const ITEM_CAT: &str = "Item";
pub const ITEM_PROPS: [&str; 5] = ["Name", "IFC Type", "Level", "Model", "GlobalId"];
/// Above this many distinct values a property is not listed value by value.
pub const MAX_LISTED_VALUES: usize = 3000;

#[derive(Clone, Debug, Default)]
pub struct PropValues {
    pub name: String,
    /// value -> element count, sorted by value
    pub values: Vec<(String, u32)>,
    pub distinct: usize,
    pub elements: u32,
}

/// Every category / property / value present in all loaded models.
#[derive(Clone, Debug, Default)]
pub struct PropIndex {
    pub cats: Vec<(String, Vec<PropValues>)>,
}

impl PropIndex {
    pub fn props(&self, cat: &str) -> Option<&Vec<PropValues>> {
        self.cats.iter().find(|c| c.0 == cat).map(|c| &c.1)
    }
    pub fn prop(&self, cat: &str, name: &str) -> Option<&PropValues> {
        self.props(cat)?.iter().find(|p| p.name == name)
    }
    pub fn count(&self) -> usize {
        self.cats.iter().map(|c| c.1.len()).sum()
    }
}

/// Leading number of a property value ("2800 mm" -> 2800, "1,5 m" -> 1.5)
pub fn leading_number(s: &str) -> Option<f64> {
    let t = s.trim().replace(',', ".");
    let end = t.char_indices().take_while(|(i, c)| c.is_ascii_digit() || *c == '.' || ((*c == '-' || *c == '+') && *i == 0) || *c == 'e' || *c == 'E').map(|(i, c)| i + c.len_utf8()).last()?;
    t[..end].parse().ok()
}

pub const STD_FIELDS: [&str; 6] = ["Any", "Name", "IFC Type", "Level", "Model", "GlobalId"];

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum SetKind {
    Explicit(Vec<ElemId>),
    Search(Vec<SearchRule>),
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SelSet {
    pub name: String,
    pub kind: SetKind,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum SelKind {
    All,
    Model,
    Set,
    Type,
    /// elementele selectate curent in model / arbore (ca in Navisworks Clash)
    Selection,
    /// o disciplina (Structure, Architecture, MEP - HVAC...), recunoscuta din tipul IFC si numele modelului
    Discipline,
}

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct Selector {
    pub kind: SelKind,
    pub value: String,
}

impl Selector {
    pub fn label(&self) -> String {
        match self.kind {
            SelKind::All => "All elements".into(),
            SelKind::Selection => "Current selection".into(),
            SelKind::Model => format!("Model: {}", self.value),
            SelKind::Set => format!("Set: {}", self.value),
            SelKind::Type => format!("Type: {}", self.value),
            SelKind::Discipline => format!("Discipline: {}", self.value),
        }
    }
}
