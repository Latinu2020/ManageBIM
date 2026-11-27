//! The properties of Plant 3D objects from the project database (Piping.dcf = SQLite), read-only.
//!
//! The Plant 3D DWG does not contain the engineering properties (Line Number, Size, Spec, Tag, Material...):
//! they are in `Piping.dcf`, next to `Project.xml`, in the project folder (one of the DWG's parents).
//! - `PnPDrawings` (PnPType = 'DWG'): the drawing's PnPID, by `PnPRelativePath` (relative to the project folder)
//!   or `Dwg Name`.
//! - `PnPDataLinks`: (DwgId, DwgHandleHigh << 32 | DwgHandleLow) -> RowId (= the object's PnPID; the link's PnPID is a different one) + RowClassName. `DwgSubIndex` 0 =
//!   the object; > 0 = sub-parts of the same entity (gasket / bolts / weld of a connector, nozzles).
//! - the class tables inherit through `PnPTables.BaseTable` (e.g. Valve -> PipeRunComponent -> EngineeringItems
//!   -> PnPBase); an object's values = the row with the same PnPID in each table of the chain.
//! - `PnPColumnAttributes`: DISPLAYNAME (display name) and ISHIDDEN (internal columns, skipped).
//! - the line: `P3dLineGroupPartRelationship` (Part = PnPID) -> `P3dLineGroup.Tag`.
use rusqlite::types::ValueRef;
use rusqlite::{Connection, OpenFlags};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

/// The data of an object from the Plant 3D database.
#[derive(Clone, Debug, Default)]
pub struct PlantData {
    /// the class of the main row (e.g. "Valve", "Pipe", "P3dConnector")
    pub class: String,
    /// (group, [(display name, value)]): "Plant 3D" for the object, "Plant 3D: <Class>" for sub-parts
    pub groups: Vec<(String, Vec<(String, String)>)>,
    /// The object's Tag (equipment, fittings, instruments), if filled in
    pub tag: Option<String>,
    /// the line number (Line Number), if known
    pub line: Option<String>,
}

/// Looks for the project's Plant 3D database: `Piping.dcf` in the DWG's folder or in a parent (max 8 levels).
pub fn find_project_db(dwg: &Path) -> Option<PathBuf> {
    let mut d = dwg.parent();
    for _ in 0..8 {
        let dir = d?;
        let p = dir.join("Piping.dcf");
        if p.is_file() {
            return Some(p);
        }
        d = dir.parent();
    }
    None
}

/// Opens the database read-only. If it is locked (Plant 3D open), we work on a temporary copy.
/// Also returns the temporary copy (to be deleted after use).
fn open_ro(db: &Path) -> Result<(Connection, Option<PathBuf>), String> {
    let flags = OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX;
    let probe = |c: &Connection| c.query_row("select count(*) from sqlite_master", [], |r| r.get::<_, i64>(0)).map(|_| ());
    if let Ok(c) = Connection::open_with_flags(db, flags) {
        if probe(&c).is_ok() {
            return Ok((c, None));
        }
    }
    let n = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
    let tmp = std::env::temp_dir().join(format!("mb_plantdb_{}_{n}.dcf", std::process::id()));
    std::fs::copy(db, &tmp).map_err(|e| format!("cannot read {}: {e}", db.display()))?;
    let c = Connection::open_with_flags(&tmp, flags).map_err(|e| e.to_string())?;
    probe(&c).map_err(|e| format!("{} is not a Plant 3D project database: {e}", db.display()))?;
    Ok((c, Some(tmp)))
}

fn text(v: ValueRef) -> Option<String> {
    let s = match v {
        ValueRef::Text(t) => String::from_utf8_lossy(t).trim().to_string(),
        ValueRef::Integer(i) => i.to_string(),
        ValueRef::Real(f) if f.is_finite() => {
            let s = format!("{f:.6}");
            s.trim_end_matches('0').trim_end_matches('.').to_string()
        }
        _ => return None,
    };
    (!s.is_empty()).then_some(s)
}

/// The display name of a column: DISPLAYNAME from the database, a few known Plant 3D names, otherwise the column
/// split into words (PartSizeLongDesc -> "Part Size Long Desc").
pub fn display_name(col: &str, display: Option<&String>) -> String {
    if let Some(d) = display.filter(|d| !d.trim().is_empty()) {
        return d.trim().to_string();
    }
    let known = [
        ("LineNumberTag", "Line Number"),
        ("PartSizeLongDesc", "Size"),
        ("PartFamilyLongDesc", "Long Description (Family)"),
        ("ShortDescription", "Description"),
        ("NominalDiameter", "Nominal Diameter"),
        ("MatchingPipeOd", "Matching Pipe OD"),
        ("Shop_Field", "Shop / Field"),
        ("Required Spec", "Required Spec"),
    ];
    if let Some((_, n)) = known.iter().find(|(c, _)| *c == col) {
        return n.to_string();
    }
    let mut out = String::new();
    let ch: Vec<char> = col.replace('_', " ").chars().collect();
    for (i, c) in ch.iter().enumerate() {
        if i > 0 && c.is_uppercase() && (ch[i - 1].is_lowercase() || ch[i - 1].is_ascii_digit() || (ch[i - 1].is_uppercase() && ch.get(i + 1).is_some_and(|n| n.is_lowercase()))) && ch[i - 1] != ' ' {
            out.push(' ');
        }
        out.push(*c);
    }
    out
}

