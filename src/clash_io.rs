//! Clash Detective as in Navisworks: the state of the tests (New / Done / Old), import / export of the tests
//! (Navisworks XML "Import / Export Clash Tests" and CSV), the clash matrix by tests (selection A in rows,
//! selection B in columns, as in the "Clash Matrix" add-ins for Navisworks) and the summary after Update All.
use crate::app::*;
use crate::clash::{ClashStatus, ClashType};
use crate::e57::Node;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// The state of a test, as in the Status column in Navisworks.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TestState {
    /// never run
    New,
    /// run, the configuration has not changed since
    Done,
    /// the configuration (selections, type, tolerance, rules) changed after the last run
    Old,
}

impl TestState {
    pub fn label(self) -> &'static str {
        match self {
            TestState::New => "New",
            TestState::Done => "Done",
            TestState::Old => "Old",
        }
    }
}

/// The fingerprint of a test's configuration (without results): it changes when the test must be run again.
pub fn run_key(t: &ClashTest) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    serde_json::to_string(&(&t.a, &t.b, t.ty, t.tol_mm.to_bits(), t.ignore_same_file, &t.rules)).unwrap_or_default().hash(&mut h);
    h.finish() | 1
}

pub fn test_state(t: &ClashTest) -> TestState {
    if t.last_run.is_empty() {
        TestState::New
    } else if t.run_key != 0 && t.run_key != run_key(t) {
        TestState::Old
    } else {
        TestState::Done
    }
}

/// The number of clashes by status: [New, Active, Reviewed, Approved, Resolved].
pub fn status_counts(t: &ClashTest) -> [usize; 5] {
    let mut c = [0; 5];
    for r in &t.results {
        c[ClashStatus::ALL.iter().position(|s| *s == r.status).unwrap_or(0)] += 1;
    }
    c
}

/// The configuration of a test (what is exported / imported; without results).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TestDef {
    pub name: String,
    pub a: Selector,
    pub b: Selector,
    pub ty: ClashType,
    pub tol_mm: f32,
    #[serde(default)]
    pub ignore_same_file: bool,
    #[serde(default)]
    pub rules: ClashRules,
    #[serde(default = "sev")]
    pub sev_mm: [f32; 2],
}

fn sev() -> [f32; 2] {
    [10.0, 50.0]
}

impl TestDef {
    pub fn of(t: &ClashTest) -> TestDef {
        TestDef { name: t.name.clone(), a: t.a.clone(), b: t.b.clone(), ty: t.ty, tol_mm: t.tol_mm, ignore_same_file: t.ignore_same_file, rules: t.rules.clone(), sev_mm: t.sev_mm }
    }
    fn apply(self, t: &mut ClashTest) {
        t.name = self.name;
        t.a = self.a;
        t.b = self.b;
        t.ty = self.ty;
        t.tol_mm = self.tol_mm;
        t.ignore_same_file = self.ignore_same_file;
        t.rules = self.rules;
        t.sev_mm = self.sev_mm;
    }
}

// ------------------------------------------------------------------ selections <-> text

/// "All elements", "Set: X", "Model: X", "Type: X", "Discipline: X" -> selector (the inverse of Selector::label).
pub fn selector_from_label(s: &str) -> Selector {
    let s = s.trim();
    let mk = |kind, v: &str| Selector { kind, value: v.trim().to_string() };
    for (p, k) in [("Set:", SelKind::Set), ("Model:", SelKind::Model), ("Type:", SelKind::Type), ("Discipline:", SelKind::Discipline)] {
        if let Some(v) = s.strip_prefix(p) {
            return mk(k, v);
        }
    }
    if s.is_empty() || s.eq_ignore_ascii_case("all elements") || s.eq_ignore_ascii_case("all") || s == "/" {
        return mk(SelKind::All, "");
    }
    // a plain name = selection set (as in Navisworks)
    mk(SelKind::Set, s)
}

fn type_label(ty: ClashType) -> &'static str {
    match ty {
        ClashType::Hard => "hard",
        ClashType::Clearance => "clearance",
        ClashType::Duplicates => "duplicate",
    }
}

fn type_from(s: &str) -> ClashType {
    let s = s.trim().to_lowercase();
    if s.starts_with("clear") {
        ClashType::Clearance
    } else if s.starts_with("dup") {
        ClashType::Duplicates
    } else {
        ClashType::Hard
    }
}

fn b64e(b: &[u8]) -> String {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD.encode(b)
}

fn b64d(s: &str) -> Option<Vec<u8>> {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD.decode(s.trim()).ok()
}

fn yes(s: &str) -> bool {
    matches!(s.trim().to_lowercase().as_str(), "1" | "yes" | "true" | "da" | "y" | "x")
}

fn num(s: &str) -> Option<f32> {
    s.trim().replace(',', ".").parse::<f32>().ok().filter(|v| v.is_finite())
}

fn all_named<'a>(n: &'a Node, name: &str, out: &mut Vec<&'a Node>) {
    for k in &n.kids {
        if k.name == name {
            out.push(k);
        } else {
            all_named(k, name, out);
        }
    }
}

