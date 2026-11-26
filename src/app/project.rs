//! Sectiune din `impl App` (vezi fisierul parinte pentru restul).
use super::*;

impl App {
    // ------------------------------------------------------------------ incarcare
    pub fn open_path(&mut self, path: &str, append: bool) {
        let lower = path.to_lowercase();
        if lower.ends_with(".bimr") {
            self.load_project(path);
            return;
        }
        if lower.ends_with(".bimd") {
            self.open_bimd(path);
            return;
        }
        if lower.ends_with(".csv") {
            // graficul se importa dupa ce modelele s-au incarcat (pentru reguli)
            self.pending_csv.push(path.to_string());
            self.open_panel(Panel::TimeLiner);
            return;
        }
        if !append {
            self.reset();
        }
        let slot = self.models.len();
        self.push_slot();
        self.settings.add_recent(path);
        self.start_load(slot, path.to_string());
    }

    pub(super) fn start_load(&mut self, slot: usize, path: String) {
        // fisier mai mare decat incape in memoria libera: avertisment inainte (citirea continua)
        if let (Ok(meta), Some(free)) = (std::fs::metadata(&path), available_memory()) {
            let need = estimated_memory(&path, meta.len());
            if need > free * 8 / 10 {
                let gb = |b: u64| b as f64 / 1_073_741_824.0;
                self.toast(format!(
                    "{} is large ({:.1} GB): it needs about {:.1} GB of memory and only {:.1} GB are free. Loading may be slow or fail - close other programs, or open a smaller part / the .bimc cache.",
                    crate::file_name(&path),
                    gb(meta.len()),
                    gb(need),
                    gb(free)
                ));
            }
        }
        // data fisierului la momentul citirii -> Refresh stie daca s-a modificat ulterior
        match file_mtime(&path) {
            Some(t) => {
                self.file_stamps.insert(slot, t);
            }
            None => {
                self.file_stamps.remove(&slot);
            }
        }
        self.changed_files.remove(&slot);
        let (tx, rx): (Sender<LoadMsg>, Receiver<LoadMsg>) = channel();
        let origin = self.origin.clone();
        let p2 = path.clone();
        let (use_cache, auto_cache) = (self.settings.use_cache, self.settings.auto_cache);
        let cancel = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let cancel_t = cancel.clone();
        std::thread::spawn(move || {
            // orice panica (cache, origine, scriere) devine eroare de incarcare - altfel incarcarea ar ramane
            // „in curs” pentru totdeauna, iar autosave-ul s-ar opri
            let tx_fail = tx.clone();
            let body = std::panic::catch_unwind(std::panic::AssertUnwindSafe(move || {
            let txp = tx.clone();
            let prog = move |f: f32, s: &str| {
                // Cancel: firul se opreste la urmatorul pas de progres (resume_unwind nu scrie crash.log)
                if cancel_t.load(std::sync::atomic::Ordering::Relaxed) {
                    std::panic::resume_unwind(Box::new(CANCELLED));
                }
                let _ = txp.send(LoadMsg::Progress(f, s.to_string()));
            };
            let lower = p2.to_lowercase();
            // cache .bimc pentru tot ce nu e deja .bimc (conversiile DWG / STEP / Navisworks sunt lente)
            // norii de puncte nu au cache .bimc (punctele nu incap in formatul cache)
            let is_e57 = lower.ends_with(".e57") || lower.ends_with(".las") || lower.ends_with(".laz");
            let is_ifc = !lower.ends_with(".bimc") && !is_e57;
            let mut note = None;
            let mut loaded = None;
            let mut cloud_raw: Option<crate::e57::RawCloud> = None;
            // like Navisworks NWC: reuse an up-to-date cache next to the IFC
            if use_cache && is_ifc {
                if let Some(c) = crate::bimc::fresh_cache(&p2) {
                    prog(0.1, "Reading cache (.bimc)");
                    match crate::bimc::read(&c) {
                        Ok(mut raw) => {
                            raw.info.push(("Loaded from".into(), crate::file_name(&c)));
                            note = Some(format!("{}: opened from cache {}", crate::file_name(&p2), crate::file_name(&c)));
                            loaded = Some(Ok(raw));
                        }
                        Err(e) => note = Some(format!("Cache ignored ({e}), reading IFC")),
                    }
                }
            }
            // incarcarea ruleaza protejat: o eroare interna devine mesaj de eroare, nu inchide aplicatia
            let loaded = loaded.unwrap_or_else(|| {
                std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    if is_e57 {
                        if lower.ends_with(".las") || lower.ends_with(".laz") { crate::e57::read_las(&p2, &prog) } else { crate::e57::read(&p2, &prog) }.map(|(raw, c)| {
                            cloud_raw = Some(c);
                            raw
                        })
                    } else {
                        crate::load_any(&p2, &prog)
                    }
                }))
                .unwrap_or_else(|e| Err(format!("Internal error while reading the file: {}", panic_text(&e))))
            });
            let res = loaded.map(|raw| {
                if auto_cache && is_ifc && note.is_none() {
                    prog(0.95, "Writing cache (.bimc)");
                    let cp = crate::bimc::cache_path(&p2);
                    note = Some(match crate::bimc::write(&raw, &cp) {
                        Ok(n) => {
                            let src = std::fs::metadata(&p2).map(|m| m.len()).unwrap_or(0);
                            format!("Cache written: {} ({:.2} MB, {:.1}x smaller than the source)", crate::file_name(&cp), n as f64 / 1e6, src as f64 / n.max(1) as f64)
                        }
                        Err(e) => format!("Could not write cache: {e}"),
                    });
                }
                prog(0.99, "Building spatial index");
                let o = {
                    let mut g = origin.lock().unwrap_or_else(|p| p.into_inner());
                    *g.get_or_insert_with(|| match &cloud_raw {
                        Some(c) => DVec3::new(c.base.x.round(), c.base.y.round(), if c.base.z.abs() > 5000.0 { c.base.z.round() } else { 0.0 }),
                        None => raw.center(),
                    })
                };
                std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    let mut m = crate::build_model(raw, &p2, o);
                    if let Some(c) = cloud_raw.take() {
                        m.attach_cloud(c, o);
                    }
                    Arc::new(m)
                }))
                    .map(|m| (m, note))
                    .map_err(|e| format!("Internal error while building the model: {}", panic_text(&e)))
            }).and_then(|r| r);
            let _ = tx.send(LoadMsg::Done(res));
            }));
            if let Err(e) = body {
                let _ = tx_fail.send(LoadMsg::Done(Err(format!("Internal error while loading: {}", panic_text(&e)))));
            }
        });
        self.loads.push(LoadJob { slot, path, progress: 0.0, stage: "Starting".into(), rx, cancel });
    }

    pub fn push_slot(&mut self) {
        self.models.push(None);
        self.trees.push(None);
        self.base_models.push(None);
        self.model_xf.push(ModelXf::default());
    }

    pub fn reload_model(&mut self, slot: usize, path: String) {
        // deja in curs de incarcare -> nu porni a doua oara
        if self.loads.iter().any(|j| j.slot == slot) {
            return;
        }
        // pastram modelul vechi: ramane afisat pana vine cel nou, apoi remapam selectia etc.
        if let Some(old) = self.base_models.get(slot).cloned().flatten() {
            self.reloading.insert(slot, old);
        }
        self.start_load(slot, path);
    }

    /// Verifica (cel mult o data la 1.5 s) ce fisiere sursa s-au modificat pe disc.
    pub fn check_file_changes(&mut self) {
        if self.last_file_check.elapsed().as_secs_f32() < 1.5 {
            return;
        }
        self.scan_file_changes();
    }

    pub(super) fn scan_file_changes(&mut self) {
        self.last_file_check = Instant::now();
        let mut changed = BTreeSet::new();
        for (i, m) in self.loaded_models() {
            if let (Some(old), Some(now)) = (self.file_stamps.get(&i), file_mtime(&m.path)) {
                if now != *old {
                    changed.insert(i);
                }
            }
        }
        self.changed_files = changed;
        self.scan_inbox();
        self.write_issue_outbox();
        self.scan_addin_errors();
    }

    /// Errors of the Revit / AutoCAD / Navisworks add-ins (written to %LOCALAPPDATA%\ManageBIM\addin.log by
    /// their guards) are shown here too, as notifications. Only errors written after ManageBIM started.
    pub(super) fn scan_addin_errors(&mut self) {
        static POS: std::sync::Mutex<Option<u64>> = std::sync::Mutex::new(None);
        let Some(path) = std::env::var_os("LOCALAPPDATA").map(|d| std::path::PathBuf::from(d).join("ManageBIM").join("addin.log")) else { return };
        let len = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
        let mut pos = POS.lock().unwrap_or_else(|e| e.into_inner());
        let from = match *pos {
            None => {
                *pos = Some(len);
                return;
            }
            Some(p) if p > len => 0, // the log was cleared / rotated
            Some(p) => p,
        };
        if len == from {
            return;
        }
        *pos = Some(len);
        drop(pos);
        let Ok(bytes) = std::fs::read(&path) else { return };
        let text = String::from_utf8_lossy(&bytes[from as usize..bytes.len().min(len as usize)]).to_string();
        for msg in addin_errors(&text) {
            self.error(msg);
        }
    }

    /// Butonul Refresh (F5): reincarca modelele ale caror fisiere s-au modificat.
    /// `force` (Ctrl+F5 / Shift+click) reincarca toate modelele, modificate sau nu.
    pub fn refresh_models(&mut self, force: bool) {
        self.scan_file_changes();
        let mut todo: Vec<(usize, String)> = Vec::new();
        let mut missing = Vec::new();
        for (i, m) in self.loaded_models() {
            if !std::path::Path::new(&m.path).exists() {
                missing.push(m.name.clone());
            } else if force || self.changed_files.contains(&i) {
                todo.push((i, m.path.clone()));
            }
        }
        for n in missing {
            self.error(format!("Refresh: file not found for {n}"));
        }
        if todo.is_empty() {
            if self.loaded_models().next().is_none() {
                self.toast("Refresh: no models loaded.");
            } else {
                self.toast("Refresh: no file has changed since it was loaded (Ctrl+F5 reloads everything).");
            }
            return;
        }
        let names: Vec<String> = todo.iter().map(|(_, p)| crate::file_name(p)).collect();
        self.toast(format!("Refreshing {} model(s): {}", todo.len(), names.join(", ")));
        for (i, p) in todo {
            self.reload_model(i, p);
        }
    }

    /// Dupa reincarcare: muta selectia, ascunderile, culorile, deplasarile, seturile
    /// si rezultatele de clash de pe modelul vechi pe cel nou, potrivind elementele dupa GUID.
    pub(super) fn remap_reloaded(&mut self, slot: usize, old: &Model, new: &Model) {
        let by_guid: HashMap<&str, u32> = new.elements.iter().enumerate().filter(|(_, e)| !e.guid.is_empty()).map(|(i, e)| (e.guid.as_str(), i as u32)).collect();
        let n_new = new.elements.len() as u32;
        let s = slot as u32;
        // element vechi -> element nou (None = elementul a disparut din fisier)
        let map_e = |e: u32| -> Option<u32> {
            let oe = old.elements.get(e as usize)?;
            if !oe.guid.is_empty() {
                return by_guid.get(oe.guid.as_str()).copied();
            }
            // fara GUID (glTF / DXF / OBJ...): pastram indexul daca inca exista
            (e < n_new).then_some(e)
        };
        let rid = |id: ElemId| -> Option<ElemId> {
            if id.m != s {
                return Some(id);
            }
            map_e(id.e).map(|e| ElemId { m: s, e })
        };

        self.selected = self.selected.iter().filter_map(|i| rid(*i)).collect();
        self.primary = self.primary.and_then(&rid);

        // ascunse: remapate; elementele noi ascunse implicit (ex. IfcSpace) raman ascunse
        let mapped: HashSet<u32> = (0..old.elements.len() as u32).filter_map(map_e).collect();
        let mut hidden: HashSet<ElemId> = self.hidden.iter().filter_map(|i| rid(*i)).collect();
        for (k, e) in new.elements.iter().enumerate() {
            if e.default_hidden && !mapped.contains(&(k as u32)) {
                hidden.insert(ElemId { m: s, e: k as u32 });
            }
        }
        self.hidden = hidden;
        self.overrides = self.overrides.iter().filter_map(|(i, c)| rid(*i).map(|n| (n, *c))).collect();
        self.elem_offsets = self.elem_offsets.iter().filter_map(|(i, o)| rid(*i).map(|n| (n, *o))).collect();
        for set in self.sets.iter_mut() {
            if let SetKind::Explicit(ids) = &mut set.kind {
                *ids = ids.iter().filter_map(|i| rid(*i)).collect();
            }
        }
        let mut stale = 0;
        for t in self.clash_tests.iter_mut() {
            for r in t.results.iter_mut() {
                match (rid(r.a), rid(r.b)) {
                    (Some(a), Some(b)) => {
                        r.a = a;
                        r.b = b;
                    }
                    _ => stale += 1,
                }
            }
        }
        let gone = old.elements.len().saturating_sub(mapped.len());
        let added = new.elements.len().saturating_sub(mapped.len());
        self.log.push(format!("Refresh {}: {} elements kept, {} new, {} removed", new.name, mapped.len(), added, gone));
        if stale > 0 {
            self.log.push(format!("Refresh {}: {stale} clash result(s) refer to removed elements - run the clash tests again", new.name));
        }
        self.clash_focus = None;
        self.min_dist_result = None;
        self.sim_cache.clear();
        self.sim_cache_day = i32::MIN;
        self.mat_cache = None;
    }

    pub fn reset(&mut self) {
        self.coord_objects.clear();
        self.coord_slot = None;
        self.coord_sel = None;
        self.user_params.clear();
        self.overlays.clear();
        // istoricul vizualizarii se refera la elementele modelelor de acum
        self.hist.clear();
        self.hist_pos = 0;
        self.hist_sig = 0;
        self.bimd_path = None;
        self.blank_project = false;
        {
            let mut r = self.renderer.lock().unwrap_or_else(|e| e.into_inner());
            for i in 0..self.models.len() {
                r.pending.push(Upload::Remove { slot: i as u32 });
            }
        }
        self.models.clear();
        self.walk_path.clear();
        self.path_run = None;
        self.trees.clear();
        self.base_models.clear();
        self.model_xf.clear();
        self.elem_offsets.clear();
        self.xf_draft.clear();
        self.animations.clear();
        self.mat_assign.clear();
        self.materials = crate::materials::full_library();
        self.home = None;
        self.mat_cache = None;
        self.cur_anim = 0;
        self.anim_play = None;
        self.anim_sim_active = false;
        self.loads.clear();
        *self.origin.lock().unwrap_or_else(|e| e.into_inner()) = None;
        self.selected.clear();
        self.primary = None;
        self.hidden.clear();
        self.overrides.clear();
        self.measures.clear();
        self.cur_measure.clear();
        self.section = Section::default();
        self.viewpoints.clear();
        self.lights.clear();
        self.light_sel = None;
        self.sets.clear();
        self.clash_tests = vec![ClashTest::new("Test 1".into())];
        self.cur_test = 0;
        self.cur_result = None;
        self.clash_focus = None;
        self.tasks.clear();
        self.cur_task = None;
        self.sim_active = false;
        self.project_path = None;
        self.colors_dirty = true;
        self.cache_types.clear();
        self.cache_props.clear();
        self.file_stamps.clear();
        self.reloading.clear();
        self.changed_files.clear();
        self.looks.clear();
        self.profile = Default::default();
        self.profile_map = None;
        self.compare = None;
        self.compare_on = false;
        self.compare_pick = (None, None);
        self.qto = Default::default();
        self.check_rules.clear();
        self.check_results = None;
        self.check_sel = None;
        self.qto_result = None;
        // lucrarile in curs se refera la proiectul vechi (indici de teste / viewpoints / elemente)
        if let Some(j) = self.clash_job.take() {
            j.prog.cancel.store(true, std::sync::atomic::Ordering::Relaxed);
        }
        self.clash_queue.clear();
        self.update_all_before = None;
        self.report_job = None;
        self.batch_queue.clear();
        if let Some(j) = &self.pt.job {
            j.cancel.store(true, std::sync::atomic::Ordering::Relaxed);
        }
        self.sel_results.clear();
        self.active_vp = None;
        self.vp_sel = 0;
        self.editing_set = None;
        self.still_path = None;
        self.pending_capture = None;
        self.still_wait = 0;
        self.still_restore = None;
    }

    pub fn remove_model(&mut self, slot: usize) {
        // scoaterea modelului virtual = stergerea obiectelor de coordonare
        if self.coord_slot == Some(slot) {
            self.coord_objects.clear();
            self.coord_slot = None;
            self.coord_sel = None;
        }
        self.file_stamps.remove(&slot);
        self.reloading.remove(&slot);
        self.changed_files.remove(&slot);
        self.looks.remove(&slot);
        if slot < self.models.len() {
            self.models[slot] = None;
            self.trees[slot] = None;
            if slot < self.base_models.len() {
                self.base_models[slot] = None;
            }
            self.elem_offsets.retain(|k, _| k.m as usize != slot);
            self.renderer.lock().unwrap_or_else(|e| e.into_inner()).pending.push(Upload::Remove { slot: slot as u32 });
            self.selected.retain(|i| i.m as usize != slot);
            self.colors_dirty = true;
        }
    }

    pub fn poll_jobs(&mut self) {
        let mut done = Vec::new();
        for (i, j) in self.loads.iter_mut().enumerate() {
            loop {
                match j.rx.try_recv() {
                    _ if j.cancel.load(std::sync::atomic::Ordering::Relaxed) => {
                        done.push((i, Err(CANCELLED.to_string())));
                        break;
                    }
                    Ok(LoadMsg::Progress(f, s)) => {
                        j.progress = f;
                        j.stage = s;
                    }
                    Ok(LoadMsg::Done(r)) => {
                        done.push((i, r));
                        break;
                    }
                    Err(std::sync::mpsc::TryRecvError::Empty) => break,
                    // firul s-a oprit fara rezultat (crash intern): incarcarea nu mai ramane blocata
                    Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                        done.push((i, Err("Internal error: the loading thread stopped unexpectedly.".into())));
                        break;
                    }
                }
            }
        }
        for (i, r) in done.into_iter().rev() {
            let job = self.loads.remove(i);
            match r {
                Ok((m, note)) => {
                    if let Some(n) = note {
                        self.log.push(n);
                    }
                    // parametrii adaugati de utilizator (din proiect)
                    let m = self.with_user_params(m);
                    let first = self.loaded_models().next().is_none();
                    while self.base_models.len() <= job.slot {
                        self.base_models.push(None);
                    }
                    while self.model_xf.len() <= job.slot {
                        self.model_xf.push(ModelXf::default());
                    }
                    self.base_models[job.slot] = Some(m.clone());
                    if let Some(old) = self.reloading.remove(&job.slot) {
                        // Refresh: pastram starea utilizatorului (selectie, ascunse, culori...)
                        self.remap_reloaded(job.slot, &old, &m);
                    } else {
                        for (k, e) in m.elements.iter().enumerate() {
                            if e.default_hidden {
                                self.hidden.insert(ElemId { m: job.slot as u32, e: k as u32 });
                            }
                        }
                    }
                    self.trees[job.slot] = Some(ModelTree::build(&m, &self.settings.tree_levels));
                    let warn = m.warnings.clone();
                    self.toast(format!("Loaded {}: {} elements, {} triangles", m.name, m.elements.len(), m.tri_total()));
                    for w in warn {
                        self.log.push(format!("{}: {w}", m.name));
                    }
                    self.models[job.slot] = Some(m);
                    self.relink_tasks = true;
                    self.apply_transform(job.slot);
                    if first && self.pending_project.is_none() {
                        let bb = self.scene_aabb();
                        self.cam.yaw = std::f32::consts::FRAC_PI_4;
                        self.cam.pitch = -0.55;
                        self.cam.fit(&bb);
                    }
                    self.colors_dirty = true;
                    self.refresh_caches();
                }
                Err(e) => {
                    if e == CANCELLED {
                        self.reloading.remove(&job.slot);
                        self.toast(format!("Loading cancelled: {}", crate::file_name(&job.path)));
                        continue;
                    }
                    // la Refresh esuat (ex. fisier inca in curs de scriere) ramane modelul vechi pe ecran
                    if self.reloading.remove(&job.slot).is_some() {
                        self.changed_files.insert(job.slot);
                        self.error(format!("Refresh failed, keeping the previous version of {}: {e}", crate::file_name(&job.path)));
                    } else {
                        self.error(format!("{}: {e}", crate::file_name(&job.path)));
                    }
                }
            }
        }
        if self.loads.is_empty() {
            if let Some(p) = self.pending_project.take() {
                self.apply_project_state(p);
            }
            for p in std::mem::take(&mut self.pending_csv) {
                self.import_tasks_csv(&p);
            }
            // modele noi incarcate: sarcinile 4D importate inainte (fara elemente) se leaga acum
            if std::mem::take(&mut self.relink_tasks) {
                let mut n = 0;
                for i in 0..self.tasks.len() {
                    if self.tasks[i].elems.is_empty() && !self.tasks[i].rule.trim().is_empty() {
                        let e = self.rule_elems(&self.tasks[i].rule.clone());
                        if !e.is_empty() {
                            self.tasks[i].elems = e;
                            n += 1;
                        }
                    }
                }
                if n > 0 {
                    self.invalidate_sim();
                    self.log.push(format!("TimeLiner: {n} tasks linked to the loaded models."));
                }
            }
        }
        // clash
        if let Some(job) = &self.clash_job {
            let msg = job.rx.try_recv();
            // firul de clash s-a oprit fara rezultat (eroare interna): nu ramane blocat pe "ruleaza"
            if let Err(std::sync::mpsc::TryRecvError::Disconnected) = msg {
                self.clash_job = None;
                self.clash_queue.clear();
                self.error("Internal error while running the clash test; the test was stopped.".to_string());
                return;
            }
            if let Ok(found) = msg {
                let t = job.test;
                let secs = job.started.elapsed().as_secs_f32();
                let cancelled = job.prog.cancel.load(std::sync::atomic::Ordering::Relaxed);
                self.clash_job = None;
                if cancelled {
                    self.clash_queue.clear();
                    self.update_all_before = None;
                    self.toast("Clash test cancelled.");
                } else {
                    self.merge_clash(t, found, secs);
                    // ca in Navisworks: dupa Run Test se deschide automat tabul Results
                    self.clash_subtab = 0; // 0 = Results (vezi ordinea tab-urilor din clash_ui)
                    self.cur_test = t;
                    self.clash_queue_next();
                }
            }
        }
    }

    // ------------------------------------------------------------------ proiect
    pub fn project_file(&self) -> ProjectFile {
        let mut hidden: Vec<ElemId> = self.hidden.iter().copied().collect();
        hidden.sort();
        let mut overrides: Vec<(ElemId, [u8; 4])> = self.overrides.iter().map(|(k, v)| (*k, *v)).collect();
        overrides.sort_by_key(|x| x.0);
        let mut pf = self.project_file_raw(hidden, overrides);
        sanitize_floats(&mut pf);
        pf
    }

    fn project_file_raw(&self, hidden: Vec<ElemId>, overrides: Vec<(ElemId, [u8; 4])>) -> ProjectFile {
        ProjectFile {
            app: "ManageBIM".into(),
            version: 1,
            models: self.models.iter().map(|m| m.as_ref().map(|m| m.path.clone()).unwrap_or_default()).collect(),
            origin: self.origin.lock().unwrap_or_else(|e| e.into_inner()).map(|o| o.to_array()),
            camera: self.cam,
            lights: self.lights.clone(),
            coord_objects: self.coord_objects.clone(),
            viewpoints: self.viewpoints.clone(),
            view_templates: self.view_templates.clone(),
            sets: self.sets.clone(),
            clash_tests: self.clash_tests.clone(),
            tasks: self.tasks.clone(),
            hidden,
            overrides,
            section: self.section.clone(),
            measures: self.measures.clone(),
            models_rel: self
                .models
                .iter()
                .map(|m| {
                    m.as_ref()
                        .map(|m| {
                            let proj_dir = self.project_path.as_ref().and_then(|p| std::path::Path::new(p).parent().map(|d| d.to_path_buf()));
                            match proj_dir {
                                Some(d) => std::path::Path::new(&m.path).strip_prefix(&d).map(|r| r.to_string_lossy().to_string()).unwrap_or_default(),
                                None => String::new(),
                            }
                        })
                        .unwrap_or_default()
                })
                .collect(),
            model_xf: self.model_xf.clone(),
            elem_offsets: {
                let mut v: Vec<(ElemId, [f32; 3])> = self.elem_offsets.iter().map(|(k, v)| (*k, *v)).collect();
                v.sort_by_key(|x| x.0);
                v
            },
            elem_scale: {
                let mut v: Vec<(ElemId, [f32; 3])> = self.elem_scale.iter().map(|(k, v)| (*k, *v)).collect();
                v.sort_by_key(|x| x.0);
                v
            },
            animations: self.animations.clone(),
            render: self.render.clone(),
            user_levels: self.level.user.clone(),
            level_zero: self.level.zero,
            use_user_levels: self.level.custom,
            show_grids: self.show_grids,
            show_levels: self.level.show_lines,
            shared_dir: self.shared_dir.clone(),
            user_grids: self.user_grids.clone(),
            use_user_grids: self.use_user_grids,
            grid_bubble: self.grid_bubble,
            grid_ext: self.grid_ext,
            materials: self.materials.clone(),
            mat_assign: self.mat_assign.clone(),
            materials_on: Some(self.materials_on),
            home: self.home,
            walk_path: self.walk_path.iter().map(|p| p.to_array()).collect(),
            profile: if self.profile.rules.is_empty() { None } else { Some(self.profile.clone()) },
            qto: Some(self.qto.clone()),
            check_rules: self.check_rules.clone(),
            user_params: self.user_params.clone(),
            title_block: Some(self.title_block.clone()),
            vp_folders: self.vp_folders.clone(),
            overlays: self.overlays.clone(),
        }
    }
}

