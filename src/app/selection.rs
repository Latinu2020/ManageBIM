//! Sectiune din `impl App` (vezi fisierul parinte pentru restul).
use super::*;

impl App {
    // ------------------------------------------------------------------ acces model
    pub fn elem(&self, id: ElemId) -> Option<(&Arc<Model>, &Element)> {
        let m = self.models.get(id.m as usize)?.as_ref()?;
        Some((m, m.elements.get(id.e as usize)?))
    }

    pub fn all_ids(&self) -> Vec<ElemId> {
        let mut v = Vec::new();
        for (mi, m) in self.models.iter().enumerate() {
            if let Some(m) = m {
                v.extend((0..m.elements.len() as u32).map(|e| ElemId { m: mi as u32, e }));
            }
        }
        v
    }

    pub fn loaded_models(&self) -> impl Iterator<Item = (usize, &Arc<Model>)> {
        self.models.iter().enumerate().filter_map(|(i, m)| m.as_ref().map(|m| (i, m)))
    }

    pub fn scene_aabb(&self) -> Aabb {
        let mut bb = Aabb::EMPTY;
        for (_, m) in self.loaded_models() {
            if m.aabb.is_valid() {
                bb.union(&m.aabb);
            }
        }
        bb
    }

    pub fn ids_aabb(&self, ids: &[ElemId]) -> Aabb {
        let mut bb = Aabb::EMPTY;
        for id in ids {
            if let Some((_, e)) = self.elem(*id) {
                if e.aabb.is_valid() {
                    bb.union(&e.aabb);
                }
            }
        }
        bb
    }

    /// Clash results are only shown while the Clash Detective panel is open.
    pub fn clash_view(&self) -> bool {
        // doar cat panoul se vede: zona de jos coborata / panouri ascunse = modelul revine la normal
        self.settings.layout.visible(Panel::Clash)
    }

    /// The 4D simulation is only shown while the TimeLiner panel is open.
    pub fn sim_view(&self) -> bool {
        (self.sim_active && self.settings.layout.visible(Panel::TimeLiner)) || self.anim_sim_active
    }

    pub fn clash_focus_active(&self) -> Option<&ClashFocus> {
        if self.clash_view() {
            self.clash_focus.as_ref()
        } else {
            None
        }
    }

    /// element -> role (1 = A, 2 = B of focused clashes, 3 / 4 = A / B of the other clashes)
    pub fn clash_roles(&self) -> HashMap<ElemId, u8> {
        let mut roles = HashMap::new();
        if !self.clash_view() {
            return roles;
        }
        if self.clash_highlight_all {
            if let Some(t) = self.clash_tests.get(self.cur_test) {
                for r in t.results.iter().filter(|r| r.status != ClashStatus::Resolved && self.clash_filter.map_or(true, |f| r.status == f)) {
                    roles.entry(r.a).or_insert(3);
                    roles.entry(r.b).or_insert(4);
                }
            }
        }
        if let Some(f) = &self.clash_focus {
            for a in &f.a {
                roles.insert(*a, 1);
            }
            for b in &f.b {
                roles.insert(*b, 2);
            }
        }
        roles
    }

    /// Called every frame: entering / leaving a module restores the full model.
    pub fn update_mode(&mut self) {
        let mode = (self.clash_view(), self.sim_view());
        if mode == self.last_mode {
            return;
        }
        let (was_clash, _) = self.last_mode;
        self.last_mode = mode;
        self.colors_dirty = true;
        if was_clash && !mode.0 {
            if let Some(c) = self.cam_before_clash.take() {
                self.cam = c;
            }
        }
        if let Some(f) = &self.clash_focus {
            let pair: BTreeSet<ElemId> = f.a.iter().chain(f.b.iter()).copied().collect();
            let first = f.a.first().copied();
            if was_clash && !mode.0 && self.selected == pair {
                self.selected.clear();
                self.primary = None;
            } else if !was_clash && mode.0 && self.selected.is_empty() {
                self.selected = pair;
                self.primary = first;
            }
        }
    }

    pub fn is_visible(&self, id: ElemId) -> bool {
        if self.hidden.contains(&id) {
            return false;
        }
        if self.sim_view() {
            match self.sim_cache.get(&id) {
                Some(SimState::Hidden) => return false,
                None if self.sim_only_linked && !self.sim_cache.is_empty() => return false,
                _ => {}
            }
        }
        true
    }

