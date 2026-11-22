//! Sectiune din `impl App` (vezi fisierul parinte pentru restul).
use super::*;

impl App {
    /// Unitatea in care scriem XML-ul Navisworks: din setari, sau (auto) unitatea documentului Navisworks din care
    /// vine modelul ("Source units" in .bimc-ul exportat de add-in), altfel metri.
    pub fn navis_units(&self) -> String {
        let s = self.settings.navis_units.trim().to_lowercase();
        if s != "auto" && !s.is_empty() && crate::markup::NW_UNITS.iter().any(|u| u.0 == s) {
            return s;
        }
        for (_, m) in self.loaded_models() {
            if let Some((_, v)) = m.info.iter().find(|(k, _)| k == "Source units") {
                if let Some(u) = crate::markup::nw_unit_code(v) {
                    return u.to_string();
                }
            }
        }
        "m".into()
    }

    /// Viewpoints + markups + comentarii + section box -> XML Navisworks (Viewpoints > Import Viewpoints).
    /// Nu are nevoie de capturi de ecran: se scrie direct.
    pub fn export_navis_xml(&mut self, path: &str) {
        if self.viewpoints.is_empty() {
            self.error("No viewpoints to export - save a viewpoint (with markups) first.");
            return;
        }
        let mut path = path.to_string();
        if !path.to_lowercase().ends_with(".xml") {
            path.push_str(".xml");
        }
        let bb = self.scene_aabb();
        let (smin, smax) = if bb.is_valid() { (self.world(bb.min).to_array(), self.world(bb.max).to_array()) } else { ([-1.0; 3], [1.0; 3]) };
        let aspect = if self.vp_rect[3] > 1.0 { self.vp_rect[2] / self.vp_rect[3] } else { 16.0 / 9.0 };
        let views: Vec<crate::markup::NwView> = self
            .viewpoints
            .iter()
            .enumerate()
            .filter(|(i, _)| self.vp_in_scope(*i))
            .map(|(_, v)| v)
            .map(|v| {
                let c = v.cam;
                let (near, far) = c.clip(&bb);
                let st = crate::markup::nw_status(&v.issue.status);
                let mut comments = Vec::new();
                if !v.comment.trim().is_empty() {
                    comments.push((v.issue.author.clone(), v.comment.clone(), v.issue.created.clone(), st));
                }
                let mut issue = format!("Issue: {} | Priority: {}", v.issue.status, v.issue.priority);
                if !v.issue.assigned.is_empty() {
                    issue.push_str(&format!(" | Assigned to: {}", v.issue.assigned));
                }
                comments.push((v.issue.author.clone(), issue, v.issue.created.clone(), st));
                for k in &v.comments {
                    comments.push((k.author.clone(), k.text.clone(), k.date.clone(), st));
                }
                let section_box = (v.section.enabled && v.section.mode == SectionMode::Box)
                    .then(|| (self.world(Vec3::from(v.section.bmin)).to_array(), self.world(Vec3::from(v.section.bmax)).to_array()));
                crate::markup::NwView {
                    name: v.name.clone(),
                    guid: v.issue.guid.clone(),
                    eye: self.world(c.eye()).to_array(),
                    fwd: c.fwd().as_dvec3().to_array(),
                    up: c.up().as_dvec3().to_array(),
                    fov_deg: c.fov_deg,
                    ortho_height: c.ortho.then(|| 2.0 * c.ortho_half() as f64),
                    aspect,
                    focal: c.focus as f64,
                    near: near as f64,
                    far: far as f64,
                    scene_min: smin,
                    scene_max: smax,
                    section_box,
                    comments,
                    redlines: v.redlines.clone(),
                    folder: v.folder.clone(),
                }
            })
            .collect();
        self.vp_filter = None;
        let n_red: usize = views.iter().map(|v| v.redlines.len()).sum();
        let units = self.navis_units();
        match std::fs::write(&path, crate::markup::navis_viewpoints_xml(&views, &units)) {
            Ok(()) => self.toast(format!("{} viewpoints ({n_red} markups) exported for Navisworks in {units}: {} - in Navisworks: Saved Viewpoints > right-click > Import Viewpoints.", views.len(), crate::file_name(&path))),
            Err(e) => self.error(format!("Cannot write {path}: {e}")),
        }
    }

