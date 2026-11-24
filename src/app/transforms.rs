//! Sectiune din `impl App` (vezi fisierul parinte pentru restul).
use super::*;

impl App {
    pub fn origin_f64(&self) -> DVec3 {
        self.origin.lock().unwrap_or_else(|e| e.into_inner()).unwrap_or(DVec3::ZERO)
    }

    /// local (render) coordinates -> real world coordinates
    pub fn world(&self, p: Vec3) -> DVec3 {
        p.as_dvec3() + self.origin_f64()
    }

    pub fn xf_matrix(&self, base: &Model, xf: &ModelXf) -> Mat4 {
        let pivot = if xf.pivot_origin { (-self.origin_f64()).as_vec3() } else { base.aabb.center() };
        let t = Vec3::new(xf.translate[0] as f32, xf.translate[1] as f32, xf.translate[2] as f32);
        Mat4::from_translation(pivot + t) * Mat4::from_rotation_z((xf.rot_z as f32).to_radians()) * Mat4::from_scale(Vec3::splat(xf.scale.max(1e-6) as f32)) * Mat4::from_translation(-pivot)
    }

    /// Rebuilds a model from its loaded geometry + model transform + element offsets.
    pub fn apply_transform(&mut self, slot: usize) {
        let Some(base) = self.base_models.get(slot).cloned().flatten() else { return };
        let xf = self.model_xf.get(slot).copied().unwrap_or_default();
        let offs: HashMap<u32, Vec3> = self.elem_offsets.iter().filter(|(k, _)| k.m as usize == slot).map(|(k, v)| (k.e, Vec3::from(*v))).collect();
        let scales: HashMap<u32, Vec3> = self.elem_scale.iter().filter(|(k, _)| k.m as usize == slot).map(|(k, v)| (k.e, Vec3::from(*v))).collect();
        let m = if xf.is_identity() && offs.is_empty() && scales.is_empty() { base } else { Arc::new(base.transformed(self.xf_matrix(&base, &xf), &offs, &scales)) };
        let (pos, nrm) = render::mesh_data(&m);
        let col = vec![200u8; pos.len() / 3 * 4];
        {
            let mut r = self.renderer.lock().unwrap_or_else(|e| e.into_inner());
            r.pending.push(Upload::Mesh { slot: slot as u32, pos, nrm, col });
            r.pending.push(Upload::Points { slot: slot as u32, cloud: m.cloud.clone() });
        }
        // bufferul nou de pe GPU are culori implicite -> la urmatorul rebuild_colors trimitem tot
        self.looks.remove(&slot);
        if slot < self.models.len() {
            self.models[slot] = Some(m);
        }
        self.min_dist_result = None;
        self.colors_dirty = true;
    }

    pub fn set_model_xf(&mut self, slot: usize, xf: ModelXf) {
        while self.model_xf.len() <= slot {
            self.model_xf.push(ModelXf::default());
        }
        self.model_xf[slot] = xf;
        self.apply_transform(slot);
    }

    pub fn move_selected(&mut self, d: Vec3) {
        let mut slots = BTreeSet::new();
        for id in self.selected.clone() {
            let o = self.elem_offsets.entry(id).or_insert([0.0; 3]);
            o[0] += d.x;
            o[1] += d.y;
            o[2] += d.z;
            if o.iter().all(|v| v.abs() < 1e-7) {
                self.elem_offsets.remove(&id);
            }
            slots.insert(id.m as usize);
        }
        for s in slots {
            self.apply_transform(s);
        }
    }

    /// Scale per-element pe axe (stil SketchUp) - non-distructiv, în jurul centrului AABB.
    pub fn scale_selected(&mut self, s: [f32; 3]) {
        let mut slots = BTreeSet::new();
        for id in self.selected.clone() {
            let e = self.elem_scale.entry(id).or_insert([1.0; 3]);
            e[0] = (e[0] * s[0]).max(0.02);
            e[1] = (e[1] * s[1]).max(0.02);
            e[2] = (e[2] * s[2]).max(0.02);
            slots.insert(id.m as usize);
        }
        for sl in slots { self.apply_transform(sl); }
    }

    /// Resetare scale pentru elementele date.
    pub fn reset_scale(&mut self, ids: &[ElemId]) {
        let mut slots = BTreeSet::new();
        for id in ids {
            if self.elem_scale.remove(id).is_some() { slots.insert(id.m as usize); }
        }
        for sl in slots { self.apply_transform(sl); }
    }

    pub fn reset_items(&mut self, ids: &[ElemId]) {
        let mut slots = BTreeSet::new();
        for id in ids {
            if self.elem_offsets.remove(id).is_some() {
                slots.insert(id.m as usize);
            }
        }
        for s in slots {
            self.apply_transform(s);
        }
    }

    pub fn reset_all_transforms(&mut self) {
        self.elem_offsets.clear();
        self.elem_scale.clear();
        for x in self.model_xf.iter_mut() {
            *x = ModelXf::default();
        }
        self.xf_draft.clear();
        for i in 0..self.models.len() {
            self.apply_transform(i);
        }
    }