fn kid<'a>(n: &'a Node, name: &str) -> Option<&'a Node> {
    n.kids.iter().find(|k| k.name == name)
}

/// A Navisworks findspec condition -> a ManageBIM search rule.
fn rule_of(c: &Node) -> Option<SearchRule> {
    let cat = kid(c, "category").and_then(|x| kid(x, "name")).map(|x| x.text.trim().to_string()).unwrap_or_default();
    let prop = kid(c, "property").and_then(|x| kid(x, "name")).map(|x| x.text.trim().to_string()).unwrap_or_default();
    let value = kid(c, "value").map(|v| kid(v, "data").map(|d| d.text.trim().to_string()).unwrap_or_else(|| v.text.trim().to_string())).unwrap_or_default();
    let op = match c.attr("test").unwrap_or("equals") {
        "equals" | "wildcard" => SearchOp::Equals,
        "not_equals" => SearchOp::NotEquals,
        "contains" => SearchOp::Contains,
        "greater_than" | "greater_than_or_equal" => SearchOp::Greater,
        "less_than" | "less_than_or_equal" => SearchOp::Less,
        "prop" => SearchOp::Defined,
        "no_prop" => SearchOp::Undefined,
        _ => SearchOp::Equals,
    };
    let or = c.attr("flags").and_then(|f| f.parse::<u32>().ok()).is_some_and(|f| f & 64 != 0);
    if prop.is_empty() {
        return None;
    }
    let (g, f) = match (cat.as_str(), prop.as_str()) {
        ("Item", "Name") => (ITEM_CAT, "Name"),
        ("Item", "Type") => (ITEM_CAT, "IFC Type"),
        ("Item", "GUID") => (ITEM_CAT, "GlobalId"),
        ("Item", "Layer") => (ITEM_CAT, "Level"),
        ("Item", "Source File") => (ITEM_CAT, "Model"),
        _ => (cat.as_str(), prop.as_str()),
    };
    Some(SearchRule::new(g, f, op, &value, or))
}

/// The result of an import: the tests, the new sets and the warnings.
pub struct Imported {
    pub tests: Vec<TestDef>,
    pub sets: Vec<SelSet>,
    pub warnings: Vec<String>,
}

/// Reads a Navisworks tests XML (Clash Detective > Import / Export > Export Clash Tests, or an XML
/// report) - and the one written by ManageBIM, which keeps the selections and rules exactly.
pub fn parse_tests_xml(text: &str) -> Result<Imported, String> {
    let text = text.trim_start_matches('\u{feff}');
    let root = crate::e57::parse_xml(text).map_err(|_| "This is not a valid XML file.".to_string())?;
    let mut warnings = Vec::new();
    // the sets from the file (and from folders)
    let mut nodes = Vec::new();
    all_named(&root, "selectionset", &mut nodes);
    let mut file_sets: Vec<SelSet> = Vec::new();
    for s in nodes {
        let name = s.attr("name").unwrap_or("").trim().to_string();
        if name.is_empty() {
            continue;
        }
        let mut conds = Vec::new();
        all_named(s, "condition", &mut conds);
        let rules: Vec<SearchRule> = conds.iter().filter_map(|c| rule_of(c)).collect();
        if rules.is_empty() {
            warnings.push(format!("Set \"{name}\": explicit Navisworks selection (item paths) - recreate it in ManageBIM or use a search set"));
        }
        file_sets.push(SelSet { name, kind: SetKind::Search(rules) });
    }
    // the exact definitions written by ManageBIM
    let extra: HashMap<String, TestDef> = text
        .find("<!--managebim-tests:")
        .and_then(|i| {
            let rest = &text[i + 20..];
            let end = rest.find("-->")?;
            b64d(&rest[..end])
        })
        .and_then(|b| serde_json::from_slice::<Vec<TestDef>>(&b).ok())
        .map(|v| v.into_iter().map(|d| (d.name.clone(), d)).collect())
        .unwrap_or_default();
    let mut tests_n = Vec::new();
    all_named(&root, "clashtest", &mut tests_n);
    if tests_n.is_empty() {
        return Err("No clash tests in this XML file (Navisworks: Clash Detective > Import / Export > Export Clash Tests).".into());
    }
    let mut used_sets: Vec<String> = Vec::new();
    let mut tests = Vec::new();
    for t in tests_n {
        let name = t.attr("name").unwrap_or("Test").trim().to_string();
        if let Some(d) = extra.get(&name) {
            for s in [&d.a, &d.b] {
                if s.kind == SelKind::Set {
                    used_sets.push(s.value.clone());
                }
            }
            tests.push(d.clone());
            continue;
        }
        let side = |tag: &str| -> Selector {
            let loc = kid(t, tag).and_then(|l| {
                let mut v = Vec::new();
                all_named(l, "locator", &mut v);
                v.first().map(|x| x.text.trim().to_string())
            });
            match loc {
                None => Selector { kind: SelKind::All, value: String::new() },
                Some(l) => {
                    if let Some(set) = l.strip_prefix("lcop_selection_set_tree/") {
                        // "Folder/Set" -> the set name; the ManageBIM labels ("Model: X") are recognised
                        let n = set.rsplit('/').next().unwrap_or(set).trim();
                        let s = selector_from_label(n);
                        if s.kind != SelKind::Set {
                            return s;
                        }
                        Selector { kind: SelKind::Set, value: n.to_string() }
                    } else if l.is_empty() || l == "/" {
                        Selector { kind: SelKind::All, value: String::new() }
                    } else if let Some(p) = l.strip_prefix('/') {
                        Selector { kind: SelKind::Model, value: p.split('/').next().unwrap_or(p).to_string() }
                    } else {
                        Selector { kind: SelKind::All, value: String::new() }
                    }
                }
            }
        };
        let (a, b) = (side("left"), side("right"));
        for s in [&a, &b] {
            if s.kind == SelKind::Set {
                used_sets.push(s.value.clone());
            }
        }
        let ty = type_from(t.attr("test_type").unwrap_or("hard"));
        // the tolerance in the file units (metres by default)
        let units = root.attr("units").or_else(|| {
            let mut v = Vec::new();
            all_named(&root, "batchtest", &mut v);
            v.first().and_then(|b| b.attr("units"))
        });
        let k = match units.unwrap_or("m") {
            "mm" => 1.0,
            "cm" => 10.0,
            "ft" => 304.8,
            "in" => 25.4,
            _ => 1000.0,
        };
        let tol_mm = t.attr("tolerance").and_then(num).map(|v| v * k).unwrap_or(0.0);
        let mut rules = ClashRules::default();
        rules.auto_assign = false;
        rules.normal_joints = false;
        // the Navisworks rules: "in the same file" is the only one with a direct equivalent
        let mut rn = Vec::new();
        all_named(t, "rule", &mut rn);
        let mut same_file = false;
        for r in rn {
            let n = format!("{} {}", r.attr("name").unwrap_or(""), r.attr("type").unwrap_or("")).to_lowercase();
            let on = r.attr("enabled").is_none_or(yes);
            if on && n.contains("file") {
                same_file = true;
            } else if on && n.contains("layer") {
                rules.same_level = true;
            }
        }
        tests.push(TestDef { name, a, b, ty, tol_mm: (tol_mm * 1000.0).round() / 1000.0, ignore_same_file: same_file, rules, sev_mm: sev() });
    }
    // only the sets used by tests (or all of them, if nothing was found)
    let sets: Vec<SelSet> = file_sets.into_iter().filter(|s| used_sets.contains(&s.name)).collect();
    Ok(Imported { tests, sets, warnings })
}