    /// Seturile ca `<selectionsets>` Navisworks: search sets -> conditii findspec (Item / Name, Type, GUID,
    /// sau categoria / proprietatea din model), seturile explicite -> Item / GUID = ... (SAU intre ele).
    /// Intoarce (xml, avertismente).
    pub fn navis_selectionsets_xml(&self) -> (String, Vec<String>) {
        self.navis_selectionsets_xml_with(&[])
    }

    /// ca `navis_selectionsets_xml`, plus seturile `extra` (ex. cele generate din selectiile testelor de clash)
    pub fn navis_selectionsets_xml_with(&self, extra: &[SelSet]) -> (String, Vec<String>) {
        use crate::markup::esc;
        let mut warn = Vec::new();
        let cond = |cat: &str, prop: &str, test: &str, value: &str, or: bool| -> String {
            // nume interne Navisworks pentru categoria Item (restul se potrivesc dupa numele afisat)
            let (ci, pi) = match (cat, prop) {
                ("Item", "Name") => (" internal=\"LcOaNode\"", " internal=\"LcOaSceneBaseUserName\""),
                ("Item", "Type") => (" internal=\"LcOaNode\"", " internal=\"LcOaSceneBaseClassUserName\""),
                ("Item", "GUID") => (" internal=\"LcOaNode\"", " internal=\"LcOaNodeGuid\""),
                ("Item", "Layer") => (" internal=\"LcOaNode\"", " internal=\"LcOaNodeLayer\""),
                ("Item", "Source File") => (" internal=\"LcOaNode\"", " internal=\"LcOaNodeSourceFile\""),
                _ => ("", ""),
            };
            let flags = if or { 74 } else { 10 };
            let val = if test == "prop" || test == "no_prop" { String::new() } else { format!("<data type=\"wstring\">{}</data>", esc(value)) };
            format!("<condition test=\"{test}\" flags=\"{flags}\"><category><name{ci}>{}</name></category><property><name{pi}>{}</name></property><value>{val}</value></condition>", esc(cat), esc(prop))
        };
        let mut s = String::from("<selectionsets>\n");
        for set in self.sets.iter().chain(extra.iter().filter(|x| !self.sets.iter().any(|y| y.name == x.name))) {
            let mut conds = String::new();
            match &set.kind {
                SetKind::Search(rules) => {
                    for (i, r) in rules.iter().enumerate() {
                        let test = match r.op {
                            SearchOp::Equals => "equals",
                            SearchOp::NotEquals => "not_equals",
                            SearchOp::Contains => "contains",
                            SearchOp::Greater => "greater_than",
                            SearchOp::Less => "less_than",
                            SearchOp::Defined => "prop",
                            SearchOp::Undefined => "no_prop",
                            SearchOp::NotContains => {
                                warn.push(format!("{}: 'does not contain' has no Navisworks equivalent - condition skipped", set.name));
                                continue;
                            }
                        };
                        // campurile ManageBIM -> Navisworks
                        let (cat, prop) = match (r.group.as_str(), r.field.as_str()) {
                            ("", "Name") | (ITEM_CAT, "Name") => ("Item", "Name"),
                            ("", "IFC Type") | (ITEM_CAT, "IFC Type") => ("Item", "Type"),
                            ("", "GlobalId") | (ITEM_CAT, "GlobalId") => ("Item", "GUID"),
                            ("", "Level") | (ITEM_CAT, "Level") => ("Item", "Layer"),
                            ("", "Model") | (ITEM_CAT, "Model") => ("Item", "Source File"),
                            ("", "Any") => {
                                warn.push(format!("{}: 'Any property' search exported as Item / Name", set.name));
                                ("Item", "Name")
                            }
                            (g, f) => (g, f),
                        };
                        conds.push_str(&cond(cat, prop, test, &r.value, r.or && i > 0));
                    }
                }
                SetKind::Explicit(ids) => {
                    let guids: Vec<String> = ids.iter().filter_map(|id| self.elem(*id).map(|(_, e)| e.guid.clone())).filter(|g| !g.is_empty()).collect();
                    if guids.len() > 2000 {
                        warn.push(format!("{}: {} elements - only the first 2000 exported", set.name, guids.len()));
                    }
                    for (i, g) in guids.iter().take(2000).enumerate() {
                        // GUID-urile din .bimc / NWC (Navisworks) se potrivesc cu Item / GUID
                        let g = g.rsplit(':').next().unwrap_or(g);
                        conds.push_str(&cond("Item", "GUID", "equals", g, i > 0));
                    }
                }
            }
            s.push_str(&format!(
                "<selectionset name=\"{}\" guid=\"{}\"><findspec mode=\"all\" disjoint=\"0\"><conditions>{conds}</conditions><locator>/</locator></findspec></selectionset>\n",
                esc(&set.name),
                crate::markup::new_guid()
            ));
        }
        s.push_str("</selectionsets>\n");
        (s, warn)
    }