fn skip_column(table: &str, col: &str, hidden: &HashSet<(String, String)>) -> bool {
    col.starts_with("PnP") || col == "AcquisitionProperties" || col == "ContentIsoSymbolDefinition" || col.ends_with("RecordId") || col == "PartFamilyId" || hidden.contains(&(table.to_string(), col.to_string()))
}

/// Reads the properties of all objects in drawing `dwg`, key = hex handle (uppercase).
/// `Ok(None)` if the drawing is not in the project.
pub fn read_for_drawing(db: &Path, dwg: &Path) -> Result<Option<HashMap<String, PlantData>>, String> {
    let (c, tmp) = open_ro(db)?;
    let r = read_conn(&c, db, dwg);
    drop(c);
    if let Some(t) = tmp {
        let _ = std::fs::remove_file(t);
    }
    r
}

fn read_conn(c: &Connection, db: &Path, dwg: &Path) -> Result<Option<HashMap<String, PlantData>>, String> {
    let e = |e: rusqlite::Error| e.to_string();
    // the drawing: by the path relative to the project folder, then by name (if unique)
    let rel = db.parent().and_then(|p| dwg.strip_prefix(p).ok()).map(|r| r.to_string_lossy().replace('/', "\\").to_lowercase());
    let file = dwg.file_name().map(|f| f.to_string_lossy().to_lowercase()).unwrap_or_default();
    let mut st = c.prepare("select PnPID, [Dwg Name], PnPRelativePath from PnPDrawings where upper(PnPType) = 'DWG'").map_err(e)?;
    let drawings: Vec<(i64, String, String)> = st
        .query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, Option<String>>(1)?.unwrap_or_default(), r.get::<_, Option<String>>(2)?.unwrap_or_default())))
        .map_err(e)?
        .flatten()
        .collect();
    let by_rel = rel.as_ref().and_then(|rel| drawings.iter().find(|d| d.2.replace('/', "\\").to_lowercase() == *rel));
    let by_name: Vec<&(i64, String, String)> = drawings.iter().filter(|d| d.1.to_lowercase() == file).collect();
    let Some(dwg_id) = by_rel.or(if by_name.len() == 1 { Some(by_name[0]) } else { None }).map(|d| d.0) else {
        return Ok(None);
    };

    // metadata: table inheritance, hidden columns, display names
    let mut base: HashMap<String, String> = HashMap::new();
    let mut st = c.prepare("select TableName, BaseTable from PnPTables").map_err(e)?;
    for (t, b) in st.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, Option<String>>(1)?.unwrap_or_default()))).map_err(e)?.flatten() {
        base.insert(t, b);
    }
    let mut hidden: HashSet<(String, String)> = HashSet::new();
    let mut display: HashMap<(String, String), String> = HashMap::new();
    if let Ok(mut st) = c.prepare("select TableName, ColumnName, AttributeName, AttributeValue from PnPColumnAttributes where AttributeName in ('ISHIDDEN', 'DISPLAYNAME')") {
        let rows = st.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?, r.get::<_, Option<String>>(3)?.unwrap_or_default())));
        for (t, col, a, v) in rows.map_err(e)?.flatten() {
            if a == "ISHIDDEN" && v.eq_ignore_ascii_case("TRUE") {
                hidden.insert((t, col));
            } else if a == "DISPLAYNAME" {
                display.insert((t, col), v);
            }
        }
    }
    // the line of each object (optional: the tables may be missing in other projects)
    let mut lines: HashMap<i64, String> = HashMap::new();
    if let Ok(mut st) = c.prepare("select r.Part, g.Tag, g.Number from P3dLineGroupPartRelationship r join P3dLineGroup g on g.PnPID = r.LineGroup") {
        if let Ok(rows) = st.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get_ref(1).ok().and_then(text), r.get_ref(2).ok().and_then(text)))) {
            for (part, tag, num) in rows.flatten() {
                if let Some(l) = tag.or(num).filter(|l| l != "?") {
                    lines.insert(part, l);
                }
            }
        }
    }

    // parts without a line (flanges, elbows, welds with "?") get the line of the connected part (P3dPartConnection),
    // breadth-first from the parts with a known line
    if let Ok(mut st) = c.prepare("select Part1, Part2 from P3dPartConnection") {
        let mut adj: HashMap<i64, Vec<i64>> = HashMap::new();
        if let Ok(rows) = st.query_map([], |r| Ok((r.get::<_, Option<i64>>(0)?, r.get::<_, Option<i64>>(1)?))) {
            for (a, b) in rows.flatten() {
                if let (Some(a), Some(b)) = (a, b) {
                    adj.entry(a).or_default().push(b);
                    adj.entry(b).or_default().push(a);
                }
            }
        }
        let mut start: Vec<i64> = lines.keys().copied().collect();
        start.sort_unstable();
        let mut queue: std::collections::VecDeque<i64> = start.into_iter().collect();
        while let Some(p) = queue.pop_front() {
            let l = lines[&p].clone();
            for n in adj.get(&p).map(|v| v.as_slice()).unwrap_or(&[]) {
                if !lines.contains_key(n) {
                    lines.insert(*n, l.clone());
                    queue.push_back(*n);
                }
            }
        }
    }
    // the table chain of a class, from the base to the class (PnPBase excluded)
    let chain = |class: &str| -> Vec<String> {
        let mut v = Vec::new();
        let mut t = class.to_string();
        while !t.is_empty() && t != "PnPBase" && v.len() < 16 && !v.contains(&t) {
            v.push(t.clone());
            t = base.get(&t).cloned().unwrap_or_default();
        }
        v.reverse();
        v
    };
    let mut cols: HashMap<String, Vec<String>> = HashMap::new();

    // new project / drawing without Plant objects: the links table may be missing - not an error
    let Ok(mut st) = c.prepare("select DwgHandleHigh, DwgHandleLow, DwgSubIndex, RowClassName, RowId from PnPDataLinks where DwgId = ?1 order by DwgHandleHigh, DwgHandleLow, DwgSubIndex") else {
        return Ok(Some(HashMap::new()));
    };
    let links: Vec<(i64, i64, i64, String, i64)> = st
        .query_map([dwg_id], |r| Ok((r.get::<_, Option<i64>>(0)?.unwrap_or(0), r.get(1)?, r.get::<_, Option<i64>>(2)?.unwrap_or(0), r.get::<_, Option<String>>(3)?.unwrap_or_default(), r.get(4)?)))
        .map_err(e)?
        .flatten()
        .collect();
    let mut out: HashMap<String, PlantData> = HashMap::new();
    for (hi, lo, sub, class, id) in links {
        let handle = format!("{:X}", ((hi as u64) << 32) | (lo as u64 & 0xFFFF_FFFF));
        let mut vals: Vec<(String, String)> = Vec::new();
        let mut tag = None;
        for t in chain(&class) {
            if !cols.contains_key(&t) {
                let names: Vec<String> = c
                    .prepare(&format!("pragma table_info(\"{}\")", t.replace('"', "")))
                    .and_then(|mut s| s.query_map([], |r| r.get::<_, String>(1)).map(|r| r.flatten().collect()))
                    .unwrap_or_default();
                cols.insert(t.clone(), names);
            }
            let names = &cols[&t];
            if names.is_empty() {
                continue;
            }
            let Ok(mut q) = c.prepare_cached(&format!("select * from \"{}\" where PnPID = ?1", t.replace('"', ""))) else { continue };
            let _ = q.query_row([id], |r| {
                for (i, col) in names.iter().enumerate() {
                    if skip_column(&t, col, &hidden) {
                        continue;
                    }
                    // "?" = unassigned value in Plant 3D (empty Tag / Line Number)
                    let Some(v) = r.get_ref(i).ok().and_then(text).filter(|v| v != "?") else { continue };
                    if col == "Tag" {
                        tag = Some(v.clone());
                    }
                    let n = display_name(col, display.get(&(t.clone(), col.clone())));
                    if !vals.iter().any(|(k, _)| *k == n) {
                        vals.push((n, v));
                    }
                }
                Ok(())
            });
        }
        let d = out.entry(handle).or_default();
        if sub == 0 {
            d.class = class.clone();
            d.tag = tag;
            d.line = vals.iter().find(|(k, _)| k == "Line Number").map(|(_, v)| v.clone()).filter(|v| v != "?").or_else(|| lines.get(&id).cloned());
            if let Some(l) = d.line.clone().filter(|_| !vals.iter().any(|(k, _)| k == "Line Number")) {
                vals.insert(0, ("Line Number".into(), l));
            }
            vals.insert(0, ("Class".into(), class.clone()));
            d.groups.insert(0, ("Plant 3D".into(), vals));
        } else if !vals.is_empty() {
            // the connector has no line of its own: it takes it from the sub-parts (gasket, weld)
            let sub_line = vals.iter().find(|(k, _)| k == "Line Number").map(|(_, v)| v.clone()).or_else(|| lines.get(&id).cloned());
            if d.line.is_none() {
                if let Some(l) = sub_line {
                    d.line = Some(l.clone());
                    if let Some(main) = d.groups.iter_mut().find(|(g, _)| g == "Plant 3D") {
                        main.1.insert(1.min(main.1.len()), ("Line Number".into(), l));
                    }
                }
            }
            let mut g = format!("Plant 3D: {class}");
            let k = d.groups.iter().filter(|(n, _)| *n == g || n.starts_with(&format!("{g} "))).count();
            if k > 0 {
                g = format!("{g} {}", k + 1);
            }
            d.groups.push((g, vals));
        }
    }
    Ok(Some(out))
}