/// CSV / Excel: one row per test. The columns are recognised by name (any order).
pub fn parse_tests_rows(rows: &[Vec<String>]) -> Result<Imported, String> {
    let h = rows.iter().position(|r| r.iter().any(|c| c.trim().eq_ignore_ascii_case("name") || c.trim().eq_ignore_ascii_case("test name") || c.trim().eq_ignore_ascii_case("test"))).ok_or("No header row with a \"Name\" column.")?;
    let cols: Vec<String> = rows[h].iter().map(|c| c.trim().trim_start_matches('\u{feff}').to_lowercase()).collect();
    let col = |names: &[&str]| cols.iter().position(|c| names.contains(&c.as_str()));
    let ci = |names: &[&str]| col(names);
    let (c_name, c_type, c_tol) = (ci(&["name", "test name", "test"]), ci(&["type", "test type"]), ci(&["tolerance (mm)", "tolerance_mm", "tolerance", "toleranta (mm)"]));
    let (c_a, c_b) = (ci(&["selection a", "selection_a", "a", "left"]), ci(&["selection b", "selection_b", "b", "right"]));
    let c_file = ci(&["ignore same file", "same file"]);
    let c_level = ci(&["ignore same level", "same level"]);
    let c_typeq = ci(&["ignore same type", "same type"]);
    let c_joint = ci(&["ignore normal joints", "normal joints"]);
    let c_jmm = ci(&["joint (mm)", "joint_mm"]);
    let c_s1 = ci(&["major from (mm)", "major (mm)"]);
    let c_s2 = ci(&["critical from (mm)", "critical (mm)"]);
    let c_assign = ci(&["auto assign", "assign"]);
    let get = |r: &Vec<String>, c: Option<usize>| c.and_then(|i| r.get(i)).map(|s| s.trim().to_string()).unwrap_or_default();
    let mut tests = Vec::new();
    for r in &rows[h + 1..] {
        let name = get(r, c_name);
        if name.is_empty() {
            continue;
        }
        let mut rules = ClashRules::default();
        rules.same_level = yes(&get(r, c_level));
        rules.same_type = yes(&get(r, c_typeq));
        if c_joint.is_some() {
            rules.normal_joints = yes(&get(r, c_joint));
        }
        if let Some(v) = num(&get(r, c_jmm)) {
            rules.joint_mm = v;
        }
        if c_assign.is_some() {
            rules.auto_assign = yes(&get(r, c_assign));
        }
        let mut sv = sev();
        if let Some(v) = num(&get(r, c_s1)) {
            sv[0] = v;
        }
        if let Some(v) = num(&get(r, c_s2)) {
            sv[1] = v.max(sv[0]);
        }
        tests.push(TestDef {
            name,
            a: selector_from_label(&get(r, c_a)),
            b: selector_from_label(&get(r, c_b)),
            ty: type_from(&get(r, c_type)),
            tol_mm: num(&get(r, c_tol)).unwrap_or(10.0),
            ignore_same_file: yes(&get(r, c_file)),
            rules,
            sev_mm: sv,
        });
    }
    if tests.is_empty() {
        return Err("No tests found (one row per test, with a Name column).".into());
    }
    Ok(Imported { tests, sets: Vec::new(), warnings: Vec::new() })
}