/// JSON nu are NaN / infinit: serde le scrie ca `null`, iar la deschidere proiectul ar fi respins.
/// Curatam valorile care pot ajunge asa (camera, viewpoints, clash-uri).
fn sanitize_floats(pf: &mut ProjectFile) {
    fn cam(c: &mut crate::camera::Camera) {
        if !(c.eye.iter().all(|v| v.is_finite()) && c.yaw.is_finite() && c.pitch.is_finite() && c.focus.is_finite() && c.fov_deg.is_finite()) {
            *c = Default::default();
        }
    }
    cam(&mut pf.camera);
    if let Some(h) = pf.home.as_mut() {
        cam(h);
    }
    for v in &mut pf.viewpoints {
        cam(&mut v.cam);
    }
    for t in &mut pf.clash_tests {
        if !t.tol_mm.is_finite() {
            t.tol_mm = 0.0;
        }
        for r in &mut t.results {
            if !r.dist.is_finite() {
                r.dist = 0.0;
            }
            for p in &mut r.point {
                if !p.is_finite() {
                    *p = 0.0;
                }
            }
            if let Some(v) = r.view.as_mut() {
                cam(v);
            }
        }
    }
}

impl App {
    pub fn save_project(&mut self, path: &str) {
        let mut path = path.to_string();
        if !path.to_lowercase().ends_with(".bimr") {
            path.push_str(".bimr");
        }
        let pf = self.project_file();
        match serde_json::to_string_pretty(&pf).map_err(|e| e.to_string()).and_then(|s| crate::formats::write_atomic(std::path::Path::new(&path), s.as_bytes(), true).map_err(|e| e.to_string())) {
            Ok(()) => {
                self.toast(format!("Project saved: {}", crate::file_name(&path)));
                self.settings.add_recent(&path);
                self.project_path = Some(path);
            }
            Err(e) => self.error(format!("Cannot save the project: {e}")),
        }
    }

