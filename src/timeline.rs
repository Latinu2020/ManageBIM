//! TimeLiner 4D: sarcini, date calendaristice, starea elementelor la o zi data.
use crate::model::ElemId;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum TaskKind {
    Construct,
    Demolish,
    Temporary,
}

impl TaskKind {
    pub fn label(&self) -> &'static str {
        match self {
            TaskKind::Construct => "Construct",
            TaskKind::Demolish => "Demolish",
            TaskKind::Temporary => "Temporary",
        }
    }
    pub fn parse(s: &str) -> TaskKind {
        let s = s.trim().to_lowercase();
        if s.starts_with("dem") {
            TaskKind::Demolish
        } else if s.starts_with("temp") {
            TaskKind::Temporary
        } else {
            TaskKind::Construct
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Task {
    pub name: String,
    pub start: i32,
    pub end: i32,
    pub kind: TaskKind,
    pub rule: String,
    pub elems: Vec<ElemId>,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum SimState {
    Normal,
    Hidden,
    Highlight([u8; 4]),
}

/// zile de la 1970-01-01 (algoritmul "days from civil")
pub fn days_from_civil(y: i32, m: u32, d: u32) -> i32 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = (y - era * 400) as u32;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe as i32 - 719468
}

pub fn civil_from_days(z: i32) -> (i32, u32, u32) {
    let z = z + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = (z - era * 146097) as u32;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe as i32 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (if m <= 2 { y + 1 } else { y }, m, d)
}

/// Accepts YYYY-MM-DD, DD.MM.YYYY, DD/MM/YYYY
pub fn parse_date(s: &str) -> Option<i32> {
    let s = s.trim();
    let parts: Vec<&str> = s.split(|c| c == '-' || c == '.' || c == '/').map(|p| p.trim()).collect();
    if parts.len() != 3 {
        return None;
    }
    let n: Vec<i32> = parts.iter().filter_map(|p| p.parse().ok()).collect();
    if n.len() != 3 {
        return None;
    }
    let (y, m, d) = if parts[0].len() == 4 { (n[0], n[1], n[2]) } else { (n[2], n[1], n[0]) };
    if !(1..=12).contains(&m) || !(1..=31).contains(&d) || !(1900..=2200).contains(&y) {
        return None;
    }
    Some(days_from_civil(y, m as u32, d as u32))
}

pub fn fmt_date(d: i32) -> String {
    let (y, m, dd) = civil_from_days(d);
    format!("{y}-{m:02}-{dd:02}")
}

pub fn today() -> i32 {
    let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    (secs / 86400) as i32
}

pub fn range(tasks: &[Task]) -> Option<(i32, i32)> {
    let s = tasks.iter().map(|t| t.start).min()?;
    let e = tasks.iter().map(|t| t.end).max()?;
    Some((s, e.max(s)))
}

pub const COL_CONSTRUCT: [u8; 4] = [60, 200, 90, 200];
pub const COL_DEMOLISH: [u8; 4] = [230, 60, 50, 200];
pub const COL_TEMP: [u8; 4] = [240, 200, 40, 200];

/// Starea fiecarui element atasat unei sarcini, la ziua `day`.
pub fn state_at(tasks: &[Task], day: i32) -> HashMap<ElemId, SimState> {
    let mut order: Vec<&Task> = tasks.iter().collect();
    order.sort_by_key(|t| t.start);
    let mut map: HashMap<ElemId, SimState> = HashMap::new();
    for t in order {
        let st = match t.kind {
            TaskKind::Construct => {
                if day < t.start {
                    SimState::Hidden
                } else if day <= t.end {
                    SimState::Highlight(COL_CONSTRUCT)
                } else {
                    SimState::Normal
                }
            }
            TaskKind::Demolish => {
                if day < t.start {
                    SimState::Normal
                } else if day <= t.end {
                    SimState::Highlight(COL_DEMOLISH)
                } else {
                    SimState::Hidden
                }
            }
            TaskKind::Temporary => {
                if day < t.start || day > t.end {
                    SimState::Hidden
                } else {
                    SimState::Highlight(COL_TEMP)
                }
            }
        };
        for e in &t.elems {
            if day >= t.start || !map.contains_key(e) {
                map.insert(*e, st);
            }
        }
    }
    map
}

/// CSV: nume;start;sfarsit;tip;regula   (separator ; sau ,)
pub fn parse_csv(text: &str) -> (Vec<Task>, Vec<String>) {
    let mut tasks = Vec::new();
    let mut errs = Vec::new();
    for (ln, line) in text.lines().enumerate() {
        let line = line.trim().trim_start_matches('\u{feff}');
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let sep = if line.contains(';') { ';' } else { ',' };
        let f: Vec<&str> = line.split(sep).map(|s| s.trim().trim_matches('"')).collect();
        if f.len() < 3 {
            errs.push(format!("Line {}: too few columns", ln + 1));
            continue;
        }
        let (Some(s), Some(e)) = (parse_date(f[1]), parse_date(f[2])) else {
            if ln > 0 {
                errs.push(format!("Line {}: invalid date", ln + 1));
            }
            continue;
        };
        tasks.push(Task {
            name: f[0].to_string(),
            start: s,
            end: e.max(s),
            kind: TaskKind::parse(f.get(3).copied().unwrap_or("")),
            rule: if f.len() > 4 { f[4..].join(";") } else { String::new() },
            elems: Vec::new(),
        });
    }
    (tasks, errs)
}

// ====================================================================== MS Project / Primavera P6 (XML)

fn local_name(n: &str) -> &str {
    n.rsplit(':').next().unwrap_or(n)
}

fn child_text<'a>(n: &'a crate::e57::Node, name: &str) -> Option<&'a str> {
    n.kids.iter().find(|k| local_name(&k.name) == name).map(|k| k.text.trim()).filter(|t| !t.is_empty())
}

