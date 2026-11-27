//! Camera: orbit, pan, zoom toward the cursor, walk, standard views.
use crate::model::Aabb;
use glam::{Mat4, Quat, Vec2, Vec3};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct Camera {
    pub eye: [f32; 3],
    pub yaw: f32,
    pub pitch: f32,
    /// the distance to the focus point
    pub focus: f32,
    pub fov_deg: f32,
    /// orthographic (parallel) projection instead of perspective
    #[serde(default)]
    pub ortho: bool,
}

impl Default for Camera {
    fn default() -> Self {
        Camera { eye: [-10.0, -10.0, 8.0], yaw: 0.785, pitch: -0.5, focus: 15.0, fov_deg: 50.0, ortho: false }
    }
}

impl Camera {
    pub fn eye(&self) -> Vec3 {
        Vec3::from(self.eye)
    }
    pub fn fwd(&self) -> Vec3 {
        Vec3::new(self.pitch.cos() * self.yaw.cos(), self.pitch.cos() * self.yaw.sin(), self.pitch.sin())
    }
    pub fn right(&self) -> Vec3 {
        self.fwd().cross(Vec3::Z).normalize_or(Vec3::X)
    }
    pub fn up(&self) -> Vec3 {
        self.right().cross(self.fwd())
    }
    pub fn target(&self) -> Vec3 {
        self.eye() + self.fwd() * self.focus
    }
    pub fn fov(&self) -> f32 {
        self.fov_deg.to_radians()
    }
    pub fn view(&self) -> Mat4 {
        Mat4::look_to_rh(self.eye(), self.fwd(), Vec3::Z)
    }
    pub fn proj(&self, aspect: f32, scene: &Aabb) -> Mat4 {
        let (near, far) = self.clip(scene);
        if self.ortho {
            let h = self.ortho_half();
            let a = aspect.max(0.01);
            // negative near plane: nothing is clipped when the camera enters the model
            return Mat4::orthographic_rh_gl(-h * a, h * a, -h, h, -far, far);
        }
        Mat4::perspective_rh_gl(self.fov(), aspect.max(0.01), near, far)
    }

    /// half height of the orthographic view volume (matches the perspective view at the focus distance)
    pub fn ortho_half(&self) -> f32 {
        self.focus.max(0.05) * (self.fov() * 0.5).tan()
    }

    /// direction from the target towards the eye, for the ViewCube
    pub fn set_dir(&mut self, from_target: Vec3) {
        let f = -from_target.normalize_or(Vec3::Z);
        let t = self.target();
        self.pitch = f.z.clamp(-1.0, 1.0).asin().clamp(-1.55, 1.55);
        self.yaw = if f.x.abs() + f.y.abs() < 1e-4 { std::f32::consts::FRAC_PI_2 } else { f.y.atan2(f.x) };
        self.eye = (t - self.fwd() * self.focus).into();
    }
    pub fn clip(&self, scene: &Aabb) -> (f32, f32) {
        if !scene.is_valid() {
            return (0.05, 2000.0);
        }
        let r = scene.size().length() * 0.5;
        let d = (scene.center() - self.eye()).length();
        let far = (d + r) * 1.2 + 1.0;
        let near = (far / 30000.0).clamp(0.005, 0.1);
        (near, far)
    }

    pub fn orbit(&mut self, pivot: Vec3, dyaw: f32, dpitch: f32) {
        let np = (self.pitch + dpitch).clamp(-1.55, 1.55);
        let dp = np - self.pitch;
        let q = Quat::from_rotation_z(dyaw) * Quat::from_axis_angle(self.right(), dp);
        self.eye = (pivot + q * (self.eye() - pivot)).into();
        self.yaw += dyaw;
        self.pitch = np;
    }

    pub fn look(&mut self, dyaw: f32, dpitch: f32) {
        self.yaw += dyaw;
        self.pitch = (self.pitch + dpitch).clamp(-1.55, 1.55);
    }

    /// offset in pixels, `h` the viewport height
    pub fn pan(&mut self, d: Vec2, h: f32) {
        let s = 2.0 * self.focus * (self.fov() * 0.5).tan() / h.max(1.0);
        let e = self.eye() + (-self.right() * d.x + self.up() * d.y) * s;
        self.eye = e.into();
    }

    /// zoom toward the direction `ray` (under the cursor)
    pub fn zoom(&mut self, steps: f32, ray: Vec3, min_step: f32) {
        let factor = 0.85f32.powf(steps);
        if self.ortho {
            // zoom toward the cursor: shrinks the volume and shifts laterally toward the point under the cursor
            let lateral = ray - self.fwd() * ray.dot(self.fwd());
            let nf = (self.focus * factor).max(0.05);
            let shift = lateral * (self.focus - nf);
            self.eye = (self.eye() + shift + self.fwd() * (self.focus - nf)).into();
            self.focus = nf;
            return;
        }
        let mv = (self.focus * (1.0 - factor)).clamp(-self.focus * 2.0, self.focus * 0.9);
        let mv = if mv.abs() < min_step { min_step * mv.signum() } else { mv };
        self.eye = (self.eye() + ray * mv).into();
        self.focus = (self.focus - mv * ray.dot(self.fwd())).max(0.05);
    }

    pub fn walk(&mut self, local: Vec3, dist: f32) {
        let f = self.fwd();
        let fh = Vec3::new(f.x, f.y, 0.0).normalize_or(f);
        let r = self.right();
        let e = self.eye() + (fh * local.y + r * local.x + Vec3::Z * local.z) * dist;
        self.eye = e.into();
    }

    pub fn fit(&mut self, bb: &Aabb) {
        if !bb.is_valid() {
            return;
        }
        let r = (bb.size().length() * 0.5).max(0.2);
        let d = r / (self.fov() * 0.5).sin() * 1.05;
        self.eye = (bb.center() - self.fwd() * d).into();
        self.focus = d;
    }

    pub fn set_view(&mut self, yaw: f32, pitch: f32, bb: &Aabb) {
        self.yaw = yaw;
        self.pitch = pitch;
        self.fit(bb);
    }

    /// the ray (origin, direction) through point `p` in [0,1]x[0,1] (top-left = 0,0)
    pub fn ray(&self, uv: Vec2, aspect: f32) -> (Vec3, Vec3) {
        let t = (self.fov() * 0.5).tan();
        let x = (uv.x * 2.0 - 1.0) * t * aspect;
        let y = (1.0 - uv.y * 2.0) * t;
        if self.ortho {
            // parallel rays; the origin far behind so it also catches what is behind the eye
            let o = self.eye() + (self.right() * x + self.up() * y) * self.focus - self.fwd() * (self.focus * 20.0 + 500.0);
            return (o, self.fwd());
        }
        let d = (self.fwd() + self.right() * x + self.up() * y).normalize();
        (self.eye(), d)
    }
}