    pub fn load_project(&mut self, path: &str) {
        let read = |p: &std::path::Path| -> Result<ProjectFile, String> { std::fs::read_to_string(p).map_err(|e| e.to_string()).and_then(|s| serde_json::from_str(&s).map_err(|e| e.to_string())) };
        let pf: ProjectFile = match read(std::path::Path::new(path)) {
            Ok(p) => p,
            Err(e) => {
                // fisier stricat (ex. o cadere in timpul unei salvari vechi): copia .bak a salvarii anterioare
                let bak = crate::formats::backup_path(std::path::Path::new(path));
                match read(&bak) {
                    Ok(p) => {
                        self.error(format!("The project file is damaged ({e}) - the previous save (.bak) was opened instead. Save to replace the damaged file."));
                        p
                    }
                    Err(_) => {
                        self.error(format!("Invalid project: {e}"));
                        return;
                    }
                }
            }
        };
        self.reset();
        *self.origin.lock().unwrap_or_else(|e| e.into_inner()) = pf.origin.map(DVec3::from_array);
        let base = std::path::Path::new(path).parent().map(|p| p.to_path_buf()).unwrap_or_default();
        // transformarile se aplica imediat ce fiecare model termina incarcarea
        self.elem_offsets = pf.elem_offsets.iter().copied().collect();
        self.elem_scale = pf.elem_scale.iter().copied().collect();
        for (i, mp) in pf.models.iter().enumerate() {
            self.push_slot();
            if let Some(x) = pf.model_xf.get(i) {
                self.model_xf[i] = *x;
            }
            // slotul modelului virtual "Coordination objects": nu e un fisier, se reface din proiect
            if mp == crate::coord_objects::COORD_PATH {
                self.coord_slot = Some(i);
                continue;
            }
            if mp.is_empty() {
                continue;
            }
            let mut real = mp.clone();
            if !std::path::Path::new(&real).exists() {
                let rel = pf.models_rel.get(i).filter(|r| !r.is_empty()).map(|r| base.join(r));
                let alt = rel.filter(|p| p.exists()).unwrap_or_else(|| base.join(crate::file_name(mp)));
                if alt.exists() {
                    real = alt.to_string_lossy().to_string();
                } else {
                    self.error(format!("Model not found: {mp}"));
                }
            }
            self.start_load(i, real);
        }
        self.project_path = Some(path.to_string());
        // proiectul salvat automat / despachetat din .bimd nu intra in lista Recent
        let internal = autosave_dir().is_some_and(|d| std::path::Path::new(path).starts_with(&d)) || published_dir().is_some_and(|d| std::path::Path::new(path).starts_with(&d));
        if !internal {
            self.settings.add_recent(path);
        }
        self.pending_project = Some(pf);
        if self.loads.is_empty() {
            if let Some(p) = self.pending_project.take() {
                self.apply_project_state(p);
            }
        }
    }

