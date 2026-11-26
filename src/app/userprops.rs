//! Sectiune din `impl App` (vezi fisierul parinte pentru restul).
use super::*;

impl App {
    /// Modelul incarcat, cu parametrii utilizatorului aplicati (copie doar daca modelul e partajat).
    pub fn with_user_params(&self, mut m: Arc<Model>) -> Arc<Model> {
        if !self.user_params.iter().any(|p| p.model == m.name) {
            return m;
        }
        let mut sh = self.user_shadow.lock().unwrap_or_else(|e| e.into_inner());
        // model proaspat (din fisier): valorile lui sunt originale; ce era al utilizatorului se rescrie
        let prev = Self::prev_user_keys(&sh, &m.name);
        sh.applied.retain(|k| k.0 != m.name);
        match Arc::get_mut(&mut m) {
            Some(mm) => apply_user_params_shadow(mm, &self.user_params, &prev, &mut sh),
            None => {
                let mut c = m.clone_model();
                apply_user_params_shadow(&mut c, &self.user_params, &prev, &mut sh);
                m = Arc::new(c);
            }
        }
        m
    }

    /// (cheie, tab, proprietate) puse de utilizator pe un model la aplicarea anterioara
    fn prev_user_keys(sh: &UserShadow, model: &str) -> HashSet<(String, String, String)> {
        sh.applied.iter().filter(|k| k.0 == model).map(|k| (k.1.clone(), k.2.clone(), k.3.clone())).collect()
    }

    /// Reaplica parametrii pe modelul din slot (fara reincarcare si fara a retrimite geometria).
    pub(super) fn refresh_user_params(&mut self, slot: usize) {
        let base = self.base_models.get_mut(slot).and_then(|b| b.take());
        let cur = self.models.get_mut(slot).and_then(|c| c.take());
        let same = matches!((&base, &cur), (Some(a), Some(b)) if Arc::ptr_eq(a, b));
        let name = base.as_ref().or(cur.as_ref()).map(|m| m.name.clone()).unwrap_or_default();
        let shadow = &self.user_shadow;
        // aceeasi stare "anterioara" pentru ambele copii (baza si cea afisata)
        let prev = {
            let mut sh = shadow.lock().unwrap_or_else(|e| e.into_inner());
            let p = Self::prev_user_keys(&sh, &name);
            sh.applied.retain(|k| k.0 != name);
            p
        };
        let patch = |m: Option<Arc<Model>>, params: &[UserParam]| -> Option<Arc<Model>> {
            let mut m = m?;
            let mut sh = shadow.lock().unwrap_or_else(|e| e.into_inner());
            match Arc::get_mut(&mut m) {
                Some(mm) => apply_user_params_shadow(mm, params, &prev, &mut sh),
                None => {
                    let mut c = m.clone_model();
                    apply_user_params_shadow(&mut c, params, &prev, &mut sh);
                    m = Arc::new(c);
                }
            }
            Some(m)
        };
        if same {
            drop(cur);
            let b = patch(base, &self.user_params);
            self.models[slot] = b.clone();
            self.base_models[slot] = b;
        } else {
            let b = patch(base, &self.user_params);
            let c = patch(cur, &self.user_params);
            if slot < self.base_models.len() {
                self.base_models[slot] = b;
            }
            self.models[slot] = c;
        }
        self.profile_map = None;
        self.colors_dirty = true;
    }

    /// Adauga / schimba un parametru pe elementele date (valoare goala = sterge parametrul), in tab-ul "User".
    pub fn set_user_param(&mut self, ids: &[ElemId], name: &str, value: &str) -> usize {
        if value.trim().is_empty() {
            return self.remove_user_param(ids, USER_GROUP, name);
        }
        self.set_user_prop(ids, USER_GROUP, name, value)
    }

