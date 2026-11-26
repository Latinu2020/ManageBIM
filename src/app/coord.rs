//! Obiectele de coordonare (clearance zones, provision for void, placeholder volumes): creare din
//! selectie, goluri automate MEP x pereti / placi, modelul virtual si testul de clash dedicat.
use super::*;
use crate::coord_objects::{void_at, CoordKind, CoordObject, COORD_NAME, COORD_PATH};

/// tipurile IFC ale instalatiilor care cer goluri (conducte, tubulatura, jgheaburi de cabluri)
fn is_service(t: &str) -> bool {
    let t = t.to_lowercase();
    ["pipe", "duct", "cablecarrier", "cable carrier", "flowsegment", "conduit", "tray"].iter().any(|k| t.contains(k)) && !t.contains("fitting") && !t.contains("terminal")
}
/// elementele care primesc goluri (pereti si placi)
fn is_host(t: &str) -> bool {
    let t = t.to_lowercase();
    (t.contains("wall") || t.contains("slab") || t.contains("floor") || t.contains("roof")) && !t.contains("opening") && !t.contains("covering")
}

impl App {
    /// Reface modelul virtual "Coordination objects" din lista de obiecte (slot dedicat).
    pub fn rebuild_coord_model(&mut self) {
        if self.coord_objects.is_empty() && self.coord_slot.is_none() {
            return;
        }
        let origin = {
            let mut o = self.origin.lock().unwrap_or_else(|e| e.into_inner());
            *o.get_or_insert(DVec3::ZERO)
        };
        let slot = match self.coord_slot {
            Some(s) if s < self.models.len() => s,
            _ => {
                let s = self.models.len();
                self.push_slot();
                self.coord_slot = Some(s);
                s
            }
        };
        let raw = crate::coord_objects::to_raw(&self.coord_objects, origin);
        let m = Arc::new(Model::from_raw(raw, COORD_NAME.into(), COORD_PATH.into(), origin));
        // obiectele ascunse din panou raman ascunse; restul se vad
        self.hidden.retain(|id| id.m as usize != slot);
        for (k, o) in self.coord_objects.iter().enumerate() {
            if !o.visible {
                self.hidden.insert(ElemId { m: slot as u32, e: k as u32 });
            }
        }
        self.selected.retain(|id| id.m as usize != slot || (id.e as usize) < self.coord_objects.len());
        self.base_models[slot] = Some(m.clone());
        self.trees[slot] = Some(ModelTree::build(&m, &self.settings.tree_levels));
        self.apply_transform(slot);
        self.refresh_caches();
    }