    pub(super) fn apply_project_state(&mut self, pf: ProjectFile) {
        // o camera cu NaN / infinit (proiect deteriorat) ar lasa vederea goala
        let cam_ok = |c: &crate::camera::Camera| c.eye.iter().all(|v| v.is_finite()) && c.yaw.is_finite() && c.pitch.is_finite() && c.focus.is_finite() && c.fov_deg.is_finite();
        self.cam = if cam_ok(&pf.camera) { pf.camera } else { Default::default() };
        self.lights = pf.lights;
        // obiectele de coordonare: modelul lor virtual se reface in slotul salvat
        self.coord_objects = pf.coord_objects;
        if !self.coord_objects.is_empty() || self.coord_slot.is_some() {
            self.rebuild_coord_model();
        }
        self.home = pf.home.filter(cam_ok);
        self.walk_path = pf.walk_path.iter().map(|p| Vec3::from(*p)).collect();
        self.path_run = None;
        self.animations = pf.animations;
        self.render = pf.render;
        // limitele din interfata (proiectul poate veni deteriorat sau din alta versiune)
        self.render.width = self.render.width.clamp(64, 7680);
        self.render.height = self.render.height.clamp(64, 4320);
        self.render.fps = self.render.fps.clamp(1, 120);
        self.level.user = pf.user_levels;
        self.level.zero = pf.level_zero;
        self.level.custom = pf.use_user_levels;
        self.show_grids = pf.show_grids;
        self.level.show_lines = pf.show_levels;
        self.shared_dir = pf.shared_dir;
        self.user_grids = pf.user_grids;
        self.use_user_grids = pf.use_user_grids;
        self.grid_bubble = pf.grid_bubble;
        self.grid_ext = pf.grid_ext;
        self.shared = Default::default();
        if !pf.materials.is_empty() {
            self.materials = pf.materials;
            // materialele din biblioteca ta raman disponibile si in proiectele mai vechi
            for m in crate::materials::user_library() {
                if !self.materials.iter().any(|x| x.name == m.name) {
                    self.materials.push(m);
                }
            }
        }
        self.mat_assign = pf.mat_assign;
        self.materials_on = pf.materials_on.unwrap_or(true);
        self.mat_cache = None;
        self.viewpoints = pf.viewpoints;
        self.view_templates = pf.view_templates;
        self.sets = pf.sets;
        self.clash_tests = if pf.clash_tests.is_empty() { vec![ClashTest::new("Test 1".into())] } else { pf.clash_tests };
        self.cur_test = 0;
        self.tasks = pf.tasks;
        // datele activitatilor: aceleasi limite ca in interfata (1900-2200); o data stricata ar bloca TimeLiner-ul
        let (d0, d1) = (crate::timeline::days_from_civil(1900, 1, 1), crate::timeline::days_from_civil(2200, 12, 31));
        for t in &mut self.tasks {
            t.start = t.start.clamp(d0, d1);
            t.end = t.end.clamp(t.start, d1);
        }
        self.hidden = pf.hidden.into_iter().collect();
        self.overrides = pf.overrides.into_iter().collect();
        self.section = pf.section;
        self.measures = pf.measures;
        self.profile = pf.profile.unwrap_or_default();
        self.profile_map = None;
        self.qto = pf.qto.unwrap_or_default();
        self.check_rules = pf.check_rules;
        self.user_params = pf.user_params;
        // modelele deja incarcate primesc parametrii acum (cele care vin dupa, la incarcare)
        let slots: Vec<usize> = self.loaded_models().map(|(i, _)| i).collect();
        if !self.user_params.is_empty() {
            for s in slots {
                self.refresh_user_params(s);
            }
        }
        self.title_block = pf.title_block.unwrap_or_default();
        self.vp_folders = pf.vp_folders;
        // desenele 2D se recitesc din fisierele lor
        let mut ovs = pf.overlays;
        for ov in &mut ovs {
            if let Err(e) = self.load_overlay_segs(ov) {
                self.log.push(format!("Overlay {}: {e}", ov.name));
            }
        }
        self.overlays = ovs;
        self.vp_folder_cur.clear();
        self.check_results = None;
        self.check_sel = None;
        self.qto_result = None;
        if let Some((s, _)) = self.sim_range() {
            self.sim_day = s as f32;
        }
        // aplică View Template-ul marcat „default", dacă există
        if let Some(i) = self.view_templates.iter().position(|t| t.is_default) {
            if let Some(t) = self.view_templates.get(i).cloned() {
                self.hidden = t.hidden.into_iter().collect();
                self.section = t.section;
                self.cam = t.cam;
                self.settings.visual_style = t.visual_style;
                self.show_grids = t.show_grids;
                self.level.show_lines = t.show_levels;
                self.section_caps = t.section_caps;
                self.overrides = t.overrides.into_iter().collect();
                self.selected = t.selected.into_iter().collect();
                self.measures = t.measures;
                self.vp_filter = t.vp_filter;
                self.vt_current = i;
                self.vt_applied = true;
            }
        }
        self.colors_dirty = true;
        self.toast("Project opened.");
    }