    pub fn types(&self) -> Vec<String> {
        let mut s = BTreeSet::new();
        for (_, m) in self.loaded_models() {
            for e in &m.elements {
                s.insert(e.ifc_type.clone());
            }
        }
        s.into_iter().collect()
    }

    pub fn clear_selection(&mut self) {
        self.selected.clear();
        self.primary = None;
        self.clash_focus = None;
        self.colors_dirty = true;
    }

    pub fn refresh_caches(&mut self) {
        self.cache_types = self.types();
        self.mat_cache = None;
        self.profile_map = None;
        self.qto_result = None;
        // modelele s-au schimbat (incarcare / Refresh / eliminare) -> rezultatul Compare nu mai e valabil
        if self.compare.take().is_some() {
            self.compare_on = false;
            self.log.push("Compare results cleared because a model changed - run Compare again.".into());
        }
        self.prop_index = self.build_prop_index();
        let mut names: BTreeSet<String> = BTreeSet::new();
        for (c, ps) in &self.prop_index.cats {
            if c != ITEM_CAT {
                names.extend(ps.iter().map(|p| p.name.clone()));
            }
        }
        self.cache_props = names.into_iter().collect();
    }

    // ------------------------------------------------------------------ Selection Tree
    /// Reconstruieste arborii tuturor modelelor dupa gruparea curenta si o salveaza in setari.
    pub fn set_tree_levels(&mut self, levels: Vec<TreeLevel>) {
        self.settings.tree_levels = levels;
        self.settings.save();
        self.rebuild_trees();
    }

    pub fn rebuild_trees(&mut self) {
        let levels = self.settings.tree_levels.clone();
        for i in 0..self.models.len() {
            let t = self.models[i].as_ref().map(|m| ModelTree::build(m, &levels));
            if i < self.trees.len() {
                self.trees[i] = t;
            }
        }
    }

    /// Numele tipului elementului (Revit "Family and Type" / "Type", IFC ObjectType / tipul definit);
    /// None daca modelul nu are asa ceva (atunci "acelasi tip" = aceeasi categorie + acelasi nume).
    pub fn type_name_of(e: &crate::model::Element) -> Option<String> {
        const KEYS: [&str; 6] = ["Family and Type", "Type Name", "Type", "ObjectType", "IfcTypeObject", "Family Type"];
        for k in KEYS {
            if let Some(p) = e.props.iter().find(|p| p.name.eq_ignore_ascii_case(k) && !p.value.trim().is_empty()) {
                return Some(p.value.trim().to_string());
            }
        }
        None
    }

    /// Select Similar: toate elementele (vizibile) din toate modelele cu aceeasi categorie / tip ca selectia.
    /// `by_type` = acelasi tip (ex. "Basic Wall: 200mm"); altfel aceeasi categorie (IfcWall / Walls).
    pub fn select_similar(&mut self, by_type: bool) {
        let sel: Vec<ElemId> = self.selected.iter().copied().collect();
        if sel.is_empty() {
            return self.toast("Select an item first.");
        }
        let mut cats: HashSet<String> = HashSet::new();
        let mut types: HashSet<(String, String)> = HashSet::new();
        for id in &sel {
            if let Some((_, e)) = self.elem(*id) {
                cats.insert(e.ifc_type.clone());
                types.insert((e.ifc_type.clone(), Self::type_name_of(e).unwrap_or_else(|| format!("name:{}", e.name))));
            }
        }
        let mut out: Vec<ElemId> = Vec::new();
        for (mi, m) in self.loaded_models() {
            for (ei, e) in m.elements.iter().enumerate() {
                let id = ElemId { m: mi as u32, e: ei as u32 };
                if self.hidden.contains(&id) || e.default_hidden {
                    continue;
                }
                let hit = if by_type {
                    cats.contains(&e.ifc_type) && types.contains(&(e.ifc_type.clone(), Self::type_name_of(e).unwrap_or_else(|| format!("name:{}", e.name))))
                } else {
                    cats.contains(&e.ifc_type)
                };
                if hit {
                    out.push(id);
                }
            }
        }
        let n = out.len();
        let what = if by_type {
            let t: Vec<String> = types.iter().map(|(c, t)| if let Some(n) = t.strip_prefix("name:") { format!("{c} \"{n}\"") } else { t.clone() }).take(3).collect();
            t.join(", ")
        } else {
            cats.iter().take(3).cloned().collect::<Vec<_>>().join(", ")
        };
        self.apply(Action::Select(out, false));
        self.toast(format!("{n} items selected - same {}: {what}", if by_type { "type" } else { "category" }));
    }
}