    /// Seturile de selectie -> XML Navisworks (Sets > Import Search Sets).
    pub fn export_navis_sets(&mut self, path: &str) {
        if self.sets.is_empty() {
            return self.error("No selection or search sets to export.");
        }
        let (sets, warn) = self.navis_selectionsets_xml();
        let path = if path.to_lowercase().ends_with(".xml") { path.to_string() } else { format!("{path}.xml") };
        match std::fs::write(&path, format!("{}{sets}</exchange>\n", Self::NW_HEAD)) {
            Ok(()) => {
                self.toast(format!("{} sets exported for Navisworks: {} - in Navisworks: Sets > Import Search Sets.", self.sets.len(), crate::file_name(&path)));
                for w in warn.iter().take(5) {
                    self.toast(w.clone());
                }
            }
            Err(e) => self.error(format!("Cannot write {path}: {e}")),
        }
    }

    /// Export IFC4: selectia, sau (fara selectie) toate elementele vizibile din proiect.
    pub fn export_ifc(&mut self, path: &str) {
        let path = with_ext(path, "ifc");
        let models: Vec<(usize, Arc<Model>)> = self.loaded_models().map(|(i, m)| (i, m.clone())).collect();
        let use_sel = self.ifc_selection && !self.selected.is_empty();
        let mut items: Vec<(&Model, &crate::model::Element)> = Vec::new();
        for (mi, m) in &models {
            for (ei, e) in m.elements.iter().enumerate() {
                let id = ElemId { m: *mi as u32, e: ei as u32 };
                let take = if use_sel { self.selected.contains(&id) } else { !self.hidden.contains(&id) && !e.default_hidden };
                if take && e.tri_count > 0 {
                    items.push((m.as_ref(), e));
                }
            }
        }
        if items.is_empty() {
            return self.error("Nothing to export: no visible items with geometry.");
        }
        let project = std::path::Path::new(&path).file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "ManageBIM".into());
        let (text, st) = crate::ifc_export::write_ifc(&items, self.origin_f64(), &project, &now_string());
        match std::fs::write(&path, text) {
            Ok(()) => self.toast(format!(
                "IFC4 exported: {} items ({} closed solids, {} reused geometries), {} properties → {}",
                st.elements,
                st.closed,
                st.instances,
                st.properties,
                crate::file_name(&path)
            )),
            Err(e) => self.error(format!("Cannot write {path}: {e}")),
        }
    }

    /// Issue-uri BCF (2.1 / 3.0) din Revit, ACC, Solibri, BIMcollab... -> viewpoints ManageBIM: camera, titlu,
    /// stare, prioritate, responsabil, descriere, comentarii si elementele selectate (dupa GUID).
    pub fn import_bcf(&mut self, path: &str) {
        let topics = match std::fs::read(path).map_err(|e| e.to_string()).and_then(crate::markup::parse_bcf) {
            Ok(t) => t,
            Err(e) => return self.error(format!("{}: {e}", crate::file_name(path))),
        };
        // GUID -> element (GlobalId IFC, Revit UniqueId, GUID Navisworks...; si dupa partea de dupa ':')
        let mut by_guid: HashMap<String, ElemId> = HashMap::new();
        for (mi, m) in self.loaded_models() {
            for (ei, e) in m.elements.iter().enumerate() {
                if !e.guid.is_empty() {
                    let id = ElemId { m: mi as u32, e: ei as u32 };
                    by_guid.insert(e.guid.clone(), id);
                    if let Some(tail) = e.guid.rsplit(':').next() {
                        by_guid.entry(tail.to_string()).or_insert(id);
                    }
                }
            }
        }
        let origin = self.origin_f64();
        let (mut n, mut found, mut wanted) = (0, 0, 0);
        for t in topics {
            let mut cam = self.cam;
            if let Some((e, d, _u, fov, h)) = t.camera {
                let f = DVec3::from_array(d).normalize_or(DVec3::X);
                cam.eye = (DVec3::from_array(e) - origin).as_vec3().into();
                cam.yaw = f.y.atan2(f.x) as f32;
                cam.pitch = (f.z.clamp(-1.0, 1.0).asin() as f32).clamp(-1.55, 1.55);
                match fov {
                    Some(fd) => {
                        cam.ortho = false;
                        cam.fov_deg = fd.clamp(10.0, 120.0);
                        cam.focus = 10.0;
                    }
                    None => {
                        cam.ortho = true;
                        cam.focus = (h as f32 * 0.5 / (cam.fov_deg.to_radians() * 0.5).tan()).max(0.05);
                    }
                }
            }
            wanted += t.selected.len();
            let selected: Vec<ElemId> = t.selected.iter().filter_map(|g| by_guid.get(g).copied()).collect();
            found += selected.len();
            let mut issue = crate::markup::Issue::default();
            issue.guid = t.guid.clone();
            if !t.status.is_empty() {
                issue.status = t.status.clone();
            }
            if !t.priority.is_empty() {
                issue.priority = t.priority.clone();
            }
            issue.assigned = t.assigned.clone();
            if !t.author.is_empty() {
                issue.author = t.author.clone();
            }
            if !t.created.is_empty() {
                issue.created = t.created.clone();
            }
            self.viewpoints.push(Viewpoint {
                name: if t.title.is_empty() { format!("BCF {}", n + 1) } else { t.title },
                cam,
                section: Section::default(),
                cad_view: crate::app::CadView::Current,
                cad_view_pos: 0.0,
                hidden: Vec::new(),
                comment: t.description,
                redlines: Vec::new(),
                issue,
                comments: t.comments,
                selected,
                measures: Vec::new(),
                folder: "BCF".into(),
            });
            n += 1;
        }
        self.open_panel(crate::layout::Panel::Viewpoints);
        let sel = if wanted > 0 { format!(", {found} of {wanted} referenced elements found in the open models") } else { String::new() };
        self.toast(format!("{n} BCF issues imported from {}{sel}.", crate::file_name(path)));
    }

    /// Viewpoints dintr-un XML Navisworks (Saved Viewpoints > Export Viewpoints, sau exportul ManageBIM):
    /// camera, section box, comentarii si markups se adauga la viewpoints-urile existente.
    pub fn import_navis_xml(&mut self, path: &str) {
        let text = match std::fs::read(path) {
            Ok(b) => String::from_utf8_lossy(&b).into_owned(),
            Err(e) => return self.error(format!("Cannot read {path}: {e}")),
        };
        let aspect = if self.vp_rect[3] > 1.0 { self.vp_rect[2] / self.vp_rect[3] } else { 16.0 / 9.0 };
        let views = match crate::markup::parse_navis_viewpoints(&text, aspect) {
            Ok(v) => v,
            Err(e) => return self.error(format!("{}: {e}", crate::file_name(path))),
        };
        let origin = self.origin_f64();
        let local = |p: [f64; 3]| (DVec3::from_array(p) - origin).as_vec3();
        let (mut n, mut nred) = (0, 0);
        for v in views {
            let f = DVec3::from_array(v.fwd).normalize_or(DVec3::X);
            let mut cam = self.cam;
            cam.eye = local(v.eye).into();
            cam.yaw = f.y.atan2(f.x) as f32;
            cam.pitch = (f.z.clamp(-1.0, 1.0).asin() as f32).clamp(-1.55, 1.55);
            cam.fov_deg = v.fov_deg.clamp(10.0, 120.0);
            cam.ortho = v.ortho_height.is_some();
            cam.focus = match v.ortho_height {
                // ortho: inaltimea vederii = 2 * focus * tan(fov / 2)
                Some(h) => (h as f32 * 0.5 / (cam.fov_deg.to_radians() * 0.5).tan()).max(0.05),
                None => (v.focal as f32).max(0.05),
            };
            let mut section = Section::default();
            if let Some((mn, mx)) = v.section_box {
                let (a, b) = (local(mn), local(mx));
                section.enabled = true;
                section.mode = SectionMode::Box;
                section.bmin = a.min(b).to_array();
                section.bmax = a.max(b).to_array();
            }
            let mut issue = crate::markup::Issue::default();
            let mut comment = String::new();
            let mut comments = Vec::new();
            for (user, body, date, st) in &v.comments {
                if let Some(rest) = body.strip_prefix("Issue: ") {
                    // comentariul de stare scris de exportul ManageBIM
                    for part in rest.split(['|', '·']).map(str::trim) {
                        if let Some(p) = part.strip_prefix("Priority: ") {
                            issue.priority = p.to_string();
                        } else if let Some(a) = part.strip_prefix("Assigned to: ") {
                            issue.assigned = a.to_string();
                        } else if !part.contains(':') {
                            issue.status = part.to_string();
                        }
                    }
                    continue;
                }
                if comments.is_empty() && comment.is_empty() {
                    issue.status = match *st {
                        "active" => "In Progress",
                        "resolved" => "Resolved",
                        "approved" => "Closed",
                        _ => "Open",
                    }
                    .into();
                    if !user.is_empty() {
                        issue.author = user.clone();
                    }
                    issue.created = date.clone();
                }
                comments.push(crate::markup::VpComment { author: user.clone(), date: date.clone(), text: body.clone() });
            }
            if let Some(first) = comments.first() {
                if comments.len() == 1 {
                    comment = first.text.clone();
                    comments.clear();
                }
            }
            if !v.guid.is_empty() {
                issue.guid = v.guid.clone();
            }
            nred += v.redlines.len();
            self.viewpoints.push(Viewpoint { name: v.name, cam, section, cad_view: crate::app::CadView::Current, cad_view_pos: 0.0, hidden: Vec::new(), comment, redlines: v.redlines, issue, comments, selected: Vec::new(), measures: Vec::new(), folder: v.folder });
            n += 1;
        }
        self.open_panel(crate::layout::Panel::Viewpoints);
        let warn = if self.loaded_models().next().is_none() { " Open the model first so the cameras land in the right place." } else { "" };
        self.toast(format!("{n} viewpoints imported from {} ({nred} markups).{warn}", crate::file_name(path)));
    }

    /// Writes a loaded model to the native .bimc format.
    pub fn export_cache(&mut self, slot: usize, path: &str) {
        let Some(m) = self.models.get(slot).and_then(|m| m.clone()) else { return };
        let mut path = path.to_string();
        if !path.to_lowercase().ends_with(".bimc") {
            path.push_str(".bimc");
        }
        let origin = self.origin.lock().unwrap_or_else(|e| e.into_inner()).unwrap_or(DVec3::ZERO);
        let raw = crate::bimc::from_model(&m, origin);
        match crate::bimc::write(&raw, &path) {
            Ok(n) => {
                let src = std::fs::metadata(&m.path).map(|x| x.len()).unwrap_or(0);
                let ratio = if src > 0 { format!(", {:.1}x smaller than the source", src as f64 / n as f64) } else { String::new() };
                self.toast(format!("Exported {} ({:.2} MB{ratio})", crate::file_name(&path), n as f64 / 1e6));
            }
            Err(e) => self.error(e),
        }
    }

    /// Exports every loaded model as `<source>.bimc` next to its source file.
    pub fn export_all_caches(&mut self) {
        let slots: Vec<(usize, String)> = self.loaded_models().filter(|(_, m)| !m.path.to_lowercase().ends_with(".bimc")).map(|(i, m)| (i, crate::bimc::cache_path(&m.path))).collect();
        if slots.is_empty() {
            self.error("No IFC / glTF models loaded.");
        }
        for (i, p) in slots {
            self.export_cache(i, &p);
        }
    }

    pub fn import_tasks_csv(&mut self, path: &str) {
        match std::fs::read(path) {
            Ok(b) => {
                let text = String::from_utf8_lossy(&b).to_string();
                // MS Project / Primavera P6 (XML) sau CSV
                let (mut tasks, errs, what) = if text.trim_start_matches('\u{feff}').trim_start().starts_with('<') {
                    match timeline::parse_schedule_xml(&text) {
                        Ok(r) => r,
                        Err(e) => return self.error(e),
                    }
                } else {
                    let (t, e) = timeline::parse_csv(&text);
                    (t, e, "CSV")
                };
                let n = tasks.len();
                for t in tasks.iter_mut() {
                    t.elems = self.rule_elems(&t.rule);
                }
                let tasks_linked = tasks.iter().filter(|t| !t.elems.is_empty()).count();
                self.tasks.extend(tasks);
                if let Some((s, _)) = self.sim_range() {
                    self.sim_day = s as f32;
                }
                self.invalidate_sim();
                let linked = tasks_linked;
                self.toast(format!("Imported {n} tasks from {what} ({linked} linked to elements)."));
                for e in errs {
                    self.log.push(e);
                }
            }
            Err(e) => self.error(format!("Cannot read the schedule: {e}")),
        }
    }

    pub fn export_tasks_csv(&self) -> String {
        let mut s = String::from("Name;Start;End;Type;Rule\n");
        for t in &self.tasks {
            s.push_str(&format!("{};{};{};{};{}\n", t.name, timeline::fmt_date(t.start), timeline::fmt_date(t.end), t.kind.label(), t.rule));
        }
        s
    }

    /// Viewer web: un singur .html cu modelul (elementele vizibile, sau selectia) - se trimite oricui,
    /// se deschide in browser fara instalare si fara internet.
    pub fn export_web(&mut self, path: &str) {
        let path = with_ext(path, "html");
        let models: Vec<(usize, Arc<Model>)> = self.loaded_models().map(|(i, m)| (i, m.clone())).collect();
        let use_sel = self.web_selection && !self.selected.is_empty();
        let mut items: Vec<(&Model, &crate::model::Element)> = Vec::new();
        for (mi, m) in &models {
            for (ei, e) in m.elements.iter().enumerate() {
                let id = ElemId { m: *mi as u32, e: ei as u32 };
                let take = if use_sel { self.selected.contains(&id) } else { !self.hidden.contains(&id) && !e.default_hidden };
                if take && e.tri_count > 0 {
                    items.push((m.as_ref(), e));
                }
            }
        }
        if items.is_empty() {
            return self.error("Nothing to export: no visible items with geometry.");
        }
        let title = self.project_path.as_ref().map(|p| model_label(&crate::file_name(p))).or_else(|| models.first().map(|(_, m)| model_label(&m.name))).unwrap_or_else(|| "ManageBIM model".into());
        let (html, st) = crate::web::html(&items, &title, &now_string());
        match std::fs::write(&path, html) {
            Ok(()) => {
                self.toast(format!("Web viewer saved: {} ({} items, {} triangles, {:.1} MB) - open it in any browser", crate::file_name(&path), st.elements, st.triangles, st.bytes as f64 / 1e6));
                crate::formats::open_with_system(std::path::Path::new(&path));
            }
            Err(e) => self.error(format!("Cannot write {path}: {e}")),
        }
    }

    /// Referinta pentru Revit / AutoCAD / Plant 3D: selectia, sau tot ce e vizibil.
    pub fn export_reference(&mut self, path: &str) {
        let path = with_ext(path, "mbref");
        let use_sel = !self.selected.is_empty();
        let models: Vec<(usize, Arc<Model>)> = self.loaded_models().map(|(i, m)| (i, m.clone())).collect();
        let mut items: Vec<(&Model, &crate::model::Element)> = Vec::new();
        for (mi, m) in &models {
            for (ei, e) in m.elements.iter().enumerate() {
                let id = ElemId { m: *mi as u32, e: ei as u32 };
                let take = if use_sel { self.selected.contains(&id) } else { !self.hidden.contains(&id) && !e.default_hidden };
                if take && e.tri_count > 0 {
                    items.push((m.as_ref(), e));
                }
            }
        }
        if items.is_empty() {
            return self.error("Nothing to export: no visible items with geometry.");
        }
        let n = items.len();
        let data = write_mbref(&items, self.origin_f64());
        match std::fs::write(&path, &data) {
            Ok(()) => self.toast(format!("Reference saved: {} ({n} items, {:.1} MB). Revit: ManageBIM › Reference; AutoCAD / Plant 3D: command MBREFERENCE.", crate::file_name(&path), data.len() as f64 / 1e6)),
            Err(e) => self.error(format!("Cannot write {path}: {e}")),
        }
    }
}