    /// Registers .bimc / .bimr (and "Open with" for .ifc) for the current user - no admin rights needed.
    #[cfg(windows)]
    pub fn register_file_types(&mut self) {
        use std::os::windows::process::CommandExt;
        let exe = match std::env::current_exe() {
            Ok(e) => e.to_string_lossy().to_string(),
            Err(e) => return self.error(format!("Cannot locate ManageBIM.exe: {e}")),
        };
        let open = format!("\"{exe}\" \"%1\"");
        let icon = format!("\"{exe}\",0");
        let mut keys: Vec<(String, Option<String>, String)> = Vec::new();
        for (ext, prog, desc) in [(".bimc", "ManageBIM.Cache", "ManageBIM model cache"), (".bimr", "ManageBIM.Project", "ManageBIM project"), ("", "ManageBIM.Ifc", "IFC model")] {
            if !ext.is_empty() {
                keys.push((format!("HKCU\\Software\\Classes\\{ext}"), None, prog.to_string()));
            }
            keys.push((format!("HKCU\\Software\\Classes\\{prog}"), None, desc.to_string()));
            keys.push((format!("HKCU\\Software\\Classes\\{prog}\\DefaultIcon"), None, icon.clone()));
            keys.push((format!("HKCU\\Software\\Classes\\{prog}\\shell\\open\\command"), None, open.clone()));
        }
        keys.push(("HKCU\\Software\\Classes\\.ifc\\OpenWithProgids".into(), Some("ManageBIM.Ifc".into()), String::new()));
        let mut failed = 0;
        for (key, name, data) in keys {
            let mut c = std::process::Command::new("reg");
            c.arg("add").arg(&key);
            match name {
                Some(n) => {
                    c.args(["/v", &n, "/t", "REG_SZ", "/d", ""]);
                }
                None => {
                    c.args(["/ve", "/d", &data]);
                }
            }
            c.arg("/f").creation_flags(0x0800_0000);
            if !c.status().map(|s| s.success()).unwrap_or(false) {
                failed += 1;
            }
        }
        if failed == 0 {
            self.toast(".bimc and .bimr files now open with ManageBIM (ManageBIM also appears in \"Open with\" for .ifc).");
        } else {
            self.error(format!("{failed} registry entries could not be written."));
        }
    }

