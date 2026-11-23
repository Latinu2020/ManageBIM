//! Sectiune din `impl App` (vezi fisierul parinte pentru restul).
use super::*;

/// Schimbarea statusului facuta de utilizator: pastram cine si cand a aprobat / rezolvat (ca Navisworks).
pub(crate) fn apply_status(r: &mut ClashResult, st: ClashStatus) {
    if r.status == st {
        return;
    }
    let who = std::env::var("USERNAME").or_else(|_| std::env::var("USER")).unwrap_or_default();
    let when = now_string();
    match st {
        ClashStatus::Approved => {
            r.approved_by = who;
            r.approved_at = when;
        }
        ClashStatus::Resolved => {
            r.resolved_by = who;
            r.resolved_at = when;
        }
        _ => {}
    }
    r.status = st;
}

impl App {
    // ------------------------------------------------------------------ clash
    /// Trimite clash-ul curent catre Revit / Navisworks prin Outbox\issues.txt:
    /// add-in-urile pot deschide apoi viewpoint-ul cu camera si elementele selectate.
    pub fn switchback(&mut self) {
        let picked = self.cur_result.or_else(|| self.sel_results.iter().next().copied());
        let Some(i) = picked else {
            self.toast("Select a clash first, then SwitchBack.");
            return;
        };
        let (name, a, b, dist) = {
            let Some(t) = self.clash_tests.get(self.cur_test) else { return };
            let Some(r) = t.results.get(i) else { return };
            (format!("SwitchBack — {}", r.name), r.a, r.b, r.dist)
        };
        let comment = format!("SwitchBack from clash {} · {} mm", name, (dist * 1000.0) as i32);
        // daca camera inca zboara spre clash, luam pozitia finala
        let cam = self.cam_anim.map(|(_, to, _)| to).unwrap_or(self.cam);
        self.viewpoints.push(Viewpoint {
            name,
            cam,
            section: self.section.clone(),
            cad_view: self.settings.cad_view,
            cad_view_pos: self.settings.cad_view_pos,
            hidden: self.hidden.iter().copied().collect(),
            comment,
            redlines: Vec::new(),
            issue: Default::default(),
            comments: Vec::new(),
            selected: vec![a, b],
            template: String::new(),
            measures: self.measures.clone(),
            folder: self.vp_folder_cur.clone(),
        });
        let n = self.viewpoints.len() - 1;
        self.active_vp = Some(n);
        self.vp_sel = n;
        self.write_issue_outbox();
        self.toast("SwitchBack sent — in Revit / Navisworks open ManageBIM Issues › the new viewpoint.");
    }

    /// Runs a test on the calling thread (batch use / tests).
    pub fn run_clash_blocking(&mut self, t: usize) {
        let test = self.clash_tests[t].clone();
        let excluded = self.excluded_items(&test.rules);
        let a: Vec<ElemId> = self.resolve_selector(&test.a).into_iter().filter(|i| !excluded.contains(i)).collect();
        let b: Vec<ElemId> = self.resolve_selector(&test.b).into_iter().filter(|i| !excluded.contains(i)).collect();
        let found = clash::run(&self.models, &a, &b, test.ty, test.tol_mm / 1000.0, test.ignore_same_file, &clash::Progress::new());
        self.merge_clash(t, found, 0.0);
    }

