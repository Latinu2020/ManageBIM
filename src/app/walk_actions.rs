//! Sectiune din `impl App` (vezi fisierul parinte pentru restul).
use super::*;

impl App {
    /// smooth camera move (ViewCube, Home, standard views, focus)
    pub fn fly_to(&mut self, to: Camera) {
        self.cam_anim = Some((self.cam, to, 0.0));
    }

    pub fn cam_anim_tick(&mut self, dt: f32) {
        if let Some((a, b, t)) = self.cam_anim {
            let nt = (t + dt / 0.45).min(1.0);
            let k = nt * nt * (3.0 - 2.0 * nt);
            let mut dy = b.yaw - a.yaw;
            while dy > std::f32::consts::PI {
                dy -= std::f32::consts::TAU;
            }
            while dy < -std::f32::consts::PI {
                dy += std::f32::consts::TAU;
            }
            let mut c = b;
            c.eye = (a.eye() + (b.eye() - a.eye()) * k).into();
            c.yaw = a.yaw + dy * k;
            c.pitch = a.pitch + (b.pitch - a.pitch) * k;
            c.focus = a.focus + (b.focus - a.focus) * k;
            self.cam = c;
            self.cam_anim = if nt >= 1.0 { None } else { Some((a, b, nt)) };
        }
    }

    /// walking with optional collision (walls) and gravity (floors)
    pub fn walk_move(&mut self, local: Vec3, dist: f32, dt: f32) {
        let before = self.cam.eye();
        let mut probe = self.cam;
        probe.walk(local, dist);
        let after = probe.eye();
        let mv = after - before;
        let mut blocked = false;
        if self.settings.collision && mv.length() > 1e-6 {
            let dir = mv.normalize();
            let h = self.settings.eye_height;
            for z in [0.0, -h * 0.45, -h * 0.75] {
                let o = before + Vec3::new(0.0, 0.0, z);
                if let Some((_, t, _, _)) = self.pick(o, dir) {
                    if t < mv.length() + 0.3 {
                        blocked = true;
                        break;
                    }
                }
            }
        }
        if !blocked {
            self.cam.eye = after.into();
        }
        let _ = dt;
    }

    /// Mers lin: tastele si joystick-ul de mouse seteaza o viteza TINTA (`walk_target`, `walk_turn_target`)
    /// in fiecare cadru; aici viteza reala se apropie de ea cu accelerare / franare (fara salturi,
    /// independent de FPS), apoi tintele se reseteaza pentru cadrul urmator.
    pub fn walk_tick(&mut self, dt: f32) {
        let dt = dt.clamp(0.0, 0.05);
        if self.path_run.is_some() {
            self.walk_path_tick(dt);
            self.walk_target = Vec3::ZERO;
            self.walk_turn_target = 0.0;
            self.walk_vel = Vec3::ZERO;
            self.walk_turn = 0.0;
            return;
        }
        let speed = self.settings.walk_speed.clamp(0.05, 50.0);
        self.walk_vel = walk::smooth(self.walk_vel, self.walk_target * speed, dt);
        self.walk_turn = walk::smooth1(self.walk_turn, self.walk_turn_target, dt);
        self.walk_target = Vec3::ZERO;
        self.walk_turn_target = 0.0;
        if self.walk_turn.abs() > 1e-4 {
            self.cam.look(self.walk_turn * dt, 0.0);
        } else {
            self.walk_turn = 0.0;
        }
        let v = self.walk_vel;
        if v.length() > 1e-3 {
            self.cam_anim = None;
            let d = v.length() * dt;
            self.walk_move(v / v.length(), d, dt);
        } else {
            self.walk_vel = Vec3::ZERO;
        }
    }

    /// se misca (sau incetineste) camera in modul mers - pentru repaint continuu
    pub fn walk_active(&self) -> bool {
        self.walk_vel != Vec3::ZERO || self.walk_turn != 0.0 || self.path_run.as_ref().map(|r| !r.paused).unwrap_or(false)
    }

    pub(super) fn walk_path_tick(&mut self, dt: f32) {
        let (speed, eye) = (self.settings.walk_speed.clamp(0.05, 50.0), self.settings.eye_height);
        let Some(run) = self.path_run.as_mut() else { return };
        self.cam_anim = None;
        if run.step(&mut self.cam, speed, eye, dt) && !run.paused {
            run.paused = true;
            self.toast("End of the walk path.");
        }
    }

    /// Punctul de pe podea de sub cursor (pentru traseul de mers): pe o suprafata orizontala / rampa
    /// direct; pe un perete sau tavan coboara cu o raza verticala pe podeaua de sub el.
    pub fn walk_floor_hit(&self, uv: glam::Vec2, aspect: f32) -> Option<Vec3> {
        let (o, d) = self.cam.ray(uv, aspect);
        let (_, _, hit, tri) = self.pick(o, d)?;
        let n = (tri[1] - tri[0]).cross(tri[2] - tri[0]).normalize_or_zero();
        // triunghi degenerat (punct de nor) sau suprafata de calcat (panta < ~60°)
        if n == Vec3::ZERO || n.z.abs() > 0.5 {
            return Some(hit);
        }
        // perete: putin inapoi spre camera, apoi in jos pana la podea
        let back = hit - d.normalize_or_zero() * 0.15;
        let (_, t, _, _) = self.pick(back + Vec3::Z * 0.05, Vec3::NEG_Z)?;
        Some(back + Vec3::Z * 0.05 - Vec3::Z * t)
    }