    /// Proiect nou, gol: aplicatia trece de pagina de start, Save (Ctrl+S) cere unde se salveaza.
    pub fn new_project(&mut self) {
        self.reset();
        self.project_path = None;
        self.blank_project = true;
        self.toast("New project - add models with Open / Append; save it with Save (Ctrl+S).");
    }

    /// Publica proiectul intr-un singur fisier .bimd (ZIP): project.bimr + fiecare model ca .bimc
    /// (norii de puncte cu fisierul original). Se deschide pe orice PC, fara fisierele sursa.
    pub fn publish_bimd(&mut self, path: &str) -> Result<String, String> {
        let path = with_ext(path, "bimd");
        let origin = self.origin_f64();
        let tmp = std::env::temp_dir().join(format!("mb_publish_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).map_err(|e| e.to_string())?;
        let mut zip = crate::markup::Zip::default();
        let mut pf = self.project_file();
        pf.models_rel = Vec::new();
        let mut total: u64 = 0;
        let mut n = 0;
        for slot in 0..self.models.len() {
            let Some(m) = self.models[slot].clone() else {
                if let Some(p) = pf.models.get_mut(slot) {
                    p.clear();
                }
                continue;
            };
            // modelul virtual al obiectelor de coordonare: se reface din proiect, nu se impacheteaza
            if m.path == crate::coord_objects::COORD_PATH {
                continue;
            }
            let stem: String = std::path::Path::new(&m.path).file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| format!("model{slot}")).chars().map(|c| if "\\/:*?\"<>|".contains(c) { '_' } else { c }).collect();
            let (name, data) = if m.cloud.is_some() {
                // nor de puncte: fisierul original (E57 / LAS)
                let ext = std::path::Path::new(&m.path).extension().map(|e| e.to_string_lossy().to_string()).unwrap_or_else(|| "e57".into());
                (format!("models/{slot}_{stem}.{ext}"), std::fs::read(&m.path).map_err(|e| format!("{}: {e}", m.name))?)
            } else {
                // geometria originala (fara transformarea modelului - ea ramane in proiect)
                let base = self.base_models.get(slot).cloned().flatten().unwrap_or(m.clone());
                let raw = crate::bimc::from_model(&base, origin);
                let f = tmp.join(format!("{slot}.bimc"));
                crate::bimc::write(&raw, &f.to_string_lossy()).map_err(|e| format!("{}: {e}", m.name))?;
                (format!("models/{slot}_{stem}.bimc"), std::fs::read(&f).map_err(|e| e.to_string())?)
            };
            total += data.len() as u64;
            if total > 3_900_000_000 {
                let _ = std::fs::remove_dir_all(&tmp);
                return Err("The models are larger than 4 GB together - .bimd cannot hold them (save a .bimr project instead).".into());
            }
            zip.add(&name, &data);
            if let Some(p) = pf.models.get_mut(slot) {
                *p = name;
            }
            n += 1;
        }
        let _ = std::fs::remove_dir_all(&tmp);
        // tot ce proiectul citeste din alte fisiere intra in pachet: desenele 2D (overlay) si texturile
        // materialelor folosite - pe alt PC calea originala nu exista
        let mut embedded: HashMap<String, String> = HashMap::new();
        let mut embed = |zip: &mut crate::markup::Zip, total: &mut u64, dir: &str, src: &str| -> Option<String> {
            if src.trim().is_empty() {
                return None;
            }
            if let Some(n) = embedded.get(src) {
                return Some(n.clone());
            }
            let data = std::fs::read(src).ok()?;
            *total += data.len() as u64;
            let fname: String = crate::file_name(src).chars().map(|c| if "\\/:*?\"<>|".contains(c) { '_' } else { c }).collect();
            let name = format!("{dir}/{}_{fname}", embedded.len());
            zip.add(&name, &data);
            embedded.insert(src.to_string(), name.clone());
            Some(name)
        };
        for ov in pf.overlays.iter_mut() {
            if let Some(n) = embed(&mut zip, &mut total, "overlays", &ov.path) {
                ov.path = n;
            }
        }
        let used: std::collections::HashSet<String> = pf.mat_assign.iter().map(|a| a.material.clone()).collect();
        for m in pf.materials.iter_mut().filter(|m| used.contains(&m.name)) {
            for f in [&mut m.texture, &mut m.normal_map, &mut m.roughness_map, &mut m.metallic_map, &mut m.ao_map, &mut m.height_map] {
                if let Some(n) = embed(&mut zip, &mut total, "textures", f) {
                    *f = n;
                }
            }
        }
        if total > 3_900_000_000 {
            return Err("The project is larger than 4 GB with its models and drawings - .bimd cannot hold it (save a .bimr project instead).".into());
        }
        let json = serde_json::to_string_pretty(&pf).map_err(|e| e.to_string())?;
        zip.add("project.bimr", json.as_bytes());
        let bytes = zip.finish();
        let part = format!("{path}.part");
        std::fs::write(&part, &bytes).map_err(|e| format!("Cannot write {path}: {e}"))?;
        std::fs::rename(&part, &path).map_err(|e| format!("Cannot write {path}: {e}"))?;
        self.settings.add_recent(&path);
        Ok(format!("Published {} ({n} models, {:.1} MB)", crate::file_name(&path), bytes.len() as f64 / 1e6))
    }

    /// Deschide un .bimd: se despacheteaza in %LOCALAPPDATA%\ManageBIM\Published\<nume>, apoi proiectul se incarca
    /// de acolo. Save scrie din nou in .bimd.
    pub fn open_bimd(&mut self, path: &str) {
        let r = (|| -> Result<std::path::PathBuf, String> {
            let data = std::fs::read(path).map_err(|e| format!("Cannot read {path}: {e}"))?;
            let zip = crate::fcstd::Zip::parse(data).map_err(|_| format!("{} is not a valid .bimd file.", crate::file_name(path)))?;
            let stamp = std::fs::metadata(path).and_then(|m| m.modified()).ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_secs()).unwrap_or(0);
            let stem = std::path::Path::new(path).file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "project".into());
            let dir = published_dir().ok_or("LOCALAPPDATA not set")?.join(format!("{stem}-{stamp}"));
            std::fs::create_dir_all(dir.join("models")).map_err(|e| e.to_string())?;
            let txt = zip.text("project.bimr").ok_or("project.bimr missing in the .bimd file")?;
            let mut pf: ProjectFile = serde_json::from_str(&txt).map_err(|e| format!("Invalid project in the .bimd file: {e}"))?;
            for m in pf.models.iter_mut().filter(|m| !m.is_empty()) {
                if m.contains("..") {
                    return Err(format!("invalid path in the .bimd file: {m}"));
                }
                let out = dir.join(m.replace('/', "\\"));
                // despachetat o singura data (acelasi .bimd = acelasi folder)
                if !out.is_file() {
                    let data = zip.read(m).ok_or_else(|| format!("{m} missing in the .bimd file"))?;
                    std::fs::write(&out, data).map_err(|e| e.to_string())?;
                }
                *m = out.to_string_lossy().to_string();
            }
            // desenele 2D si texturile impachetate: despachetate langa modele, caile refacute
            let extract = |p: &mut String| -> Result<(), String> {
                if !(p.starts_with("overlays/") || p.starts_with("textures/")) {
                    return Ok(());
                }
                if p.contains("..") {
                    return Err(format!("invalid path in the .bimd file: {p}"));
                }
                let out = dir.join(p.replace('/', "\\"));
                if !out.is_file() {
                    if let Some(parent) = out.parent() {
                        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
                    }
                    let data = zip.read(p).ok_or_else(|| format!("{p} missing in the .bimd file"))?;
                    std::fs::write(&out, data).map_err(|e| e.to_string())?;
                }
                *p = out.to_string_lossy().to_string();
                Ok(())
            };
            for ov in pf.overlays.iter_mut() {
                extract(&mut ov.path)?;
            }
            for m in pf.materials.iter_mut() {
                for f in [&mut m.texture, &mut m.normal_map, &mut m.roughness_map, &mut m.metallic_map, &mut m.ao_map, &mut m.height_map] {
                    extract(f)?;
                }
            }
            let bimr = dir.join("project.bimr");
            std::fs::write(&bimr, serde_json::to_string_pretty(&pf).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
            Ok(bimr)
        })();
        match r {
            Ok(bimr) => {
                self.load_project(&bimr.to_string_lossy());
                self.project_path = None;
                self.bimd_path = Some(path.to_string());
                self.settings.add_recent(path);
            }
            Err(e) => self.error(e),
        }
    }