    fn next_coord_id(&self) -> u64 {
        let t = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_micros() as u64).unwrap_or(1);
        let max = self.coord_objects.iter().map(|o| o.id).max().unwrap_or(0);
        t.max(max + 1)
    }

    /// cutia selectiei (fara obiectele de coordonare), in coordonate locale
    fn selection_box(&self) -> Option<(Vec3, Vec3)> {
        let mut bb = Aabb::EMPTY;
        for id in &self.selected {
            if Some(id.m as usize) == self.coord_slot {
                continue;
            }
            if let Some((_, e)) = self.elem(*id) {
                if e.aabb.is_valid() {
                    bb.union(&e.aabb);
                }
            }
        }
        bb.is_valid().then_some((bb.min, bb.max))
    }

    /// Obiect nou: in jurul selectiei (+ marja) sau, fara selectie, in centrul vederii.
    pub fn add_coord_object(&mut self, kind: CoordKind) {
        let margin = match kind {
            CoordKind::Clearance => self.coord_margin.max(0.0),
            CoordKind::ProvisionForVoid => 0.05,
            CoordKind::Placeholder => 0.0,
        };
        let (a, b) = match self.selection_box() {
            Some((a, b)) => (a - Vec3::splat(margin), b + Vec3::splat(margin)),
            None => {
                let c = self.cam.target();
                let h = match kind {
                    CoordKind::ProvisionForVoid => Vec3::new(0.3, 0.3, 0.3),
                    _ => Vec3::new(0.6, 0.6, 1.0),
                };
                (c - h, c + h)
            }
        };
        let n = self.coord_objects.iter().filter(|o| o.kind == kind).count() + 1;
        let mut o = CoordObject::new(self.next_coord_id(), kind, a, b, n);
        if kind == CoordKind::Clearance {
            if let Some((_, e)) = self.selected.iter().next().and_then(|id| self.elem(*id)) {
                o.name = format!("Clearance - {}", e.name);
            }
        }
        self.coord_objects.push(o);
        self.coord_sel = Some(self.coord_objects.len() - 1);
        self.coord_open = true;
        self.rebuild_coord_model();
        self.toast(format!("{} added - edit its size in the Coordination objects window; it takes part in Clash Detective.", kind.label()));
    }

    /// Goluri propuse automat: fiecare instalatie (conducta / tubulatura / jgheab) care traverseaza un
    /// perete sau o placa primeste un gol cu marja `void_margin` (doar in selectie, daca exista una).
    pub fn auto_voids(&mut self) {
        let restrict: Option<BTreeSet<ElemId>> = (!self.selected.is_empty()).then(|| self.selected.clone());
        let mut services = Vec::new();
        let mut hosts = Vec::new();
        for (mi, m) in self.loaded_models() {
            if Some(mi) == self.coord_slot || m.cloud.is_some() {
                continue;
            }
            for (ei, e) in m.elements.iter().enumerate() {
                let id = ElemId { m: mi as u32, e: ei as u32 };
                if e.tri_count == 0 || self.hidden.contains(&id) || !e.aabb.is_valid() {
                    continue;
                }
                if is_service(&e.ifc_type) && restrict.as_ref().is_none_or(|r| r.contains(&id)) {
                    services.push((id, e.name.clone(), e.aabb));
                } else if is_host(&e.ifc_type) {
                    hosts.push((id, e.name.clone(), e.guid.clone(), e.aabb));
                }
            }
        }
        if services.is_empty() {
            return self.error("No pipes, ducts or cable trays found (select some, or load an MEP model). Voids are made where they cross walls and slabs.");
        }
        // golurile deja existente (acelasi loc) nu se dubleaza
        let existing: Vec<(Vec3, Vec3)> = self.coord_objects.iter().filter(|o| o.kind == CoordKind::ProvisionForVoid).map(|o| (Vec3::from(o.min), Vec3::from(o.max))).collect();
        let mut added = 0;
        let margin = self.void_margin.max(0.0);
        for (_, sname, sb) in &services {
            for (_, hname, hguid, hb) in &hosts {
                if !sb.overlaps(hb, 0.0) {
                    continue;
                }
                // trecere reala: instalatia traverseaza TOATA grosimea gazdei (nu merge in lungul ei, in perete)
                let hs = hb.size();
                let thin = if hs.x <= hs.y && hs.x <= hs.z { 0 } else if hs.y <= hs.z { 1 } else { 2 };
                if sb.min[thin] > hb.min[thin] + 1e-3 || sb.max[thin] < hb.max[thin] - 1e-3 {
                    continue;
                }
                // ...si directia ei principala e perpendiculara pe gazda (o tubulatura in lungul unui
                // perete subtire il "acopera" in grosime, dar nu trece prin el)
                let ss = sb.size();
                if ss[thin] < ss.max_element() * 0.8 {
                    continue;
                }
                let Some((a, b)) = void_at((sb.min, sb.max), (hb.min, hb.max), margin) else { continue };
                if existing.iter().any(|(ea, eb)| (*ea - a).length() < 0.02 && (*eb - b).length() < 0.02) {
                    continue;
                }
                let n = self.coord_objects.iter().filter(|o| o.kind == CoordKind::ProvisionForVoid).count() + 1;
                let mut o = CoordObject::new(self.next_coord_id(), CoordKind::ProvisionForVoid, a, b, n);
                o.name = format!("Void {n} - {sname}");
                o.host = if hguid.is_empty() { hname.clone() } else { format!("{hname} ({hguid})") };
                o.note = format!("Margin {:.0} mm", margin * 1000.0);
                self.coord_objects.push(o);
                added += 1;
                if added >= 5000 {
                    break;
                }
            }
        }
        if added == 0 {
            return self.toast("No new crossings: the services do not pass through walls or slabs (or voids already exist).");
        }
        self.coord_open = true;
        self.rebuild_coord_model();
        self.toast(format!("{added} provision(s) for void proposed where services cross walls / slabs - review them in the Coordination objects window."));
    }

    /// Test de clash: obiectele de coordonare contra restului modelului (ex. ce intra in zonele de mentenanta).
    pub fn coord_clash_test(&mut self) {
        if self.coord_objects.is_empty() {
            return self.error("Add a clearance zone, void or placeholder first.");
        }
        let mut t = ClashTest::new("Coordination objects vs model".into());
        t.a = Selector { kind: SelKind::Model, value: COORD_NAME.into() };
        t.b = Selector { kind: SelKind::All, value: String::new() };
        t.ty = ClashType::Hard;
        t.tol_mm = 0.0;
        // obiectele de coordonare nu se verifica intre ele
        t.ignore_same_file = true;
        self.clash_tests.push(t);
        self.cur_test = self.clash_tests.len() - 1;
        self.open_panel(crate::layout::Panel::Clash);
        self.toast("Clash test created: Coordination objects vs all models - press Run Test.");
    }

    /// Trimite obiectele de coordonare VIZIBILE (clearance, voids, placeholders) catre add-in-ul Revit,
    /// doar pentru afisare: %LOCALAPPDATA%\ManageBIM\Outbox\coordination.txt, coordonate WORLD in metri.
    /// In Revit, comanda "ManageBIM Coordination" le arata ca forme transparente (Generic Models),
    /// fara sa taie sau sa modifice elementele modelului.
    pub fn send_coord_to_revit(&mut self) {
        let o = self.origin_f64();
        let objs: Vec<&CoordObject> = self.coord_objects.iter().filter(|c| c.visible).collect();
        if objs.is_empty() {
            return self.error("No visible coordination objects to send.");
        }
        let mut s = String::from("# ManageBIM coordination objects for Revit (display only) - world coordinates in metres\n");
        s.push_str("# kind\tname\tcentre x y z\tsize x y z\trotation deg\tr g b a\tguid\tstatus\n");
        for c in &objs {
            let ctr = c.center().as_dvec3() + o;
            let sz = c.size();
            let col = c.kind.color();
            let kind = match c.kind {
                CoordKind::Clearance => "clearance",
                CoordKind::ProvisionForVoid => "void",
                CoordKind::Placeholder => "placeholder",
            };
            s.push_str(&format!(
                "{kind}\t{}\t{:.4} {:.4} {:.4}\t{:.4} {:.4} {:.4}\t{:.3}\t{} {} {} {}\t{}\t{}\n",
                c.name.replace(['\t', '\n'], " "),
                ctr.x, ctr.y, ctr.z,
                sz.x, sz.y, sz.z,
                c.rot_deg,
                col[0], col[1], col[2], col[3],
                crate::ifc_export::guid_from(&c.guid_seed()),
                c.status,
            ));
        }
        let Some(dir) = crate::formats::data_root().map(|d| d.join("ManageBIM").join("Outbox")) else { return self.error("LOCALAPPDATA is not set") };
        let _ = std::fs::create_dir_all(&dir);
        match std::fs::write(dir.join("coordination.txt"), s) {
            Ok(()) => self.toast(format!("{} coordination object(s) sent to Revit - in Revit press ManageBIM › Coordination to show them.", objs.len())),
            Err(e) => self.error(format!("Cannot write the coordination objects for Revit: {e}")),
        }
    }

    pub fn remove_coord_object(&mut self, i: usize) {
        if i < self.coord_objects.len() {
            self.coord_objects.remove(i);
            self.coord_sel = None;
            // indexii elementelor de dupa cel sters se schimba
            if let Some(s) = self.coord_slot {
                self.selected.retain(|id| id.m as usize != s);
            }
            self.rebuild_coord_model();
        }
    }
}