    pub fn run_clash(&mut self, t: usize) {
        if self.clash_job.is_some() {
            return;
        }
        let Some(test) = self.clash_tests.get(t) else { return };
        let excluded = self.excluded_items(&test.rules);
        let a: Vec<ElemId> = self.resolve_selector(&test.a).into_iter().filter(|i| !excluded.contains(i)).collect();
        let b: Vec<ElemId> = self.resolve_selector(&test.b).into_iter().filter(|i| !excluded.contains(i)).collect();
        if a.is_empty() || b.is_empty() {
            self.error("Selection A or B contains no elements.");
            return;
        }
        let models: Vec<Option<Arc<Model>>> = self.models.clone();
        let (ty, tol, ign) = (test.ty, test.tol_mm / 1000.0, test.ignore_same_file);
        let prog = clash::Progress::new();
        let p2 = prog.clone();
        let (tx, rx) = channel();
        std::thread::spawn(move || {
            // la o eroare interna tx se inchide fara mesaj -> poll_jobs opreste testul curat
            if let Ok(r) = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| clash::run(&models, &a, &b, ty, tol, ign, &p2))) {
                let _ = tx.send(r);
            }
        });
        self.clash_job = Some(ClashJob { test: t, prog, rx, started: Instant::now() });
    }

    /// Items that a test must never use (exclude sets + exclude rules).
    pub fn excluded_items(&self, rules: &ClashRules) -> HashSet<ElemId> {
        let mut ex = HashSet::new();
        for n in &rules.exclude_sets {
            if let Some(set) = self.sets.iter().find(|s| &s.name == n) {
                ex.extend(self.resolve_set(set));
            }
        }
        let active: Vec<&SearchRule> = rules.exclude_rules.iter().filter(|r| !r.op.needs_value() || !r.value.trim().is_empty()).collect();
        if !active.is_empty() {
            for id in self.all_ids() {
                if active.iter().any(|r| self.rule_match(id, r)) {
                    ex.insert(id);
                }
            }
        }
        ex
    }

    /// Pair rules: same file / level / type / property value / set.
    pub fn pair_ignored(&self, test: &ClashTest, set_members: &[HashSet<ElemId>], a: ElemId, b: ElemId) -> bool {
        let r = &test.rules;
        if test.ignore_same_file && a.m == b.m {
            return true;
        }
        let (Some((_, ea)), Some((_, eb))) = (self.elem(a), self.elem(b)) else { return false };
        if r.same_level && !ea.storey.is_empty() && ea.storey == eb.storey {
            return true;
        }
        if r.same_type && ea.ifc_type == eb.ifc_type {
            return true;
        }
        for pn in &r.same_props {
            let va = ea.props.iter().find(|p| p.name.eq_ignore_ascii_case(pn)).map(|p| p.value.as_str());
            let vb = eb.props.iter().find(|p| p.name.eq_ignore_ascii_case(pn)).map(|p| p.value.as_str());
            if let (Some(x), Some(y)) = (va, vb) {
                if !x.is_empty() && x.eq_ignore_ascii_case(y) {
                    return true;
                }
            }
        }
        set_members.iter().any(|s| s.contains(&a) && s.contains(&b))
    }

    pub fn same_set_members(&self, rules: &ClashRules) -> Vec<HashSet<ElemId>> {
        rules.same_sets.iter().filter_map(|n| self.sets.iter().find(|s| &s.name == n)).map(|s| self.resolve_set(s).into_iter().collect()).collect()
    }

    pub fn merge_clash(&mut self, t: usize, found: Vec<clash::Found>, secs: f32) {
        let now = now_string();
        // reguli: perechi ignorate + elemente excluse
        // testul poate fi sters / proiectul inchis cat timp rula: rezultatul se ignora, nu crapa aplicatia
        let Some(test_cfg) = self.clash_tests.get(t).cloned() else { return };
        let excluded = self.excluded_items(&test_cfg.rules);
        let set_members = self.same_set_members(&test_cfg.rules);
        let before = found.len();
        let found: Vec<clash::Found> = found.into_iter().filter(|f| !self.pair_ignored(&test_cfg, &set_members, f.a, f.b) && !self.normal_joint_ignored(&test_cfg.rules, f.a, f.b, f.dist)).collect();
        let n_ignored = before - found.len();
        let stale: HashSet<(String, String)> = test_cfg
            .results
            .iter()
            .filter(|r| excluded.contains(&r.a) || excluded.contains(&r.b) || self.pair_ignored(&test_cfg, &set_members, r.a, r.b))
            .map(|r| (r.ga.clone(), r.gb.clone()))
            .collect();
        let guid = |s: &App, id: ElemId| s.elem(id).map(|(m, e)| format!("{}|{}", m.name, if e.guid.is_empty() { e.name.clone() } else { e.guid.clone() })).unwrap_or_default();
        let new_items: Vec<(clash::Found, String, String)> = found.into_iter().map(|f| {
            let ga = guid(self, f.a);
            let gb = guid(self, f.b);
            (f, ga, gb)
        }).collect();
        let test = &mut self.clash_tests[t];
        let mut old: HashMap<(String, String), ClashResult> = test.results.drain(..).map(|r| ((r.ga.clone(), r.gb.clone()), r)).collect();
        let mut next = old.values().filter_map(|r| r.name.strip_prefix("Clash").and_then(|n| n.parse::<u32>().ok())).max().unwrap_or(0) + 1;
        let mut out = Vec::new();
        let (mut n_new, mut n_act) = (0, 0);
        for (f, ga, gb) in new_items {
            let key = (ga.clone(), gb.clone());
            let prev = old.remove(&key).or_else(|| old.remove(&(gb.clone(), ga.clone())));
            let r = match prev {
                Some(mut p) => {
                    p.status = match p.status {
                        ClashStatus::New | ClashStatus::Active => ClashStatus::Active,
                        ClashStatus::Resolved => ClashStatus::New,
                        s => s,
                    };
                    p.a = f.a;
                    p.b = f.b;
                    p.point = f.point.into();
                    p.dist = f.dist;
                    p
                }
                None => {
                    let n = format!("Clash{next}");
                    next += 1;
                    ClashResult { name: n, a: f.a, b: f.b, ga, gb, point: f.point.into(), dist: f.dist, status: ClashStatus::New, assigned: String::new(), comment: String::new(), found: now.clone(), group: String::new(), approved_by: String::new(), approved_at: String::new(), resolved_by: String::new(), resolved_at: String::new(), b_is_scan: false, view: None }
                }
            };
            if r.status == ClashStatus::New {
                n_new += 1;
            } else {
                n_act += 1;
            }
            out.push(r);
        }
        // rezultatele vechi excluse acum de reguli dispar; restul devin Resolved
        old.retain(|k, _| !stale.contains(k));
        let n_res = old.len();
        let mut res_old: Vec<ClashResult> = old.into_values().collect();
        res_old.sort_by(|x, y| x.name.cmp(&y.name));
        for mut r in res_old {
            r.status = ClashStatus::Resolved;
            out.push(r);
        }
        test.results = out;
        test.last_run = now;
        test.run_key = crate::clash_io::run_key(&test_cfg);
        let name = test.name.clone();
        self.cur_result = None;
        self.sel_results.clear();
        let ign = if n_ignored > 0 { format!(", {n_ignored} ignored by rules") } else { String::new() };
        self.toast(format!("{name}: {n_new} new, {n_act} active, {n_res} resolved{ign} ({secs:.1} s)"));
        self.colors_dirty = true;
        if test_cfg.rules.auto_assign {
            self.auto_assign_clashes(t);
        }
    }

    /// conexiune normala (perete-placa, grinda-stalp, teava-suport...) cu patrundere sub prag: nu e clash real
    pub fn normal_joint_ignored(&self, rules: &ClashRules, a: ElemId, b: ElemId, dist: f32) -> bool {
        if !rules.normal_joints || dist > 0.0 {
            return false;
        }
        let (Some((_, ea)), Some((_, eb))) = (self.elem(a), self.elem(b)) else { return false };
        crate::clash_smart::normal_joint(crate::clash_smart::kind_of(ea), crate::clash_smart::kind_of(eb)) && dist.abs() * 1000.0 <= rules.joint_mm.max(0.0)
    }

    /// Responsabilul clash-urilor fara responsabil: disciplina care se muta (structura > arhitectura > instalatii).
    pub fn auto_assign_clashes(&mut self, t: usize) {
        let Some(test) = self.clash_tests.get(t) else { return };
        let who: Vec<Option<String>> = test
            .results
            .iter()
            .map(|r| {
                if !r.assigned.is_empty() {
                    return None;
                }
                let (ma, ea) = self.elem(r.a)?;
                let (mb, eb) = self.elem(r.b)?;
                let da = crate::clash_smart::discipline_of(ea, &ma.name);
                let db = crate::clash_smart::discipline_of(eb, &mb.name);
                crate::clash_smart::responsible(da, db).map(|d| d.label().to_string())
            })
            .collect();
        if let Some(test) = self.clash_tests.get_mut(t) {
            for (r, w) in test.results.iter_mut().zip(who) {
                if let Some(w) = w {
                    r.assigned = w;
                }
            }
        }
    }

    // ------------------------------------------------------------------ clash groups
    pub fn group_names(&self, t: usize) -> Vec<String> {
        let mut v: Vec<String> = Vec::new();
        for r in &self.clash_tests[t].results {
            if !r.group.is_empty() && !v.contains(&r.group) {
                v.push(r.group.clone());
            }
        }
        v
    }

    pub fn new_group(&mut self, t: usize, idx: &[usize]) {
        if idx.is_empty() {
            return;
        }
        let names = self.group_names(t);
        let mut n = names.len() + 1;
        while names.contains(&format!("Group {n}")) {
            n += 1;
        }
        let name = format!("Group {n}");
        for i in idx {
            if let Some(r) = self.clash_tests[t].results.get_mut(*i) {
                r.group = name.clone();
            }
        }
        self.open_groups.insert(name.clone());
        self.toast(format!("{} clashes grouped as \"{name}\".", idx.len()));
    }

    pub fn ungroup(&mut self, t: usize, idx: &[usize]) {
        for i in idx {
            if let Some(r) = self.clash_tests[t].results.get_mut(*i) {
                r.group.clear();
            }
        }
    }

    pub fn auto_group(&mut self, t: usize, by: GroupBy, cell: f32) {
        let keys: Vec<String> = self.clash_tests[t]
            .results
            .iter()
            .map(|r| {
                let ea = self.elem(r.a).map(|x| x.1);
                let eb = self.elem(r.b).map(|x| x.1);
                match by {
                    GroupBy::Level => format!("Level: {}", ea.map(|e| if e.storey.is_empty() { "(no level)".to_string() } else { e.storey.clone() }).unwrap_or_default()),
                    GroupBy::ItemA => format!("A: {}", ea.map(|e| e.name.clone()).unwrap_or_default()),
                    GroupBy::ItemB => format!("B: {}", eb.map(|e| e.name.clone()).unwrap_or_default()),
                    GroupBy::TypePair => format!("{} × {}", ea.map(|e| e.ifc_type.clone()).unwrap_or_default(), eb.map(|e| e.ifc_type.clone()).unwrap_or_default()),
                    GroupBy::Grid => format!("Grid {}", self.grid_location(Vec3::from(r.point))),
                    GroupBy::System => format!("System: {}", ea.and_then(crate::clash_smart::system_of).or_else(|| eb.and_then(crate::clash_smart::system_of)).unwrap_or_else(|| "(no system)".into())),
                    GroupBy::ModelA => format!("A: {}", self.elem(r.a).map(|(m, _)| m.name.clone()).unwrap_or_default()),
                    GroupBy::ModelB => format!("B: {}", self.elem(r.b).map(|(m, _)| m.name.clone()).unwrap_or_default()),
                    GroupBy::Assigned => format!("Assigned: {}", if r.assigned.trim().is_empty() { "(none)" } else { &r.assigned }),
                    GroupBy::Status => format!("Status: {}", r.status.label()),
                    GroupBy::Proximity => {
                        let c = cell.max(0.1);
                        let p = r.point;
                        format!("Area {:+.0}/{:+.0}/{:+.0}", (p[0] / c).floor(), (p[1] / c).floor(), (p[2] / c).floor())
                    }
                }
            })
            .collect();
        let mut count: HashMap<&String, usize> = HashMap::new();
        for k in &keys {
            *count.entry(k).or_default() += 1;
        }
        let mut n = 0;
        let mut groups = HashSet::new();
        for (r, k) in self.clash_tests[t].results.iter_mut().zip(keys.iter()) {
            // un singur clash nu formeaza grup
            if count[k] > 1 {
                r.group = k.clone();
                groups.insert(k.clone());
                n += 1;
            } else {
                r.group.clear();
            }
        }
        self.toast(format!("Auto-grouped by {}: {} groups, {n} clashes.", by.label().to_lowercase(), groups.len()));
    }

    /// Combined status of a group: the "most open" status of its members.
    pub fn group_status(&self, t: usize, members: &[usize]) -> ClashStatus {
        let res = &self.clash_tests[t].results;
        for s in ClashStatus::ALL {
            if members.iter().any(|i| res[*i].status == s) {
                return s;
            }
        }
        ClashStatus::Resolved
    }

    pub fn elem_label(&self, id: ElemId) -> String {
        self.elem(id).map(|(_, e)| e.name.clone()).unwrap_or_else(|| "(missing)".into())
    }

    pub fn clash_report_html(&self, t: usize) -> String {
        let test = &self.clash_tests[t];
        let esc = |s: &str| s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;");
        let mut h = String::new();
        h.push_str("<!DOCTYPE html><html lang=\"en\"><head><meta charset=\"utf-8\"><title>Clash report</title><style>");
        h.push_str("body{font-family:Segoe UI,Arial,sans-serif;margin:24px;color:#222}table{border-collapse:collapse;width:100%;font-size:13px}th,td{border:1px solid #ccc;padding:5px 8px;text-align:left}th{background:#2d3e50;color:#fff}tr:nth-child(even){background:#f4f6f8}.s{font-weight:600}.sum span{display:inline-block;margin-right:18px}");
        h.push_str("</style></head><body>");
        h.push_str(&format!("<h1>Clash Detective Report — {}</h1>", esc(&test.name)));
        h.push_str(&format!(
            "<p>Generated by ManageBIM on {}. Type: <b>{}</b>, tolerance {} mm. Selection A: {} · Selection B: {}. Last run: {}.</p>",
            now_string(),
            test.ty.label(),
            test.tol_mm,
            esc(&test.a.label()),
            esc(&test.b.label()),
            esc(&test.last_run)
        ));
        let rc = test.rules.count(test.ignore_same_file);
        if rc > 0 {
            h.push_str(&format!("<p>Rules applied: {rc}.</p>"));
        }
        h.push_str("<p class=\"sum\">");
        for s in ClashStatus::ALL {
            h.push_str(&format!("<span>{}: <b>{}</b></span>", s.label(), test.count(s)));
        }
        h.push_str(&format!("<span>Total: <b>{}</b></span></p>", test.results.len()));
        h.push_str("<table><tr><th>Group</th><th>Name</th><th>Status</th><th>Severity</th><th>Distance (m)</th><th>Item A</th><th>Type A</th><th>Item B</th><th>Type B</th><th>Level</th><th>Location (grid / level)</th><th>Point X,Y,Z (local)</th><th>Found</th><th>Assigned to</th><th>Comment</th></tr>");
        let grids = self.scene_grids();
        let mut order: Vec<&ClashResult> = test.results.iter().collect();
        order.sort_by_key(|r| (r.group.is_empty(), r.group.clone()));
        for r in order {
            let gl = grid_location_in(&grids, Vec3::from(r.point));
            let (ta, la, sa) = self.elem(r.a).map(|(m, e)| (e.ifc_type.clone(), format!("{} [{}]", e.name, m.name), e.storey.clone())).unwrap_or_default();
            let (tb, lb) = self.elem(r.b).map(|(m, e)| (e.ifc_type.clone(), format!("{} [{}]", e.name, m.name))).unwrap_or_default();
            let sv = test.severity(r.dist);
            let sc = sv.color();
            let c = r.status.color();
            h.push_str(&format!(
                "<tr><td>{}</td><td>{}</td><td class=\"s\" style=\"color:rgb({},{},{})\">{}</td><td style=\"color:rgb({},{},{})\">{}</td><td>{:.3}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{:.2}, {:.2}, {:.2}</td><td>{}</td><td>{}</td><td>{}</td></tr>",
                esc(&r.group), esc(&r.name), c[0], c[1], c[2], r.status.label(), sc[0], sc[1], sc[2], sv.label(), r.dist, esc(&la), esc(&ta), esc(&lb), esc(&tb), esc(&sa), esc(&gl),
                r.point[0], r.point[1], r.point[2], esc(&r.found), esc(&r.assigned), esc(&r.comment)
            ));
        }
        h.push_str("</table></body></html>");
        h
    }

    /// Randurile tabelului pentru raportul PDF: (status, severitate, elemente, distanta).
    pub fn clash_report_pdf_rows(&self, t: usize) -> Vec<(String, String, String, String, String)> {
        let test = &self.clash_tests[t];
        test.results.iter().map(|r| {
            let la = self.elem(r.a).map(|(_, e)| e.name.clone()).unwrap_or_default();
            let rb = self.elem(r.b).map(|(_, e)| e.name.clone()).unwrap_or_default();
            (r.status.label().to_string(), test.severity(r.dist).label().to_string(), format!("{} / {}", la, rb), format!("{:.3} m", r.dist), r.group.clone())
        }).collect()
    }

    pub fn clash_report_csv(&self, t: usize) -> String {
        let test = &self.clash_tests[t];
        let q = |s: &str| format!("\"{}\"", s.replace('"', "\"\""));
        let mut s = String::from("\u{feff}Group;Name;Status;Severity;Distance_m;ItemA;TypeA;ModelA;GuidA;ItemB;TypeB;ModelB;GuidB;Level;Location;X;Y;Z;Found;AssignedTo;Comment\n");
        let grids = self.scene_grids();
        let mut order: Vec<&ClashResult> = test.results.iter().collect();
        order.sort_by_key(|r| (r.group.is_empty(), r.group.clone()));
        for r in order {
            let a = self.elem(r.a);
            let b = self.elem(r.b);
            s.push_str(&format!(
                "{};{};{};{:.4};{};{};{};{};{};{};{};{};{};{};{:.3};{:.3};{:.3};{};{};{}\n",
                q(&r.group),
                q(&r.name),
                r.status.label(),
                r.dist,
                q(&a.map(|x| x.1.name.clone()).unwrap_or_default()),
                q(&a.map(|x| x.1.ifc_type.clone()).unwrap_or_default()),
                q(&a.map(|x| x.0.name.clone()).unwrap_or_default()),
                q(&a.map(|x| x.1.guid.clone()).unwrap_or_default()),
                q(&b.map(|x| x.1.name.clone()).unwrap_or_default()),
                q(&b.map(|x| x.1.ifc_type.clone()).unwrap_or_default()),
                q(&b.map(|x| x.0.name.clone()).unwrap_or_default()),
                q(&b.map(|x| x.1.guid.clone()).unwrap_or_default()),
                q(&a.map(|x| x.1.storey.clone()).unwrap_or_default()),
                q(&grid_location_in(&grids, Vec3::from(r.point))),
                r.point[0],
                r.point[1],
                r.point[2],
                q(&r.found),
                q(&r.assigned),
                q(&r.comment)
            ));
        }
        s
    }

    /// distanta minima intre primele doua elemente selectate
    pub fn min_distance_selected(&mut self) {
        let ids: Vec<ElemId> = self.selected.iter().copied().take(2).collect();
        if ids.len() != 2 {
            self.error("Select exactly two elements.");
            return;
        }
        let (Some((ma, ea)), Some((mb, eb))) = (self.elem(ids[0]), self.elem(ids[1])) else { return };
        let r = clash::bvh_min_dist(&ea.bvh, ma.tris(ea), &eb.bvh, mb.tris(eb), f32::INFINITY);
        self.min_dist_result = r;
        match r {
            Some((d, _)) => self.toast(format!("Shortest distance between the elements: {}", fmt_len(d as f64))),
            None => self.error("The distance could not be computed."),
        }
    }

    /// Testele de clash (+ seturile folosite) -> XML Navisworks (Clash Detective > Import Clash Tests).
    pub fn export_navis_clash(&mut self, path: &str) {
        self.export_clash_tests_xml(path);
    }

    /// Rezultatele unui test de clash ca "vederi": camera izometrica pe clash, cutia celor doua elemente
    /// (pentru section box), GUID-urile lor si descrierea. Coordonate WORLD, metri.
    pub fn clash_views(&self, ti: usize) -> Vec<(String, String, [f64; 3], [f64; 3], [f64; 3], ([f64; 3], [f64; 3]), Vec<String>, String)> {
        let Some(t) = self.clash_tests.get(ti) else { return Vec::new() };
        t.results
            .iter()
            .filter(|r| r.status != ClashStatus::Resolved && r.status != ClashStatus::Approved)
            .map(|r| {
                let bb = self.ids_aabb(&[r.a, r.b]);
                let p = Vec3::from(r.point);
                let size = if bb.is_valid() { bb.size().length() } else { 2.0 };
                let mut c = self.cam;
                c.yaw = std::f32::consts::FRAC_PI_4;
                c.pitch = -0.5;
                c.ortho = false;
                let dist = (size * 0.9).clamp(1.5, 60.0) / (c.fov().max(0.3) * 0.5).tan();
                let mut eye = p - c.fwd() * dist;
                // viewpoint-ul salvat de utilizator pe rezultat are prioritate
                if let Some(v) = r.view {
                    c = v;
                    eye = v.eye();
                }
                let pad = Vec3::splat((size * 0.15).max(0.3));
                let (mn, mx) = if bb.is_valid() { (bb.min - pad, bb.max + pad) } else { (p - Vec3::splat(1.0), p + Vec3::splat(1.0)) };
                let na = self.elem(r.a).map(|(_, e)| e.name.clone()).unwrap_or_default();
                let nb = self.elem(r.b).map(|(_, e)| e.name.clone()).unwrap_or_default();
                let mut desc = format!("{} | {} <-> {} | {} {}", t.name, na, nb, if r.dist < 0.0 { "penetration" } else { "distance" }, fmt_len(r.dist.abs() as f64));
                if !r.comment.is_empty() {
                    desc.push_str(&format!(" | {}", r.comment));
                }
                // GUID-ul real al elementului (r.ga / r.gb sunt "model|guid", interne testului de clash)
                let guids = [r.a, r.b].iter().filter_map(|id| self.elem(*id).map(|(_, e)| e.guid.clone())).filter(|g| !g.is_empty()).collect();
                (
                    r.name.clone(),
                    format!("{:?}", r.status),
                    self.world(eye).to_array(),
                    c.fwd().as_dvec3().to_array(),
                    c.up().as_dvec3().to_array(),
                    (self.world(mn).to_array(), self.world(mx).to_array()),
                    guids,
                    desc,
                )
            })
            .collect()
    }

    /// Clash-urile testului curent -> viewpoints Navisworks (camera + section box + comentariu), importabile
    /// cu Saved Viewpoints > Import Viewpoints in orice model Navisworks cu aceleasi coordonate.
    pub fn export_clash_navis(&mut self, path: &str) {
        let views = self.clash_views(self.cur_test);
        if views.is_empty() {
            return self.error("No open clash results in the current test - run the test first.");
        }
        let bb = self.scene_aabb();
        let (smin, smax) = if bb.is_valid() { (self.world(bb.min).to_array(), self.world(bb.max).to_array()) } else { ([-1.0; 3], [1.0; 3]) };
        let aspect = if self.vp_rect[3] > 1.0 { self.vp_rect[2] / self.vp_rect[3] } else { 16.0 / 9.0 };
        let user = crate::markup::user_name();
        let now = crate::markup::now_iso();
        let nv: Vec<crate::markup::NwView> = views
            .into_iter()
            .map(|(name, status, eye, fwd, up, sbox, _, desc)| crate::markup::NwView {
                name,
                guid: crate::markup::new_guid(),
                eye,
                fwd,
                up,
                fov_deg: self.cam.fov_deg,
                ortho_height: None,
                aspect,
                focal: 5.0,
                near: 0.05,
                far: 2000.0,
                scene_min: smin,
                scene_max: smax,
                section_box: Some(sbox),
                comments: vec![(user.clone(), desc, now.clone(), if status == "New" { "new" } else { "active" })],
                redlines: Vec::new(),
                folder: "Clash Detective".into(),
            })
            .collect();
        let path = if path.to_lowercase().ends_with(".xml") { path.to_string() } else { format!("{path}.xml") };
        let units = self.navis_units();
        match std::fs::write(&path, crate::markup::navis_viewpoints_xml(&nv, &units)) {
            Ok(()) => self.toast(format!("{} clashes exported as Navisworks viewpoints: {} - Saved Viewpoints > right-click > Import Viewpoints.", nv.len(), crate::file_name(&path))),
            Err(e) => self.error(format!("Cannot write {path}: {e}")),
        }
    }

    /// Clash-urile testului curent -> BCF 2.1 (Revit, ACC, Solibri, BIMcollab, Navisworks cu BCF): un topic pe clash,
    /// cu camera si cele doua elemente selectate.
    pub fn export_clash_bcf(&mut self, path: &str) {
        let views = self.clash_views(self.cur_test);
        if views.is_empty() {
            return self.error("No open clash results in the current test - run the test first.");
        }
        let user = crate::markup::user_name();
        let now = crate::markup::now_iso();
        let items: Vec<crate::markup::ReportItem> = views
            .into_iter()
            .enumerate()
            .map(|(i, (name, status, eye, fwd, up, _, guids, desc))| crate::markup::ReportItem {
                index: i + 1,
                name: format!("Clash {name}"),
                description: desc,
                issue: crate::markup::Issue {
                    guid: crate::markup::new_guid(),
                    status: if status == "New" { "Open".into() } else { "In Progress".into() },
                    priority: "Normal".into(),
                    assigned: String::new(),
                    author: user.clone(),
                    created: now.clone(),
                },
                comments: Vec::new(),
                redlines: 0,
                eye,
                dir: fwd,
                up,
                fov_deg: self.cam.fov_deg,
                ortho_height: None,
                selected_guids: guids,
                hidden_guids: Vec::new(),
                image: Vec::new(),
                image_w: 0,
                image_h: 0,
            })
            .collect();
        let path = if path.to_lowercase().ends_with(".bcf") || path.to_lowercase().ends_with(".bcfzip") { path.to_string() } else { format!("{path}.bcf") };
        let project = self.project_path.as_ref().map(|p| crate::file_name(p)).unwrap_or_else(|| "ManageBIM clashes".into());
        match std::fs::write(&path, crate::markup::bcf_zip_version(&project, &items, self.settings.bcf_version)) {
            Ok(()) => self.toast(format!("{} clashes exported as BCF topics: {}", items.len(), crate::file_name(&path))),
            Err(e) => self.error(format!("Cannot write {path}: {e}")),
        }
    }

    pub fn clash_matrix(&self) -> ClashMatrix {
        let mut models: Vec<String> = self.loaded_models().map(|(_, m)| crate::app::model_label(&m.name)).collect();
        models.dedup();
        let slot_ix: HashMap<u32, usize> = self.loaded_models().map(|(s, m)| (s as u32, models.iter().position(|n| *n == crate::app::model_label(&m.name)).unwrap_or(0))).collect();
        let mut cells: HashMap<(usize, usize), [usize; 3]> = HashMap::new();
        for t in &self.clash_tests {
            for r in &t.results {
                let (Some(&i), Some(&j)) = (slot_ix.get(&r.a.m), slot_ix.get(&r.b.m)) else { continue };
                let k = match r.status {
                    ClashStatus::New | ClashStatus::Active => 0,
                    ClashStatus::Reviewed | ClashStatus::Approved => 1,
                    ClashStatus::Resolved => 2,
                };
                cells.entry((i.min(j), i.max(j))).or_insert([0; 3])[k] += 1;
            }
        }
        ClashMatrix { models, cells }
    }

    /// Matricea in Excel: foaia "Open" (deschise) si foaia "All" (toate), modele pe randuri si coloane.
    pub fn export_clash_matrix(&mut self, path: &str) {
        use crate::qto::{Cell, Sheet};
        let m = self.clash_matrix();
        if m.cells.is_empty() {
            return self.error("No clash results yet - run the clash tests first.");
        }
        let mut headers = vec!["Model".to_string()];
        headers.extend(m.models.iter().cloned());
        headers.push("Total".into());
        let sheet = |name: &str, f: &dyn Fn([usize; 3]) -> usize| {
            let rows: Vec<Vec<Cell>> = (0..m.models.len())
                .map(|i| {
                    let mut r = vec![Cell::S(m.models[i].clone())];
                    let mut tot = 0;
                    for j in 0..m.models.len() {
                        let v = if j < i { 0 } else { f(m.get(i, j)) };
                        tot += v;
                        r.push(if j < i { Cell::Empty } else { Cell::N(v as f64) });
                    }
                    r.push(Cell::N(tot as f64));
                    r
                })
                .collect();
            Sheet { name: name.into(), title: format!("Clash matrix - {name} clashes (all tests)"), headers: headers.clone(), widths: std::iter::once(30.0).chain(std::iter::repeat(16.0)).take(headers.len()).collect(), rows, total_from: Some(1) }
        };
        let data = crate::qto::xlsx(&[sheet("Open", &|c| c[0]), sheet("All", &|c| c[0] + c[1] + c[2])]);
        let p = with_ext(path, "xlsx");
        match std::fs::write(&p, data) {
            Ok(()) => self.toast(format!("Clash matrix saved: {}", crate::file_name(&p))),
            Err(e) => self.error(format!("Cannot write {p}: {e}")),
        }
    }
}