    /// Fisierul ales intr-un dialog: exporturile / importurile ruleaza protejat - o eroare interna devine mesaj,
    /// nu inchide aplicatia (proiectul ramane deschis).
    pub fn handle_path(&mut self, purpose: Purpose, path: String) {
        let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| self.handle_path_inner(purpose, path)));
        if let Err(e) = r {
            self.error(format!("Internal error ({}) - the operation was stopped, the project is unchanged on disk.", panic_text(&e)));
        }
    }

    pub(super) fn handle_path_inner(&mut self, purpose: Purpose, path: String) {
        let path = path.trim().trim_matches('"').to_string();
        if path.is_empty() {
            return;
        }
        let write = |s: &mut App, content: String, what: &str| match std::fs::write(&path, content) {
            Ok(()) => s.toast(format!("{what} saved: {}", crate::file_name(&path))),
            Err(e) => s.error(format!("Cannot write the file: {e}")),
        };
        match purpose {
            Purpose::OpenModel { append } => self.open_path(&path, append),
            Purpose::OpenProject => self.open_path(&path, false),
            Purpose::PublishBimd => match self.publish_bimd(&path) {
                Ok(m) => {
                    self.bimd_path = Some(with_ext(&path, "bimd"));
                    self.toast(m);
                }
                Err(e) => self.error(e),
            },
            Purpose::SaveProject => self.save_project(&path),
            Purpose::ExportClashHtml => {
                let c = self.clash_report_html(self.cur_test);
                write(self, c, "HTML report");
            }
            Purpose::ExportClashPdf => {
                let rows = self.clash_report_pdf_rows(self.cur_test);
                let test = self.clash_tests.get(self.cur_test).map(|t| t.name.clone()).unwrap_or_default();
                let pdf = crate::pdf::clash_report_pdf(&self.project_path.clone().unwrap_or_default(), &test, &rows, &super::now_string(), &self.title_block);
                match std::fs::write(&path, pdf) {
                    Ok(()) => self.toast(format!("Clash report PDF exported: {}", crate::file_name(&path))),
                    Err(e) => self.error(format!("Cannot write PDF: {e}")),
                }
            }
            Purpose::ExportClashCsv => {
                let c = self.clash_report_csv(self.cur_test);
                write(self, c, "CSV report");
            }
            Purpose::ExportClashBcf => self.export_clash_bcf(&path),
            Purpose::ExportIfc => self.export_ifc(&path),
            Purpose::ExportGlb => self.export_glb(&path),
            Purpose::ExportGrids => self.export_grids(&path),
            Purpose::ImportGridsCad => self.import_grids_cad(&path, None),
            Purpose::ExportReference => self.export_reference(&path),
            Purpose::ExportClashMatrix => self.export_test_matrix(&path),
            Purpose::ImportClashTests => self.import_clash_tests(&path),
            Purpose::ImportUserProps => self.open_prop_import(&path),
            Purpose::ExportUserProps => self.export_user_props(&path),
            Purpose::ExportClashTestsCsv => self.export_clash_tests_csv(&path),
            Purpose::ExportWeb => self.export_web(&path),
            Purpose::ExportAsBuilt => self.export_asbuilt_xlsx(&path),
            Purpose::ExportSectionCad | Purpose::ExportSectionPdf => self.export_section_cad(&path),
            Purpose::ExportCobie => match std::fs::write(&path, crate::cobie::workbook(self)) {
                Ok(()) => self.toast(format!("COBie workbook saved: {}", crate::file_name(&path))),
                Err(e) => self.error(format!("Cannot write {path}: {e}")),
            },
            Purpose::ImportOverlay => self.import_overlay(&path),
            Purpose::ImportCheckRules => self.import_check_rules(&path),
            Purpose::ExportCheckRules => self.export_check_rules(&path),
            Purpose::ExportIds => self.export_ids(&path),
            Purpose::ExportCheckXlsx => self.export_check_xlsx(&path),
            Purpose::ExportCheckBcf => self.export_check_bcf(&path),
            Purpose::SelectScriptsDir => {
                self.settings.scripts_dir = path.clone();
                self.settings.save();
                self.script_open = true;
                self.toast(format!("Scripts folder: {path}"));
            }
            Purpose::SharedFolder => {
                self.shared_dir = Some(path.clone());
                self.shared = Default::default();
                self.shared_sync(true);
                self.toast(format!("Issues are shared in {path}. Colleagues: open the same models and choose the same folder."));
            }
            Purpose::ExportClashNavis => self.export_clash_navis(&path),
            Purpose::ImportBcf => self.import_bcf(&path),
            Purpose::ImportTasksCsv => {
                self.import_tasks_csv(&path);
                if std::mem::take(&mut self.anim_4d_pending) && !self.tasks.is_empty() {
                    self.import_4d_animation();
                }
            }
            Purpose::ExportTasksCsv => {
                let c = self.export_tasks_csv();
                write(self, c, "Schedule CSV");
            }
            Purpose::ExportCache(slot) => self.export_cache(slot, &path),
            Purpose::ExportVideo => self.start_render_job(&path),
            Purpose::ImportTextures => self.import_texture_materials(std::slice::from_ref(&path)),
            Purpose::ImportPbr | Purpose::ImportPbrFolder => self.import_pbr_packs(std::slice::from_ref(&path)),
            Purpose::ExportVpHtml => self.start_report(0, &path),
            Purpose::ExportVpXlsx => self.start_report(1, &path),
            Purpose::ExportVpPdf => self.start_report(3, &path),
            Purpose::ExportBatch => {
                let dir = std::path::PathBuf::from(&path);
                let stem = "ManageBIM-export";
                self.batch_queue = vec![
                    (3, dir.join(format!("{stem}-sheets.pdf")).to_string_lossy().to_string()),
                    (0, dir.join(format!("{stem}-report.html")).to_string_lossy().to_string()),
                    (1, dir.join(format!("{stem}-report.xlsx")).to_string_lossy().to_string()),
                    (2, dir.join(format!("{stem}-issues.bcf")).to_string_lossy().to_string()),
                ];
                if !self.batch_queue.is_empty() {
                    let (kind, p) = self.batch_queue.remove(0);
                    self.start_report(kind, &p);
                }
                self.toast("Batch export started (PDF, HTML, Excel, BCF)…");
            }
            Purpose::ExportBcf => self.start_report(2, &path),
            Purpose::ExportSettingsProfile => {
                let p = std::path::Path::new(&path);
                match self.settings.export_profile(p) {
                    Ok(()) => self.toast(format!("Settings profile saved: {}", crate::file_name(&path))),
                    Err(e) => self.error(format!("Cannot save profile: {e}")),
                }
            }
            Purpose::ImportSettingsProfile => {
                match crate::theme::Settings::import_profile(std::path::Path::new(&path)) {
                    Ok(s) => {
                        crate::app::set_unit(match s.unit_system.as_str() { "mm" => crate::app::Unit::Mm, "in" => crate::app::Unit::In, "ft" => crate::app::Unit::Ft, _ => crate::app::Unit::M });
                        crate::app::set_precision(s.unit_precision);
                        self.settings = s;
                        self.toast(format!("Settings loaded from {}", crate::file_name(&path)));
                    }
                    Err(e) => self.error(format!("Cannot load profile: {e}")),
                }
            }
            Purpose::ExportNavisXml => self.export_navis_xml(&path),
            Purpose::ImportNavisXml => self.import_navis_xml(&path),
            Purpose::ExportNavisSets => self.export_navis_sets(&path),
            Purpose::ExportNavisClash => self.export_navis_clash(&path),
            Purpose::MaterialNormal | Purpose::MaterialRoughness | Purpose::MaterialMetallic | Purpose::MaterialAo | Purpose::MaterialHeight => {
                if crate::materials::load_texture(&path).is_ok() {
                    if let Some(m) = self.materials.get_mut(self.mat_sel) {
                        match purpose {
                            Purpose::MaterialNormal => m.normal_map = path.clone(),
                            Purpose::MaterialRoughness => m.roughness_map = path.clone(),
                            Purpose::MaterialMetallic => m.metallic_map = path.clone(),
                            Purpose::MaterialAo => m.ao_map = path.clone(),
                            _ => m.height_map = path.clone(),
                        }
                    }
                    self.pbr_layers.clear();
                    self.materials_changed();
                } else {
                    self.error("The file could not be read as a texture (PNG / JPG only).");
                }
            }
            Purpose::MaterialTexture => {
                if crate::materials::load_texture(&path).is_ok() {
                    if let Some(m) = self.materials.get_mut(self.mat_sel) {
                        m.texture = path.clone();
                        m.color = [255, 255, 255];
                    }
                    self.tex_failed.remove(&path);
                    self.pbr_layers.clear();
                    self.materials_changed();
                } else {
                    self.error("Choose a PNG or JPG image.");
                }
            }
            Purpose::ExportMaterials if path.to_lowercase().ends_with(".csv") || path.to_lowercase().ends_with(".xlsx") => {
                let res = if path.to_lowercase().ends_with(".csv") { std::fs::write(&path, crate::tables::materials_csv(&self.materials)) } else { std::fs::write(&path, crate::tables::materials_xlsx(&self.materials)) };
                match res {
                    Ok(()) => self.toast(format!("{} materials exported to {}", self.materials.len(), crate::file_name(&path))),
                    Err(e) => self.error(format!("Cannot write the file: {e}")),
                }
            }
            Purpose::ImportMaterials if path.to_lowercase().ends_with(".csv") || path.to_lowercase().ends_with(".xlsx") => match crate::tables::read_rows(&path).and_then(|r| crate::tables::materials_from_rows(&r)) {
                Ok(v) => {
                    let n = v.len();
                    for m in v {
                        if let Some(x) = self.materials.iter_mut().find(|x| x.name == m.name) {
                            *x = m;
                        } else {
                            self.materials.push(m);
                        }
                    }
                    self.materials_changed();
                    self.toast(format!("Imported {n} materials from {}.", crate::file_name(&path)));
                }
                Err(e) => self.error(format!("Material table: {e}")),
            },
            Purpose::ExportMaterialX => match crate::materialx::write(self.materials.clone(), self.selected.iter().copied().collect(), &path) {
                Ok(n) => self.toast(format!("MaterialX exported: {n} materials → {}", crate::file_name(&path))),
                Err(e) => self.error(e),
            },
            Purpose::ImportMaterialX => match crate::materialx::read(&path) {
                Ok(v) => {
                    let mut n = 0;
                    for m in v {
                        if let Some(x) = self.materials.iter_mut().find(|x| x.name == m.name) {
                            *x = m;
                        } else {
                            self.materials.push(m);
                            n += 1;
                        }
                    }
                    self.pending_mat_assign.clear();
                    self.materials_changed();
                    self.toast(format!("MaterialX: {} materials imported ({} new).", crate::file_name(&path), n));
                }
                Err(e) => self.error(e),
            },
            Purpose::ExportMaterials => {
                let mut p = path.clone();
                if !p.to_lowercase().ends_with(".bimmat") {
                    p.push_str(".bimmat");
                }
                match serde_json::to_string_pretty(&self.materials).map_err(|e| e.to_string()).and_then(|j| std::fs::write(&p, j).map_err(|e| e.to_string())) {
                    Ok(()) => self.toast(format!("{} materials exported to {}", self.materials.len(), crate::file_name(&p))),
                    Err(e) => self.error(e),
                }
            }
            Purpose::ImportMaterials => match std::fs::read_to_string(&path).map_err(|e| e.to_string()).and_then(|t| serde_json::from_str::<Vec<Material>>(&t).map_err(|e| e.to_string())) {
                Ok(v) => {
                    let n = v.len();
                    for m in v {
                        if let Some(x) = self.materials.iter_mut().find(|x| x.name == m.name) {
                            *x = m;
                        } else {
                            self.materials.push(m);
                        }
                    }
                    self.materials_changed();
                    self.toast(format!("Imported {n} materials."));
                }
                Err(e) => self.error(format!("Invalid material library: {e}")),
            },
            Purpose::RenderImage => {
                let mut p = path.clone();
                if !p.to_lowercase().ends_with(".png") {
                    p.push_str(".png");
                }
                self.still_path = Some(p);
                // ca la video: fara nuanta de selectie; „Render with materials” = stilul Realistic cu materiale
                if self.still_restore.is_none() {
                    let prev = (self.settings.visual_style, self.materials_on, std::mem::take(&mut self.selected));
                    if self.render.with_materials {
                        self.settings.visual_style = crate::render::VisualStyle::Realistic;
                        if self.mat_assign.is_empty() && self.loaded_models().next().is_some() {
                            self.auto_materials();
                        }
                        self.materials_on = true;
                        self.materials_changed();
                    }
                    self.still_restore = Some(prev);
                    self.colors_dirty = true;
                }
                // doua cadre: culorile noi se trimit la GPU inainte de captura
                self.still_wait = 2;
                let ss = if self.render.supersample { 2 } else { 1 };
                self.pending_capture = Some((self.render.width * ss, self.render.height * ss));
            }
            Purpose::ExportPathTrace => {
                let p = if path.to_lowercase().ends_with(".png") { path.clone() } else { format!("{path}.png") };
                self.export_pathtrace(&p)
            }
            Purpose::ExportSets => {
                let mut path = path.clone();
                if !path.to_lowercase().ends_with(".bimsets") {
                    path.push_str(".bimsets");
                }
                match serde_json::to_string_pretty(&self.sets) {
                    Ok(j) => match std::fs::write(&path, j) {
                        Ok(()) => self.toast(format!("{} sets exported to {}", self.sets.len(), crate::file_name(&path))),
                        Err(e) => self.error(format!("Cannot write the file: {e}")),
                    },
                    Err(e) => self.error(e.to_string()),
                }
            }
            Purpose::ImportSets => match std::fs::read_to_string(&path).map_err(|e| e.to_string()).and_then(|t| serde_json::from_str::<Vec<SelSet>>(&t).map_err(|e| e.to_string())) {
                Ok(v) => {
                    let n = v.len();
                    for mut s in v {
                        while self.sets.iter().any(|x| x.name == s.name) {
                            s.name.push_str(" (2)");
                        }
                        self.sets.push(s);
                    }
                    self.toast(format!("Imported {n} sets."));
                }
                Err(e) => self.error(format!("Invalid sets file: {e}")),
            },
            Purpose::ExportProfile if path.to_lowercase().ends_with(".csv") || path.to_lowercase().ends_with(".xlsx") => {
                let res = if path.to_lowercase().ends_with(".csv") { std::fs::write(&path, crate::tables::profile_csv(&self.profile)) } else { std::fs::write(&path, crate::tables::profile_xlsx(&self.profile)) };
                match res {
                    Ok(()) => self.toast(format!("Colour profile exported ({} colours): {}", self.profile.rules.len(), crate::file_name(&path))),
                    Err(e) => self.error(format!("Cannot write the file: {e}")),
                }
            }
            Purpose::ExportProfile => {
                let p = with_ext(&path, "bimprofile");
                let j = self.profile.to_file_json();
                match std::fs::write(&p, j) {
                    Ok(()) => self.toast(format!("Profile exported ({} colours): {}", self.profile.rules.len(), crate::file_name(&p))),
                    Err(e) => self.error(format!("Cannot write the file: {e}")),
                }
            }
            Purpose::ImportProfile => match if path.to_lowercase().ends_with(".csv") || path.to_lowercase().ends_with(".xlsx") {
                crate::tables::read_rows(&path).and_then(|r| crate::tables::profile_from_rows(&r))
            } else {
                std::fs::read_to_string(&path).map_err(|e| e.to_string()).and_then(|t| crate::profiler::Profile::from_file_json(&t))
            } {
                Ok(p) => {
                    self.profile = p;
                    self.profile.active = true;
                    self.profile_recount();
                    let matched: u32 = self.profile.rules.iter().map(|r| r.count).sum();
                    let unc = self.profile_uncovered();
                    let extra = if unc > 0 { format!(" {unc} values in these models have no colour yet - use \"Add missing values\".") } else { String::new() };
                    self.toast(format!("Profile imported: {} / {}, {} colours, {matched} items coloured.{extra}", self.profile.cat, self.profile.prop, self.profile.rules.len()));
                    self.open_panel(Panel::Profiler);
                }
                Err(e) => self.error(e),
            },
            Purpose::ExportQuickChecksCsv => {
                let p = with_ext(&path, "csv");
                match std::fs::write(&p, self.quick_checks_csv()) {
                    Ok(()) => self.toast(format!("Quick checks report saved: {}", crate::file_name(&p))),
                    Err(e) => self.error(format!("Cannot write the file: {e}")),
                }
            }
            Purpose::ExportCompareParamsCsv => {
                let Some(r) = &self.compare else { return self.error("Run Compare first.") };
                let Some(mb) = self.models.get(r.b).cloned().flatten() else { return self.error("Compare: the models are no longer loaded.") };
                let p = with_ext(&path, "csv");
                let n = self.compare_params.len();
                match std::fs::write(&p, crate::compare::param_csv(&self.compare_params, &mb)) {
                    Ok(()) => self.toast(format!("Parameter changes saved ({n} rows): {}", crate::file_name(&p))),
                    Err(e) => self.error(format!("Cannot write the file: {e}")),
                }
            }
            Purpose::ExportCompare | Purpose::ExportCompareCsv => {
                let Some(r) = &self.compare else { return self.error("Run Compare first.") };
                let (Some(ma), Some(mb)) = (self.models.get(r.a).cloned().flatten(), self.models.get(r.b).cloned().flatten()) else { return self.error("Compare: the models are no longer loaded.") };
                let (p, text, what) = if purpose == Purpose::ExportCompare {
                    (with_ext(&path, "bimcmp"), serde_json::to_string_pretty(&r.to_file(&ma, &mb, now_string())).unwrap_or_default(), "Comparison")
                } else {
                    (with_ext(&path, "csv"), r.to_csv(&ma, &mb), "Comparison report")
                };
                match std::fs::write(&p, text) {
                    Ok(()) => self.toast(format!("{what} saved: {}", crate::file_name(&p))),
                    Err(e) => self.error(format!("Cannot write the file: {e}")),
                }
            }
            Purpose::ImportCompare => {
                let f = match std::fs::read_to_string(&path).map_err(|e| e.to_string()).and_then(|t| crate::compare::CompareFile::parse(&t)) {
                    Ok(f) => f,
                    Err(e) => return self.error(e),
                };
                // modelele: cele alese in panou, altfel cele cu aceleasi nume ca in fisier
                let by_name = |n: &str| self.loaded_models().find(|(_, m)| m.name == n).map(|(i, _)| i);
                let a = by_name(&f.a_name).or(self.compare_pick.0);
                let b = by_name(&f.b_name).or(self.compare_pick.1);
                let (Some(a), Some(b)) = (a, b) else { return self.error("Import comparison: load both versions (A and B) first.") };
                let (Some(ma), Some(mb)) = (self.models.get(a).cloned().flatten(), self.models.get(b).cloned().flatten()) else { return self.error("Import comparison: load both versions (A and B) first.") };
                if a == b {
                    return self.error("Import comparison: A and B must be two different models.");
                }
                let (r, missing) = f.apply(&ma, a, &mb, b);
                self.compare_pick = (Some(a), Some(b));
                self.compare_tol = f.tol_mm;
                let note = if missing > 0 { format!(" {missing} items from the file were not found in these models.") } else { String::new() };
                self.toast(format!("Comparison imported ({} → {}, {}).{note}", f.a_name, f.b_name, f.created));
                self.compare = Some(r);
                self.refresh_compare_params();
                self.compare_on = true;
                self.colors_dirty = true;
                self.open_panel(Panel::Compare);
            }
            Purpose::ExportTree => {
                let p = with_ext(&path, "bimtree");
                let f = TreeFile { app: "ManageBIM".into(), kind: TREE_FILE_KIND.into(), version: 1, levels: self.settings.tree_levels.clone() };
                match std::fs::write(&p, serde_json::to_string_pretty(&f).unwrap_or_default()) {
                    Ok(()) => self.toast(format!("Tree layout saved: {}", crate::file_name(&p))),
                    Err(e) => self.error(format!("Cannot write the file: {e}")),
                }
            }
            Purpose::ImportTree => match std::fs::read_to_string(&path).map_err(|e| e.to_string()).and_then(|t| serde_json::from_str::<TreeFile>(&t).map_err(|e| e.to_string())) {
                Ok(f) if f.kind == TREE_FILE_KIND && !f.levels.is_empty() => {
                    let desc = f.levels.iter().map(|l| l.prop.clone()).collect::<Vec<_>>().join(" › ");
                    self.set_tree_levels(f.levels);
                    self.toast(format!("Tree layout imported: {desc}"));
                }
                Ok(_) => self.error("This is not a ManageBIM tree layout file."),
                Err(e) => self.error(format!("Invalid tree layout file: {e}")),
            },
            Purpose::ExportQtoXlsx | Purpose::ExportQtoCsv => {
                self.qto_update();
                let Some(r) = &self.qto_result else { return };
                let title = self.project_path.as_deref().map(crate::file_name).unwrap_or_else(|| "ManageBIM".into());
                let (p, bytes) = if purpose == Purpose::ExportQtoXlsx {
                    (with_ext(&path, "xlsx"), r.to_xlsx(&title, &|id| self.qto_item_info(id)))
                } else {
                    (with_ext(&path, "csv"), r.summary_csv().into_bytes())
                };
                let n = r.rows.len();
                match std::fs::write(&p, bytes) {
                    Ok(()) => self.toast(format!("Quantities exported ({n} groups): {}", crate::file_name(&p))),
                    Err(e) => self.error(format!("Cannot write the file: {e}")),
                }
            }
            Purpose::ExportQtoTemplate => {
                let p = with_ext(&path, "bimqto");
                match std::fs::write(&p, self.qto.to_file_json()) {
                    Ok(()) => self.toast(format!("Quantification template saved: {}", crate::file_name(&p))),
                    Err(e) => self.error(format!("Cannot write the file: {e}")),
                }
            }
            Purpose::ImportQtoTemplate => match std::fs::read_to_string(&path).map_err(|e| e.to_string()).and_then(|t| crate::qto::QtoSetup::from_file_json(&t)) {
                Ok(s) => {
                    self.toast(format!("Quantification template loaded: {}", s.name));
                    self.qto = s;
                    self.qto_result = None;
                    self.open_panel(Panel::Quantities);
                }
                Err(e) => self.error(e),
            },
        }
    }

    /// Deschide dialogul de fisiere (Windows) sau un camp text (alte sisteme).
    pub fn request_path(&mut self, purpose: Purpose) {
        #[cfg(any(windows, unix))]
        if crate::formats::native_dialogs() {
            let d = rfd::FileDialog::new();
            let r = match purpose {
                Purpose::OpenModel { .. } => d.set_title("Open Model").add_filter("All supported models", &crate::SUPPORTED[..crate::SUPPORTED_MODELS])
                    .add_filter("Navisworks / Revit (through Navisworks)", &crate::formats::NAVIS_EXT)
                    .add_filter("IFC / ManageBIM", &["ifc", "ifczip", "bimc"])
                    .add_filter("CAD (DWG, DXF, STEP, IGES, BREP, FCStd)", &["dwg", "dxf", "step", "stp", "iges", "igs", "brep", "brp", "fcstd"])
                    .add_filter("3D meshes - Blender / Rhino / SketchUp exports (glTF, FBX, OBJ, STL, PLY)", &["glb", "gltf", "fbx", "obj", "stl", "ply"])
                    .add_filter("Point clouds (E57, LAS)", &["e57", "las"])
                    .add_filter("All files", &["*"]).pick_files().map(|v| v.into_iter().map(|p| p.to_string_lossy().to_string()).collect::<Vec<_>>()),
                Purpose::PublishBimd => d.set_title("Publish project + models (.bimd)").add_filter("ManageBIM published project", &["bimd"]).set_file_name(self.project_path.as_ref().map(|p| format!("{}.bimd", std::path::Path::new(p).file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default())).unwrap_or_else(|| "project.bimd".into())).save_file().map(|p| vec![p.to_string_lossy().to_string()]),
                Purpose::OpenProject => d.set_title("Open Project").add_filter("ManageBIM project (.bimr / .bimd)", &["bimr", "bimd"]).pick_file().map(|p| vec![p.to_string_lossy().to_string()]),
                Purpose::SaveProject => d.set_title("Save Project").add_filter("ManageBIM project", &["bimr"]).set_file_name("project.bimr").save_file().map(|p| vec![p.to_string_lossy().to_string()]),
                Purpose::ExportClashHtml => d.set_title("Clash report (HTML)").add_filter("HTML", &["html"]).set_file_name("clash_report.html").save_file().map(|p| vec![p.to_string_lossy().to_string()]),
                Purpose::ExportClashCsv => d.set_title("Clash report (CSV)").add_filter("CSV", &["csv"]).set_file_name("clash_report.csv").save_file().map(|p| vec![p.to_string_lossy().to_string()]),
                Purpose::ExportClashPdf => d.set_title("Clash report (PDF)").add_filter("PDF", &["pdf"]).set_file_name("clash_report.pdf").save_file().map(|p| vec![p.to_string_lossy().to_string()]),
                Purpose::ExportReference => {
                    let dir = outbox_dir().map(|d| { let _ = std::fs::create_dir_all(&d); d }).unwrap_or_default();
                    let name = self.bimd_path.as_ref().or(self.project_path.as_ref()).and_then(|p| std::path::Path::new(p).file_stem().map(|s| s.to_string_lossy().to_string())).unwrap_or_else(|| "reference".into());
                    d.set_title("Reference for Revit / AutoCAD / Plant 3D").add_filter("ManageBIM reference", &["mbref"]).set_directory(dir).set_file_name(format!("{name}.mbref")).save_file().map(|p| vec![p.to_string_lossy().to_string()])
                }
                Purpose::ExportClashMatrix => d.set_title("Clash matrix (Excel)").add_filter("Excel workbook", &["xlsx"]).set_file_name("clash_matrix.xlsx").save_file().map(|p| vec![p.to_string_lossy().to_string()]),
                Purpose::ImportUserProps => d.set_title("Import properties onto elements").add_filter("Excel / CSV", &["xlsx", "csv", "txt"]).pick_file().map(|p| vec![p.to_string_lossy().to_string()]),
                Purpose::ExportUserProps => d.set_title("Export properties (Excel template)").add_filter("Excel workbook", &["xlsx"]).set_file_name("properties.xlsx").save_file().map(|p| vec![p.to_string_lossy().to_string()]),
                Purpose::ImportClashTests => d.set_title("Import Clash Tests").add_filter("Clash tests (Navisworks XML, CSV, Excel)", &["xml", "csv", "xlsx"]).pick_file().map(|p| vec![p.to_string_lossy().to_string()]),
                Purpose::ExportClashTestsCsv => d.set_title("Export Clash Tests (CSV)").add_filter("CSV", &["csv"]).set_file_name("clash_tests.csv").save_file().map(|p| vec![p.to_string_lossy().to_string()]),
                Purpose::ExportWeb => d.set_title("Web viewer (HTML)").add_filter("Web page", &["html"]).set_file_name("model.html").save_file().map(|p| vec![p.to_string_lossy().to_string()]),
                Purpose::ExportAsBuilt => d.set_title("As-built report (Excel)").add_filter("Excel workbook", &["xlsx"]).set_file_name("as_built.xlsx").save_file().map(|p| vec![p.to_string_lossy().to_string()]),
                Purpose::ExportSectionCad => d.set_title("Section cut (DWG / DXF)").add_filter("AutoCAD DWG (2018)", &["dwg"]).add_filter("DXF", &["dxf"]).add_filter("PDF sheet", &["pdf"]).set_file_name("section.dwg").save_file().map(|p| vec![p.to_string_lossy().to_string()]),
                Purpose::ExportSectionPdf => d.set_title("Section cut on sheet (PDF)").add_filter("PDF", &["pdf"]).set_file_name("section.pdf").save_file().map(|p| vec![p.to_string_lossy().to_string()]),
                Purpose::ExportCobie => d.set_title("COBie handover (Excel)").add_filter("Excel workbook", &["xlsx"]).set_file_name("cobie.xlsx").save_file().map(|p| vec![p.to_string_lossy().to_string()]),
                Purpose::ImportOverlay => d.set_title("2D drawing to overlay on the model").add_filter("DWG / DXF", &["dwg", "dxf"]).pick_file().map(|p| vec![p.to_string_lossy().to_string()]),
                Purpose::ImportCheckRules => d.set_title("Import IDS or rules").add_filter("IDS / ManageBIM rules", &["ids", "xml", "mbrules", "json"]).pick_file().map(|p| vec![p.to_string_lossy().to_string()]),
                Purpose::ExportCheckRules => d.set_title("Save rules").add_filter("ManageBIM rules", &["mbrules"]).set_file_name("rules.mbrules").save_file().map(|p| vec![p.to_string_lossy().to_string()]),
                Purpose::ExportIds => d.set_title("Export IDS (buildingSMART)").add_filter("IDS 1.0", &["ids"]).set_file_name("requirements.ids").save_file().map(|p| vec![p.to_string_lossy().to_string()]),
                Purpose::ExportCheckXlsx => d.set_title("Model check report (Excel)").add_filter("Excel workbook", &["xlsx"]).set_file_name("model_check.xlsx").save_file().map(|p| vec![p.to_string_lossy().to_string()]),
                Purpose::ExportCheckBcf => d.set_title("Model check issues (BCF)").add_filter("BCF", &["bcf", "bcfzip"]).set_file_name("model_check.bcf").save_file().map(|p| vec![p.to_string_lossy().to_string()]),
                Purpose::ImportGridsCad => d.set_title("Import grid axes from DWG / DXF").add_filter("DWG / DXF", &["dwg", "dxf"]).pick_file().map(|p| vec![p.to_string_lossy().to_string()]),
                Purpose::ExportGrids => d.set_title("Export grid axes").add_filter("DXF (AutoCAD, any CAD)", &["dxf"]).add_filter("AutoCAD DWG (2018)", &["dwg"]).set_file_name("grids.dxf").save_file().map(|p| vec![p.to_string_lossy().to_string()]),
                Purpose::SharedFolder => d.set_title("Shared issues folder (network / SharePoint / OneDrive)").pick_folder().map(|p| vec![p.to_string_lossy().to_string()]),
                Purpose::SelectScriptsDir => d.set_title("Folder with your .rhai scripts").pick_folder().map(|p| vec![p.to_string_lossy().to_string()]),
                Purpose::ExportIfc => d.set_title("Export to IFC4").add_filter("IFC", &["ifc"]).set_file_name("export.ifc").save_file().map(|p| vec![p.to_string_lossy().to_string()]),
                Purpose::ExportGlb => d.set_title("Export to glTF (Blender, Rhino, Twinmotion, Unreal)").add_filter("glTF binary", &["glb"]).set_file_name("model.glb").save_file().map(|p| vec![p.to_string_lossy().to_string()]),
                Purpose::ExportClashBcf => d.set_title("Export clashes as BCF").add_filter("BIM Collaboration Format", &["bcf", "bcfzip"]).set_file_name("clashes.bcf").save_file().map(|p| vec![p.to_string_lossy().to_string()]),
                Purpose::ExportClashNavis => d.set_title("Export clashes as Navisworks viewpoints").add_filter("Navisworks viewpoints XML", &["xml"]).set_file_name("clashes_viewpoints.xml").save_file().map(|p| vec![p.to_string_lossy().to_string()]),
                Purpose::ImportBcf => d.set_title("Import BCF issues").add_filter("BIM Collaboration Format", &["bcf", "bcfzip"]).pick_file().map(|p| vec![p.to_string_lossy().to_string()]),
                Purpose::ImportTasksCsv => d.set_title("Import Schedule (CSV, MS Project, Primavera)").add_filter("Schedule", &["csv", "txt", "xml"]).pick_file().map(|p| vec![p.to_string_lossy().to_string()]),
                Purpose::ExportTasksCsv => d.set_title("Export CSV Schedule").add_filter("CSV", &["csv"]).set_file_name("schedule.csv").save_file().map(|p| vec![p.to_string_lossy().to_string()]),
                Purpose::ExportVideo => {
                    let ext = self.render.format.ext();
                    let name = format!("{}.{ext}", self.animations.get(self.cur_anim).map(|a| a.name.clone()).unwrap_or_else(|| "animation".into()));
                    d.set_title("Export animation").add_filter(ext.to_uppercase(), &[ext]).set_file_name(name).save_file().map(|p| vec![p.to_string_lossy().to_string()])
                }
                Purpose::RenderImage => d.set_title("Render image").add_filter("PNG image", &["png"]).set_file_name("render.png").save_file().map(|p| vec![p.to_string_lossy().to_string()]),
Purpose::ExportPathTrace => d.set_title("Export path trace").add_filter("PNG image", &["png"]).set_file_name("pathtrace.png").save_file().map(|p| vec![p.to_string_lossy().to_string()]),
                Purpose::ImportTextures => {
                    if let Some(v) = d.clone().set_title("Import images as materials").add_filter("Images", &["png", "jpg", "jpeg"]).pick_files() {
                        let v: Vec<String> = v.into_iter().map(|p| p.to_string_lossy().to_string()).collect();
                        self.import_texture_materials(&v);
                    }
                    None
                }
                Purpose::ImportPbr => {
                    if let Some(v) = d.clone().set_title("Import PBR material packs (.zip)").add_filter("PBR pack (ZIP)", &["zip"]).pick_files() {
                        let v: Vec<String> = v.into_iter().map(|p| p.to_string_lossy().to_string()).collect();
                        self.import_pbr_packs(&v);
                    }
                    None
                }
                Purpose::ImportPbrFolder => d.set_title("Folder with the PBR images (albedo, normal, roughness...)").pick_folder().map(|p| vec![p.to_string_lossy().to_string()]),
                Purpose::MaterialTexture | Purpose::MaterialNormal | Purpose::MaterialRoughness | Purpose::MaterialMetallic | Purpose::MaterialAo | Purpose::MaterialHeight => d.set_title("Choose texture image").add_filter("Images", &["png", "jpg", "jpeg"]).pick_file().map(|p| vec![p.to_string_lossy().to_string()]),
                Purpose::ExportMaterials => d.set_title("Export material library").add_filter("ManageBIM materials", &["bimmat"]).add_filter("Excel workbook", &["xlsx"]).add_filter("CSV (Excel)", &["csv"]).set_file_name("materials.bimmat").save_file().map(|p| vec![p.to_string_lossy().to_string()]),
                Purpose::ImportMaterials => d.set_title("Import material library").add_filter("Materials / Excel / CSV", &["bimmat", "json", "xlsx", "csv"]).pick_file().map(|p| vec![p.to_string_lossy().to_string()]),
                Purpose::ExportMaterialX => d.set_title("Export MaterialX (OpenPBR Surface)").add_filter("MaterialX", &["mtlx"]).set_file_name("materials.mtlx").save_file().map(|p| vec![p.to_string_lossy().to_string()]),
                Purpose::ImportMaterialX => d.set_title("Import MaterialX (OpenPBR Surface)").add_filter("MaterialX", &["mtlx", "xml"]).pick_file().map(|p| vec![p.to_string_lossy().to_string()]),
                Purpose::ExportVpHtml => d.set_title("Viewpoints report (HTML)").add_filter("HTML report", &["html"]).set_file_name("viewpoints_report.html").save_file().map(|p| vec![p.to_string_lossy().to_string()]),
                Purpose::ExportVpXlsx => d.set_title("Viewpoints report (Excel)").add_filter("Excel workbook", &["xlsx"]).set_file_name("viewpoints_report.xlsx").save_file().map(|p| vec![p.to_string_lossy().to_string()]),
                Purpose::ExportVpPdf => d.set_title("Sheets (PDF)").add_filter("PDF", &["pdf"]).set_file_name("sheets.pdf").save_file().map(|p| vec![p.to_string_lossy().to_string()]),
                Purpose::ExportBatch => d.set_title("Batch export folder (PDF, HTML, Excel, BCF)").pick_folder().map(|p| vec![p.to_string_lossy().to_string()]),
                Purpose::ExportSettingsProfile => d.set_title("Export settings profile (.mbprofile)").add_filter("ManageBIM profile", &["mbprofile"]).set_file_name("settings.mbprofile").save_file().map(|p| vec![p.to_string_lossy().to_string()]),
                Purpose::ImportSettingsProfile => d.set_title("Import settings profile (.mbprofile)").add_filter("ManageBIM profile", &["mbprofile"]).pick_file().map(|p| vec![p.to_string_lossy().to_string()]),
                Purpose::ExportBcf => d.set_title("Export BCF 2.1").add_filter("BIM Collaboration Format", &["bcf", "bcfzip"]).set_file_name("issues.bcf").save_file().map(|p| vec![p.to_string_lossy().to_string()]),
                Purpose::ExportNavisXml => d.set_title("Export viewpoints for Navisworks").add_filter("Navisworks viewpoints XML", &["xml"]).set_file_name("viewpoints.xml").save_file().map(|p| vec![p.to_string_lossy().to_string()]),
                Purpose::ImportNavisXml => d.set_title("Import Navisworks viewpoints").add_filter("Navisworks viewpoints XML", &["xml"]).pick_file().map(|p| vec![p.to_string_lossy().to_string()]),
                Purpose::ExportNavisSets => d.set_title("Export sets for Navisworks").add_filter("Navisworks search sets XML", &["xml"]).set_file_name("sets.xml").save_file().map(|p| vec![p.to_string_lossy().to_string()]),
                Purpose::ExportNavisClash => d.set_title("Export clash tests for Navisworks").add_filter("Navisworks clash tests XML", &["xml"]).set_file_name("clash_tests.xml").save_file().map(|p| vec![p.to_string_lossy().to_string()]),
                Purpose::ExportSets => d.set_title("Export sets").add_filter("ManageBIM sets", &["bimsets"]).set_file_name("sets.bimsets").save_file().map(|p| vec![p.to_string_lossy().to_string()]),
                Purpose::ImportSets => d.set_title("Import sets").add_filter("ManageBIM sets", &["bimsets", "json"]).pick_file().map(|p| vec![p.to_string_lossy().to_string()]),
                Purpose::ExportProfile => {
                    let name = format!("{}.bimprofile", self.profile.prop.replace(['/', '\\', ':'], "_"));
                    d.set_title("Export Appearance Profile").add_filter("ManageBIM appearance profile", &["bimprofile"]).add_filter("Excel workbook", &["xlsx"]).add_filter("CSV (Excel)", &["csv"]).set_file_name(name).save_file().map(|p| vec![p.to_string_lossy().to_string()])
                }
                Purpose::ImportProfile => d.set_title("Import Appearance Profile").add_filter("Profile / Excel / CSV", &["bimprofile", "json", "xlsx", "csv"]).pick_file().map(|p| vec![p.to_string_lossy().to_string()]),
                Purpose::ExportCompare => d.set_title("Save comparison").add_filter("ManageBIM comparison", &["bimcmp"]).set_file_name("comparison.bimcmp").save_file().map(|p| vec![p.to_string_lossy().to_string()]),
                Purpose::ImportCompare => d.set_title("Open comparison").add_filter("ManageBIM comparison", &["bimcmp", "json"]).pick_file().map(|p| vec![p.to_string_lossy().to_string()]),
                Purpose::ExportCompareCsv => d.set_title("Comparison report (CSV)").add_filter("CSV", &["csv"]).set_file_name("comparison.csv").save_file().map(|p| vec![p.to_string_lossy().to_string()]),
                Purpose::ExportQuickChecksCsv => d.set_title("Quick checks report (CSV)").add_filter("CSV", &["csv"]).set_file_name("quick_checks.csv").save_file().map(|p| vec![p.to_string_lossy().to_string()]),
                Purpose::ExportCompareParamsCsv => d.set_title("Parameter changes (CSV)").add_filter("CSV", &["csv"]).set_file_name("parameter_changes.csv").save_file().map(|p| vec![p.to_string_lossy().to_string()]),
                Purpose::ExportTree => d.set_title("Export tree layout").add_filter("ManageBIM tree layout", &["bimtree"]).set_file_name("tree.bimtree").save_file().map(|p| vec![p.to_string_lossy().to_string()]),
                Purpose::ImportTree => d.set_title("Import tree layout").add_filter("ManageBIM tree layout", &["bimtree", "json"]).pick_file().map(|p| vec![p.to_string_lossy().to_string()]),
                Purpose::ExportQtoXlsx => {
                    let name = format!("{}.xlsx", self.qto.name.replace(['/', '\\', ':'], "_"));
                    d.set_title("Export quantities (Excel)").add_filter("Excel workbook", &["xlsx"]).set_file_name(name).save_file().map(|p| vec![p.to_string_lossy().to_string()])
                }
                Purpose::ExportQtoCsv => d.set_title("Export quantities (CSV)").add_filter("CSV", &["csv"]).set_file_name("quantities.csv").save_file().map(|p| vec![p.to_string_lossy().to_string()]),
                Purpose::ExportQtoTemplate => {
                    let name = format!("{}.bimqto", self.qto.name.replace(['/', '\\', ':'], "_"));
                    d.set_title("Save quantification template").add_filter("ManageBIM quantification template", &["bimqto"]).set_file_name(name).save_file().map(|p| vec![p.to_string_lossy().to_string()])
                }
                Purpose::ImportQtoTemplate => d.set_title("Open quantification template").add_filter("ManageBIM quantification template", &["bimqto", "json"]).pick_file().map(|p| vec![p.to_string_lossy().to_string()]),
                Purpose::ExportCache(slot) => {
                    let name = self.models.get(slot).and_then(|m| m.as_ref()).map(|m| format!("{}.bimc", m.name)).unwrap_or_else(|| "model.bimc".into());
                    d.set_title("Export ManageBIM cache").add_filter("ManageBIM cache", &["bimc"]).set_file_name(name).save_file().map(|p| vec![p.to_string_lossy().to_string()])
                }
            };
            if let Some(paths) = r {
                for (i, p) in paths.into_iter().enumerate() {
                    let pur = match purpose {
                        Purpose::OpenModel { append } => Purpose::OpenModel { append: append || i > 0 },
                        o => o,
                    };
                    self.handle_path(pur, p);
                }
            }
            return;
        }
        // fara dialog nativ (sesiune fara D-Bus / portal): calea se scrie intr-un camp
        {
            let text = match purpose {
                Purpose::SaveProject => self.project_path.clone().unwrap_or_else(|| "project.bimr".into()),
                Purpose::ExportClashHtml => "clash_report.html".into(),
                Purpose::ExportClashCsv => "clash_report.csv".into(),
                Purpose::ExportClashPdf => "clash_report.pdf".into(),
                Purpose::ExportClashBcf => "clashes.bcf".into(),
                Purpose::ExportIfc => "export.ifc".into(),
                Purpose::ExportGlb => "model.glb".into(),
                Purpose::SharedFolder => String::new(),
                Purpose::PublishBimd => "project.bimd".into(),
                Purpose::ExportGrids => "grids.dxf".into(),
                Purpose::ImportGridsCad => String::new(),
                Purpose::ExportReference => "reference.mbref".into(),
                Purpose::ExportClashMatrix => "clash_matrix.xlsx".into(),
                Purpose::ImportClashTests => "clash_tests.xml".into(),
                Purpose::ImportUserProps => String::new(),
                Purpose::ExportUserProps => "properties.xlsx".into(),
                Purpose::ExportClashTestsCsv => "clash_tests.csv".into(),
                Purpose::ExportWeb => "model.html".into(),
                Purpose::ExportAsBuilt => "as_built.xlsx".into(),
                Purpose::ExportSectionCad => "section.dwg".into(),
                Purpose::ExportSectionPdf => "section.pdf".into(),
                Purpose::ExportCobie => "cobie.xlsx".into(),
                Purpose::ImportOverlay => String::new(),
                Purpose::ImportCheckRules => String::new(),
                Purpose::ExportCheckRules => "rules.mbrules".into(),
                Purpose::ExportIds => "requirements.ids".into(),
                Purpose::ExportCheckXlsx => "model_check.xlsx".into(),
                Purpose::ExportCheckBcf => "model_check.bcf".into(),
                Purpose::ExportClashNavis => "clashes_viewpoints.xml".into(),
                Purpose::ExportTasksCsv => "schedule.csv".into(),
                Purpose::ExportSets => "sets.bimsets".into(),
                Purpose::ExportMaterials => "materials.bimmat".into(),
                Purpose::ExportMaterialX => "materials.mtlx".into(),
                Purpose::ImportMaterialX => "materials.mtlx".into(),
                Purpose::ExportVpHtml => "viewpoints_report.html".into(),
                Purpose::ExportVpXlsx => "viewpoints_report.xlsx".into(),
                Purpose::ExportVpPdf => "sheets.pdf".into(),
                Purpose::ExportBatch | Purpose::ExportSettingsProfile | Purpose::ImportSettingsProfile => "settings.mbprofile".into(),
                Purpose::ExportBcf => "issues.bcf".into(),
                Purpose::ExportNavisXml => "viewpoints.xml".into(),
                Purpose::ExportNavisSets => "sets.xml".into(),
                Purpose::ExportNavisClash => "clash_tests.xml".into(),
                Purpose::ExportVideo => format!("animation.{}", self.render.format.ext()),
                Purpose::RenderImage => "render.png".into(),
Purpose::ExportPathTrace => "pathtrace.png".into(),
                Purpose::ExportProfile => "profile.bimprofile".into(),
                Purpose::ExportCompare => "comparison.bimcmp".into(),
                Purpose::ExportCompareCsv => "comparison.csv".into(),
                Purpose::ExportCompareParamsCsv => "parameter_changes.csv".into(),
                Purpose::ExportQuickChecksCsv => "quick_checks.csv".into(),
                Purpose::ExportTree => "tree.bimtree".into(),
                Purpose::ExportQtoXlsx => "quantities.xlsx".into(),
                Purpose::ExportQtoCsv => "quantities.csv".into(),
                Purpose::ExportQtoTemplate => "quantities.bimqto".into(),
                Purpose::ExportCache(slot) => self.models.get(slot).and_then(|m| m.as_ref()).map(|m| crate::bimc::cache_path(&m.path)).unwrap_or_default(),
                _ => String::new(),
            };
            self.prompt = Some(PathPrompt { purpose, text });
        }
    }

    pub fn xform_start(&mut self, mode: XformMode, selection: bool) {
        self.xform = Some(XformPick { mode, selection, pts: Vec::new() });
        self.grid_pick = 0;
    }

    /// Un punct ales in 3D (cu normala fetei); la ultimul punct se aplica transformarea.
    pub fn xform_point(&mut self, p: Vec3, n: Vec3, id: ElemId) {
        let Some(mut x) = self.xform.take() else { return };
        x.pts.push((p, n, id));
        if x.pts.len() < x.mode.needed() {
            self.xform = Some(x);
            return;
        }
        let r = self.xform_apply(&x);
        match r {
            Ok(msg) => self.toast(msg),
            Err(e) => self.error(e),
        }
    }

    pub(super) fn xform_apply(&mut self, x: &XformPick) -> Result<String, String> {
        let p = &x.pts;
        // (unghi in grade in jurul verticalei prin pivot, translatie)
        let (angle, pivot, delta) = match x.mode {
            XformMode::Move => (0.0, p[0].0, p[1].0 - p[0].0),
            XformMode::Rotate => {
                let a = z_angle(p[1].0 - p[0].0, p[2].0 - p[0].0).ok_or("Rotate: the direction points are too close to the pivot.")?;
                (a, p[0].0, Vec3::ZERO)
            }
            XformMode::Align => {
                let (pa, na) = (p[0].0, p[0].1.normalize_or_zero());
                let (pb, nb) = (p[1].0, p[1].1.normalize_or_zero());
                // fata de mutat ajunge cu spatele pe fata tinta: normala ei = -normala tintei (rotatie doar in plan)
                let ang = if na.truncate().length() > 0.3 && nb.truncate().length() > 0.3 { z_angle(na, -nb).unwrap_or(0.0) } else { 0.0 };
                let rot = Mat4::from_rotation_z(ang.to_radians());
                let pa_rot = pivot_rotate(pa, pa, &rot);
                let na_rot = rot.transform_vector3(na);
                // dupa rotatie (in jurul lui pa), lipim fata: miscam pe normala tintei pana pe planul ei
                let dist = (pb - pa_rot).dot(nb);
                let d = if na_rot.dot(nb).abs() > 0.5 { nb * dist } else { pb - pa_rot };
                (ang, pa, d)
            }
        };
        if x.selection {
            if angle.abs() > 1e-4 {
                return Err("Selected items can only be moved - rotate / align whole models (choose \"Model\").".into());
            }
            if self.selected.is_empty() {
                return Err("Nothing selected.".into());
            }
            self.move_selected(delta);
            return Ok(format!("Moved {} items by {}", self.selected.len(), fmt_vec_len(delta)));
        }
        let slot = p[0].2.m as usize;
        let Some(base) = self.base_models.get(slot).cloned().flatten() else { return Err("The model is not loaded.".into()) };
        let mut xf = self.model_xf.get(slot).copied().unwrap_or_default();
        let piv_m = if xf.pivot_origin { (-self.origin_f64()).as_vec3() } else { base.aabb.center() };
        // M' = T(delta) * T(P) Rz(a) T(-P) * M  ->  rot_z += a, (pivot + t') = P + Rz(a)(pivot + t - P) + delta
        let r = Mat4::from_rotation_z(angle.to_radians());
        let t = Vec3::new(xf.translate[0] as f32, xf.translate[1] as f32, xf.translate[2] as f32);
        let new_pt = pivot + r.transform_vector3(piv_m + t - pivot) + delta;
        let t2 = new_pt - piv_m;
        xf.translate = [t2.x as f64, t2.y as f64, t2.z as f64];
        xf.rot_z = ((xf.rot_z + angle as f64) * 1e6).round() / 1e6;
        let name = base.name.clone();
        self.set_model_xf(slot, xf);
        Ok(match x.mode {
            XformMode::Move => format!("{name} moved by {}", fmt_vec_len(delta)),
            XformMode::Rotate => format!("{name} rotated by {angle:.3}°"),
            XformMode::Align => format!("{name} aligned: rotated {angle:.3}°, moved {}", fmt_vec_len(delta)),
        })
    }
}
