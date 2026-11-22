//! Sectiune din `impl App` (vezi fisierul parinte pentru restul).
use super::*;

impl App {
    // ------------------------------------------------------------------ Quantification
    /// Semnatura starii de care depinde rezultatul (configuratie + selectie / elemente ascunse).
    pub fn qto_signature(&self) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        serde_json::to_string(&self.qto).unwrap_or_default().hash(&mut h);
        match self.qto.scope {
            crate::qto::QtoScope::All => {}
            crate::qto::QtoScope::Visible => {
                let mut v: Vec<&ElemId> = self.hidden.iter().collect();
                v.sort();
                v.hash(&mut h);
            }
            crate::qto::QtoScope::Selection => self.selected.hash(&mut h),
        }
        self.models.iter().map(|m| m.as_ref().map(|m| Arc::as_ptr(m) as usize).unwrap_or(0)).collect::<Vec<_>>().hash(&mut h);
        h.finish()
    }

    /// Recalculeaza cantitatile daca s-a schimbat ceva (configuratie, modele, selectie...).
    pub fn qto_update(&mut self) {
        let sig = self.qto_signature();
        if self.qto_result.is_none() || sig != self.qto_sig {
            self.qto_result = Some(self.qto_compute(&self.qto));
            self.qto_sig = sig;
        }
    }

    pub fn qto_compute(&self, setup: &crate::qto::QtoSetup) -> crate::qto::QtoResult {
        use crate::qto::{geo_qty, GeoQty, QtoResult, QtoRow, QtoScope, QtyColumn};
        use rayon::prelude::*;
        // elementele luate in calcul
        let ids: Vec<ElemId> = match setup.scope {
            QtoScope::All => self.all_ids(),
            QtoScope::Visible => self.all_ids().into_iter().filter(|i| self.is_visible(*i)).collect(),
            QtoScope::Selection => self.selected.iter().copied().collect(),
        };
        let ids: Vec<ElemId> = ids.into_iter().filter(|i| self.elem(*i).is_some()).collect();
        // geometria doar daca o cere vreo coloana; in paralel
        let need_geo = setup.columns.iter().any(|c| matches!(c, QtyColumn::Volume | QtyColumn::Surface | QtyColumn::PlanArea | QtyColumn::Length | QtyColumn::Height));
        let models = &self.models;
        let geo: Vec<GeoQty> = if need_geo {
            ids.par_iter()
                .map(|id| {
                    let m = models[id.m as usize].as_ref().unwrap();
                    geo_qty(m, &m.elements[id.e as usize])
                })
                .collect()
        } else {
            vec![GeoQty::default(); ids.len()]
        };
        let ncol = setup.columns.len();
        let mut units = vec![String::new(); ncol];
        let mut groups: HashMap<Vec<String>, QtoRow> = HashMap::new();
        for (k, id) in ids.iter().enumerate() {
            let (m, e) = self.elem(*id).unwrap();
            let keys: Vec<String> = setup
                .group_by
                .iter()
                .map(|lv| elem_values(m, e, &lv.cat, &lv.prop).into_iter().map(str::trim).find(|v| !v.is_empty()).map(String::from).unwrap_or_else(|| format!("(no {})", lv.prop.to_lowercase())))
                .collect();
            let g = &geo[k];
            let vals: Vec<Option<f64>> = setup
                .columns
                .iter()
                .enumerate()
                .map(|(ci, c)| match c {
                    QtyColumn::Count => Some(1.0),
                    QtyColumn::Volume => Some(g.volume),
                    QtyColumn::Surface => Some(g.surface),
                    QtyColumn::PlanArea => Some(g.plan),
                    QtyColumn::Length => Some(g.length),
                    QtyColumn::Height => Some(g.height),
                    QtyColumn::Prop { cat, prop } => elem_values(m, e, cat, prop).into_iter().find_map(|v| {
                        let x = leading_number(v)?;
                        if units[ci].is_empty() {
                            units[ci] = crate::profiler::unit_of(v);
                        }
                        Some(x)
                    }),
                })
                .collect();
            let row = groups.entry(keys.clone()).or_insert_with(|| QtoRow { keys, ids: Vec::new(), item_values: Vec::new(), values: vec![None; ncol], missing: vec![0; ncol] });
            for (ci, v) in vals.iter().enumerate() {
                match v {
                    Some(x) => row.values[ci] = Some(row.values[ci].unwrap_or(0.0) + x),
                    None => row.missing[ci] += 1,
                }
            }
            row.ids.push(*id);
            row.item_values.push(vals);
        }
        let mut rows: Vec<QtoRow> = groups.into_values().collect();
        rows.sort_by(|a, b| {
            for (x, y) in a.keys.iter().zip(&b.keys) {
                let (nx, ny) = (x.starts_with("(no "), y.starts_with("(no "));
                let o = nx.cmp(&ny).then_with(|| crate::profiler::natural_cmp(x, y));
                if o != std::cmp::Ordering::Equal {
                    return o;
                }
            }
            std::cmp::Ordering::Equal
        });
        let mut totals = vec![None; ncol];
        let mut missing = vec![0u32; ncol];
        for r in &rows {
            for ci in 0..ncol {
                if let Some(v) = r.values[ci] {
                    totals[ci] = Some(totals[ci].unwrap_or(0.0) + v);
                }
                missing[ci] += r.missing[ci];
            }
        }
        QtoResult { setup: setup.clone(), rows, totals, missing, count: ids.len(), units }
    }

    /// (model, nume, tip IFC, nivel, GlobalId) pentru foaia detaliata din Excel
    pub fn qto_item_info(&self, id: ElemId) -> [String; 5] {
        match self.elem(id) {
            Some((m, e)) => [m.name.clone(), e.name.clone(), e.ifc_type.clone(), e.storey.clone(), e.guid.clone()],
            None => Default::default(),
        }
    }

    // ------------------------------------------------------------------ Appearance Profiler
    /// Valorile proprietatii profilului in toate modelele: (valoare -> nr. elemente, valori numerice, unitate).
    pub(super) fn profile_values(&self) -> (HashMap<String, u32>, Vec<f64>, String) {
        let (cat, prop) = (&self.profile.cat, &self.profile.prop);
        let mut counts: HashMap<String, u32> = HashMap::new();
        let mut nums: Vec<f64> = Vec::new();
        let mut unit = String::new();
        for (_, m) in self.loaded_models() {
            for e in &m.elements {
                let vals = elem_values(m, e, cat, prop);
                if let Some(v) = vals.iter().map(|v| v.trim()).find(|v| !v.is_empty()) {
                    *counts.entry(v.to_string()).or_insert(0) += 1;
                }
                if let Some((v, x)) = vals.iter().find_map(|v| leading_number(v).map(|x| (*v, x))) {
                    if unit.is_empty() {
                        unit = crate::profiler::unit_of(v);
                    }
                    nums.push(x);
                }
            }
        }
        (counts, nums, unit)
    }

    /// Reface harta element -> regula si numaratorile (dupa import sau editarea regulilor).
    pub fn profile_recount(&mut self) {
        self.profile_map = None;
        let mut counts = vec![0u32; self.profile.rules.len()];
        for r in self.profile_map().values() {
            if let Some(c) = counts.get_mut(*r as usize) {
                *c += 1;
            }
        }
        for (r, c) in self.profile.rules.iter_mut().zip(counts) {
            r.count = c;
        }
        self.colors_dirty = true;
    }

    /// Dupa import: adauga culori pentru valorile din modelele curente care nu sunt in profil.
    pub fn profile_add_missing(&mut self) {
        let (counts, _, _) = self.profile_values();
        let n = self.profile.add_missing(counts.into_iter().collect());
        self.profile_recount();
        self.toast(if n == 0 { "Profiler: every value already has a colour.".to_string() } else { format!("Profiler: {n} new values added.") });
    }

    /// Valori din modele fara regula (doar modul By value) - pentru avertizarea dupa import.
    pub fn profile_uncovered(&self) -> usize {
        if self.profile.mode != crate::profiler::ProfileMode::Values {
            return 0;
        }
        let (counts, _, _) = self.profile_values();
        counts.keys().filter(|v| !self.profile.rules.iter().any(|r| r.label.eq_ignore_ascii_case(v))).count()
    }

    /// Genereaza regulile profilului din valorile proprietatii alese, pe toate modelele.
    pub fn profile_generate(&mut self) {
        use crate::profiler::ProfileMode;
        let prop = self.profile.prop.clone();
        let (counts, nums, unit) = self.profile_values();
        match self.profile.mode {
            ProfileMode::Values => {
                let dropped = self.profile.build_values(counts.into_iter().collect());
                if dropped > 0 {
                    self.toast(format!("Profiler: {dropped} rare values left out (max {} colours).", crate::profiler::MAX_RULES));
                }
            }
            ProfileMode::Ranges => {
                if nums.is_empty() {
                    self.error(format!("Profiler: \"{prop}\" has no numeric values. Use \"By value\" instead."));
                }
                self.profile.build_ranges(&nums, &unit);
            }
        }
        if self.profile.rules.is_empty() {
            self.toast(format!("Profiler: no element has \"{prop}\"."));
        }
        self.profile.active = !self.profile.rules.is_empty();
        self.profile_map = None;
        self.colors_dirty = true;
    }

    /// element -> indexul regulii (doar elementele care intra intr-o regula)
    pub fn profile_map(&mut self) -> &HashMap<ElemId, u16> {
        if self.profile_map.is_none() {
            let mut map = HashMap::new();
            for (mi, m) in self.loaded_models() {
                for (ei, e) in m.elements.iter().enumerate() {
                    let vals = elem_values(m, e, &self.profile.cat, &self.profile.prop);
                    if let Some(r) = self.profile.rule_for(&vals) {
                        map.insert(ElemId { m: mi as u32, e: ei as u32 }, r as u16);
                    }
                }
            }
            self.profile_map = Some(map);
        }
        self.profile_map.as_ref().unwrap()
    }

    /// Selecteaza elementele unei reguli (click pe o linie din legenda).
    pub fn profile_select(&mut self, rule: usize, add: bool) {
        let ids: Vec<ElemId> = self.profile_map().iter().filter(|(_, r)| **r as usize == rule).map(|(id, _)| *id).collect();
        self.apply(Action::Select(ids, add));
    }

    // ------------------------------------------------------------------ Compare
    pub fn run_compare(&mut self) {
        let (Some(a), Some(b)) = self.compare_pick else {
            return self.error("Compare: choose model A (old version) and model B (new version).");
        };
        if a == b {
            return self.error("Compare: A and B must be two different models.");
        }
        let (Some(ma), Some(mb)) = (self.models.get(a).cloned().flatten(), self.models.get(b).cloned().flatten()) else {
            return self.error("Compare: one of the models is not loaded.");
        };
        let t = Instant::now();
        let r = crate::compare::compare(&ma, a, &mb, b, self.compare_tol);
        let c = r.counts;
        self.toast(format!(
            "Compare {} → {}: {} removed, {} added, {} geometry, {} properties changed, {} unchanged ({:.0} ms)",
            ma.name, mb.name, c[0], c[1], c[2], c[3], c[4], t.elapsed().as_secs_f64() * 1e3
        ));
        self.compare = Some(r);
        self.refresh_compare_params();
        self.compare_on = true;
        self.colors_dirty = true;
    }

    /// Verificarile rapide de calitate (fara reguli IDS) pe toate modelele incarcate.
    pub fn run_quick_checks(&mut self) {
        let models: Vec<(usize, Arc<Model>)> = self.loaded_models().map(|(i, m)| (i, m.clone())).collect();
        if models.is_empty() {
            return self.error("Open a model first.");
        }
        let refs: Vec<(usize, &Model)> = models.iter().map(|(i, m)| (*i, m.as_ref())).collect();
        let t = Instant::now();
        self.quick_checks = crate::check::quick_checks(&refs);
        let bad = self.quick_checks.iter().filter(|q| !q.result.failed.is_empty()).count();
        self.toast(format!("Quick checks: {bad} of {} found problems ({:.0} ms)", self.quick_checks.len(), t.elapsed().as_secs_f64() * 1e3));
    }

    /// Raport CSV al verificarilor rapide: verificare; GUID; element; tip; nivel; motiv.
    pub fn quick_checks_csv(&self) -> String {
        let q = |s: &str| if s.contains(';') || s.contains('"') { format!("\"{}\"", s.replace('"', "\"\"")) } else { s.to_string() };
        let mut s = String::from("{feff}Check;GUID;Element;Type;Level;Reason
");
        for c in &self.quick_checks {
            for (id, why) in &c.result.failed {
                if let Some((_, e)) = self.elem(*id) {
                    s.push_str(&format!("{};{};{};{};{};{}
", q(c.name), q(&e.guid), q(&e.name), q(&e.ifc_type), q(&e.storey), q(why)));
                }
            }
        }
        s
    }

    /// Recalculeaza lista proprietatilor schimbate dupa un Compare.
    pub fn refresh_compare_params(&mut self) {
        self.compare_param_pick = None;
        self.compare_params = match &self.compare {
            Some(r) => match (self.models.get(r.a).cloned().flatten(), self.models.get(r.b).cloned().flatten()) {
                (Some(ma), Some(mb)) => crate::compare::param_changes(&ma, &mb, r),
                _ => Vec::new(),
            },
            None => Vec::new(),
        };
    }

    pub fn compare_clear(&mut self) {
        self.compare = None;
        self.compare_params.clear();
        self.compare_on = false;
        self.colors_dirty = true;
    }

    pub fn profile_clear(&mut self) {
        self.profile.rules.clear();
        self.profile.active = false;
        self.profile_map = None;
        self.colors_dirty = true;
    }

    /// Compara modelele (fara nori) cu toti norii de puncte incarcati.
    pub fn run_asbuilt(&mut self) {
        let t = std::time::Instant::now();
        let models: Vec<(usize, Arc<Model>)> = self.loaded_models().map(|(i, m)| (i, m.clone())).collect();
        let clouds: Vec<&crate::model::PointCloud> = models.iter().filter_map(|(_, m)| m.cloud.as_deref()).collect();
        if clouds.is_empty() {
            return self.error("Load a point cloud first (E57 / LAS: Open or Append), together with the model.");
        }
        let p = self.asbuilt_params;
        let occ = crate::asbuilt::Occupancy::build(&clouds, p.tol * 0.5);
        let mut items: Vec<(usize, &Model, u32)> = Vec::new();
        for (mi, m) in &models {
            if m.cloud.is_some() {
                continue;
            }
            for (ei, e) in m.elements.iter().enumerate() {
                let id = ElemId { m: *mi as u32, e: ei as u32 };
                if e.tri_count > 0 && !e.default_hidden && !self.hidden.contains(&id) {
                    items.push((*mi, m.as_ref(), ei as u32));
                }
            }
        }
        if items.is_empty() {
            return self.error("No visible model elements to check - load the design model next to the point cloud.");
        }
        let res = crate::asbuilt::check(&items, &occ, p);
        let count = |s: crate::asbuilt::Status| res.iter().filter(|r| r.status == s).count();
        self.toast(format!(
            "As-built check: {} built, {} partial, {} missing, {} not scanned ({} points, {:.1} s)",
            count(crate::asbuilt::Status::Built),
            count(crate::asbuilt::Status::Partial),
            count(crate::asbuilt::Status::Missing),
            count(crate::asbuilt::Status::NotScanned),
            thousands_app(occ.points),
            t.elapsed().as_secs_f32()
        ));
        self.asbuilt_colors(false);
        self.asbuilt = Some(res);
        self.asbuilt_colors(true);
    }

    /// Coloreaza elementele dupa stare (verde / galben / rosu / gri) sau scoate culorile.
    /// Transforma diferentele as-built (Missing / Partial) in rezultate de clash „Scan vs Model”.
    pub fn add_scan_clashes(&mut self) {
        let Some(res) = self.asbuilt.clone() else {
            self.error("Run Scan vs Model first (Analysis › As-built).");
            return;
        };
        let bad: Vec<_> = res.into_iter().filter(|r| matches!(r.status, crate::asbuilt::Status::Missing | crate::asbuilt::Status::Partial)).collect();
        if bad.is_empty() {
            self.toast("No scan discrepancies to add (only Missing / Partial are clashes).");
            return;
        }
        let idx = if let Some(i) = self.clash_tests.iter().position(|t| t.name == "Scan vs Model") {
            i
        } else {
            self.clash_tests.push(ClashTest::new("Scan vs Model".into()));
            self.clash_tests.len() - 1
        };
        let now = now_string();
        let mut added = 0;
        for r in bad {
            let Some((_, e)) = self.elem(r.id) else { continue };
            let ga = e.guid.clone();
            if self.clash_tests[idx].results.iter().any(|x| x.b_is_scan && x.ga == ga) {
                continue;
            }
            let c = (e.aabb.min + e.aabb.max) * 0.5;
            let n = self.clash_tests[idx].results.len() + 1;
            self.clash_tests[idx].results.push(ClashResult {
                name: format!("Scan {n}"),
                a: r.id,
                b: r.id,
                ga: ga.clone(),
                gb: "POINT-CLOUD".into(),
                point: [c.x, c.y, c.z],
                dist: self.asbuilt_params.tol,
                status: ClashStatus::New,
                assigned: String::new(),
                comment: format!("As-built: {} · coverage {:.0}%", r.status.label(), r.coverage * 100.0),
                found: now.clone(),
                group: String::new(),
                approved_by: String::new(),
                approved_at: String::new(),
                resolved_by: String::new(),
                resolved_at: String::new(),
                b_is_scan: true,
            });
            added += 1;
        }
        self.cur_test = idx;
        self.clash_subtab = 0;
        self.colors_dirty = true;
        self.toast(format!("{added} scan discrepancies added to Clash Detective › Scan vs Model."));
    }

    pub fn asbuilt_colors(&mut self, on: bool) {
        for id in std::mem::take(&mut self.asbuilt_painted) {
            self.overrides.remove(&id);
        }
        if on {
            if let Some(res) = &self.asbuilt {
                for r in res {
                    self.overrides.insert(r.id, r.status.color());
                    self.asbuilt_painted.push(r.id);
                }
            }
        }
        self.asbuilt_show = on;
        self.colors_dirty = true;
    }

    pub fn export_asbuilt_xlsx(&mut self, path: &str) {
        use crate::qto::{Cell, Sheet};
        let Some(res) = &self.asbuilt else { return self.error("Run the as-built check first.") };
        let mut rows = Vec::new();
        for r in res {
            if let Some((m, e)) = self.elem(r.id) {
                rows.push(vec![Cell::S(r.status.label().into()), Cell::N((r.coverage as f64 * 1000.0).round() / 10.0), Cell::S(e.name.clone()), Cell::S(e.ifc_type.clone()), Cell::S(e.storey.clone()), Cell::S(model_label(&m.name)), Cell::S(e.guid.clone())]);
            }
        }
        let sheet = Sheet {
            name: "As-built".into(),
            title: format!("Model vs point cloud - tolerance {:.0} mm", self.asbuilt_params.tol * 1000.0),
            headers: ["Status", "Coverage %", "Element", "Type", "Level", "Model", "GUID"].iter().map(|s| s.to_string()).collect(),
            widths: vec![14.0, 12.0, 36.0, 22.0, 18.0, 24.0, 26.0],
            rows,
            total_from: None,
        };
        let p = with_ext(path, "xlsx");
        match std::fs::write(&p, crate::qto::xlsx(&[sheet])) {
            Ok(()) => self.toast(format!("As-built report saved: {}", crate::file_name(&p))),
            Err(e) => self.error(format!("Cannot write {p}: {e}")),
        }
    }

    /// Ruleaza toate regulile active pe modelele incarcate.
    pub fn run_check(&mut self) {
        if self.check_rules.is_empty() {
            return self.error("No rules yet - add a rule, load the starter rules or import an IDS file.");
        }
        let t = std::time::Instant::now();
        let models: Vec<(usize, Arc<Model>)> = self.loaded_models().map(|(i, m)| (i, m.clone())).collect();
        if models.is_empty() {
            return self.error("Open a model first.");
        }
        let refs: Vec<(usize, &Model)> = models.iter().map(|(i, m)| (*i, m.as_ref())).collect();
        let res = crate::check::run(&self.check_rules, &refs);
        let failed: usize = res.iter().map(|r| r.failed.len()).sum();
        let checked: usize = res.iter().map(|r| r.checked).sum();
        self.check_results = Some(res);
        self.toast(format!("Model check: {checked} checks, {failed} failed ({:.1} s)", t.elapsed().as_secs_f32()));
    }

    /// Importa specificatiile unui fisier IDS ca reguli (se adauga la cele existente).
    pub fn import_ids(&mut self, path: &str) {
        let txt = match std::fs::read(path) {
            Ok(b) => String::from_utf8_lossy(&b).to_string(),
            Err(e) => return self.error(format!("Cannot read {path}: {e}")),
        };
        match crate::check::parse_ids(&txt, &crate::file_name(path)) {
            Ok((rules, warn)) => {
                let n = rules.len();
                // aceeasi specificatie importata din nou: o inlocuim
                let src = crate::file_name(path);
                self.check_rules.retain(|r| !(r.source == src && rules.iter().any(|x| x.name == r.name)));
                self.check_rules.extend(rules);
                self.check_results = None;
                self.check_warn = warn;
                self.open_panel(Panel::Check);
                self.toast(format!("IDS imported: {n} specifications from {src}"));
            }
            Err(e) => self.error(format!("{}: {e}", crate::file_name(path))),
        }
    }

    /// Regulile Model Checker ca fisier buildingSMART IDS 1.0.
    pub fn export_ids(&mut self, path: &str) {
        if self.check_rules.is_empty() {
            return self.error("No rules to export.");
        }
        let p = with_ext(path, "ids");
        let title = self.project_path.as_ref().map(|p| model_label(&crate::file_name(p))).unwrap_or_else(|| "ManageBIM requirements".into());
        match std::fs::write(&p, crate::check::write_ids(&self.check_rules, &title, &now_string())) {
            Ok(()) => self.toast(format!("{} specifications exported as IDS 1.0: {}", self.check_rules.len(), crate::file_name(&p))),
            Err(e) => self.error(format!("Cannot write {p}: {e}")),
        }
    }

    pub fn export_check_rules(&mut self, path: &str) {
        let p = with_ext(path, "mbrules");
        match serde_json::to_string_pretty(&self.check_rules).map_err(|e| e.to_string()).and_then(|s| std::fs::write(&p, s).map_err(|e| e.to_string())) {
            Ok(()) => self.toast(format!("{} rules saved: {}", self.check_rules.len(), crate::file_name(&p))),
            Err(e) => self.error(format!("Cannot write {p}: {e}")),
        }
    }

    pub fn import_check_rules(&mut self, path: &str) {
        let lower = path.to_lowercase();
        if lower.ends_with(".ids") || lower.ends_with(".xml") {
            return self.import_ids(path);
        }
        match std::fs::read_to_string(path).map_err(|e| e.to_string()).and_then(|s| serde_json::from_str::<Vec<crate::check::Rule>>(&s).map_err(|e| e.to_string())) {
            Ok(r) => {
                let n = r.len();
                self.check_rules.extend(r);
                self.check_results = None;
                self.open_panel(Panel::Check);
                self.toast(format!("{n} rules loaded"));
            }
            Err(e) => self.error(format!("{}: not a ManageBIM rules file ({e})", crate::file_name(path))),
        }
    }

    /// Raport Excel: foaia "Summary" (o linie pe regula) + "Failures" (fiecare element care nu respecta).
    pub fn export_check_xlsx(&mut self, path: &str) {
        use crate::qto::{Cell, Sheet};
        let Some(res) = &self.check_results else { return self.error("Run the model check first.") };
        let mut sum = Vec::new();
        let mut fail = Vec::new();
        for (r, x) in self.check_rules.iter().zip(res) {
            if !r.enabled {
                continue;
            }
            let status = if x.checked == 0 { "Not applicable" } else if x.failed.is_empty() { "Pass" } else { "Fail" };
            sum.push(vec![Cell::S(r.name.clone()), Cell::S(status.into()), Cell::S(r.applies_text()), Cell::S(r.requirements.iter().map(|f| f.describe()).collect::<Vec<_>>().join("; ")), Cell::N(x.checked as f64), Cell::N(x.passed() as f64), Cell::N(x.failed.len() as f64)]);
            for (id, why) in &x.failed {
                if let Some((m, e)) = self.elem(*id) {
                    fail.push(vec![Cell::S(r.name.clone()), Cell::S(e.name.clone()), Cell::S(e.ifc_type.clone()), Cell::S(e.storey.clone()), Cell::S(model_label(&m.name)), Cell::S(e.guid.clone()), Cell::S(why.clone())]);
                }
            }
        }
        let s1 = Sheet { name: "Summary".into(), title: "Model check - summary".into(), headers: ["Rule", "Status", "Applies to", "Requirements", "Checked", "Passed", "Failed"].iter().map(|s| s.to_string()).collect(), widths: vec![36.0, 14.0, 40.0, 60.0, 12.0, 12.0, 12.0], rows: sum, total_from: Some(4) };
        let s2 = Sheet { name: "Failures".into(), title: "Model check - elements that fail".into(), headers: ["Rule", "Element", "Type", "Level", "Model", "GUID", "Problem"].iter().map(|s| s.to_string()).collect(), widths: vec![32.0, 36.0, 22.0, 18.0, 24.0, 26.0, 70.0], rows: fail, total_from: None };
        let p = with_ext(path, "xlsx");
        match std::fs::write(&p, crate::qto::xlsx(&[s1, s2])) {
            Ok(()) => self.toast(format!("Model check report saved: {}", crate::file_name(&p))),
            Err(e) => self.error(format!("Cannot write {p}: {e}")),
        }
    }

    /// BCF: un topic pentru fiecare regula care nu trece, cu elementele gresite selectate si o camera pe ele.
    pub fn export_check_bcf(&mut self, path: &str) {
        let Some(res) = self.check_results.clone() else { return self.error("Run the model check first.") };
        let user = crate::markup::user_name();
        let now = crate::markup::now_iso();
        let mut items = Vec::new();
        for (r, x) in self.check_rules.iter().zip(&res) {
            if !r.enabled || x.failed.is_empty() {
                continue;
            }
            let ids: Vec<ElemId> = x.failed.iter().map(|f| f.0).collect();
            let bb = self.ids_aabb(&ids);
            let mut c = self.cam;
            c.ortho = false;
            if bb.is_valid() {
                c.yaw = std::f32::consts::FRAC_PI_4;
                c.pitch = -0.5;
                c.fit(&bb);
            }
            let guids: Vec<String> = ids.iter().filter_map(|id| self.elem(*id).map(|(_, e)| e.guid.clone())).filter(|g| !g.is_empty()).collect();
            let mut desc = format!("{} of {} elements fail. Applies to: {}. Requirements: {}.", x.failed.len(), x.checked, r.applies_text(), r.requirements.iter().map(|f| f.describe()).collect::<Vec<_>>().join("; "));
            for (id, why) in x.failed.iter().take(20) {
                let n = self.elem(*id).map(|(_, e)| e.name.clone()).unwrap_or_default();
                desc.push_str(&format!("\n- {n}: {why}"));
            }
            if x.failed.len() > 20 {
                desc.push_str(&format!("\n... and {} more", x.failed.len() - 20));
            }
            items.push(crate::markup::ReportItem {
                index: items.len() + 1,
                name: format!("Model check: {}", r.name),
                description: desc,
                issue: crate::markup::Issue { guid: crate::markup::new_guid(), status: "Open".into(), priority: "Normal".into(), assigned: String::new(), author: user.clone(), created: now.clone() },
                comments: Vec::new(),
                redlines: 0,
                eye: self.world(Vec3::from(c.eye)).to_array(),
                dir: c.fwd().as_dvec3().to_array(),
                up: c.up().as_dvec3().to_array(),
                fov_deg: c.fov_deg,
                ortho_height: None,
                selected_guids: guids,
                hidden_guids: Vec::new(),
                image: Vec::new(),
                image_w: 0,
                image_h: 0,
            });
        }
        if items.is_empty() {
            return self.toast("All rules pass - nothing to export.");
        }
        let path = if path.to_lowercase().ends_with(".bcf") || path.to_lowercase().ends_with(".bcfzip") { path.to_string() } else { format!("{path}.bcf") };
        let project = self.project_path.as_ref().map(|p| crate::file_name(p)).unwrap_or_else(|| "ManageBIM model check".into());
        match std::fs::write(&path, crate::markup::bcf_zip_version(&project, &items, self.settings.bcf_version)) {
            Ok(()) => self.toast(format!("{} issues exported as BCF: {}", items.len(), crate::file_name(&path))),
            Err(e) => self.error(format!("Cannot write {path}: {e}")),
        }
    }

    /// Elementele care nu respecta regula `ri` (sau toate regulile, daca None).
    pub fn check_failed_ids(&self, ri: Option<usize>) -> Vec<ElemId> {
        let Some(res) = &self.check_results else { return Vec::new() };
        let mut v: Vec<ElemId> = res.iter().enumerate().filter(|(i, _)| ri.map(|r| r == *i).unwrap_or(true)).flat_map(|(_, r)| r.failed.iter().map(|f| f.0)).collect();
        v.sort();
        v.dedup();
        v
    }
}