/// Data dintr-un camp XML ("2026-01-05T08:00:00" sau "2026-01-05").
fn xml_date(s: &str) -> Option<i32> {
    parse_date(s.get(..10).unwrap_or(s))
}

/// Regula de legare la elemente scrisa la sfarsitul numelui, intre paranteze drepte:
/// "Turnare placa L1 [Level=Level 1;Type=IfcSlab]" -> ("Turnare placa L1", "Level=Level 1;Type=IfcSlab").
pub fn split_rule(name: &str) -> (String, String) {
    let t = name.trim();
    if t.ends_with(']') {
        if let Some(i) = t.rfind('[') {
            return (t[..i].trim().to_string(), t[i + 1..t.len() - 1].trim().to_string());
        }
    }
    (t.to_string(), String::new())
}

/// Planificare din MS Project (XML, „Save As > XML”) sau Primavera P6 (XML export).
/// Activitatile sumar (MS Project) se sar. Regula de legare: [regula] la sfarsitul numelui,
/// sau campul Text1 (MS Project) / Notes; tipul (Construct / Demolish / Temporary): campul Text2 sau numele.
/// Intoarce sarcinile, avertismentele si formatul recunoscut.
pub fn parse_schedule_xml(text: &str) -> Result<(Vec<Task>, Vec<String>, &'static str), String> {
    let root = crate::e57::parse_xml(text.trim_start_matches('\u{feff}')).map_err(|_| "This is not a valid XML file.".to_string())?;
    let root = if root.name.is_empty() { root.kids.into_iter().find(|k| !k.name.starts_with('?')).ok_or("Empty XML file.")? } else { root };
    let mut tasks = Vec::new();
    let mut warn = Vec::new();
    let mut push = |name: &str, start: Option<&str>, end: Option<&str>, rule: Option<&str>, kind: Option<&str>, warn: &mut Vec<String>| {
        let (s, e) = (start.and_then(xml_date), end.and_then(xml_date));
        let (Some(s), Some(e)) = (s, e) else {
            warn.push(format!("{name}: no start / finish date - skipped"));
            return;
        };
        let (clean, bracket) = split_rule(name);
        let rule = if !bracket.is_empty() { bracket } else { rule.unwrap_or("").trim().to_string() };
        let kind = TaskKind::parse(kind.unwrap_or_else(|| if clean.to_lowercase().contains("demol") { "demolish" } else if clean.to_lowercase().contains("temp") { "temporary" } else { "" }));
        tasks.push(Task { name: clean, start: s, end: e.max(s), kind, rule, elems: Vec::new() });
    };
    match local_name(&root.name) {
        "Project" => {
            // MS Project: Project / Tasks / Task
            let ext = |t: &crate::e57::Node, field: &str| -> Option<String> {
                t.kids.iter().filter(|k| local_name(&k.name) == "ExtendedAttribute").find(|k| child_text(k, "FieldID") == Some(field)).and_then(|k| child_text(k, "Value").map(|s| s.to_string()))
            };
            let Some(list) = root.kids.iter().find(|k| local_name(&k.name) == "Tasks") else { return Err("MS Project XML without <Tasks>.".into()) };
            for t in list.kids.iter().filter(|k| local_name(&k.name) == "Task") {
                let Some(name) = child_text(t, "Name") else { continue };
                if child_text(t, "Summary") == Some("1") || child_text(t, "IsNull") == Some("1") {
                    continue;
                }
                // Text1 = 188743731, Text2 = 188743734
                let rule = ext(t, "188743731").or_else(|| child_text(t, "Notes").map(|s| s.to_string()));
                let kind = ext(t, "188743734");
                push(name, child_text(t, "Start"), child_text(t, "Finish"), rule.as_deref(), kind.as_deref(), &mut warn);
            }
            Ok((tasks, warn, "MS Project"))
        }
        "APIBusinessObjects" => {
            // Primavera P6: APIBusinessObjects / Project / Activity (sau Activity direct)
            let mut acts: Vec<&crate::e57::Node> = Vec::new();
            fn walk<'a>(n: &'a crate::e57::Node, out: &mut Vec<&'a crate::e57::Node>) {
                for k in &n.kids {
                    if local_name(&k.name) == "Activity" {
                        out.push(k);
                    } else if matches!(local_name(&k.name), "Project" | "BaselineProject") {
                        walk(k, out);
                    }
                }
            }
            walk(&root, &mut acts);
            for a in acts {
                let Some(name) = child_text(a, "Name") else { continue };
                let start = child_text(a, "PlannedStartDate").or(child_text(a, "StartDate"));
                let end = child_text(a, "PlannedFinishDate").or(child_text(a, "FinishDate"));
                push(name, start, end, None, None, &mut warn);
            }
            Ok((tasks, warn, "Primavera P6"))
        }
        o => Err(format!("Unknown schedule XML (root <{o}>): use MS Project XML or Primavera P6 XML.")),
    }
}