    /// Scrie proprietatea `name` = `value` in tab-ul `group` pe elementele date (valoarea poate fi goala).
    pub fn set_user_prop(&mut self, ids: &[ElemId], group: &str, name: &str, value: &str) -> usize {
        let (group, name) = (group.trim(), name.trim());
        if name.is_empty() || group.is_empty() {
            return 0;
        }
        let mut slots: BTreeSet<usize> = BTreeSet::new();
        let mut n = 0;
        for id in ids {
            let Some((m, e)) = self.elem(*id) else { continue };
            let (model, key) = (m.name.clone(), user_key(e, id.e as usize));
            self.user_params.retain(|p| !(p.model == model && p.key == key && p.group == group && p.name.eq_ignore_ascii_case(name)));
            self.user_params.push(UserParam { model, key, name: name.to_string(), value: value.trim().to_string(), group: group.to_string() });
            slots.insert(id.m as usize);
            n += 1;
        }
        for s in slots {
            self.refresh_user_params(s);
        }
        n
    }

    /// Sterge proprietatea `name` din tab-ul `group` (name gol = tot tab-ul) de pe elementele date.
    pub fn remove_user_param(&mut self, ids: &[ElemId], group: &str, name: &str) -> usize {
        let keys: HashSet<(String, String)> = ids.iter().filter_map(|id| self.elem(*id).map(|(m, e)| (m.name.clone(), user_key(e, id.e as usize)))).collect();
        let before = self.user_params.len();
        self.user_params.retain(|p| !(keys.contains(&(p.model.clone(), p.key.clone())) && p.group == group && (name.is_empty() || p.name.eq_ignore_ascii_case(name))));
        let n = before - self.user_params.len();
        let slots: BTreeSet<usize> = ids.iter().map(|i| i.m as usize).collect();
        for s in slots {
            self.refresh_user_params(s);
        }
        n
    }

    /// Redenumeste o proprietate (din tab-ul `group`) pe elementele date.
    pub fn rename_user_prop(&mut self, ids: &[ElemId], group: &str, old: &str, new: &str) {
        let new = new.trim();
        if new.is_empty() || new == old {
            return;
        }
        let keys: HashSet<(String, String)> = ids.iter().filter_map(|id| self.elem(*id).map(|(m, e)| (m.name.clone(), user_key(e, id.e as usize)))).collect();
        for p in &mut self.user_params {
            if keys.contains(&(p.model.clone(), p.key.clone())) && p.group == group && p.name == old {
                p.name = new.to_string();
            }
        }
        let slots: BTreeSet<usize> = ids.iter().map(|i| i.m as usize).collect();
        for s in slots {
            self.refresh_user_params(s);
        }
    }

    /// Redenumeste un tab de date utilizator pe elementele date.
    pub fn rename_user_tab(&mut self, ids: &[ElemId], old: &str, new: &str) {
        let new = new.trim();
        if new.is_empty() || new == old {
            return;
        }
        let keys: HashSet<(String, String)> = ids.iter().filter_map(|id| self.elem(*id).map(|(m, e)| (m.name.clone(), user_key(e, id.e as usize)))).collect();
        for p in &mut self.user_params {
            if keys.contains(&(p.model.clone(), p.key.clone())) && p.group == old {
                p.group = new.to_string();
            }
        }
        let slots: BTreeSet<usize> = ids.iter().map(|i| i.m as usize).collect();
        for s in slots {
            self.refresh_user_params(s);
        }
    }

    /// Tab-urile de date utilizator ale unui element, cu proprietatile lor (in ordinea adaugarii).
    pub fn user_tabs_of(&self, id: ElemId) -> Vec<(String, Vec<(String, String)>)> {
        let Some((m, e)) = self.elem(id) else { return Vec::new() };
        let key = user_key(e, id.e as usize);
        let mut out: Vec<(String, Vec<(String, String)>)> = Vec::new();
        for p in self.user_params.iter().filter(|p| p.model == m.name && p.key == key) {
            match out.iter_mut().find(|g| g.0 == p.group) {
                Some(g) => g.1.push((p.name.clone(), p.value.clone())),
                None => out.push((p.group.clone(), vec![(p.name.clone(), p.value.clone())])),
            }
        }
        out
    }