pub const CSV_HEADERS: [&str; 22] = [
    "Name", "Status", "Type", "Tolerance (mm)", "Selection A", "Selection B", "Last Run", "Clashes", "New", "Active", "Reviewed", "Approved", "Resolved",
    "Ignore same file", "Ignore same level", "Ignore same type", "Ignore normal joints", "Joint (mm)", "Auto assign", "Major from (mm)", "Critical from (mm)", "Run",
];

// ------------------------------------------------------------------ the matrix by tests

/// The test matrix: selection A in rows, selection B in columns; each cell = the tests of that pair.
pub struct TestMatrix {
    pub rows: Vec<String>,
    pub cols: Vec<String>,
    /// (row, column) -> the test indices
    pub cells: HashMap<(usize, usize), Vec<usize>>,
}

impl TestMatrix {
    /// [New, Active, Reviewed, Approved, Resolved] for a cell (the sum of its tests).
    pub fn counts(&self, tests: &[ClashTest], r: usize, c: usize) -> Option<[usize; 5]> {
        let ix = self.cells.get(&(r, c))?;
        let mut s = [0; 5];
        for &i in ix {
            if let Some(t) = tests.get(i) {
                let k = status_counts(t);
                for j in 0..5 {
                    s[j] += k[j];
                }
            }
        }
        Some(s)
    }
}

pub fn test_matrix(tests: &[ClashTest]) -> TestMatrix {
    let mut rows: Vec<String> = Vec::new();
    let mut cols: Vec<String> = Vec::new();
    let mut cells: HashMap<(usize, usize), Vec<usize>> = HashMap::new();
    for (i, t) in tests.iter().enumerate() {
        let (a, b) = (t.a.label(), t.b.label());
        let r = rows.iter().position(|x| *x == a).unwrap_or_else(|| {
            rows.push(a.clone());
            rows.len() - 1
        });
        let c = cols.iter().position(|x| *x == b).unwrap_or_else(|| {
            cols.push(b.clone());
            cols.len() - 1
        });
        cells.entry((r, c)).or_default().push(i);
    }
    TestMatrix { rows, cols, cells }
}

/// The summary after Update All: per test, (name, before [5], after [5]).
pub type UpdateSummary = Vec<(String, [usize; 5], [usize; 5])>;

impl App {
    // ------------------------------------------------------------------ the Navisworks buttons

    /// Reset All: deletes the results of all tests (the tests remain, with the New status).
    pub fn clash_reset_all(&mut self) {
        for t in &mut self.clash.tests {
            t.results.clear();
            t.last_run.clear();
            t.run_key = 0;
        }
        self.after_clash_edit();
        self.toast("All clash results removed - the tests are back to New.");
    }

    /// Compact All: deletes the Resolved clashes from all tests.
    pub fn clash_compact_all(&mut self) {
        let mut n = 0;
        for t in &mut self.clash.tests {
            let before = t.results.len();
            t.results.retain(|r| r.status != ClashStatus::Resolved);
            n += before - t.results.len();
        }
        self.after_clash_edit();
        self.toast(format!("Compact All: {n} resolved clashes removed."));
    }

    /// Delete All: deletes all tests (an empty test remains).
    pub fn clash_delete_all(&mut self) {
        self.clash.tests.clear();
        self.clash.tests.push(ClashTest::new("Test 1".into()));
        self.after_clash_edit();
        self.toast("All clash tests deleted.");
    }

    fn after_clash_edit(&mut self) {
        self.cur_test = 0;
        self.cur_result = None;
        self.sel_results.clear();
        self.clash.focus = None;
        self.colors_dirty = true;
    }

    /// Update All: runs all tests one after another; at the end the summary appears (what is new, what is active, what was resolved).
    pub fn clash_update_all(&mut self) {
        if self.clash.job.is_some() || self.clash.tests.is_empty() {
            return;
        }
        self.update_all_before = Some(self.clash.tests.iter().map(|t| (t.name.clone(), status_counts(t))).collect());
        self.update_summary = None;
        self.update_all_cur = Some(self.cur_test);
        self.clash.queue = (1..self.clash.tests.len()).collect();
        self.cur_test = 0;
        self.run_clash(0);
        if self.clash.job.is_none() {
            // the first test could not start (empty selection): we move on to the next one
            self.clash_queue_next();
        }
    }

