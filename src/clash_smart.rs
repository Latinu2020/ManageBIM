//! "Smart" clash rules: recognising the elements (discipline + kind), the normal connections that
//! are not real clashes (wall-slab, beam-column, pipe-support, insulation-pipe...), the assignee
//! (the discipline that moves) and the system for grouping.

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
    /// who has priority (does not move): structure > architecture > ductwork > pipes > electrical
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
    Opening, // doors, windows
    Stair,
    Railing,
    Covering, // finishes, ceilings
    Pipe,
    Duct,
    Cable,
    Fitting,
    Support, // supports, clamps, brackets
    Insulation,
    Terminal, // air terminals, sprinklers, fixtures
    Equipment,
    Other,
}

fn has(t: &str, keys: &[&str]) -> bool {
    keys.iter().any(|k| t.contains(k))
}

/// The kind of the element, from its IFC type / Revit category / name.
pub fn kind_of(e: &Element) -> Kind {
    let t = format!("{} {}", e.ifc_type, e.name).to_lowercase();
    // the order matters: the most specific first
    if has(&t, &["insulation", "izolatie", "lagging"]) {
        return Kind::Insulation;
    }
    if has(&t, &["hanger", "support", "bracket", "clamp", "suport", "bratar", "consol"]) {
        return Kind::Support;
    }
    if has(&t, &["fitting", "elbow", "reducer", "fiting"]) {
        return Kind::Fitting;
    }
    // Order matters: "conduit" contains "duct" and "conducta" contains "conduct", so checking
    // ducts first mislabelled cable trays and Romanian pipes as HVAC ducts.
    if has(&t, &["cable", "conduit", "tray", "cablu", "jgheab"]) {
        return Kind::Cable;
    }
    if has(&t, &["pipe", "teava", "conducta"]) {
        return Kind::Pipe;
    }
    if has(&t, &["duct", "tubulatur"]) {
        return Kind::Duct;
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

/// Discipline: from the kind, then from the model name (ARH / STR / MEP...) if the kind does not tell.
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

/// The pairs that touch normally in construction (they are not real clashes as long as the penetration is small).
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

/// The element's system (for grouping): System Name / System Classification / System Type...
pub fn system_of(e: &Element) -> Option<String> {
    const KEYS: [&str; 6] = ["System Name", "System Classification", "System Type", "System", "SystemName", "Sistem"];
    for k in KEYS {
        if let Some(p) = e.props.iter().find(|p| p.name.eq_ignore_ascii_case(k) && !p.value.trim().is_empty()) {
            return Some(p.value.trim().to_string());
        }
    }
    None
}

/// Who has to resolve it: the discipline with the lower priority (it moves).
pub fn responsible(da: Discipline, db: Discipline) -> Option<Discipline> {
    if da == db || (da == Discipline::Other && db == Discipline::Other) {
        return None;
    }
    Some(if da.priority() <= db.priority() { da } else { db })
}
