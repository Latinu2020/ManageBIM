//! Issues shared through a common folder (network, synced SharePoint / OneDrive): each viewpoint /
//! issue = one `<guid>.json` file. Each ManageBIM sends what changed on its side and takes what is newer in the folder.
//! The camera / section / dimensions are written with the scene origin of the one who saves and converted on reading; elements
//! (hidden / selected) are written as GUIDs. Deletions leave a "deleted" file (so they disappear for the others too).
//! Conflict (the same issue changed both here and in the folder since the last sync): the folder variant is taken,
//! and the local one is kept as a new viewpoint "(my copy)".
use crate::app::{App, Viewpoint};
use crate::model::ElemId;
use glam::{DVec3, Vec3};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Serialize, Deserialize)]
pub struct SharedIssue {
    pub app: String,
    pub version: u32,
    pub guid: String,
    /// milliseconds since 1970 (last modification)
    pub modified: u64,
    pub by: String,
    #[serde(default)]
    pub deleted: bool,
    /// the scene origin of the writer (the local coordinates in the viewpoint are relative to it)
    pub origin: [f64; 3],
    pub vp: Option<Viewpoint>,
    #[serde(default)]
    pub hidden: Vec<String>,
    #[serde(default)]
    pub selected: Vec<String>,
}

/// The sync state (only in the current session).
#[derive(Default)]
pub struct SharedState {
    /// guid -> (modified in the folder, the hash of the local viewpoint at the last sync)
    pub known: HashMap<String, (u64, u64)>,
    /// file -> modification date (we do not re-read unchanged files)
    pub mtimes: HashMap<String, std::time::SystemTime>,
    pub last: Option<std::time::Instant>,
    pub status: String,
}

/// A valid GUID ends up in the file name: we accept only hexadecimal and '-'
/// (a corrupt JSON file in the common folder cannot write outside it).
fn safe_guid(g: &str) -> String {
    g.chars().filter(|c| c.is_ascii_hexdigit() || *c == '-').collect()
}

fn now_ms() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

fn user() -> String {
    std::env::var("USERNAME").or_else(|_| std::env::var("USER")).unwrap_or_else(|_| "user".into())
}

fn hash_vp(v: &Viewpoint) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    serde_json::to_string(v).unwrap_or_default().hash(&mut h);
    h.finish() | 1
}

/// New GUID (no libraries): time + counter + address, in 8-4-4-4-12 format.
pub fn new_guid() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static C: AtomicU64 = AtomicU64::new(0);
    let t = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0) as u64;
    let c = C.fetch_add(1, Ordering::Relaxed);
    let mut x = t ^ c.wrapping_mul(0x9e37_79b9_7f4a_7c15) ^ (std::process::id() as u64) << 32;
    let mut part = || {
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        x
    };
    let (a, b) = (part(), part());
    format!("{:08x}-{:04x}-4{:03x}-{:04x}-{:012x}", (a >> 32) as u32, (a >> 16) as u16, (a & 0xfff) as u16, (b >> 48) as u16 & 0x3fff | 0x8000, b & 0xffff_ffff_ffff)
}

/// Shifts the local coordinates of a viewpoint by `d` (a different scene origin).
pub fn shift_vp(v: &mut Viewpoint, d: Vec3) {
    if d.length_squared() == 0.0 {
        return;
    }
    v.cam.eye = (Vec3::from(v.cam.eye) + d).to_array();
    for i in 0..3 {
        v.section.bmin[i] += d[i];
        v.section.bmax[i] += d[i];
    }
    for p in &mut v.section.planes {
        let axis = match p.dir {
            0 | 1 => 2,
            2 | 3 => 0,
            _ => 1,
        };
        p.offset += d[axis];
    }
    for m in &mut v.measures {
        for p in &mut m.pts {
            *p = (Vec3::from(*p) + d).to_array();
        }
    }
}