    /// the next test in the queue; at the end, the Update All summary
    pub fn clash_queue_next(&mut self) {
        while !self.clash.queue.is_empty() && self.clash.job.is_none() {
            let n = self.clash.queue.remove(0);
            self.cur_test = n;
            self.run_clash(n);
        }
        if self.clash.job.is_none() && self.clash.queue.is_empty() {
            if let Some(before) = self.update_all_before.take() {
                let sum: UpdateSummary = self
                    .clash.tests
                    .iter()
                    .map(|t| {
                        let b = before.iter().find(|x| x.0 == t.name).map(|x| x.1).unwrap_or([0; 5]);
                        (t.name.clone(), b, status_counts(t))
                    })
                    .collect();
                self.update_summary = Some(sum);
                // back to the test you had open
                if let Some(c) = self.update_all_cur.take() {
                    self.cur_test = c.min(self.clash.tests.len().saturating_sub(1));
                    self.cur_result = None;
                    self.sel_results.clear();
                }
            }
        }
    }

    // ------------------------------------------------------------------ import

    /// Import Clash Tests: Navisworks / ManageBIM XML, CSV or Excel. Tests with the same name are updated
    /// (the results remain), the others are added.
    pub fn import_clash_tests(&mut self, path: &str) {
        let lower = path.to_lowercase();
        let parsed = if lower.ends_with(".xml") {
            std::fs::read(path).map_err(|e| e.to_string()).and_then(|b| parse_tests_xml(&String::from_utf8_lossy(&b)))
        } else {
            crate::tables::read_rows(path).and_then(|r| parse_tests_rows(&r))
        };
        let imp = match parsed {
            Ok(i) => i,
            Err(e) => return self.error(format!("{}: {e}", crate::file_name(path))),
        };
        let mut n_sets = 0;
        for s in imp.sets {
            if !self.sets.iter().any(|x| x.name == s.name) {
                self.sets.push(s);
                n_sets += 1;
            }
        }
        // the default empty test ("Test 1" with no results) is replaced
        if self.clash.tests.len() == 1 && self.clash.tests[0].results.is_empty() && self.clash.tests[0].last_run.is_empty() && !imp.tests.iter().any(|t| t.name == self.clash.tests[0].name) {
            self.clash.tests.clear();
        }
        let (mut added, mut updated) = (0, 0);
        let mut missing: Vec<String> = Vec::new();
        for d in imp.tests {
            for s in [&d.a, &d.b] {
                if s.kind == SelKind::Set && !self.sets.iter().any(|x| x.name == s.value) && !missing.contains(&s.value) {
                    missing.push(s.value.clone());
                }
            }
            if let Some(t) = self.clash.tests.iter_mut().find(|t| t.name == d.name) {
                d.apply(t);
                updated += 1;
            } else {
                let mut t = ClashTest::new(d.name.clone());
                d.apply(&mut t);
                self.clash.tests.push(t);
                added += 1;
            }
        }
        self.after_clash_edit();
        let sets = if n_sets > 0 { format!(", {n_sets} search sets") } else { String::new() };
        self.toast(format!("Clash tests imported from {}: {added} new, {updated} updated{sets}. Press Update All to run them.", crate::file_name(path)));
        if !missing.is_empty() {
            self.toast(format!("Selection sets not found in this project: {} - create them (Sets) before running.", missing.join(", ")));
        }
        for w in imp.warnings.into_iter().take(4) {
            self.log.push(w);
        }
    }

    // ------------------------------------------------------------------ export

    /// The sets required by the test selections that do not exist as sets: Model / Type / Discipline -> search sets.
    fn selector_sets(&self) -> Vec<SelSet> {
        let mut out: Vec<SelSet> = Vec::new();
        for t in &self.clash.tests {
            for s in [&t.a, &t.b] {
                let name = s.label();
                if out.iter().any(|x| x.name == name) {
                    continue;
                }
                let rules = match s.kind {
                    SelKind::Model => vec![SearchRule::new(ITEM_CAT, "Model", SearchOp::Contains, &model_label(&s.value), false)],
                    SelKind::Type => vec![SearchRule::new(ITEM_CAT, "IFC Type", SearchOp::Equals, &s.value, false)],
                    SelKind::Discipline => {
                        // the types from the loaded models that belong to the discipline (OR between them)
                        let mut types: Vec<String> = Vec::new();
                        for (_, m) in self.loaded_models() {
                            for e in &m.elements {
                                if crate::clash_smart::discipline_of(e, &m.name).label() == s.value && !e.ifc_type.is_empty() && !types.contains(&e.ifc_type) {
                                    types.push(e.ifc_type.clone());
                                }
                            }
                        }
                        types.sort();
                        types.iter().enumerate().map(|(i, ty)| SearchRule::new(ITEM_CAT, "IFC Type", SearchOp::Equals, ty, i > 0)).collect()
                    }
                    _ => continue,
                };
                out.push(SelSet { name, kind: SetKind::Search(rules) });
            }
        }
        out
    }

