//! Issue-uri partajate printr-un folder comun (retea, SharePoint / OneDrive sincronizat): fiecare viewpoint /
//! issue = un fisier `<guid>.json`. Fiecare ManageBIM trimite ce s-a schimbat la el si preia ce e mai nou in folder.
//! Camera / sectiunea / cotele se scriu cu originea scenei celui care salveaza si se convertesc la citire; elementele
//! (ascunse / selectate) se scriu ca GUID-uri. Stergerile lasa un fisier "deleted" (ca sa dispara si la ceilalti).
//! Conflict (acelasi issue schimbat si aici si in folder de la ultima sincronizare): varianta din folder se preia,
//! iar cea locala se pastreaza ca viewpoint nou "(my copy)".
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
    /// milisecunde de la 1970 (ultima modificare)
    pub modified: u64,
    pub by: String,
    #[serde(default)]
    pub deleted: bool,
    /// originea scenei celui care a scris (coordonatele locale din viewpoint sunt fata de ea)
    pub origin: [f64; 3],
    pub vp: Option<Viewpoint>,
    #[serde(default)]
    pub hidden: Vec<String>,
    #[serde(default)]
    pub selected: Vec<String>,
}

/// Starea sincronizarii (doar in sesiunea curenta).
#[derive(Default)]
pub struct SharedState {
    /// guid -> (modified din folder, hash-ul viewpoint-ului local la ultima sincronizare)
    pub known: HashMap<String, (u64, u64)>,
    /// fisier -> data modificarii (nu recitim fisierele neschimbate)
    pub mtimes: HashMap<String, std::time::SystemTime>,
    pub last: Option<std::time::Instant>,
    pub status: String,
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

/// GUID nou (fara biblioteci): timp + contor + adresa, in format 8-4-4-4-12.
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

/// Muta coordonatele locale ale unui viewpoint cu `d` (alta origine a scenei).
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
    /// Sincronizare cu folderul comun (cel mult o data la 4 secunde, sau imediat cu `force`).
    pub fn shared_sync(&mut self, force: bool) {
        let Some(dir) = self.shared_dir.clone() else { return };
        if self.loaded_models().next().is_none() || !self.loads.is_empty() {
            return;
        }
        if !force && self.shared.last.map_or(false, |t| t.elapsed().as_secs_f32() < 4.0) {
            return;
        }
        self.shared.last = Some(std::time::Instant::now());
        let dirp = std::path::PathBuf::from(&dir);
        if std::fs::create_dir_all(&dirp).is_err() || !dirp.is_dir() {
            self.shared.status = format!("Folder not reachable: {dir}");
            return;
        }
        let o = self.origin_f64();
        // guid pentru fiecare issue
        for v in &mut self.viewpoints {
            if v.issue.guid.is_empty() {
                v.issue.guid = new_guid();
            }
        }
        // ---- 1) preluare: fisierele noi / schimbate din folder
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
                if p.extension().map_or(true, |x| x != "json") {
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
                if let Some(mt) = mt {
                    self.shared.mtimes.insert(key, mt);
                }
                let known = self.shared.known.get(&si.guid).copied();
                if known.map_or(false, |k| si.modified <= k.0) {
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
                        // schimbat si aici de la ultima sincronizare -> pastram si varianta locala
                        let changed_here = known.map_or(false, |k| k.1 != hash_vp(&self.viewpoints[i]));
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
                let h = hash_vp(self.viewpoints.iter().find(|v| v.issue.guid == si.guid).unwrap());
                self.shared.known.insert(si.guid.clone(), (si.modified, h));
                pulled += 1;
            }
        }
        // ---- 2) trimitere: ce s-a schimbat aici (sau e nou), plus stergerile
        let mut pushed = 0;
        let by = user();
        let write = |si: &SharedIssue| -> Option<std::time::SystemTime> {
            let f = dirp.join(format!("{}.json", si.guid));
            let tmp = dirp.join(format!("{}.json.tmp", si.guid));
            let txt = serde_json::to_string_pretty(si).ok()?;
            std::fs::write(&tmp, txt).ok()?;
            std::fs::rename(&tmp, &f).ok()?;
            std::fs::metadata(&f).and_then(|m| m.modified()).ok()
        };
        for i in 0..self.viewpoints.len() {
            let v = &self.viewpoints[i];
            let h = hash_vp(v);
            if self.shared.known.get(&v.issue.guid).map_or(false, |k| k.1 == h) {
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

/// Ora locala "HH:MM".
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