    /// Citeste un Excel / CSV pentru import si deschide fereastra de potrivire.
    pub fn open_prop_import(&mut self, path: &str) {
        let rows = match crate::tables::read_rows(path) {
            Ok(r) => r,
            Err(e) => return self.error(format!("{}: {e}", crate::file_name(path))),
        };
        if rows.len() < 2 {
            return self.error("The file needs a header row and at least one data row.");
        }
        let header = 0;
        let h: Vec<String> = rows[header].iter().map(|c| c.trim().trim_start_matches('\u{feff}').to_lowercase()).collect();
        let key_col = h.iter().position(|c| ["key", "globalid", "guid", "ifcguid", "ifc guid", "uniqueid", "unique id"].contains(&c.as_str())).or_else(|| h.iter().position(|c| c == "name")).unwrap_or(0);
        let match_by = match h.get(key_col).map(|s| s.as_str()) {
            Some("name") => "Name".to_string(),
            Some("key") => "Key".to_string(),
            Some("globalid") | Some("guid") | Some("ifcguid") | Some("ifc guid") => "GlobalId".to_string(),
            Some(_) => rows[header][key_col].trim().to_string(),
            None => "GlobalId".to_string(),
        };
        self.prop_import = Some(PropImport { path: path.to_string(), rows, header, key_col, match_by, tab: "User Data".into(), only_selected: false });
    }

    /// cheia de potrivire a unui element pentru importul de proprietati
    pub(super) fn prop_match_values(&self, id: ElemId, by: &str) -> Vec<String> {
        let Some((m, e)) = self.elem(id) else { return Vec::new() };
        match by {
            "Key" => vec![user_key(e, id.e as usize), format!("{}|{}", m.name, user_key(e, id.e as usize))],
            "GlobalId" => crate::sync::ids_of(e),
            "Name" => vec![e.name.trim().to_string()],
            p => e.props.iter().filter(|q| prop_display_name(&q.name).eq_ignore_ascii_case(p)).map(|q| q.value.trim().to_string()).collect(),
        }
    }

    /// Cate randuri din import se potrivesc cu elemente (randuri potrivite, elemente).
    pub fn prop_import_preview(&self, imp: &PropImport) -> (usize, usize) {
        let idx = self.prop_import_index(imp);
        let mut rows_ok = 0;
        let mut elems = 0;
        for r in &imp.rows[imp.header + 1..] {
            let k = r.get(imp.key_col).map(|s| s.trim().to_string()).unwrap_or_default();
            if let Some(v) = idx.get(&norm_key(&k)) {
                rows_ok += 1;
                elems += v.len();
            }
        }
        (rows_ok, elems)
    }

    pub(super) fn prop_import_index(&self, imp: &PropImport) -> HashMap<String, Vec<ElemId>> {
        let ids: Vec<ElemId> = if imp.only_selected { self.selected.iter().copied().collect() } else { self.all_ids() };
        let mut idx: HashMap<String, Vec<ElemId>> = HashMap::new();
        for id in ids {
            for v in self.prop_match_values(id, &imp.match_by) {
                if !v.is_empty() {
                    idx.entry(norm_key(&v)).or_default().push(id);
                }
            }
        }
        for v in idx.values_mut() {
            v.sort();
            v.dedup();
        }
        idx
    }