    /// The tests XML for Navisworks (Clash Detective > Import / Export > Import Clash Tests) +
    /// the sets used + the exact ManageBIM definitions (a comment, ignored by Navisworks).
    pub fn clash_tests_xml(&self) -> (String, Vec<String>) {
        use crate::markup::esc;
        let extra = self.selector_sets();
        let locator = |s: &Selector| -> String {
            match s.kind {
                SelKind::Set => format!("lcop_selection_set_tree/{}", s.value),
                SelKind::All => "/".to_string(),
                _ => format!("lcop_selection_set_tree/{}", s.label()),
            }
        };
        let mut tests = String::new();
        for t in &self.clash.tests {
            let side = |tag: &str, s: &Selector| format!("<{tag}><clashselection selfintersect=\"0\" primtypes=\"1\"><locator>{}</locator></clashselection></{tag}>", esc(&locator(s)));
            // the ManageBIM rules have no reliable equivalent in the Navisworks schema: they stay in the managebim-tests comment
            let rules = "<rules/>";
            tests.push_str(&format!(
                "    <clashtest name=\"{}\" test_type=\"{}\" status=\"new\" tolerance=\"{:.6}\" merge_composites=\"0\">\n      <linkage mode=\"none\"/>\n      {}\n      {}\n      {rules}\n    </clashtest>\n",
                esc(&t.name),
                type_label(t.ty),
                t.tol_mm as f64 / 1000.0,
                side("left", &t.a),
                side("right", &t.b)
            ));
        }
        // the project sets + the ones generated from selections
        let (sets, warn) = self.navis_selectionsets_xml_with(&extra);
        let defs: Vec<TestDef> = self.clash.tests.iter().map(TestDef::of).collect();
        let json = serde_json::to_vec(&defs).unwrap_or_default();
        let x = format!(
            "{}<batchtest name=\"ManageBIM\" internal_name=\"ManageBIM\" units=\"m\">\n  <clashtests>\n{tests}  </clashtests>\n{sets}<!--managebim-tests:{}-->\n</batchtest>\n</exchange>\n",
            NW_HEAD,
            b64e(&json)
        );
        (x, warn)
    }

    pub fn export_clash_tests_xml(&mut self, path: &str) {
        if self.clash.tests.is_empty() {
            return self.error("No clash tests to export.");
        }
        let (x, warn) = self.clash_tests_xml();
        let path = if path.to_lowercase().ends_with(".xml") { path.to_string() } else { format!("{path}.xml") };
        match std::fs::write(&path, x) {
            Ok(()) => {
                self.toast(format!("{} clash tests exported: {} - in Navisworks: Clash Detective > Import / Export > Import Clash Tests.", self.clash.tests.len(), crate::file_name(&path)));
                for w in warn.into_iter().take(3) {
                    self.log.push(w);
                }
            }
            Err(e) => self.error(format!("Cannot write {path}: {e}")),
        }
    }

    pub fn clash_tests_csv(&self) -> String {
        let yn = |b: bool| if b { "Yes" } else { "No" }.to_string();
        let rows: Vec<Vec<String>> = self
            .clash.tests
            .iter()
            .map(|t| {
                let c = status_counts(t);
                vec![
                    t.name.clone(),
                    test_state(t).label().into(),
                    type_label(t.ty).into(),
                    format!("{}", t.tol_mm),
                    t.a.label(),
                    t.b.label(),
                    t.last_run.clone(),
                    t.results.len().to_string(),
                    c[0].to_string(),
                    c[1].to_string(),
                    c[2].to_string(),
                    c[3].to_string(),
                    c[4].to_string(),
                    yn(t.ignore_same_file),
                    yn(t.rules.same_level),
                    yn(t.rules.same_type),
                    yn(t.rules.normal_joints),
                    format!("{}", t.rules.joint_mm),
                    yn(t.rules.auto_assign),
                    format!("{}", t.sev_mm[0]),
                    format!("{}", t.sev_mm[1]),
                    "Yes".into(),
                ]
            })
            .collect();
        crate::tables::to_csv(&CSV_HEADERS, &rows)
    }

    pub fn export_clash_tests_csv(&mut self, path: &str) {
        let path = if path.to_lowercase().ends_with(".csv") { path.to_string() } else { format!("{path}.csv") };
        match std::fs::write(&path, self.clash_tests_csv()) {
            Ok(()) => self.toast(format!("{} clash tests exported: {} (edit in Excel, then Import Clash Tests).", self.clash.tests.len(), crate::file_name(&path))),
            Err(e) => self.error(format!("Cannot write {path}: {e}")),
        }
    }

    // ------------------------------------------------------------------ the matrix in Excel