/// Models sent from AutoCAD / Plant 3D through the add-in (`.bimc`, guid = "acad:<handle>", info "Drawing" =
/// the DWG path): fills in the properties from the project database `Piping.dcf` - the same as when opening
/// the DWG directly. Only the missing properties (the add-in may already send some of them), as the group
/// "Plant 3D" etc. Returns (database, filled elements), `Ok(None)` if it is not a Plant 3D model with a database found.
pub fn attach_sent(model: &mut crate::model::RawModel) -> Result<Option<(PathBuf, usize)>, String> {
    let is_acad = model.info.iter().any(|(k, v)| k == "Source application" && v.contains("AutoCAD"));
    let Some(dwg) = model.info.iter().find(|(k, _)| k == "Drawing").map(|(_, v)| PathBuf::from(v)) else { return Ok(None) };
    if !is_acad || !model.elements.iter().any(|e| e.guid.starts_with("acad:")) {
        return Ok(None);
    }
    let Some(db) = find_project_db(&dwg) else { return Ok(None) };
    let Some(map) = read_for_drawing(&db, &dwg)? else { return Ok(None) };
    let mut hit = 0;
    for el in model.elements.iter_mut() {
        let Some(h) = el.guid.strip_prefix("acad:") else { continue };
        let Some(d) = map.get(&h.to_ascii_uppercase()) else { continue };
        let before = el.props.len();
        for (g, vals) in &d.groups {
            for (n, v) in vals {
                // the add-in writes the "Plant 3D" group with the internal names; the database gives the display names - we do not duplicate
                let dup = el.props.iter().any(|p| p.name.eq_ignore_ascii_case(n) && (p.group == *g || p.group.starts_with("Plant 3D")));
                if !dup {
                    el.props.push(crate::model::Prop { group: g.clone(), name: n.clone(), value: v.clone() });
                }
            }
        }
        if let Some(l) = &d.line {
            if !el.props.iter().any(|p| p.name.eq_ignore_ascii_case("Line Number")) {
                el.props.push(crate::model::Prop { group: "Plant 3D".into(), name: "Line Number".into(), value: l.clone() });
            }
        }
        if el.props.len() > before {
            hit += 1;
        }
    }
    Ok(Some((db, hit)))
}