    /// Inapoi la pagina de start: proiectul se inchide (salvarea automata pastreaza o copie).
    pub fn close_to_start(&mut self) {
        self.reset();
        self.project_path = None;
        self.blank_project = false;
        self.confirm_start = false;
    }

    /// E ceva deschis (modele, proiect nou sau issue-uri) care s-ar pierde?
    pub fn has_open_project(&self) -> bool {
        self.loaded_models().next().is_some() || !self.loads.is_empty() || self.blank_project || self.project_path.is_some() || self.bimd_path.is_some() || !self.viewpoints.is_empty()
    }

    /// Din pagina de start: cu un proiect deschis se intreaba intai daca se salveaza.
    pub fn start_action(&mut self, a: StartAction) {
        if self.has_open_project() {
            self.start_pending = Some(a);
            self.confirm_start = true;
        } else {
            self.run_start_action(a);
        }
    }

    pub fn run_start_action(&mut self, a: StartAction) {
        self.confirm_start = false;
        self.start_pending = None;
        match a {
            StartAction::New => {
                self.new_project();
                self.show_start = false;
            }
            StartAction::Open(p) => {
                self.close_to_start();
                self.open_path(&p, false);
                self.show_start = false;
            }
            StartAction::OpenModelDialog => {
                self.close_to_start();
                self.show_start = false;
                self.request_path(Purpose::OpenModel { append: false });
            }
            StartAction::OpenProjectDialog => {
                self.close_to_start();
                self.show_start = false;
                self.request_path(Purpose::OpenProject);
            }
            StartAction::Close => {
                self.close_to_start();
                self.show_start = true;
            }
        }
    }
}