    /// The matrix by tests in Excel: the "Matrix" sheet (open / total for the A x B pair), one sheet per status
    /// and the "Tests" sheet (each test with all statuses).
    pub fn export_test_matrix(&mut self, path: &str) {
        use crate::qto::{Cell, Sheet};
        let m = test_matrix(&self.clash.tests);
        if m.cells.is_empty() {
            return self.error("No clash tests yet.");
        }
        let tests = &self.clash.tests;
        let mut headers = vec!["Selection A \\ Selection B".to_string()];
        headers.extend(m.cols.iter().cloned());
        headers.push("Total".into());
        let widths: Vec<f64> = std::iter::once(34.0).chain(std::iter::repeat(18.0)).take(headers.len()).collect();
        let sheet = |name: &str, title: &str, f: &dyn Fn([usize; 5]) -> usize| {
            let rows: Vec<Vec<Cell>> = (0..m.rows.len())
                .map(|r| {
                    let mut row = vec![Cell::S(m.rows[r].clone())];
                    let mut tot = 0;
                    for c in 0..m.cols.len() {
                        match m.counts(tests, r, c) {
                            Some(k) => {
                                let v = f(k);
                                tot += v;
                                row.push(Cell::N(v as f64));
                            }
                            None => row.push(Cell::Empty),
                        }
                    }
                    row.push(Cell::N(tot as f64));
                    row
                })
                .collect();
            Sheet { name: name.into(), title: title.into(), headers: headers.clone(), widths: widths.clone(), rows, total_from: Some(1) }
        };
        // the main sheet: the cell text = the tests and their statuses
        let main_rows: Vec<Vec<Cell>> = (0..m.rows.len())
            .map(|r| {
                let mut row = vec![Cell::S(m.rows[r].clone())];
                let mut tot = 0;
                for c in 0..m.cols.len() {
                    match m.cells.get(&(r, c)) {
                        Some(ix) => {
                            let txt: Vec<String> = ix
                                .iter()
                                .map(|&i| {
                                    let t = &tests[i];
                                    let k = status_counts(t);
                                    tot += k[0] + k[1];
                                    format!("{}: {} open / {} (N {} A {} Rv {} Ap {} Rs {})", t.name, k[0] + k[1], t.results.len(), k[0], k[1], k[2], k[3], k[4])
                                })
                                .collect();
                            row.push(Cell::S(txt.join("\n")));
                        }
                        None => row.push(Cell::Empty),
                    }
                }
                row.push(Cell::N(tot as f64));
                row
            })
            .collect();
        let main = Sheet { name: "Matrix".into(), title: "Clash matrix by test - open (New + Active) / total per test".into(), headers: headers.clone(), widths: std::iter::once(34.0).chain(std::iter::repeat(40.0)).take(headers.len()).collect(), rows: main_rows, total_from: None };
        let th: Vec<String> = ["Test", "Status", "Type", "Tolerance (mm)", "Selection A", "Selection B", "Last Run", "Clashes", "New", "Active", "Reviewed", "Approved", "Resolved"].iter().map(|s| s.to_string()).collect();
        let trows: Vec<Vec<Cell>> = tests
            .iter()
            .map(|t| {
                let k = status_counts(t);
                let mut r = vec![Cell::S(t.name.clone()), Cell::S(test_state(t).label().into()), Cell::S(t.ty.label().into()), Cell::N(t.tol_mm as f64), Cell::S(t.a.label()), Cell::S(t.b.label()), Cell::S(t.last_run.clone()), Cell::N(t.results.len() as f64)];
                r.extend(k.iter().map(|v| Cell::N(*v as f64)));
                r
            })
            .collect();
        let tsheet = Sheet { name: "Tests".into(), title: "Clash tests".into(), headers: th, widths: vec![28.0, 9.0, 11.0, 13.0, 28.0, 28.0, 18.0, 9.0, 8.0, 8.0, 9.0, 9.0, 9.0], rows: trows, total_from: Some(7) };
        let data = crate::qto::xlsx(&[
            main,
            sheet("Open", "Open clashes (New + Active)", &|k| k[0] + k[1]),
            sheet("New", "New clashes", &|k| k[0]),
            sheet("Active", "Active clashes", &|k| k[1]),
            sheet("Reviewed", "Reviewed clashes", &|k| k[2]),
            sheet("Approved", "Approved clashes", &|k| k[3]),
            sheet("Resolved", "Resolved clashes", &|k| k[4]),
            sheet("Total", "All clashes", &|k| k.iter().sum()),
            tsheet,
        ]);
        let p = if path.to_lowercase().ends_with(".xlsx") { path.to_string() } else { format!("{path}.xlsx") };
        match std::fs::write(&p, data) {
            Ok(()) => self.toast(format!("Clash matrix saved: {}", crate::file_name(&p))),
            Err(e) => self.error(format!("Cannot write {p}: {e}")),
        }
    }

    /// New tests from the set matrix: one test for each ticked pair that does not have a test yet.
    pub fn create_tests_from_matrix(&mut self, pairs: &[(Selector, Selector)], ty: ClashType, tol_mm: f32) -> usize {
        let mut n = 0;
        if self.clash.tests.len() == 1 && self.clash.tests[0].results.is_empty() && self.clash.tests[0].last_run.is_empty() && !pairs.is_empty() {
            self.clash.tests.clear();
        }
        for (a, b) in pairs {
            if self.clash.tests.iter().any(|t| (&t.a, &t.b) == (a, b)) {
                continue;
            }
            let short = |s: &Selector| if s.kind == SelKind::All { "All".to_string() } else { s.value.clone() };
            let mut t = ClashTest::new(format!("{} vs {}", short(a), short(b)));
            t.a = a.clone();
            t.b = b.clone();
            t.ty = ty;
            t.tol_mm = tol_mm;
            self.clash.tests.push(t);
            n += 1;
        }
        self.after_clash_edit();
        n
    }
}

