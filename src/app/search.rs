//! Sectiune din `impl App` (vezi fisierul parinte pentru restul).
use super::*;

impl App {
    /// Builds the category -> property -> value index over ALL elements of ALL models.
    pub fn build_prop_index(&self) -> PropIndex {
        let mut map: HashMap<(String, String), HashMap<String, u32>> = HashMap::new();
        let mut order_cats: Vec<String> = Vec::new();
        let mut seen_cat: HashSet<String> = HashSet::new();
        let mut add = |c: &str, n: &str, v: &str, map: &mut HashMap<(String, String), HashMap<String, u32>>| {
            if v.is_empty() {
                return;
            }
            if !seen_cat.contains(c) {
                seen_cat.insert(c.to_string());
                order_cats.push(c.to_string());
            }
            let e = map.entry((c.to_string(), n.to_string())).or_default();
            if let Some(x) = e.get_mut(v) {
                *x += 1;
            } else {
                e.insert(v.to_string(), 1);
            }
        };
        for (_, m) in self.loaded_models() {
            for e in &m.elements {
                add(ITEM_CAT, "Name", &e.name, &mut map);
                add(ITEM_CAT, "IFC Type", &e.ifc_type, &mut map);
                add(ITEM_CAT, "Level", &e.storey, &mut map);
                add(ITEM_CAT, "Model", &m.name, &mut map);
                add(ITEM_CAT, "GlobalId", &e.guid, &mut map);
                for p in &e.props {
                    add(&p.group, &p.name, &p.value, &mut map);
                }
            }
        }
        let mut cats: BTreeMap<String, Vec<PropValues>> = BTreeMap::new();
        for ((c, n), vals) in map {
            let elements = vals.values().sum();
            let distinct = vals.len();
            let mut values: Vec<(String, u32)> = if distinct <= MAX_LISTED_VALUES { vals.into_iter().collect() } else { Vec::new() };
            values.sort_by(|a, b| natural_cmp(&a.0, &b.0));
            cats.entry(c).or_default().push(PropValues { name: n, values, distinct, elements });
        }
        let rank = |c: &str| match c {
            ITEM_CAT => 0,
            "Element" => 1,
            _ => 2,
        };
        let mut out: Vec<(String, Vec<PropValues>)> = cats.into_iter().collect();
        for (c, ps) in out.iter_mut() {
            if c == ITEM_CAT {
                ps.sort_by_key(|p| ITEM_PROPS.iter().position(|x| *x == p.name).unwrap_or(99));
            } else {
                ps.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
            }
        }
        out.sort_by(|a, b| rank(&a.0).cmp(&rank(&b.0)).then(a.0.to_lowercase().cmp(&b.0.to_lowercase())));
        let _ = order_cats;
        PropIndex { cats: out }
    }

    /// Find with Navisworks-style scope: only the ticked models, optionally inside the selection.
    pub fn find_scoped(&self, rules: &[SearchRule], in_selection: bool) -> Vec<ElemId> {
        let cand: Vec<ElemId> = if in_selection { self.selected.iter().copied().collect() } else { self.all_ids() };
        cand.into_iter().filter(|id| !self.find_models_off.contains(&(id.m as usize)) && self.matches(*id, rules)).collect()
    }

    /// Rules for the values ticked in the Properties tree: values of one property are OR-ed.
    pub fn rules_from_checked(&self) -> Vec<SearchRule> {
        let mut v = Vec::new();
        for (i, (c, n, val)) in self.prop_checked.iter().enumerate() {
            v.push(SearchRule::new(c, n, SearchOp::Equals, val, i > 0));
        }
        v
    }

    pub fn prop_names(&self) -> Vec<String> {
        let mut s = BTreeSet::new();
        for (_, m) in self.loaded_models() {
            for e in m.elements.iter().take(20000) {
                for p in &e.props {
                    if p.group != "Element" {
                        s.insert(p.name.clone());
                    }
                }
            }
        }
        s.into_iter().collect()
    }

    // ------------------------------------------------------------------ cautare
    /// Rules are AND-ed; a rule flagged `or` starts a new block, blocks are OR-ed.
    pub fn matches(&self, id: ElemId, rules: &[SearchRule]) -> bool {
        if rules.is_empty() {
            return false;
        }
        let mut blocks: Vec<Vec<&SearchRule>> = vec![Vec::new()];
        for (i, r) in rules.iter().enumerate() {
            if r.or && i > 0 {
                blocks.push(Vec::new());
            }
            blocks.last_mut().unwrap().push(r);
        }
        blocks.iter().filter(|b| !b.is_empty()).any(|b| b.iter().all(|r| self.rule_match(id, r)))
    }