/// mesajul intern al unei incarcari anulate de utilizator
pub const CANCELLED: &str = "cancelled by the user";

/// Memoria estimata pentru un fisier deschis: geometria triangulata + proprietatile ocupa de cateva ori
/// dimensiunea fisierului text (IFC / DXF / OBJ); cache-ul .bimc si norii de puncte sunt mai compacti.
pub fn estimated_memory(path: &str, size: u64) -> u64 {
    let ext = std::path::Path::new(path).extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
    let factor = match ext.as_str() {
        "bimc" | "e57" | "las" | "laz" | "glb" => 2,
        "ifc" | "ifczip" | "dxf" | "obj" | "fbx" | "stl" | "ply" => 4,
        // DWG / STEP / Navisworks: conversie externa, geometria rezultata e mult mai mare decat fisierul
        _ => 6,
    };
    size.saturating_mul(factor)
}

/// Memoria fizica libera acum (octeti); None daca nu se poate afla.
pub fn available_memory() -> Option<u64> {
    #[cfg(windows)]
    {
        #[repr(C)]
        struct MemoryStatusEx {
            length: u32,
            load: u32,
            total_phys: u64,
            avail_phys: u64,
            total_page: u64,
            avail_page: u64,
            total_virtual: u64,
            avail_virtual: u64,
            avail_ext_virtual: u64,
        }
        extern "system" {
            fn GlobalMemoryStatusEx(s: *mut MemoryStatusEx) -> i32;
        }
        let mut s = MemoryStatusEx { length: std::mem::size_of::<MemoryStatusEx>() as u32, load: 0, total_phys: 0, avail_phys: 0, total_page: 0, avail_page: 0, total_virtual: 0, avail_virtual: 0, avail_ext_virtual: 0 };
        // SAFETY: structura are dimensiunea si campurile din documentatia Win32 (MEMORYSTATUSEX)
        (unsafe { GlobalMemoryStatusEx(&mut s) } != 0).then_some(s.avail_phys)
    }
    #[cfg(not(windows))]
    {
        let s = std::fs::read_to_string("/proc/meminfo").ok()?;
        let kb: u64 = s.lines().find(|l| l.starts_with("MemAvailable:"))?.split_whitespace().nth(1)?.parse().ok()?;
        Some(kb * 1024)
    }
}

#[cfg(test)]
mod memory_tests {
    #[test]
    fn free_memory_is_known_and_estimates_scale() {
        let free = super::available_memory().expect("free memory");
        assert!(free > 64 * 1024 * 1024, "{free}");
        assert_eq!(super::estimated_memory("a.IFC", 100), 400);
        assert_eq!(super::estimated_memory("a.bimc", 100), 200);
        assert_eq!(super::estimated_memory("a.dwg", 100), 600);
    }
}

/// The add-in errors in a piece of addin.log: "2026-11-26 20:24:01 [Revit] Export failed: System.X: message"
/// (followed by the stack trace) -> "Revit add-in: Export failed - message".
pub fn addin_errors(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    for line in text.lines() {
        let Some(open) = line.find(" [") else { continue };
        let Some(close) = line[open..].find("] ").map(|c| open + c) else { continue };
        let app = &line[open + 2..close];
        let msg = &line[close + 2..];
        let Some(f) = msg.find(" failed: ") else { continue };
        let what = &msg[..f];
        let mut err = msg[f + 9..].trim();
        // "System.InvalidOperationException: text" -> "text"
        if let Some(c) = err.find(": ") {
            if err[..c].contains('.') && !err[..c].contains(' ') {
                err = &err[c + 2..];
            }
        }
        out.push(format!("{app} add-in: {what} failed - {err} (details in addin.log)"));
    }
    out
}

#[cfg(test)]
mod addin_log_tests {
    #[test]
    fn errors_are_read_from_the_log() {
        let log = "2026-11-26 20:24:01 [Revit] Export failed: System.InvalidOperationException: The view is not 3D
   at ManageBIM.Revit.X.Run()
2026-11-26 20:25:00 [AutoCAD] BIMCSEND: 12 objects
2026-11-26 20:26:00 [Navisworks] Levels (window) failed: Object reference not set
";
        let e = super::addin_errors(log);
        assert_eq!(e.len(), 2);
        assert_eq!(e[0], "Revit add-in: Export failed - The view is not 3D (details in addin.log)");
        assert!(e[1].starts_with("Navisworks add-in: Levels (window) failed - Object reference"));
    }
}
