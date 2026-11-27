//! Section of `impl App` (see the parent file for the rest).
use super::*;

impl App {
    /// Starts / stops the selection synchronized with Revit and Navisworks.
    pub fn set_sync(&mut self, on: bool) {
        self.settings.sync_selection = on;
        self.settings.save();
        self.sync.enabled_at = std::time::SystemTime::now();
        self.sync.sent = u64::MAX;
        self.sync.seen = [None; 2];
        if on {
            // "Send to ManageBIM" from Revit / Navisworks: the sent model is added to the scene immediately.
            self.scan_inbox();
            self.toast("Live Link on: selection follows Revit / Navisworks; new models sent from them are added automatically (Revit: ManageBIM › Sync Selection; Navisworks: Tool Add-ins › Sync Selection).");
        }
    }

    pub(super) fn sel_hash(&self) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        self.selected.len().hash(&mut h);
        for id in &self.selected {
            id.hash(&mut h);
        }
        h.finish()
    }

    /// identifier -> element, for all loaded models (rebuilt when the models change)
    pub(super) fn sync_index(&mut self) -> &HashMap<String, ElemId> {
        let key: usize = self.loaded_models().map(|(i, m)| m.elements.len() * 31 + i).sum::<usize>() + self.loaded_models().count();
        if self.sync.index_key != key || self.sync.index.is_empty() {
            let mut idx: HashMap<String, ElemId> = HashMap::new();
            for (mi, m) in self.loaded_models() {
                for (ei, e) in m.elements.iter().enumerate() {
                    for s in crate::sync::ids_of(e) {
                        idx.entry(s).or_insert(ElemId { m: mi as u32, e: ei as u32 });
                    }
                }
            }
            self.sync.index = idx;
            self.sync.index_key = key;
        }
        &self.sync.index
    }

    /// Called on every frame: sends the selection when it changes, reads the add-ins' selection.
    pub fn sync_tick(&mut self) {
        if !self.settings.sync_selection || self.sync.last_poll.elapsed().as_millis() < 250 {
            return;
        }
        self.sync.last_poll = Instant::now();
        // ---- received from Revit / Navisworks
        if let Some(dir) = crate::sync::sync_dir() {
            for (k, (file, who)) in crate::sync::PEERS.iter().enumerate() {
                let p = dir.join(file);
                let Ok(t) = std::fs::metadata(&p).and_then(|m| m.modified()) else { continue };
                if self.sync.seen[k] == Some(t) {
                    continue;
                }
                self.sync.seen[k] = Some(t);
                self.sync.peer_seen[k] = Some(Instant::now());
                // the old selection (from before the link started) is not applied
                if t < self.sync.enabled_at {
                    continue;
                }
                let Some(ids) = std::fs::read_to_string(&p).ok().and_then(|s| crate::sync::parse_selection(&s)) else { continue };
                let idx = self.sync_index().clone();
                let mut found: Vec<ElemId> = ids.iter().filter_map(|s| idx.get(s).copied()).collect();
                found.sort();
                found.dedup();
                let n_found = found.len();
                let current: Vec<ElemId> = self.selected.iter().copied().collect();
                if found != current {
                    self.apply(Action::Select(found.clone(), false));
                    if self.settings.sync_follow && !found.is_empty() {
                        self.apply(Action::Focus(found));
                    }
                }
                // we do not send it back (no echo)
                self.sync.sent = self.sel_hash();
                if !ids.is_empty() && n_found == 0 {
                    self.sync.last_msg = format!("{who}: selection not found in the loaded models");
                } else {
                    self.sync.last_msg = format!("{who}: {n_found} items");
                }
            }
        }
        // ---- sent to Revit / Navisworks
        let h = self.sel_hash();
        if h != self.sync.sent {
            self.sync.sent = h;
            let sel: Vec<ElemId> = self.selected.iter().copied().take(20_000).collect();
            let lines: Vec<Vec<String>> = sel.iter().filter_map(|id| self.elem(*id).map(|(_, e)| crate::sync::ids_of(e))).filter(|v| !v.is_empty()).collect();
            if let Err(e) = crate::sync::write_selection(crate::sync::OURS, &lines) {
                self.sync.last_msg = format!("Cannot write the selection: {e}");
            }
        }
    }
}