    pub fn rule_match(&self, id: ElemId, r: &SearchRule) -> bool {
        let Some((m, e)) = self.elem(id) else { return false };
        let val = r.value.trim().to_lowercase();
        let num = leading_number(&r.value);
        // valoarea(ile) campului
        let item_val = |f: &str| -> Option<&str> {
            match f {
                "Name" => Some(e.name.as_str()),
                "IFC Type" => Some(e.ifc_type.as_str()),
                "Level" => Some(e.storey.as_str()),
                "Model" => Some(m.name.as_str()),
                "GlobalId" => Some(e.guid.as_str()),
                _ => None,
            }
        };
        let vals: Vec<&str> = if !r.group.is_empty() {
            if r.group == ITEM_CAT {
                if r.field == "(any)" {
                    ITEM_PROPS.iter().filter_map(|f| item_val(f)).collect()
                } else {
                    item_val(&r.field).into_iter().collect()
                }
            } else {
                e.props
                    .iter()
                    .filter(|p| p.group.eq_ignore_ascii_case(&r.group) && (r.field == "(any)" || p.name.eq_ignore_ascii_case(&r.field)))
                    .map(|p| p.value.as_str())
                    .collect()
            }
        } else {
            match r.field.as_str() {
            "Any" => {
                let mut v: Vec<&str> = vec![e.name.as_str(), e.ifc_type.as_str(), e.storey.as_str(), e.guid.as_str()];
                v.extend(e.props.iter().map(|p| p.value.as_str()));
                v
            }
            "Name" => vec![e.name.as_str()],
            "IFC Type" => vec![e.ifc_type.as_str()],
            "Level" => vec![e.storey.as_str()],
            "Model" => vec![m.name.as_str()],
            "GlobalId" => vec![e.guid.as_str()],
            f => e
                .props
                .iter()
                .filter(|p| p.name.eq_ignore_ascii_case(f) || format!("{} / {}", p.group, p.name).eq_ignore_ascii_case(f))
                .map(|p| p.value.as_str())
                .collect(),
            }
        };
        let present: Vec<&str> = vals.into_iter().filter(|v| !v.is_empty()).collect();
        let lc = |s: &str| s.to_lowercase();
        match r.op {
            SearchOp::Defined => !present.is_empty(),
            SearchOp::Undefined => present.is_empty(),
            SearchOp::Equals => present.iter().any(|v| lc(v) == val),
            SearchOp::NotEquals => !present.iter().any(|v| lc(v) == val),
            SearchOp::Contains => present.iter().any(|v| lc(v).contains(&val)),
            SearchOp::NotContains => !present.iter().any(|v| lc(v).contains(&val)),
            SearchOp::Greater => num.is_some_and(|n| present.iter().any(|v| leading_number(v).is_some_and(|x| x > n))),
            SearchOp::Less => num.is_some_and(|n| present.iter().any(|v| leading_number(v).is_some_and(|x| x < n))),
        }
    }

    pub fn find(&self, rules: &[SearchRule]) -> Vec<ElemId> {
        self.all_ids().into_iter().filter(|id| self.matches(*id, rules)).collect()
    }

    pub fn resolve_set(&self, s: &SelSet) -> Vec<ElemId> {
        match &s.kind {
            SetKind::Explicit(v) => v.iter().copied().filter(|id| self.elem(*id).is_some()).collect(),
            SetKind::Search(r) => self.find(r),
        }
    }

    pub fn resolve_selector(&self, s: &Selector) -> Vec<ElemId> {
        match s.kind {
            SelKind::All => self.all_ids(),
            SelKind::Selection => self.selected.iter().copied().collect(),
            SelKind::Model => self
                .loaded_models()
                .filter(|(_, m)| m.name == s.value)
                .flat_map(|(i, m)| (0..m.elements.len() as u32).map(move |e| ElemId { m: i as u32, e }))
                .collect(),
            SelKind::Set => self.sets.iter().find(|x| x.name == s.value).map(|x| self.resolve_set(x)).unwrap_or_default(),
            SelKind::Type => self.all_ids().into_iter().filter(|id| self.elem(*id).is_some_and(|(_, e)| e.ifc_type == s.value)).collect(),
            SelKind::Discipline => self.all_ids().into_iter().filter(|id| self.elem(*id).is_some_and(|(m, e)| crate::clash_smart::discipline_of(e, &m.name).label() == s.value)).collect(),
        }
    }

    /// Regula TimeLiner: "set:Nume" sau "camp=valoare; camp=valoare" sau text liber
    pub fn rule_elems(&self, rule: &str) -> Vec<ElemId> {
        let rule = rule.trim();
        if rule.is_empty() {
            return Vec::new();
        }
        if let Some(n) = rule.strip_prefix("set:") {
            return self.sets.iter().find(|s| s.name.eq_ignore_ascii_case(n.trim())).map(|s| self.resolve_set(s)).unwrap_or_default();
        }
        let mut rules = Vec::new();
        for part in rule.split(';') {
            let part = part.trim();
            if part.is_empty() {
                continue;
            }
            if let Some((f, v)) = part.split_once('=') {
                let f = match f.trim().to_lowercase().as_str() {
                    "name" | "nume" => "Name".to_string(),
                    "type" | "tip" | "ifc type" | "tip ifc" => "IFC Type".to_string(),
                    "level" | "nivel" | "storey" => "Level".to_string(),
                    "model" => "Model".to_string(),
                    o => o.to_string(),
                };
                rules.push(SearchRule { field: f, op: SearchOp::Equals, value: v.trim().to_string(), or: false, group: String::new() });
            } else {
                rules.push(SearchRule { field: "Any".into(), op: SearchOp::Contains, value: part.to_string(), or: false, group: String::new() });
            }
        }
        self.find(&rules)
    }

