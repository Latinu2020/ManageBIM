//! Section of `impl App` (see the parent file for the rest).
use super::*;

impl App {
    /// Every 3 minutes, if the project changed: a copy in Autosave (the user's .bimr file stays untouched).
    pub fn autosave_tick(&mut self) {
        if self.loaded_models().next().is_none() || !self.loads.is_empty() {
            return;
        }
        let due = self.autosave.last.is_none_or(|t| t.elapsed().as_secs() >= 180);
        if !due {
            return;
        }
        self.autosave.last = Some(Instant::now());
        let Ok(json) = serde_json::to_string(&self.project_file()) else { return };
        let sig = {
            use std::hash::{Hash, Hasher};
            let mut h = std::collections::hash_map::DefaultHasher::new();
            json.hash(&mut h);
            h.finish() | 1
        };
        if sig == self.autosave.sig {
            return;
        }
        let Some(dir) = autosave_dir() else { return };
        let pid = std::process::id();
        let (f, tmp) = (dir.join(format!("autosave-{pid}.bimr")), dir.join(format!("autosave-{pid}.bimr.tmp")));
        if std::fs::write(&tmp, json).is_ok() && std::fs::rename(&tmp, &f).is_ok() {
            let _ = std::fs::write(dir.join(format!("autosave-{pid}.txt")), self.project_path.clone().unwrap_or_default());
            self.autosave.sig = sig;
        }
    }

    /// Opens the autosaved project; the normal save then goes to the original project (if it exists).
    pub fn autosave_recover(&mut self) {
        let Some((file, orig, _)) = self.autosave.recover.take() else { return };
        self.load_project(&file);
        self.project_path = orig.clone();
        let _ = std::fs::remove_file(&file);
        self.toast(match orig {
            Some(p) => format!("Recovered. Save (Ctrl+S) to keep the changes in {}.", crate::file_name(&p)),
            None => "Recovered. Use Save As to keep the project.".to_string(),
        });
    }

    pub fn autosave_discard(&mut self) {
        if let Some((file, _, _)) = self.autosave.recover.take() {
            let _ = std::fs::remove_file(&file);
            let _ = std::fs::remove_file(std::path::Path::new(&file).with_extension("txt"));
        }
    }
}
