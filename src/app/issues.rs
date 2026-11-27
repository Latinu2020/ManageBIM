//! Section of `impl App` (see the parent file for the rest).
use super::*;

impl App {
    /// The issues (viewpoints) for the Revit / Navisworks add-ins ("ManageBIM Issues"), in
    /// %LOCALAPPDATA%\ManageBIM\Outbox\issues.txt - plain text, WORLD coordinates in meters:
    ///   MBISSUES 1
    ///   VIEW <tab> name <tab> status <tab> guid <tab> priority <tab> assignee
    ///   CAM <tab> ex ey ez <tab> fx fy fz <tab> ux uy uz <tab> fov_deg <tab> ortho_height (0 = perspective)
    ///   BOX <tab> minx miny minz <tab> maxx maxy maxz        (if it has a section box)
    ///   SEL <tab> guid <tab> guid ...                        (the selected elements)
    ///   TXT <tab> text (\n encoded as \\n)                   (description + comments)
    ///   END
    /// It is rewritten only when something changes (atomic write).
    pub fn write_issue_outbox(&mut self) {
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        serde_json::to_string(&self.viewpoints).unwrap_or_default().hash(&mut h);
        self.origin_f64().to_array().iter().for_each(|v| v.to_bits().hash(&mut h));
        let sig = h.finish() | 1;
        if sig == self.outbox_sig {
            return;
        }
        self.outbox_sig = sig;
        let Some(dir) = crate::formats::data_root().map(|d| d.join("ManageBIM").join("Outbox")) else { return };
        if std::fs::create_dir_all(&dir).is_err() {
            return;
        }
        let clean = |s: &str| s.replace('\\', "\\\\").replace('\t', " ").replace("\r\n", "\\n").replace('\n', "\\n");
        let v3 = |p: DVec3| format!("{:.6} {:.6} {:.6}", p.x, p.y, p.z);
        let mut s = String::from("MBISSUES 1\n");
        for v in &self.viewpoints {
            let c = v.cam;
            s.push_str(&format!("VIEW\t{}\t{}\t{}\t{}\t{}\n", clean(&v.name), clean(&v.issue.status), clean(&v.issue.guid), clean(&v.issue.priority), clean(&v.issue.assigned)));
            if !v.folder.is_empty() {
                s.push_str(&format!("FLD\t{}\n", clean(&v.folder)));
            }
            s.push_str(&format!(
                "CAM\t{}\t{}\t{}\t{:.4}\t{:.6}\n",
                v3(self.world(c.eye())),
                v3(c.fwd().as_dvec3()),
                v3(c.up().as_dvec3()),
                c.fov_deg,
                if c.ortho { 2.0 * c.ortho_half() as f64 } else { 0.0 }
            ));
            if v.section.enabled && v.section.mode == SectionMode::Box {
                s.push_str(&format!("BOX\t{}\t{}\n", v3(self.world(Vec3::from(v.section.bmin))), v3(self.world(Vec3::from(v.section.bmax)))));
            }
            let sel: Vec<String> = v.selected.iter().filter_map(|id| self.elem(*id).map(|(_, e)| e.guid.clone())).filter(|g| !g.is_empty()).take(2000).collect();
            if !sel.is_empty() {
                s.push_str(&format!("SEL\t{}\n", sel.iter().map(|g| clean(g)).collect::<Vec<_>>().join("\t")));
            }
            let mut text = v.comment.clone();
            for k in &v.comments {
                text.push_str(&format!("\n{} ({}): {}", k.author, crate::markup::pretty_date(&k.date), k.text));
            }
            if !text.trim().is_empty() {
                s.push_str(&format!("TXT\t{}\n", clean(text.trim())));
            }
            // markups as polylines (RED) and texts (RTX): color, thickness, points relative to the view center /
            // its height (y down) - the add-ins put them on a plane in front of the camera
            for r in &v.redlines {
                let col = format!("{} {} {}", r.color[0], r.color[1], r.color[2]);
                if r.kind == crate::markup::RedKind::Text {
                    if let Some(p) = r.pts.first() {
                        s.push_str(&format!("RTX\t{col}\t{:.1}\t{:.5} {:.5}\t{}\n", r.width, p[0], p[1], clean(&r.text)));
                    }
                    continue;
                }
                for pl in crate::markup::redline_polylines(r) {
                    if pl.len() >= 2 {
                        let pts: Vec<String> = pl.iter().map(|p| format!("{:.5} {:.5}", p[0], p[1])).collect();
                        s.push_str(&format!("RED\t{col}\t{:.1}\t{}\n", r.width, pts.join(" ")));
                    }
                }
            }
            s.push_str("END\n");
        }
        let path = dir.join("issues.txt");
        let tmp = dir.join("issues.txt.tmp");
        if std::fs::write(&tmp, s).is_ok() {
            let _ = std::fs::rename(&tmp, &path);
        }
    }

    /// "Send to ManageBIM" from Revit / AutoCAD / Plant 3D / Navisworks: the add-in writes the model to
    /// %LOCALAPPDATA%\ManageBIM\Inbox\<source>.bimc (atomic write: .tmp + rename).
    /// A new file is added to the scene; one already open and rewritten reloads by itself (live),
    /// keeping the selection, the hidden items and the colors (matching by GUID).
    pub(super) fn scan_inbox(&mut self) {
        let Some(dir) = inbox_dir() else { return };
        let Ok(rd) = std::fs::read_dir(&dir) else { return };
        static STARTED: std::sync::OnceLock<std::time::SystemTime> = std::sync::OnceLock::new();
        let started = *STARTED.get_or_init(|| std::time::SystemTime::now() - std::time::Duration::from_secs(5));
        let loading: HashSet<String> = self.loads.iter().map(|j| j.path.to_lowercase()).collect();
        let loaded: HashMap<String, usize> = self.loaded_models().map(|(i, m)| (m.path.to_lowercase(), i)).collect();
        let mut new_files = Vec::new();
        for e in rd.flatten() {
            let p = e.path();
            if crate::formats::ext_of(&p.to_string_lossy()) != "bimc" {
                continue;
            }
            let ps = p.to_string_lossy().to_string();
            let key = ps.to_lowercase();
            if loading.contains(&key) {
                continue;
            }
            if let Some(&slot) = loaded.get(&key) {
                if self.changed_files.contains(&slot) {
                    self.changed_files.remove(&slot);
                    self.toast(format!("Live update: {}", file_stem(&ps)));
                    self.reload_model(slot, ps);
                }
            } else if file_mtime(&ps).map(|t| t > started).unwrap_or(false) && !self.inbox_seen.contains(&key) {
                // only what arrived after startup (we do not reopen the old sends at every start)
                self.inbox_seen.insert(key);
                new_files.push(ps);
            }
        }
        for p in new_files {
            self.toast(format!("Received: {}", file_stem(&p)));
            let append = self.loaded_models().next().is_some() || !self.loads.is_empty();
            self.open_path(&p, append);
        }
    }
}