/// Attaches the properties from the project database to the Plant 3D elements of the model (key = guid = handle).
/// The piping (Pipe, fittings, connectors, supports) is grouped by lines (storey = Line Number); the names
/// use the Tag, or the line respectively. Returns (database, elements with properties, Plant 3D elements), or
/// the error; `Ok(None)` if there is no database / the drawing is not in the project.
pub fn attach(model: &mut crate::model::RawModel, dwg: &Path) -> Result<Option<(PathBuf, usize, usize)>, String> {
    let Some(db) = find_project_db(dwg) else { return Ok(None) };
    let Some(map) = read_for_drawing(&db, dwg)? else { return Ok(None) };
    let piping = ["Pipe", "Pipe Inline Asset", "Pipe Connector", "Pipe Support"];
    let (mut hit, mut total) = (0, 0);
    for el in model.elements.iter_mut() {
        if !el.props.iter().any(|p| p.name == "Entity" && p.value.to_ascii_uppercase().starts_with("ACPP")) {
            continue;
        }
        total += 1;
        let Some(d) = map.get(&el.guid) else { continue };
        hit += 1;
        for (g, vals) in &d.groups {
            for (n, v) in vals {
                el.props.push(crate::model::Prop { group: g.clone(), name: n.clone(), value: v.clone() });
            }
        }
        if let Some(t) = &d.tag {
            el.name = format!("{} {t}", el.ifc_type);
        } else if let Some(l) = &d.line {
            el.name = format!("{} {l} / {}", el.ifc_type, el.guid);
        }
        if let Some(l) = d.line.as_ref().filter(|_| piping.contains(&el.ifc_type.as_str())) {
            el.storey = format!("Line {l}");
        }
        if let Some(p) = el.props.iter_mut().find(|p| p.group == "Element" && p.name == "Name") {
            p.value = el.name.clone();
        }
    }
    Ok(Some((db, hit, total)))
}
