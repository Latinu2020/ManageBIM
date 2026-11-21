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