impl App {
    /// Sync with the common folder (at most once every 4 seconds, or immediately with `force`).
    pub fn shared_sync(&mut self, force: bool) {
        let Some(dir) = self.shared_dir.clone() else { return };
        if self.loaded_models().next().is_none() || !self.loads.is_empty() {
            return;
        }
        if !force && self.shared.last.is_some_and(|t| t.elapsed().as_secs_f32() < 4.0) {
            return;
        }
        self.shared.last = Some(std::time::Instant::now());
        let dirp = std::path::PathBuf::from(&dir);
        if std::fs::create_dir_all(&dirp).is_err() || !dirp.is_dir() {
            self.shared.status = format!("Folder not reachable: {dir}");
            return;
        }
        let o = self.origin_f64();
        // guid for each issue
        for v in &mut self.viewpoints {
            if v.issue.guid.is_empty() {
                v.issue.guid = new_guid();
            }
        }
        // ---- 1) fetch: the new / changed files in the folder
        let mut by_guid: HashMap<String, ElemId> = HashMap::new();
        let mut guid_of: HashMap<ElemId, String> = HashMap::new();
        for (mi, m) in self.loaded_models() {
            for (ei, e) in m.elements.iter().enumerate() {
                if !e.guid.is_empty() {
                    let id = ElemId { m: mi as u32, e: ei as u32 };
                    by_guid.entry(e.guid.clone()).or_insert(id);
                    guid_of.insert(id, e.guid.clone());
                }
            }
        }
        let mut pulled = 0;
        let mut conflicts = 0;
        let mut files = 0;
        if let Ok(rd) = std::fs::read_dir(&dirp) {
            for e in rd.flatten() {
                let p = e.path();
                if p.extension().is_none_or(|x| x != "json") {
                    continue;
                }
                files += 1;
                let key = p.to_string_lossy().to_string();
                let mt = e.metadata().and_then(|m| m.modified()).ok();
                if mt.is_some() && self.shared.mtimes.get(&key) == mt.as_ref() {
                    continue;
                }
                let Ok(txt) = std::fs::read_to_string(&p) else { continue };
                let Ok(si) = serde_json::from_str::<SharedIssue>(&txt) else { continue };
                if si.guid.is_empty() || safe_guid(&si.guid) != si.guid {
                    continue; // invalid GUID: we ignore the file (we do not use it as a file name)
                }
                if let Some(mt) = mt {
                    self.shared.mtimes.insert(key, mt);
                }
                let known = self.shared.known.get(&si.guid).copied();
                if known.is_some_and(|k| si.modified <= k.0) {
                    continue;
                }
                let local = self.viewpoints.iter().position(|v| v.issue.guid == si.guid);
                if si.deleted {
                    if let Some(i) = local {
                        self.viewpoints.remove(i);
                        self.active_vp = None;
                        pulled += 1;
                    }
                    self.shared.known.insert(si.guid.clone(), (si.modified, 0));
                    continue;
                }
                let Some(mut vp) = si.vp else { continue };
                shift_vp(&mut vp, (DVec3::from_array(si.origin) - o).as_vec3());
                vp.hidden = si.hidden.iter().filter_map(|g| by_guid.get(g).copied()).collect();
                vp.selected = si.selected.iter().filter_map(|g| by_guid.get(g).copied()).collect();
                vp.issue.guid = si.guid.clone();
                match local {
                    Some(i) => {
                        // changed here too since the last sync -> we also keep the local variant
                        let changed_here = known.is_some_and(|k| k.1 != hash_vp(&self.viewpoints[i]));
                        if changed_here {
                            let mut mine = self.viewpoints[i].clone();
                            mine.name = format!("{} (my copy)", mine.name);
                            mine.issue.guid = new_guid();
                            self.viewpoints.push(mine);
                            conflicts += 1;
                        }
                        self.viewpoints[i] = vp;
                    }
                    None => self.viewpoints.push(vp),
                }
                if let Some(v) = self.viewpoints.iter().find(|v| v.issue.guid == si.guid) {
                    let h = hash_vp(v);
                    self.shared.known.insert(si.guid.clone(), (si.modified, h));
                }
                pulled += 1;
            }
        }
        // ---- 2) send: what changed here (or is new), plus the deletions
        let mut pushed = 0;
        let by = user();
        // A unique temporary file per process: two colleagues saving the same issue at the same
        // moment do not overwrite each other (the rename stays atomic).
        let pid = std::process::id();
        let tmp_seq = std::sync::atomic::AtomicU64::new(0);
        let write = |si: &SharedIssue| -> Option<std::time::SystemTime> {
            let guid = safe_guid(&si.guid);
            if guid.is_empty() {
                return None;
            }
            let f = dirp.join(format!("{guid}.json"));
            let n = tmp_seq.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            let tmp = dirp.join(format!("{guid}.{pid}.{n}.json.tmp"));
            let txt = serde_json::to_string_pretty(si).ok()?;
            std::fs::write(&tmp, txt).ok()?;
            if std::fs::rename(&tmp, &f).is_err() {
                let _ = std::fs::remove_file(&tmp);
                return None;
            }
            std::fs::metadata(&f).and_then(|m| m.modified()).ok()
        };
        for i in 0..self.viewpoints.len() {
            let v = &self.viewpoints[i];
            let h = hash_vp(v);
            if self.shared.known.get(&v.issue.guid).is_some_and(|k| k.1 == h) {
                continue;
            }
            let mut c = v.clone();
            let hidden = c.hidden.iter().filter_map(|id| guid_of.get(id).cloned()).collect();
            let selected = c.selected.iter().filter_map(|id| guid_of.get(id).cloned()).collect();
            c.hidden.clear();
            c.selected.clear();
            let si = SharedIssue { app: "ManageBIM".into(), version: 1, guid: v.issue.guid.clone(), modified: now_ms(), by: by.clone(), deleted: false, origin: o.to_array(), vp: Some(c), hidden, selected };
            if let Some(mt) = write(&si) {
                self.shared.mtimes.insert(dirp.join(format!("{}.json", si.guid)).to_string_lossy().to_string(), mt);
                self.shared.known.insert(si.guid.clone(), (si.modified, h));
                pushed += 1;
            }
        }
        let gone: Vec<String> = self.shared.known.iter().filter(|(g, k)| k.1 != 0 && !self.viewpoints.iter().any(|v| &v.issue.guid == *g)).map(|(g, _)| g.clone()).collect();
        for g in gone {
            let si = SharedIssue { app: "ManageBIM".into(), version: 1, guid: g.clone(), modified: now_ms(), by: by.clone(), deleted: true, origin: o.to_array(), vp: None, hidden: Vec::new(), selected: Vec::new() };
            if let Some(mt) = write(&si) {
                self.shared.mtimes.insert(dirp.join(format!("{g}.json")).to_string_lossy().to_string(), mt);
            }
            self.shared.known.insert(g, (si.modified, 0));
            pushed += 1;
        }
        if conflicts > 0 {
            self.toast(format!("Shared issues: {conflicts} changed by you and by a colleague - their version is kept, yours is saved as \"(my copy)\"."));
        } else if pulled > 0 {
            self.toast(format!("Shared issues: {pulled} updated from the shared folder."));
        }
        self.shared.status = format!("{} issues in the folder · last sync {}", files.max(self.viewpoints.len()), clock());
        let _ = pushed;
    }
}

/// Local time "HH:MM".
fn clock() -> String {
    let s = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0) + crate::ui::local_offset_secs();
    format!("{:02}:{:02}", s.rem_euclid(86_400) / 3600, s.rem_euclid(3600) / 60)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn guids_are_unique_and_well_formed() {
        let a = new_guid();
        let b = new_guid();
        assert_ne!(a, b);
        assert_eq!(a.len(), 36);
        assert_eq!(a.matches('-').count(), 4);
    }
}