pub const NW_HEAD: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\" ?>\n<exchange xmlns:xsi=\"http://www.w3.org/2001/XMLSchema-instance\" xsi:noNamespaceSchemaLocation=\"http://download.autodesk.com/us/navisworks/schemas/nw-exchange-12.0.xsd\" units=\"m\" filename=\"\" filepath=\"\">\n";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn labels_round_trip() {
        for s in ["All elements", "Set: Ducts", "Model: Block_A_MEP", "Type: IfcWall", "Discipline: Structure"] {
            assert_eq!(selector_from_label(s).label(), s);
        }
        assert_eq!(selector_from_label("Pipes").kind, SelKind::Set);
    }

    #[test]
    fn navisworks_xml_import() {
        let x = r#"<?xml version="1.0" encoding="UTF-8" ?>
<exchange units="m"><batchtest name="b" internal_name="b" units="m"><clashtests>
<clashtest name="Struct vs MEP" test_type="clearance" status="ok" tolerance="0.0250" merge_composites="1"><linkage mode="none"/>
<left><clashselection selfintersect="0" primtypes="1"><locator>lcop_selection_set_tree/Folder/Structure</locator></clashselection></left>
<right><clashselection selfintersect="0" primtypes="1"><locator>lcop_selection_set_tree/MEP</locator></clashselection></right><rules/></clashtest>
</clashtests>
<selectionsets><viewfolder name="Folder"><selectionset name="Structure"><findspec mode="all" disjoint="0"><conditions>
<condition test="equals" flags="10"><category><name internal="LcOaNode">Item</name></category><property><name internal="LcOaSceneBaseClassUserName">Type</name></property><value><data type="wstring">IfcBeam</data></value></condition>
<condition test="equals" flags="74"><category><name internal="LcOaNode">Item</name></category><property><name>Type</name></property><value><data type="wstring">IfcColumn</data></value></condition>
</conditions><locator>/</locator></findspec></selectionset></viewfolder>
<selectionset name="MEP"><findspec mode="all" disjoint="0"><conditions><condition test="contains" flags="10"><category><name>Element</name></category><property><name>System Type</name></property><value><data type="wstring">Supply</data></value></condition></conditions><locator>/</locator></findspec></selectionset>
</selectionsets></batchtest></exchange>"#;
        let imp = parse_tests_xml(x).unwrap();
        assert_eq!(imp.tests.len(), 1);
        let t = &imp.tests[0];
        assert_eq!(t.ty, ClashType::Clearance);
        assert!((t.tol_mm - 25.0).abs() < 0.01);
        assert_eq!(t.a, Selector { kind: SelKind::Set, value: "Structure".into() });
        assert_eq!(imp.sets.len(), 2);
        let SetKind::Search(r) = &imp.sets[0].kind else { panic!() };
        assert_eq!(r.len(), 2);
        assert_eq!(r[0].field, "IFC Type");
        assert!(r[1].or);
        let SetKind::Search(r) = &imp.sets[1].kind else { panic!() };
        assert_eq!((r[0].group.as_str(), r[0].field.as_str(), r[0].op), ("Element", "System Type", SearchOp::Contains));
    }

    #[test]
    fn csv_round_trip() {
        let mut a = App::new(vec![]);
        a.clash.tests.clear();
        let mut t = ClashTest::new("S vs M".into());
        t.a = selector_from_label("Discipline: Structure");
        t.b = selector_from_label("Set: Ducts");
        t.ty = ClashType::Clearance;
        t.tol_mm = 50.0;
        t.rules.same_level = true;
        a.clash.tests.push(t);
        let csv = a.clash_tests_csv();
        let imp = parse_tests_rows(&crate::tables::csv_rows(&csv)).unwrap();
        let d = &imp.tests[0];
        assert_eq!((d.name.as_str(), d.ty, d.tol_mm), ("S vs M", ClashType::Clearance, 50.0));
        assert_eq!(d.a.label(), "Discipline: Structure");
        assert_eq!(d.b.label(), "Set: Ducts");
        assert!(d.rules.same_level);
        // XML: the ManageBIM definitions are kept exactly
        let (x, _) = a.clash_tests_xml();
        let imp = parse_tests_xml(&x).unwrap();
        assert_eq!(imp.tests[0].a.label(), "Discipline: Structure");
        assert!(imp.tests[0].rules.same_level);
        assert!(x.contains("lcop_selection_set_tree/Discipline: Structure"));
    }

    #[test]
    fn matrix_by_test() {
        let mut t1 = ClashTest::new("1".into());
        t1.a = selector_from_label("Set: S");
        t1.b = selector_from_label("Set: M");
        let mut t2 = t1.clone();
        t2.b = selector_from_label("Set: E");
        let mut t3 = t1.clone();
        t3.a = selector_from_label("Set: A");
        let m = test_matrix(&[t1, t2, t3]);
        assert_eq!(m.rows, vec!["Set: S", "Set: A"]);
        assert_eq!(m.cols, vec!["Set: M", "Set: E"]);
        assert_eq!(m.cells[&(0, 1)], vec![1]);
        assert!(m.cells.get(&(1, 1)).is_none());
    }
}