    /// Porneste / opreste (pauza) mersul pe traseu.
    pub fn walk_path_play(&mut self) {
        if let Some(r) = self.path_run.as_mut() {
            if r.dist >= r.length() {
                r.dist = 0.0;
            }
            r.paused = !r.paused;
            return;
        }
        match walk::PathRun::new(&self.walk_path) {
            Some(r) => {
                self.tool = Tool::Select;
                self.nav = NavMode::Walk;
                self.path_run = Some(r);
            }
            None => self.toast("Draw a walk path first: Walk Path > Draw, then click at least 2 points on the floor."),
        }
    }

    pub fn walk_path_stop(&mut self) {
        self.path_run = None;
    }

    /// Traseul ca animatie (Animator): se poate edita, reda si exporta video.
    pub fn walk_path_to_animation(&mut self) {
        let Some(r) = walk::PathRun::new(&self.walk_path) else {
            self.toast("Draw a walk path first (at least 2 points).");
            return;
        };
        let name = format!("Walk path {}", self.animations.len() + 1);
        let a = r.to_animation(name.clone(), self.settings.walk_speed, self.settings.eye_height, self.cam.fov_deg);
        let secs = a.duration();
        self.stop_anim();
        self.animations.push(a);
        self.cur_anim = self.animations.len() - 1;
        self.anim_time = 0.0;
        self.open_panel(crate::layout::Panel::Animation);
        self.toast(format!("Animation \"{name}\" created ({:.0} m, {:.0} s) - press ▶ in Animations to play it.", r.length(), secs));
    }

    pub fn gravity_tick(&mut self, dt: f32) {
        if !(self.settings.gravity && self.nav == NavMode::Walk) {
            self.fall_speed = 0.0;
            return;
        }
        let eye = self.cam.eye();
        let h = self.settings.eye_height;
        if let Some((_, t, _, _)) = self.pick(eye + Vec3::new(0.0, 0.0, -0.05), Vec3::NEG_Z) {
            let ground = eye.z - 0.05 - t;
            let target = ground + h;
            if eye.z > target + 0.01 {
                self.fall_speed = (self.fall_speed + 9.81 * dt).min(30.0);
                let nz = (eye.z - self.fall_speed * dt).max(target);
                self.cam.eye[2] = nz;
            } else if eye.z < target - 0.01 && target - eye.z < 0.6 {
                // trepte / rampe: urca lin
                self.cam.eye[2] = eye.z + (target - eye.z).min(4.0 * dt);
                self.fall_speed = 0.0;
            } else {
                self.fall_speed = 0.0;
            }
        }
    }
}

impl App {
    /// Traseul de mers pentru Navisworks: `Outbox\walkpath.txt`, citit de comanda add-in-ului
    /// "ManageBIM Walk path" (o animatie de viewpoint-uri). Puncte in coordonate reale (metri), la ochi.
    pub fn send_walk_path_to_navisworks(&mut self) {
        if self.walk_path.len() < 2 {
            return self.error("Draw a walk path first (at least two points).");
        }
        let Some(dir) = crate::formats::data_root().map(|d| d.join("ManageBIM").join("Outbox")) else { return };
        if let Err(e) = std::fs::create_dir_all(&dir) {
            return self.error(format!("Cannot create {}: {e}", dir.display()));
        }
        let name = self.project_path.as_deref().map(crate::file_name).unwrap_or_else(|| "Walk path".into());
        let text = walk_path_text(&format!("Walk - {name}"), self.settings.walk_speed, self.settings.eye_height, self.cam.fov_deg, &self.walk_path.iter().map(|p| self.world(*p)).collect::<Vec<_>>());
        let p = dir.join("walkpath.txt");
        match crate::formats::write_atomic(&p, text.as_bytes(), false) {
            Ok(()) => self.toast("Walk path sent - in Navisworks: ManageBIM › Walk path (it becomes a viewpoint animation in Saved Viewpoints)."),
            Err(e) => self.error(format!("Cannot write {}: {e}", p.display())),
        }
    }
}

/// Formatul MBWALK 1: NAME, SPEED (m/s), EYE (m), FOV (grade), apoi un rand P x y z per punct (pardoseala, metri).
pub fn walk_path_text(name: &str, speed: f32, eye: f32, fov: f32, pts: &[glam::DVec3]) -> String {
    let mut s = String::from("MBWALK 1\n");
    s.push_str(&format!("NAME\t{}\n", name.replace(['\t', '\n', '\r'], " ")));
    s.push_str(&format!("SPEED\t{speed:.3}\nEYE\t{eye:.3}\nFOV\t{fov:.2}\n"));
    for p in pts {
        s.push_str(&format!("P\t{:.4} {:.4} {:.4}\n", p.x, p.y, p.z));
    }
    s
}

#[cfg(test)]
mod walk_text_tests {
    #[test]
    fn walk_path_file_format() {
        let t = super::walk_path_text("Walk - A", 1.4, 1.7, 60.0, &[glam::DVec3::new(0.0, 0.0, 0.0), glam::DVec3::new(10.0, 0.0, 0.0)]);
        assert!(t.starts_with("MBWALK 1\nNAME\tWalk - A\nSPEED\t1.400\nEYE\t1.700\nFOV\t60.00\n"));
        assert_eq!(t.lines().filter(|l| l.starts_with("P\t")).count(), 2);
    }
}