    // ------------------------------------------------------------------ actiuni
    pub fn apply(&mut self, a: Action) {
        match a {
            Action::Select(ids, add) => {
                if !add {
                    self.selected.clear();
                }
                for id in &ids {
                    if add && ids.len() == 1 && self.selected.contains(id) {
                        self.selected.remove(id);
                    } else {
                        self.selected.insert(*id);
                    }
                }
                self.primary = ids.last().copied().filter(|i| self.selected.contains(i));
                self.colors_dirty = true;
            }
            Action::Hide(ids) => {
                for id in ids {
                    self.hidden.insert(id);
                    self.selected.remove(&id);
                }
                self.colors_dirty = true;
            }
            Action::Show(ids) => {
                for id in ids {
                    self.hidden.remove(&id);
                }
                self.colors_dirty = true;
            }
            Action::Isolate(ids) => {
                let keep: HashSet<ElemId> = ids.into_iter().collect();
                self.hidden = self.all_ids().into_iter().filter(|i| !keep.contains(i)).collect();
                for k in &keep {
                    self.hidden.remove(k);
                }
                self.colors_dirty = true;
            }
            Action::ShowAll => {
                self.hidden.clear();
                self.colors_dirty = true;
            }
            Action::Focus(ids) => {
                let bb = if ids.is_empty() { self.scene_aabb() } else { self.ids_aabb(&ids) };
                let mut t = self.cam;
                t.fit(&bb);
                self.fly_to(t);
            }
            Action::RemoveModel(slot) => {
                let name = self.models.get(slot).and_then(|m| m.as_ref()).map(|m| m.name.clone()).unwrap_or_default();
                self.remove_model(slot);
                self.hist.clear();
                self.hist_pos = 0;
                self.hist_sig = 0;
                self.refresh_caches();
                self.toast(format!("Removed from the project: {name} (the file is not deleted)."));
            }
            Action::FocusClash(t, r) => self.apply(Action::FocusClashes(t, vec![r])),
            Action::FocusClashes(t, rs) => {
                let Some(test) = self.clash_tests.get(t) else { return };
                let res: Vec<ClashResult> = rs.iter().filter_map(|r| test.results.get(*r).cloned()).collect();
                if res.is_empty() {
                    return;
                }
                if self.cam_before_clash.is_none() {
                    self.cam_before_clash = Some(self.cam);
                }
                self.cur_result = rs.last().copied();
                let mut f = ClashFocus::default();
                self.selected.clear();
                for r in &res {
                    f.a.push(r.a);
                    f.b.push(r.b);
                    f.points.push(Vec3::from(r.point));
                    self.selected.insert(r.a);
                    self.selected.insert(r.b);
                    self.hidden.remove(&r.a);
                    self.hidden.remove(&r.b);
                }
                self.primary = Some(res[0].a);
                let animate = self.settings.clash_ui.animate;
                if let Some(v) = res[0].view.filter(|_| res.len() == 1) {
                    // viewpoint salvat pe rezultat (ca in Navisworks): are prioritate fata de auto-zoom
                    if animate {
                        self.fly_to(v);
                    } else {
                        self.cam_anim = None;
                        self.cam = v;
                    }
                } else if self.clash_auto_zoom {
                    let bb = if res.len() == 1 {
                        // apropie camera de punctul de clash (elementele mari ar indeparta prea mult camera)
                        let bb = self.ids_aabb(&[res[0].a, res[0].b]);
                        let p = f.points[0];
                        let r0 = (bb.size().length() * 0.5).min(4.0).max(0.6);
                        Aabb { min: p - Vec3::splat(r0), max: p + Vec3::splat(r0) }
                    } else {
                        let mut bb = Aabb::EMPTY;
                        for p in &f.points {
                            bb.grow(*p - Vec3::splat(0.8));
                            bb.grow(*p + Vec3::splat(0.8));
                        }
                        bb
                    };
                    if animate {
                        let mut t = self.cam_anim.map(|(_, to, _)| to).unwrap_or(self.cam);
                        t.fit(&bb);
                        self.fly_to(t);
                    } else {
                        self.cam_anim = None;
                        self.cam.fit(&bb);
                    }
                }
                self.clash_focus = Some(f);
                self.colors_dirty = true;
            }
        }
    }
}