    /// Scrie proprietatile din fisier pe elementele potrivite: fiecare coloana (in afara de cheie) = o proprietate.
    pub fn run_prop_import(&mut self) {
        let Some(imp) = self.prop_import.take() else { return };
        let idx = self.prop_import_index(&imp);
        let head: Vec<String> = imp.rows[imp.header].iter().map(|c| c.trim().trim_start_matches('\u{feff}').to_string()).collect();
        // coloanele de identificare din sablonul exportat nu devin proprietati
        let skip = ["key", "model", "name", "type", "globalid", "level"];
        let cols: Vec<usize> = (0..head.len()).filter(|&c| c != imp.key_col && !head[c].is_empty() && !(skip.contains(&head[c].to_lowercase().as_str()) && head.iter().any(|h| h.eq_ignore_ascii_case("key")))).collect();
        let tab = if imp.tab.trim().is_empty() { "User Data".to_string() } else { imp.tab.trim().to_string() };
        let mut slots: BTreeSet<usize> = BTreeSet::new();
        let (mut n_rows, mut n_vals) = (0, 0);
        let mut missing = 0;
        let mut new_params: Vec<UserParam> = Vec::new();
        for r in &imp.rows[imp.header + 1..] {
            let k = r.get(imp.key_col).map(|s| s.trim().to_string()).unwrap_or_default();
            if k.is_empty() {
                continue;
            }
            let Some(ids) = idx.get(&norm_key(&k)) else {
                missing += 1;
                continue;
            };
            n_rows += 1;
            for &id in ids {
                let Some((m, e)) = self.elem(id) else { continue };
                let (model, key) = (m.name.clone(), user_key(e, id.e as usize));
                for &c in &cols {
                    let v = r.get(c).map(|s| s.trim().to_string()).unwrap_or_default();
                    if v.is_empty() {
                        continue;
                    }
                    new_params.push(UserParam { model: model.clone(), key: key.clone(), name: head[c].clone(), value: v, group: tab.clone() });
                    n_vals += 1;
                }
                slots.insert(id.m as usize);
            }
        }
        // aceeasi proprietate pe acelasi element: valoarea noua o inlocuieste pe cea veche
        let newk: HashSet<(String, String, String, String)> = new_params.iter().map(|p| (p.model.clone(), p.key.clone(), p.group.clone(), p.name.to_lowercase())).collect();
        self.user_params.retain(|p| !newk.contains(&(p.model.clone(), p.key.clone(), p.group.clone(), p.name.to_lowercase())));
        self.user_params.extend(new_params);
        for s in slots {
            self.refresh_user_params(s);
        }
        self.refresh_caches();
        let miss = if missing > 0 { format!(", {missing} rows without a matching element") } else { String::new() };
        self.toast(format!("Properties imported into tab \"{tab}\": {n_vals} values from {n_rows} rows{miss}."));
    }

    /// Sablonul Excel: un rand pe element (selectia sau toate vizibile), cheia + proprietatile utilizatorului.
    pub fn export_user_props(&mut self, path: &str) {
        use crate::qto::{Cell, Sheet};
        let ids: Vec<ElemId> = if self.selected.is_empty() { self.all_ids().into_iter().filter(|i| !self.hidden.contains(i)).collect() } else { self.selected.iter().copied().collect() };
        let ids: Vec<ElemId> = ids.into_iter().filter(|i| self.elem(*i).is_some_and(|(_, e)| e.tri_count > 0)).collect();
        if ids.is_empty() {
            return self.error("No elements to export.");
        }
        let mut names: Vec<String> = Vec::new();
        for p in &self.user_params {
            if !names.contains(&p.name) {
                names.push(p.name.clone());
            }
        }
        if names.is_empty() {
            names.push("Status".into());
        }
        let mut headers: Vec<String> = ["Key", "Model", "Name", "Type", "Level"].iter().map(|s| s.to_string()).collect();
        headers.extend(names.iter().cloned());
        let rows: Vec<Vec<Cell>> = ids
            .iter()
            .filter_map(|&id| {
                let (m, e) = self.elem(id)?;
                let key = user_key(e, id.e as usize);
                let mut r = vec![Cell::S(key.clone()), Cell::S(m.name.clone()), Cell::S(e.name.clone()), Cell::S(e.ifc_type.clone()), Cell::S(e.storey.clone())];
                for n in &names {
                    let v = self.user_params.iter().find(|p| p.model == m.name && p.key == key && &p.name == n).map(|p| p.value.clone()).unwrap_or_default();
                    r.push(Cell::S(v));
                }
                Some(r)
            })
            .collect();
        let widths: Vec<f64> = [40.0, 24.0, 30.0, 20.0, 14.0].into_iter().chain(std::iter::repeat(18.0)).take(headers.len()).collect();
        let n = rows.len();
        let sheet = Sheet { name: "Properties".into(), title: "ManageBIM properties - fill the columns, add new ones, then Import Properties (matched by Key)".into(), headers, widths, rows, total_from: None };
        let p = with_ext(path, "xlsx");
        match std::fs::write(&p, crate::qto::xlsx(&[sheet])) {
            Ok(()) => self.toast(format!("{n} elements exported: {} - fill in the values (new columns = new properties), then Import Properties.", crate::file_name(&p))),
            Err(e) => self.error(format!("Cannot write {p}: {e}")),
        }
    }
}
