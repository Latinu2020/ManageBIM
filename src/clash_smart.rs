//! Reguli de clash "inteligente": recunoasterea elementelor (disciplina + fel), conexiunile normale care
//! nu sunt clash-uri reale (perete-placa, grinda-stalp, teava-suport, izolatie-teava...), responsabilul
//! (disciplina care se muta) si sistemul pentru grupare.

use crate::model::Element;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Discipline {
    Structure,
    Architecture,
    Ducts,
    Pipes,
    Electrical,
    Other,
}

impl Discipline {
    pub fn label(self) -> &'static str {
        match self {
            Discipline::Structure => "Structure",
            Discipline::Architecture => "Architecture",
            Discipline::Ducts => "MEP - HVAC",
            Discipline::Pipes => "MEP - Plumbing",
            Discipline::Electrical => "MEP - Electrical",
            Discipline::Other => "Other",
        }
    }
    /// cine are prioritate (nu se muta): structura > arhitectura > tubulatura > tevi > electric
    pub fn priority(self) -> i32 {
        match self {
            Discipline::Structure => 50,
            Discipline::Architecture => 40,
            Discipline::Ducts => 30,
            Discipline::Pipes => 20,
            Discipline::Electrical => 10,
            Discipline::Other => 0,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    Wall,
    Slab,
    Beam,
    Column,
    Footing,
    Roof,
    Opening, // usi, ferestre
    Stair,
    Railing,
    Covering, // finisaje, plafoane
    Pipe,
    Duct,
    Cable,
    Fitting,
    Support, // suporti, bratari, console
    Insulation,
    Terminal, // guri de aer, sprinklere, corpuri
    Equipment,
    Other,
}

fn has(t: &str, keys: &[&str]) -> bool {
    keys.iter().any(|k| t.contains(k))
}

/// Felul elementului, din tipul IFC / categoria Revit / numele lui.
pub fn kind_of(e: &Element) -> Kind {
    let t = format!("{} {}", e.ifc_type, e.name).to_lowercase();
    // ordinea conteaza: cele mai specifice intai
    if has(&t, &["insulation", "izolatie", "lagging"]) {
        return Kind::Insulation;
    }
    if has(&t, &["hanger", "support", "bracket", "clamp", "suport", "bratar", "consol"]) {
        return Kind::Support;
    }
    if has(&t, &["fitting", "elbow", "reducer", "fiting"]) {
        return Kind::Fitting;
    }
    if has(&t, &["duct", "conduct", "tubulatur"]) {
        return Kind::Duct;
    }
    if has(&t, &["pipe", "teava", "conducta"]) {
        return Kind::Pipe;
    }
    if has(&t, &["cable", "conduit", "tray", "cablu", "jgheab"]) {
        return Kind::Cable;
    }
    if has(&t, &["terminal", "diffuser", "grille", "sprinkler", "lightfixture", "lighting fixture", "air terminal"]) {
        return Kind::Terminal;
    }
    if has(&t, &["equipment", "unit", "boiler", "pump", "fan", "tank", "flowmovingdevice", "energyconversion"]) {
        return Kind::Equipment;
    }
    if has(&t, &["footing", "foundation", "pile", "fundat"]) {
        return Kind::Footing;
    }
    if has(&t, &["column", "stalp"]) {
        return Kind::Column;
    }
    if has(&t, &["beam", "framing", "grinda", "member"]) {
        return Kind::Beam;
    }
    if has(&t, &["roof", "acoperis"]) {
        return Kind::Roof;
    }
    if has(&t, &["slab", "floor", "placa", "planseu"]) {
        return Kind::Slab;
    }
    if has(&t, &["door", "window", "usa", "fereastr", "curtain"]) {
        return Kind::Opening;
    }
    if has(&t, &["stair", "ramp", "scara"]) {
        return Kind::Stair;
    }
    if has(&t, &["railing", "balustrad"]) {
        return Kind::Railing;
    }
    if has(&t, &["covering", "ceiling", "finish", "plafon", "pardoseal"]) {
        return Kind::Covering;
    }
    if has(&t, &["wall", "perete"]) {
        return Kind::Wall;
    }
    Kind::Other
}

/// Disciplina: din fel, apoi din numele modelului (ARH / STR / MEP...) daca felul nu spune.
pub fn discipline_of(e: &Element, model_name: &str) -> Discipline {
    use Kind::*;
    let structural = format!("{} {}", e.ifc_type, e.name).to_lowercase().contains("structural");
    match kind_of(e) {
        Beam | Column | Footing => Discipline::Structure,
        Slab | Wall if structural => Discipline::Structure,
        Slab => Discipline::Structure,
        Wall | Roof | Opening | Stair | Railing | Covering => Discipline::Architecture,
        Duct => Discipline::Ducts,
        Pipe | Insulation => Discipline::Pipes,
        Cable => Discipline::Electrical,
        Fitting | Support | Terminal | Equipment | Other => {
            let m = model_name.to_lowercase();
            if has(&m, &["hvac", "vent", "duct", "climat"]) {
                Discipline::Ducts
            } else if has(&m, &["plumb", "sanit", "pipe", "fire", "sprink", "incendiu"]) {
                Discipline::Pipes
            } else if has(&m, &["elec", "light", "cable"]) {
                Discipline::Electrical
            } else if has(&m, &["mep", "mech", "inst"]) {
                Discipline::Pipes
            } else if has(&m, &["str", "struct", "rezist"]) {
                Discipline::Structure
            } else if has(&m, &["arh", "arch"]) {
                Discipline::Architecture
            } else {
                Discipline::Other
            }
        }
    }
}

/// Perechile care se ating normal in constructie (nu sunt clash-uri reale cat timp patrunderea e mica).
pub fn normal_joint(a: Kind, b: Kind) -> bool {
    use Kind::*;
    let pair = |x: Kind, y: Kind| (a == x && b == y) || (a == y && b == x);
    pair(Wall, Slab)
        || pair(Wall, Wall)
        || pair(Wall, Column)
        || pair(Wall, Beam)
        || pair(Wall, Roof)
        || pair(Beam, Column)
        || pair(Beam, Slab)
        || pair(Beam, Beam)
        || pair(Column, Slab)
        || pair(Column, Footing)
        || pair(Slab, Slab)
        || pair(Slab, Footing)
        || pair(Slab, Roof)
        || pair(Beam, Roof)
        || pair(Opening, Wall)
        || pair(Stair, Slab)
        || pair(Railing, Stair)
        || pair(Railing, Slab)
        || pair(Covering, Wall)
        || pair(Covering, Slab)
        || pair(Pipe, Fitting)
        || pair(Duct, Fitting)
        || pair(Fitting, Fitting)
        || pair(Pipe, Support)
        || pair(Duct, Support)
        || pair(Cable, Support)
        || pair(Insulation, Pipe)
        || pair(Insulation, Duct)
        || pair(Insulation, Fitting)
        || pair(Terminal, Duct)
        || pair(Terminal, Pipe)
        || pair(Equipment, Pipe)
        || pair(Equipment, Duct)
}

/// Sistemul elementului (pentru grupare): System Name / System Classification / System Type...
pub fn system_of(e: &Element) -> Option<String> {
    const KEYS: [&str; 6] = ["System Name", "System Classification", "System Type", "System", "SystemName", "Sistem"];
    for k in KEYS {
        if let Some(p) = e.props.iter().find(|p| p.name.eq_ignore_ascii_case(k) && !p.value.trim().is_empty()) {
            return Some(p.value.trim().to_string());
        }
    }
    None
}

/// Cine trebuie sa rezolve: disciplina cu prioritatea mai mica (ea se muta).
pub fn responsible(da: Discipline, db: Discipline) -> Option<Discipline> {
    if da == db || (da == Discipline::Other && db == Discipline::Other) {
        return None;
    }
    Some(if da.priority() <= db.priority() { da } else { db })
}
