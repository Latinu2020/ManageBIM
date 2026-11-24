//! Randare OpenGL (glow) a modelelor federate, in interiorul unui callback egui.
use crate::model::Model;
use crate::video::Frame;
use eframe::glow::{self, HasContext};
use glam::{Mat4, Vec3, Vec4};
use std::collections::HashMap;

pub struct GpuMesh {
    vao: glow::VertexArray,
    vbo_pos: glow::Buffer,
    vbo_nrm: glow::Buffer,
    vbo_col: glow::Buffer,
    vbo_mat: glow::Buffer,
    count: i32,
}

pub enum Upload {
    Mesh { slot: u32, pos: Vec<f32>, nrm: Vec<i8>, col: Vec<u8> },
    Colors { slot: u32, col: Vec<u8>, mat: Vec<u8> },
    /// doar o portiune din culori (elementele care s-au schimbat), incepand de la varful `first`
    ColorRange { slot: u32, first: u32, col: Vec<u8>, mat: Vec<u8> },
    Remove { slot: u32 },
    /// punctele unui nor (E57); None = slotul nu are nor
    Points { slot: u32, cloud: Option<std::sync::Arc<crate::model::PointCloud>> },
    /// all texture layers (TEX_SIZE x TEX_SIZE RGBA each)
    Textures(Vec<Vec<u8>>),
}

/// Ce se deseneaza dintr-un model in acest cadru (intervale de varfuri): all = elementele vizibile (pasul opac),
/// transp = doar cele cu ceva transparent (pasul transparent). Fara lista = tot modelul (ca inainte).
pub struct DrawList {
    pub all: Vec<(i32, i32)>,
    pub transp: Vec<(i32, i32)>,
    /// intervale de puncte (nor de puncte): scanarile vizibile, primele N% din fiecare
    pub points: Vec<(i32, i32)>,
}

struct GpuPoints {
    vao: glow::VertexArray,
    vbo_pos: glow::Buffer,
    vbo_col: glow::Buffer,
    count: i32,
}

pub struct Renderer {
    /// listele de desenare pe slot, calculate de aplicatie pentru vederea curenta (None = tot)
    pub lists: Option<HashMap<u32, DrawList>>,
    prog: Option<glow::Program>,
    meshes: HashMap<u32, GpuMesh>,
    pprog: Option<glow::Program>,
    points: HashMap<u32, GpuPoints>,
    pub pending: Vec<Upload>,
    pub error: Option<String>,
    /// off-screen render request (Render tab) and its result
    pub capture: Option<CaptureReq>,
    pub captured: Option<Result<Frame, String>>,
    fbo: Option<(glow::Framebuffer, glow::Renderbuffer, glow::Renderbuffer, u32, u32)>,
    tex: Option<glow::Texture>,
    /// umbrire de contact (SSAO): programul, tintele (cache pe marime) si un VAO gol pentru triunghiul pe tot ecranul
    aoprog: Option<glow::Program>,
    ao_targets: Vec<AoTargets>,
    ao_vao: Option<glow::VertexArray>,
    /// ceva n-a mers (FBO incomplet etc.): desenare normala pana la repornire
    pub ao_failed: bool,
    /// umbre: programul de adancime, FBO + textura de adancime, si matricea / parametrii cadrului curent
    sprog: Option<glow::Program>,
    shadow_fbo: Option<(glow::Framebuffer, glow::Texture)>,
    shadow_state: Option<(Mat4, f32, f32)>,
    shadow_failed: bool,
}

/// marimea hartii de umbre (pixeli)
const SHADOW_SIZE: i32 = 2048;

struct AoTargets {
    w: i32,
    h: i32,
    samples: i32,
    /// FBO multisample (antialiasing) - None cand samples <= 1
    ms: Option<(glow::Framebuffer, glow::Renderbuffer, glow::Renderbuffer)>,
    fbo: glow::Framebuffer,
    col: glow::Texture,
    dep: glow::Texture,
}

#[derive(Clone, Copy)]
pub struct CaptureReq {
    pub params: FrameParams,
    pub w: u32,
    pub h: u32,
    pub bg_top: [u8; 3],
    pub bg_bottom: [u8; 3],
}

#[derive(Clone, Copy)]
pub struct FrameParams {
    pub view_proj: Mat4,
    pub eye: Vec3,
    pub planes: [Vec4; 6],
    pub nplanes: i32,
    /// direction towards the sun (world)
    pub sun: Vec3,
    /// lumina: x = expunere, y = ambient, z = intensitatea soarelui (1 = implicit)
    pub light: Vec3,
    /// marimea punctelor norilor (pixeli)
    pub point_size: f32,
    /// fetele taiate de sectiune desenate pline (capace)
    pub caps: bool,
    /// umbrire de contact: 0 = oprita; altfel taria (0..1)
    pub ao: f32,
    /// proiectia: (P22, P32, ortografic) pentru adancimea liniara si P11 (scara pe Y)
    /// margini fine (0 = oprite; altfel taria)
    pub edges: f32,
    pub depth_k: [f32; 4],
    /// stilul de afisare
    pub style: VisualStyle,
    /// umbre aruncate de soare + sfera care cuprinde scena (centru, raza) pentru harta de umbre
    pub shadows: bool,
    /// lampi in model: xyz = pozitie, w = putere (lux / 20)
    pub lamps: [Vec4; 8],
    /// rgb = culoare, w = raza (m)
    pub lamp_data: [Vec4; 8],
    /// xyz = directia luminii (spre tinta), w = tipul: -2 punct, 2 distanta (soare), altfel cos(jumatate de con) pentru spot
    pub lamp_dir: [Vec4; 8],
    pub nlamps: i32,
    /// scena de noapte: cer si lumina ambientala aproape stinse, doar lampile lumineaza
    pub night: bool,
    pub scene: Vec4,
}

const VS_P: &str = r#"
layout(location=0) in vec3 a_pos;
layout(location=1) in vec4 a_col;
uniform mat4 u_vp;
uniform float u_size;
out vec3 v_pos;
out vec4 v_col;
void main() {
    v_pos = a_pos;
    v_col = a_col;
    gl_Position = u_vp * vec4(a_pos, 1.0);
    gl_PointSize = u_size;
}
"#;

const FS_P: &str = r#"
in vec3 v_pos;
in vec4 v_col;
uniform vec4 u_planes[6];
uniform int u_nplanes;
uniform float u_exposure;
out vec4 o_col;
void main() {
    for (int i = 0; i < 6; i++) {
        if (i >= u_nplanes) break;
        if (dot(u_planes[i].xyz, v_pos) > u_planes[i].w) discard;
    }
    o_col = vec4(min(v_col.rgb * u_exposure, vec3(1.0)), 1.0);
}
"#;

const VS_S: &str = r#"
layout(location=0) in vec3 a_pos;
layout(location=2) in vec4 a_col;
uniform mat4 u_vp;
out vec3 v_pos;
out float v_a;
void main() {
    v_pos = a_pos;
    v_a = a_col.a;
    gl_Position = u_vp * vec4(a_pos, 1.0);
}
"#;

// pasul de umbra: doar adancimea; sectiunile si elementele transparente / ascunse nu arunca umbre
const FS_S: &str = r#"
in vec3 v_pos;
in float v_a;
uniform vec4 u_planes[6];
uniform int u_nplanes;
out vec4 o_col;
void main() {
    for (int i = 0; i < 6; i++) {
        if (i >= u_nplanes) break;
        if (dot(u_planes[i].xyz, v_pos) > u_planes[i].w) discard;
    }
    if (v_a < 0.6) discard;
    o_col = vec4(1.0);
}
"#;

const VS: &str = r#"
layout(location=0) in vec3 a_pos;
layout(location=1) in vec4 a_nrm;
layout(location=2) in vec4 a_col;
layout(location=3) in vec4 a_mat;
uniform mat4 u_vp;
out vec3 v_pos;
out vec3 v_nrm;
out vec4 v_col;
out vec4 v_mat;
void main() {
    v_pos = a_pos;
    v_nrm = a_nrm.xyz;
    v_col = a_col;
    v_mat = a_mat;
    gl_Position = u_vp * vec4(a_pos, 1.0);
}
"#;

const FS: &str = r#"
in vec3 v_pos;
in vec3 v_nrm;
in vec4 v_col;
in vec4 v_mat;
uniform vec4 u_planes[6];
uniform int u_nplanes;
uniform int u_pass;
uniform vec3 u_eye;
uniform vec3 u_sun;
uniform vec3 u_light;
uniform int u_caps;
uniform sampler2DArray u_tex;
uniform int u_style;   // 0 realist, 1 umbrit, 2 concept, 3 wireframe, 4 hidden-line
out vec4 o_col;
uniform sampler2D u_shadow;
uniform mat4 u_svp;
uniform vec4 u_sp;     // x = marimea unui texel in lume, y = adancimea totala (m), z = umbre pornite
uniform vec4 u_lamps[8];     // xyz = pozitie, w = putere
uniform vec4 u_lamp_data[8]; // rgb = culoare liniara, w = raza (0 = nelimitata)
uniform vec4 u_lamp_dir[8];  // xyz = directia (spre tinta), w = -2 punct / 2 distanta / cos(jumatate de con) spot
uniform int u_nlamps;
uniform int u_night;

// 1 = in lumina, 0 = in umbra (3x3 PCF: margini moi)
float shadow_at(vec3 p, vec3 n) {
    if (u_sp.z < 0.5) return 1.0;
    float nl = max(dot(n, u_sun), 0.0);
    vec3 q = p + n * u_sp.x * (1.5 + 2.0 * (1.0 - nl));
    vec4 s = u_svp * vec4(q, 1.0);
    vec3 c = s.xyz / s.w * 0.5 + 0.5;
    if (c.x <= 0.0 || c.x >= 1.0 || c.y <= 0.0 || c.y >= 1.0 || c.z >= 1.0) return 1.0;
    float bias = 0.015 / u_sp.y;
    vec2 t = vec2(1.0) / vec2(textureSize(u_shadow, 0));
    float lit = 0.0;
    for (int y = -1; y <= 1; y++)
        for (int x = -1; x <= 1; x++)
            lit += (c.z - bias <= texture(u_shadow, c.xy + vec2(float(x), float(y)) * t).r) ? 1.0 : 0.0;
    return lit / 9.0;
}

float hash(vec2 p) { return fract(sin(dot(p, vec2(127.1, 311.7))) * 43758.5453); }
float noise(vec2 p) {
    vec2 i = floor(p);
    vec2 f = fract(p);
    f = f * f * (3.0 - 2.0 * f);
    return mix(mix(hash(i), hash(i + vec2(1.0, 0.0)), f.x), mix(hash(i + vec2(0.0, 1.0)), hash(i + vec2(1.0, 1.0)), f.x), f.y);
}

vec3 pattern(int pt, vec3 base, vec3 p, vec3 n, float sc) {
    vec3 an = abs(n);
    vec2 uv;
    if (an.z >= an.x && an.z >= an.y) uv = p.xy;
    else if (an.x >= an.y) uv = p.yz;
    else uv = p.xz;
    uv /= sc;
    // anti-aliasing: details smaller than a pixel fade to the average colour (like mipmapping)
    float fw = max(fwidth(uv.x), fwidth(uv.y));
    vec3 c = base;
    vec3 avg = base;
    float freq = 1.0;
    if (pt == 1) {
        vec2 b = vec2(uv.x, uv.y * 3.0);
        if (mod(floor(b.y), 2.0) > 0.5) b.x += 0.5;
        vec2 f = fract(b);
        float mortar = clamp(step(f.x, 0.035) + step(f.y, 0.09), 0.0, 1.0);
        float v = 0.82 + 0.3 * hash(floor(b)) + 0.08 * noise(uv * 30.0);
        c = mix(base * v, vec3(0.80, 0.78, 0.74), mortar);
        avg = mix(base * 0.97, vec3(0.80, 0.78, 0.74), 0.12);
        freq = 3.0 / 0.09;
    } else if (pt == 2) {
        vec2 f = fract(uv);
        float g = clamp(step(f.x, 0.03) + step(f.y, 0.03), 0.0, 1.0);
        c = mix(base * (0.92 + 0.12 * hash(floor(uv))), base * 0.62, g);
        avg = base * 0.96;
        freq = 1.0 / 0.03;
    } else if (pt == 3) {
        float r = uv.x * 7.0 + noise(uv * vec2(1.5, 10.0)) * 2.5;
        float w = 0.5 + 0.5 * sin(r * 3.1416);
        c = base * (0.74 + 0.36 * w);
        avg = base * 0.92;
        freq = 10.0;
    } else if (pt == 4) {
        float nn = noise(uv * 6.0) * 0.55 + noise(uv * 25.0) * 0.30 + noise(uv * 90.0) * 0.15 * (1.0 - smoothstep(0.004, 0.02, fw));
        c = base * (0.80 + 0.32 * nn);
        avg = base * 0.96;
        freq = 6.0;
    } else if (pt == 5) {
        float nn = noise(uv * 14.0) * 0.6 + noise(uv * 60.0) * 0.4;
        c = base * (0.65 + 0.55 * nn);
        avg = base * 0.92;
        freq = 30.0;
    } else if (pt == 6) {
        vec2 f = fract(uv * vec2(1.0, 0.5));
        float j = clamp(step(f.x, 0.012) + step(f.y, 0.006), 0.0, 1.0);
        c = mix(base, base * 0.55, j);
        avg = base * 0.98;
        freq = 1.0 / 0.012;
    }
    float fade = smoothstep(0.25, 0.9, fw * freq);
    return mix(c, avg, fade);
}

void main() {
    for (int i = 0; i < 6; i++) {
        if (i >= u_nplanes) break;
        if (dot(u_planes[i].xyz, v_pos) > u_planes[i].w) discard;
    }
    float a = v_col.a;
    if (a < 0.01) discard;
    if (u_pass == 0 && a < 0.99) discard;
    if (u_pass == 1 && a >= 0.99) discard;
    vec3 n = normalize(v_nrm);
    vec3 vd = normalize(u_eye - v_pos);
    if (dot(n, vd) < 0.0) {
        // fata interioara vazuta prin taietura: capac plin, hasurat (doar cu sectiune activa, elemente opace)
        if (u_caps == 1 && u_nplanes > 0 && u_pass == 0) {
            float h = mod(gl_FragCoord.x + gl_FragCoord.y, 9.0) < 1.6 ? 0.6 : 1.0;
            vec3 cc = mix(v_col.rgb, vec3(0.55), 0.3) * 0.8 * h * u_light.x;
            o_col = vec4(min(cc, vec3(1.0)), 1.0);
            return;
        }
        n = -n;
    }
    vec3 base = v_col.rgb;
    int pt = int(v_mat.x * 255.0 + 0.5);
    // stilurile fara materiale: culorile din fisier (umbrit) sau aproape alb, mat (concept)
    if (u_style == 1 || u_style == 3 || u_style == 4) pt = 0;
    if (u_style == 2) { pt = 0; base = mix(base, vec3(0.94, 0.94, 0.92), 0.78); }
    if (u_style == 4) { base = vec3(0.86, 0.88, 0.91); }
    float pbr_rough = -1.0;
    float pbr_metal = 0.0;
    vec3 an = abs(n);
    vec2 uv = an.z >= an.x && an.z >= an.y ? v_pos.xy : (an.x >= an.y ? v_pos.yz : v_pos.xz);
    uv /= (0.05 + v_mat.w * 4.0);
    if (pt == 200) {
        // PBR: doua straturi impachetate (albedo*AO + roughness / normal + metallic)
        // stratul e in octetul 1 al v_mat (gloss-ul vine din harta, deci octetul e liber)
        int l = int(v_mat.y * 255.0 + 0.5);
        vec4 ca = texture(u_tex, vec3(uv, float(l)));
        vec4 cs = texture(u_tex, vec3(uv, float(l + 1)));
        base = ca.rgb;
        pbr_rough = ca.a;
        pbr_metal = cs.a;
        // baza tangenta din derivatele pozitiei si ale UV-ului (normal mapping fara atribute UV)
        vec3 dp1 = dFdx(v_pos), dp2 = dFdy(v_pos);
        vec2 du1 = dFdx(uv), du2 = dFdy(uv);
        vec3 T = dp1 * du2.y - dp2 * du1.y;
        vec3 B = dp2 * du1.x - dp1 * du2.x;
        float det = dot(T, T) + dot(B, B);
        if (det > 1e-12) {
            T = normalize(T - n * dot(n, T));
            B = normalize(cross(n, T));
            vec3 tn = cs.rgb * 2.0 - 1.0;
            vec3 mapped = normalize(T * tn.x + B * tn.y + n * max(tn.z, 0.05));
            n = normalize(mix(n, mapped, 0.9));
        }
    } else if (pt >= 16) {
        base = texture(u_tex, vec3(uv, float(pt - 16))).rgb;
    } else if (pt > 0) base = pattern(pt, base, v_pos, n, 0.05 + v_mat.w * 4.0);
    float head = abs(dot(n, vd));
    float shv = shadow_at(v_pos, n);
    if (u_sp.w > 0.5) { o_col = vec4(vec3(shv), 1.0); return; }
    float sun = max(dot(n, u_sun), 0.0) * shv;
    float sky = 0.5 + 0.5 * n.z;
    // ambient emisferic (cer rece, sol cald) + soare cald + lumina de umplere din partea opusa soarelui
    vec3 amb = mix(vec3(0.90, 0.86, 0.80), vec3(0.94, 0.97, 1.04), sky);
    vec3 fdir = normalize(vec3(-u_sun.x, -u_sun.y, 0.45));
    float fill = max(dot(n, fdir), 0.0);
    // noaptea: cerul, soarele si lumina de umplere aproape stinse - lampile din model dau lumina
    float dayf = u_night == 1 ? 0.0 : 1.0;
    float ambf = u_night == 1 ? 0.07 : 1.0;
    vec3 k = (amb * (0.21 + 0.14 * sky) * u_light.y + vec3(0.31 * head + 0.07 * fill * u_light.y)) * ambf + vec3(1.02, 0.99, 0.94) * (0.30 * sun * u_light.z) * dayf;
    // umbra aruncata: fetele intoarse spre soare dar acoperite devin vizibil mai inchise (ca afara, unde soarele domina)
    k *= mix(1.0, mix(0.62, 1.0, shv), smoothstep(0.0, 0.25, dot(n, u_sun)) * step(0.5, u_sp.z) * dayf);
    // lampi in model ( Interactive Ray Trace ca in Navisworks ): lumina cade pe suprafete in timp real.
    // Punct = in toate directiile; Spot = con orientat spre tinta (rotirea din gizmo se vede imediat);
    // Distant = raze paralele pe directia luminii. Raza 0 = fara cadere cu distanta.
    // Se adauga DUPA umbra soarelui: lampile nu sunt intunecate de umbra soarelui.
    vec3 lamp_spec = vec3(0.0);
    vec3 klamp = vec3(0.0);
    for (int li = 0; li < 8; li++) {
        if (li >= u_nlamps) break;
        vec4 ld = u_lamp_dir[li];
        vec3 L;
        float att;
        float radius = u_lamp_data[li].w;
        if (ld.w > 1.5) {
            L = -ld.xyz;
            att = u_lamps[li].w * 9.0;
        } else {
            vec3 lv = u_lamps[li].xyz - v_pos;
            float d = length(lv);
            L = lv / max(d, 0.001);
            att = radius <= 0.001 ? u_lamps[li].w * 26.0 : u_lamps[li].w * 26.0 / (1.0 + (d / radius) * (d / radius));
            if (ld.w > -1.5) {
                // con: centrul cel mai luminos, marginea moale (ca in randarea offline)
                float ca = dot(-L, ld.xyz);
                float cone = clamp((ca - ld.w) / max(1.0 - ld.w, 1e-4), 0.0, 1.0);
                att *= pow(cone, 1.5);
            }
        }
        float nl = max(dot(n, L), 0.0);
        klamp += u_lamp_data[li].rgb * nl * att;
        // luciu de la lampa: punctul luminos se vede pe suprafetele lucioase
        vec3 hl = normalize(L + vd);
        lamp_spec += u_lamp_data[li].rgb * pow(max(dot(n, hl), 0.0), 48.0) * nl * att * 0.25;
    }
    k += klamp;
    // un luciu mic pentru orice material (suprafetele nu mai par plate / de hartie)
    // harta de rugozitate (0 = oglinda) e inversul luciului; harta de metalicitate inlocuieste valoarea globala
    float gloss = pbr_rough >= 0.0 ? clamp(1.0 - pbr_rough, 0.03, 1.0) : (u_style == 0 ? max(v_mat.y, 0.07) : ((u_style == 2 || u_style == 4) ? 0.0 : 0.07));
    float metal = u_style == 0 ? (pbr_rough >= 0.0 ? pbr_metal : v_mat.z) : 0.0;
    if (u_style == 2 || u_style == 4) {
        // concept / hidden-line: lumina moale, fara reflexii
        // lampile se vad si aici (pata de lumina colorata), noaptea fondul e stins
        vec3 cc = (base * (0.62 + 0.30 * head + 0.10 * sky + 0.08 * sun) * mix(0.12, 1.0, dayf) + base * klamp * 0.6) * u_light.x;
        o_col = vec4(min(cc, vec3(1.0)) * a, a);
        return;
    }
    vec3 h = normalize(u_sun + vd);
    float spec = pow(max(dot(n, h), 0.0), mix(12.0, 160.0, gloss)) * gloss * shv * dayf;
    float rim = pow(1.0 - head, 4.0) * gloss * 0.35;
    vec3 sc = mix(vec3(1.0), base, metal);
    vec3 c = base * k * (1.0 - 0.35 * metal) + sc * (spec * 0.85 + rim * ambf) + metal * base * 0.22 * sky * ambf + lamp_spec * max(gloss, 0.15);
    if (u_style == 0) {
        // realist: reflexia cerului (Fresnel) - sticla, metal, suprafete lucioase prind cerul si solul
        vec3 rv = reflect(-vd, n);
        vec3 env = mix(vec3(0.34, 0.32, 0.29), mix(vec3(0.78, 0.84, 0.92), vec3(0.42, 0.58, 0.85), clamp(rv.z, 0.0, 1.0)), smoothstep(-0.15, 0.25, rv.z));
        vec3 f0 = mix(vec3(0.04), base, metal);
        vec3 fres = f0 + (1.0 - f0) * pow(1.0 - head, 5.0);
        bool glass = a < 0.95;
        float refl = glass ? 0.9 : gloss * 0.6;
        c = mix(c, env * u_light.z * 1.1 * mix(0.04, 1.0, dayf), clamp(fres * refl, 0.0, 1.0));
        // sticla: mai opaca la unghiuri razante (ca in realitate)
        if (glass) a = clamp(a + (1.0 - a) * pow(1.0 - head, 3.0) * 0.8, 0.0, 1.0);
        // tonuri filmice (ACES): contrast si saturatie naturale, luminile nu se ard
        // ACES ridica tonurile medii: expunerea compensata (un gri mediu ramane aproape neschimbat)
        c *= u_light.x * 0.85;
        c = clamp((c * (2.51 * c + 0.03)) / (c * (2.43 * c + 0.59) + 0.14), 0.0, 1.0);
        o_col = vec4(c * a, a);
        return;
    }
    // umar moale in loc de taiere la alb: luminile puternice nu se ard
    c *= u_light.x;
    vec3 e = max(c - 0.8, 0.0);
    c = min(c, vec3(0.8)) + e / (1.0 + e * 5.0);
    o_col = vec4(c * a, a);
}
"#;

impl Renderer {
    pub fn new() -> Renderer {
        Renderer { lists: None, prog: None, meshes: HashMap::new(), pprog: None, points: HashMap::new(), aoprog: None, ao_targets: Vec::new(), ao_vao: None, ao_failed: false, sprog: None, shadow_fbo: None, shadow_state: None, shadow_failed: false, pending: Vec::new(), error: None, capture: None, captured: None, fbo: None, tex: None }
    }

    unsafe fn init(&mut self, gl: &glow::Context) {
        if self.prog.is_some() || self.error.is_some() {
            return;
        }
        let header = if gl.version().is_embedded { "#version 300 es\nprecision highp float;\nprecision highp sampler2DArray;\n" } else { "#version 330 core\n" };
        let prog = gl.create_program().expect("program");
        let mut shaders = Vec::new();
        for (ty, src) in [(glow::VERTEX_SHADER, VS), (glow::FRAGMENT_SHADER, FS)] {
            let s = gl.create_shader(ty).expect("shader");
            gl.shader_source(s, &format!("{header}{src}"));
            gl.compile_shader(s);
            if !gl.get_shader_compile_status(s) {
                self.error = Some(format!("Shader error: {}", gl.get_shader_info_log(s)));
                return;
            }
            gl.attach_shader(prog, s);
            shaders.push(s);
        }
        gl.link_program(prog);
        if !gl.get_program_link_status(prog) {
            self.error = Some(format!("Shader link error: {}", gl.get_program_info_log(prog)));
            return;
        }
        for s in shaders {
            gl.detach_shader(prog, s);
            gl.delete_shader(s);
        }
        self.prog = Some(prog);
        // programul pentru norii de puncte; daca nu merge, modelele se vad oricum (doar fara puncte)
        self.pprog = compile_program(gl, header, VS_P, FS_P).ok();
        // SSAO: fara el, desenarea ramane cea obisnuita
        let hdr_ao = if gl.version().is_embedded { "#version 300 es\nprecision highp float;\nprecision highp sampler2D;\n" } else { header };
        self.aoprog = compile_program(gl, hdr_ao, VS_AO, FS_AO).ok();
        self.ao_vao = gl.create_vertex_array().ok();
        // umbre: fara ele, desenarea ramane cea obisnuita
        self.sprog = compile_program(gl, header, VS_S, FS_S).ok();
        let n = crate::materials::tex_size() as usize;
        self.upload_textures(gl, &[vec![200u8; n * n * 4]]);
    }

    unsafe fn upload_textures(&mut self, gl: &glow::Context, layers: &[Vec<u8>]) {
        if layers.is_empty() {
            return;
        }
        if let Some(t) = self.tex.take() {
            gl.delete_texture(t);
        }
        let n = crate::materials::tex_size() as i32;
        let Ok(t) = gl.create_texture() else { return };
        gl.active_texture(glow::TEXTURE1);
        gl.bind_texture(glow::TEXTURE_2D_ARRAY, Some(t));
        let mut all = Vec::with_capacity(layers.len() * (n * n * 4) as usize);
        for l in layers {
            all.extend_from_slice(l);
        }
        gl.pixel_store_i32(glow::UNPACK_ALIGNMENT, 1);
        gl.tex_image_3d(glow::TEXTURE_2D_ARRAY, 0, glow::RGBA8 as i32, n, n, layers.len() as i32, 0, glow::RGBA, glow::UNSIGNED_BYTE, Some(&all));
        gl.generate_mipmap(glow::TEXTURE_2D_ARRAY);
        gl.tex_parameter_i32(glow::TEXTURE_2D_ARRAY, glow::TEXTURE_MIN_FILTER, glow::LINEAR_MIPMAP_LINEAR as i32);
        gl.tex_parameter_i32(glow::TEXTURE_2D_ARRAY, glow::TEXTURE_MAG_FILTER, glow::LINEAR as i32);
        gl.tex_parameter_i32(glow::TEXTURE_2D_ARRAY, glow::TEXTURE_WRAP_S, glow::REPEAT as i32);
        gl.tex_parameter_i32(glow::TEXTURE_2D_ARRAY, glow::TEXTURE_WRAP_T, glow::REPEAT as i32);
        gl.bind_texture(glow::TEXTURE_2D_ARRAY, None);
        gl.active_texture(glow::TEXTURE0);
        self.tex = Some(t);
    }

    unsafe fn apply_uploads(&mut self, gl: &glow::Context) {
        let pending = std::mem::take(&mut self.pending);
        for up in pending {
            match up {
                Upload::Mesh { slot, pos, nrm, col } => {
                    let nv = pos.len() / 3;
                    if let Some(old) = self.meshes.remove(&slot) {
                        delete_mesh(gl, old);
                    }
                    let vao = gl.create_vertex_array().unwrap();
                    gl.bind_vertex_array(Some(vao));
                    let vbo_pos = gl.create_buffer().unwrap();
                    gl.bind_buffer(glow::ARRAY_BUFFER, Some(vbo_pos));
                    gl.buffer_data_u8_slice(glow::ARRAY_BUFFER, bytes_f32(&pos), glow::STATIC_DRAW);
                    gl.enable_vertex_attrib_array(0);
                    gl.vertex_attrib_pointer_f32(0, 3, glow::FLOAT, false, 12, 0);
                    let vbo_nrm = gl.create_buffer().unwrap();
                    gl.bind_buffer(glow::ARRAY_BUFFER, Some(vbo_nrm));
                    let nb: &[u8] = std::slice::from_raw_parts(nrm.as_ptr() as *const u8, nrm.len());
                    gl.buffer_data_u8_slice(glow::ARRAY_BUFFER, nb, glow::STATIC_DRAW);
                    gl.enable_vertex_attrib_array(1);
                    gl.vertex_attrib_pointer_f32(1, 4, glow::BYTE, true, 4, 0);
                    let vbo_col = gl.create_buffer().unwrap();
                    gl.bind_buffer(glow::ARRAY_BUFFER, Some(vbo_col));
                    gl.buffer_data_u8_slice(glow::ARRAY_BUFFER, &col, glow::DYNAMIC_DRAW);
                    gl.enable_vertex_attrib_array(2);
                    gl.vertex_attrib_pointer_f32(2, 4, glow::UNSIGNED_BYTE, true, 4, 0);
                    let vbo_mat = gl.create_buffer().unwrap();
                    gl.bind_buffer(glow::ARRAY_BUFFER, Some(vbo_mat));
                    gl.buffer_data_u8_slice(glow::ARRAY_BUFFER, &vec![0u8; nv * 4], glow::DYNAMIC_DRAW);
                    gl.enable_vertex_attrib_array(3);
                    gl.vertex_attrib_pointer_f32(3, 4, glow::UNSIGNED_BYTE, true, 4, 0);
                    gl.bind_vertex_array(None);
                    gl.bind_buffer(glow::ARRAY_BUFFER, None);
                    self.meshes.insert(slot, GpuMesh { vao, vbo_pos, vbo_nrm, vbo_col, vbo_mat, count: nv as i32 });
                }
                Upload::Colors { slot, col, mat } => {
                    if let Some(m) = self.meshes.get(&slot) {
                        gl.bind_buffer(glow::ARRAY_BUFFER, Some(m.vbo_col));
                        gl.buffer_data_u8_slice(glow::ARRAY_BUFFER, &col, glow::DYNAMIC_DRAW);
                        gl.bind_buffer(glow::ARRAY_BUFFER, Some(m.vbo_mat));
                        gl.buffer_data_u8_slice(glow::ARRAY_BUFFER, &mat, glow::DYNAMIC_DRAW);
                        gl.bind_buffer(glow::ARRAY_BUFFER, None);
                    }
                }
                Upload::ColorRange { slot, first, col, mat } => {
                    if let Some(m) = self.meshes.get(&slot) {
                        // 4 octeti per varf; ignoram intervale din afara bufferului (mesh inlocuit intre timp)
                        let off = first as usize * 4;
                        if off + col.len() <= m.count as usize * 4 && col.len() == mat.len() {
                            gl.bind_buffer(glow::ARRAY_BUFFER, Some(m.vbo_col));
                            gl.buffer_sub_data_u8_slice(glow::ARRAY_BUFFER, off as i32, &col);
                            gl.bind_buffer(glow::ARRAY_BUFFER, Some(m.vbo_mat));
                            gl.buffer_sub_data_u8_slice(glow::ARRAY_BUFFER, off as i32, &mat);
                            gl.bind_buffer(glow::ARRAY_BUFFER, None);
                        }
                    }
                }
                Upload::Textures(layers) => self.upload_textures(gl, &layers),
                Upload::Remove { slot } => {
                    if let Some(old) = self.meshes.remove(&slot) {
                        delete_mesh(gl, old);
                    }
                    if let Some(old) = self.points.remove(&slot) {
                        delete_points(gl, old);
                    }
                }
                Upload::Points { slot, cloud } => {
                    if let Some(old) = self.points.remove(&slot) {
                        delete_points(gl, old);
                    }
                    let Some(c) = cloud else { continue };
                    if c.pos.is_empty() {
                        continue;
                    }
                    let (Ok(vao), Ok(vbo_pos), Ok(vbo_col)) = (gl.create_vertex_array(), gl.create_buffer(), gl.create_buffer()) else { continue };
                    gl.bind_vertex_array(Some(vao));
                    gl.bind_buffer(glow::ARRAY_BUFFER, Some(vbo_pos));
                    let pb: &[u8] = std::slice::from_raw_parts(c.pos.as_ptr() as *const u8, c.pos.len() * 12);
                    gl.buffer_data_u8_slice(glow::ARRAY_BUFFER, pb, glow::STATIC_DRAW);
                    gl.enable_vertex_attrib_array(0);
                    gl.vertex_attrib_pointer_f32(0, 3, glow::FLOAT, false, 12, 0);
                    gl.bind_buffer(glow::ARRAY_BUFFER, Some(vbo_col));
                    let cb: &[u8] = std::slice::from_raw_parts(c.col.as_ptr() as *const u8, c.col.len() * 4);
                    gl.buffer_data_u8_slice(glow::ARRAY_BUFFER, cb, glow::STATIC_DRAW);
                    gl.enable_vertex_attrib_array(1);
                    gl.vertex_attrib_pointer_f32(1, 4, glow::UNSIGNED_BYTE, true, 4, 0);
                    gl.bind_vertex_array(None);
                    gl.bind_buffer(glow::ARRAY_BUFFER, None);
                    self.points.insert(slot, GpuPoints { vao, vbo_pos, vbo_col, count: c.pos.len().min(c.col.len()) as i32 });
                }
            }
        }
    }

    /// # Safety
    /// Must be called inside the egui_glow paint callback, with the GL context current.
    pub unsafe fn paint(&mut self, gl: &glow::Context, p: &FrameParams, lists: Option<&HashMap<u32, DrawList>>) {
        self.init(gl);
        self.apply_uploads(gl);
        if self.prog.is_none() {
            return;
        }
        self.shadow_state = if p.shadows { self.shadow_pass(gl, p, lists) } else { None };
        let wire = p.style == VisualStyle::Wireframe;
        if wire {
            gl.polygon_mode(glow::FRONT_AND_BACK, glow::LINE);
        }
        if wire || !((p.ao > 0.0 || p.edges > 0.0) && self.draw_ao(gl, p, lists)) {
            gl.clear_depth_f32(1.0);
            gl.depth_mask(true);
            gl.clear(glow::DEPTH_BUFFER_BIT);
            self.draw(gl, p, lists);
        }
        if wire {
            // egui deseneaza cu triunghiuri pline dupa noi
            gl.polygon_mode(glow::FRONT_AND_BACK, glow::FILL);
        }
        if let Some(req) = self.capture.take() {
            let r = self.capture_frame(gl, &req);
            self.captured = Some(r);
        }
    }

    unsafe fn draw(&self, gl: &glow::Context, p: &FrameParams, lists: Option<&HashMap<u32, DrawList>>) {
        let Some(prog) = self.prog else { return };
        gl.enable(glow::DEPTH_TEST);
        gl.depth_func(glow::LEQUAL);
        gl.depth_mask(true);
        gl.disable(glow::CULL_FACE);
        gl.use_program(Some(prog));
        gl.uniform_matrix_4_f32_slice(gl.get_uniform_location(prog, "u_vp").as_ref(), false, &p.view_proj.to_cols_array());
        gl.uniform_3_f32(gl.get_uniform_location(prog, "u_eye").as_ref(), p.eye.x, p.eye.y, p.eye.z);
        gl.uniform_3_f32(gl.get_uniform_location(prog, "u_sun").as_ref(), p.sun.x, p.sun.y, p.sun.z);
        gl.uniform_3_f32(gl.get_uniform_location(prog, "u_light").as_ref(), p.light.x, p.light.y, p.light.z);
        let mut pl = [0f32; 24];
        for (i, v) in p.planes.iter().enumerate() {
            pl[i * 4..i * 4 + 4].copy_from_slice(&v.to_array());
        }
        gl.uniform_4_f32_slice(gl.get_uniform_location(prog, "u_planes").as_ref(), &pl);
        gl.uniform_1_i32(gl.get_uniform_location(prog, "u_nplanes").as_ref(), p.nplanes);
        // lampi (max 8), ca in Navisworks Interactive Ray Trace
        let mut lv = [0.0f32; 32];
        let mut lc = [0.0f32; 32];
        for (i, l) in p.lamps.iter().enumerate() {
            lv[i * 4..i * 4 + 4].copy_from_slice(&l.to_array());
            lc[i * 4..i * 4 + 4].copy_from_slice(&p.lamp_data[i].to_array());
        }
        gl.uniform_4_f32_slice(gl.get_uniform_location(prog, "u_lamps").as_ref(), &lv);
        gl.uniform_4_f32_slice(gl.get_uniform_location(prog, "u_lamp_data").as_ref(), &lc);
        let mut ld = [0.0f32; 32];
        for (i, l) in p.lamp_dir.iter().enumerate() {
            ld[i * 4..i * 4 + 4].copy_from_slice(&l.to_array());
        }
        gl.uniform_4_f32_slice(gl.get_uniform_location(prog, "u_lamp_dir").as_ref(), &ld);
        gl.uniform_1_i32(gl.get_uniform_location(prog, "u_nlamps").as_ref(), p.nlamps);
        gl.uniform_1_i32(gl.get_uniform_location(prog, "u_night").as_ref(), p.night as i32);
        gl.uniform_1_i32(gl.get_uniform_location(prog, "u_caps").as_ref(), p.caps as i32);
        gl.uniform_1_i32(gl.get_uniform_location(prog, "u_style").as_ref(), p.style.code());
        let u_pass = gl.get_uniform_location(prog, "u_pass");
        gl.active_texture(glow::TEXTURE1);
        gl.bind_texture(glow::TEXTURE_2D_ARRAY, self.tex);
        gl.uniform_1_i32(gl.get_uniform_location(prog, "u_tex").as_ref(), 1);
        // harta de umbre pe unitatea 4
        match (self.shadow_state, self.shadow_fbo) {
            (Some((svp, texel, range)), Some((_, stex))) => {
                gl.active_texture(glow::TEXTURE4);
                gl.bind_texture(glow::TEXTURE_2D, Some(stex));
                gl.uniform_1_i32(gl.get_uniform_location(prog, "u_shadow").as_ref(), 4);
                gl.uniform_matrix_4_f32_slice(gl.get_uniform_location(prog, "u_svp").as_ref(), false, &svp.to_cols_array());
                gl.uniform_4_f32(gl.get_uniform_location(prog, "u_sp").as_ref(), texel, range, 1.0, if std::env::var_os("MB_SHADOW_DEBUG").is_some() { 1.0 } else { 0.0 });
            }
            _ => {
                gl.uniform_1_i32(gl.get_uniform_location(prog, "u_shadow").as_ref(), 4);
                gl.uniform_4_f32(gl.get_uniform_location(prog, "u_sp").as_ref(), 1.0, 1.0, 0.0, 0.0);
            }
        }
        gl.active_texture(glow::TEXTURE1);
        // pasul 1: opac
        gl.disable(glow::BLEND);
        gl.uniform_1_i32(u_pass.as_ref(), 0);
        for (slot, m) in &self.meshes {
            gl.bind_vertex_array(Some(m.vao));
            draw_ranges(gl, m.count, lists.and_then(|l| l.get(slot)).map(|d| &d.all));
        }
        // norii de puncte (opaci), cu programul lor
        if let (Some(pp), false) = (self.pprog, self.points.is_empty()) {
            gl.use_program(Some(pp));
            if !gl.version().is_embedded {
                gl.enable(glow::PROGRAM_POINT_SIZE);
            }
            gl.uniform_matrix_4_f32_slice(gl.get_uniform_location(pp, "u_vp").as_ref(), false, &p.view_proj.to_cols_array());
            gl.uniform_1_f32(gl.get_uniform_location(pp, "u_size").as_ref(), p.point_size.clamp(1.0, 16.0));
            gl.uniform_1_f32(gl.get_uniform_location(pp, "u_exposure").as_ref(), p.light.x);
            gl.uniform_4_f32_slice(gl.get_uniform_location(pp, "u_planes").as_ref(), &pl);
            gl.uniform_1_i32(gl.get_uniform_location(pp, "u_nplanes").as_ref(), p.nplanes);
            for (slot, g) in &self.points {
                gl.bind_vertex_array(Some(g.vao));
                match lists.and_then(|l| l.get(slot)) {
                    None => gl.draw_arrays(glow::POINTS, 0, g.count),
                    Some(d) => {
                        for &(first, n) in &d.points {
                            if first < 0 || n <= 0 || first >= g.count {
                                continue;
                            }
                            gl.draw_arrays(glow::POINTS, first, n.min(g.count - first));
                        }
                    }
                }
            }
            gl.use_program(Some(prog));
        }
        // pasul 2: transparent
        gl.enable(glow::BLEND);
        gl.blend_func(glow::ONE, glow::ONE_MINUS_SRC_ALPHA);
        gl.depth_mask(false);
        gl.uniform_1_i32(u_pass.as_ref(), 1);
        for (slot, m) in &self.meshes {
            gl.bind_vertex_array(Some(m.vao));
            draw_ranges(gl, m.count, lists.and_then(|l| l.get(slot)).map(|d| &d.transp));
        }
        gl.depth_mask(true);
        gl.bind_vertex_array(None);
        gl.active_texture(glow::TEXTURE4);
        gl.bind_texture(glow::TEXTURE_2D, None);
        gl.active_texture(glow::TEXTURE1);
        gl.bind_texture(glow::TEXTURE_2D_ARRAY, None);
        gl.active_texture(glow::TEXTURE0);
        gl.disable(glow::DEPTH_TEST);
    }

    /// Renders the scene into an off-screen framebuffer of any size and reads it back.
    unsafe fn capture_frame(&mut self, gl: &glow::Context, req: &CaptureReq) -> Result<Frame, String> {
        let (w, h) = (req.w.max(16), req.h.max(16));
        let max = gl.get_parameter_i32(glow::MAX_RENDERBUFFER_SIZE).max(2048) as u32;
        if w > max || h > max {
            return Err(format!("The graphics card supports at most {max} px - lower the resolution or turn off supersampling."));
        }
        let prev_fb = gl.get_parameter_framebuffer(glow::FRAMEBUFFER_BINDING);
        let mut vp = [0i32; 4];
        gl.get_parameter_i32_slice(glow::VIEWPORT, &mut vp);
        let scissor = gl.is_enabled(glow::SCISSOR_TEST);
        if self.fbo.map_or(true, |f| f.3 != w || f.4 != h) {
            if let Some((fb, c, d, _, _)) = self.fbo.take() {
                gl.delete_framebuffer(fb);
                gl.delete_renderbuffer(c);
                gl.delete_renderbuffer(d);
            }
            let fb = gl.create_framebuffer()?;
            let c = gl.create_renderbuffer()?;
            let d = gl.create_renderbuffer()?;
            gl.bind_renderbuffer(glow::RENDERBUFFER, Some(c));
            gl.renderbuffer_storage(glow::RENDERBUFFER, glow::RGBA8, w as i32, h as i32);
            gl.bind_renderbuffer(glow::RENDERBUFFER, Some(d));
            gl.renderbuffer_storage(glow::RENDERBUFFER, glow::DEPTH_COMPONENT24, w as i32, h as i32);
            gl.bind_renderbuffer(glow::RENDERBUFFER, None);
            gl.bind_framebuffer(glow::FRAMEBUFFER, Some(fb));
            gl.framebuffer_renderbuffer(glow::FRAMEBUFFER, glow::COLOR_ATTACHMENT0, glow::RENDERBUFFER, Some(c));
            gl.framebuffer_renderbuffer(glow::FRAMEBUFFER, glow::DEPTH_ATTACHMENT, glow::RENDERBUFFER, Some(d));
            self.fbo = Some((fb, c, d, w, h));
        }
        let (fb, ..) = self.fbo.unwrap();
        gl.bind_framebuffer(glow::FRAMEBUFFER, Some(fb));
        let status = gl.check_framebuffer_status(glow::FRAMEBUFFER);
        let mut res = Err(format!("Off-screen framebuffer not available (0x{status:x})."));
        if status == glow::FRAMEBUFFER_COMPLETE {
            gl.viewport(0, 0, w as i32, h as i32);
            gl.disable(glow::SCISSOR_TEST);
            gl.clear_color(0.0, 0.0, 0.0, 0.0);
            gl.clear_depth_f32(1.0);
            gl.depth_mask(true);
            gl.color_mask(true, true, true, true);
            gl.clear(glow::COLOR_BUFFER_BIT | glow::DEPTH_BUFFER_BIT);
            // umbrele pentru tot modelul (captura nu foloseste listele vederii)
            self.shadow_state = if req.params.shadows { self.shadow_pass(gl, &req.params, None) } else { None };
            gl.bind_framebuffer(glow::FRAMEBUFFER, Some(fb));
            gl.viewport(0, 0, w as i32, h as i32);
            // capturile (randari / video) deseneaza tot modelul, la calitate maxima (si cu SSAO)
            if !(req.params.ao > 0.0 && self.draw_ao(gl, &req.params, None)) {
                gl.bind_framebuffer(glow::FRAMEBUFFER, Some(fb));
                gl.viewport(0, 0, w as i32, h as i32);
                self.draw(gl, &req.params, None);
            }
            let mut buf = vec![0u8; (w * h * 4) as usize];
            gl.pixel_store_i32(glow::PACK_ALIGNMENT, 1);
            gl.read_pixels(0, 0, w as i32, h as i32, glow::RGBA, glow::UNSIGNED_BYTE, glow::PixelPackData::Slice(&mut buf));
            // rasturnare verticala + compunere peste fundal (culori premultiplicate)
            let mut out = vec![0u8; buf.len()];
            let row = (w * 4) as usize;
            for y in 0..h as usize {
                let src = &buf[(h as usize - 1 - y) * row..(h as usize - y) * row];
                let t = y as f32 / (h - 1).max(1) as f32;
                let bg: [f32; 3] = std::array::from_fn(|i| req.bg_top[i] as f32 * (1.0 - t) + req.bg_bottom[i] as f32 * t);
                let dst = &mut out[y * row..(y + 1) * row];
                for (s, d) in src.chunks_exact(4).zip(dst.chunks_exact_mut(4)) {
                    let a = s[3] as f32 / 255.0;
                    for i in 0..3 {
                        d[i] = (s[i] as f32 + bg[i] * (1.0 - a)).min(255.0) as u8;
                    }
                    d[3] = 255;
                }
            }
            res = Ok(Frame { w, h, rgba: out });
        }
        gl.bind_framebuffer(glow::FRAMEBUFFER, prev_fb);
        gl.viewport(vp[0], vp[1], vp[2], vp[3]);
        if scissor {
            gl.enable(glow::SCISSOR_TEST);
        }
        res
    }
}

const VS_AO: &str = r#"
out vec2 v_uv;
void main() {
    // un triunghi care acopera tot ecranul
    vec2 p = vec2(float((gl_VertexID << 1) & 2), float(gl_VertexID & 2));
    v_uv = p;
    gl_Position = vec4(p * 2.0 - 1.0, 0.0, 1.0);
}
"#;

const FS_AO: &str = r#"
in vec2 v_uv;
uniform sampler2D u_col;
uniform sampler2D u_dep;
uniform vec2 u_texel;
uniform vec4 u_k;      // P22, P32, ortografic (1/0), P11
uniform float u_h;     // inaltimea in pixeli
uniform float u_str;   // taria
uniform float u_edge;  // margini (0 = oprite)
uniform int u_style;   // 4 = hidden-line (desen tehnic)
out vec4 o_col;
float lin(float d) {
    float z = d * 2.0 - 1.0;
    if (u_k.z > 0.5) return -(z - u_k.y) / u_k.x;
    return u_k.y / (z + u_k.x);
}
void main() {
    vec4 c = texture(u_col, v_uv);
    float d0 = texture(u_dep, v_uv).r;
    if (d0 >= 0.99999 || c.a <= 0.0) { o_col = c; return; }
    float z0 = lin(d0);
    // raza ~0.45 m in lume, proiectata pe ecran (limitata 3..48 px)
    float rw = 0.45;
    float pscale = 0.5 * u_h * u_k.w;
    float rpx = clamp(u_k.z > 0.5 ? rw * pscale : rw * pscale / max(z0, 1e-3), 3.0, 48.0);
    // rotatie per pixel (zgomot fin, fara benzi)
    float ang = 6.2831853 * fract(52.9829189 * fract(dot(gl_FragCoord.xy, vec2(0.06711056, 0.00583715))));
    float occ = 0.0;
    const int N = 16;
    for (int i = 0; i < N; i++) {
        float t = (float(i) + 0.5) / float(N);
        float a = ang + float(i) * 2.39996323;
        vec2 o = vec2(cos(a), sin(a)) * sqrt(t) * rpx * u_texel;
        float ds = texture(u_dep, v_uv + o).r;
        if (ds >= 0.99999) continue;
        float diff = z0 - lin(ds);
        occ += smoothstep(0.015, 0.12, diff) * (1.0 - smoothstep(rw, rw * 2.5, diff));
    }
    // umbre mai moi: atenuare mai blanda
    float ao = 1.0 - u_str * 0.5 * occ / float(N);
    // margini fine: salturi de adancime (contur) si schimbari de panta (muchii). Pe o suprafata plana
    // 1/z (perspectiva) sau z (ortografic) variaza liniar pe ecran -> derivata a doua ~0.
    float edge = 0.0;
    if (u_edge > 0.0) {
        float zl = lin(texture(u_dep, v_uv - vec2(u_texel.x, 0.0)).r);
        float zr = lin(texture(u_dep, v_uv + vec2(u_texel.x, 0.0)).r);
        float zd = lin(texture(u_dep, v_uv - vec2(0.0, u_texel.y)).r);
        float zu = lin(texture(u_dep, v_uv + vec2(0.0, u_texel.y)).r);
        float jump = max(max(abs(zl - z0), abs(zr - z0)), max(abs(zd - z0), abs(zu - z0))) / max(z0, 1e-3);
        bool ortho = u_k.z > 0.5;
        float q0 = ortho ? z0 : 1.0 / z0;
        float ql = ortho ? zl : 1.0 / zl;
        float qr = ortho ? zr : 1.0 / zr;
        float qd = ortho ? zd : 1.0 / zd;
        float qu = ortho ? zu : 1.0 / zu;
        float lapAbs = abs(ql + qr - 2.0 * q0) + abs(qd + qu - 2.0 * q0);
        // ortografic: raportat la marimea pixelului in lume; perspectiva: relativ la 1/z
        float pix = 2.0 / max(u_k.w * u_h, 1e-3);
        float crease = ortho ? smoothstep(0.3, 1.0, lapAbs / pix) : smoothstep(0.0009, 0.004, lapAbs / max(abs(q0), 1e-6));
        // Hidden line: DOAR conturul general (silueta / salt de adancime) - fara crease.
        edge = u_style == 4 ? smoothstep(0.02, 0.06, jump) : max(smoothstep(0.02, 0.06, jump), crease * 0.8);
    }
    if (u_style == 4) {
        // HIDDEN LINE stil Rhino 3D: hartie alba, suprafete albe cu umbrire fina,
        // DOAR conturul exterior negru (fara muchii interne / crease).
        float ink = clamp(edge * 1.7, 0.0, 1.0);
        vec3 paper = vec3(0.985, 0.988, 0.992);                  // fundal alb
        vec3 shade = mix(paper, vec3(0.82, 0.84, 0.87), 0.22 * ao); // umbra moale pe suprafete
        vec3 col = mix(shade, vec3(0.10, 0.12, 0.15), ink);       // cerneala neagra pe muchii
        o_col = vec4(col, c.a);
        return;
    }
    o_col = vec4(c.rgb * ao * (1.0 - 0.45 * u_edge * edge), c.a);
}
"#;

impl Renderer {
    /// Scena cu umbrire de contact: desenata intr-un FBO (multisample cand framebuffer-ul curent are MSAA),
    /// rezolvata in texturi de culoare / adancime, apoi compusa in framebuffer-ul curent cu SSAO.
    /// Intoarce false daca nu se poate (atunci se deseneaza normal).
    unsafe fn draw_ao(&mut self, gl: &glow::Context, p: &FrameParams, lists: Option<&HashMap<u32, DrawList>>) -> bool {
        let (Some(aoprog), Some(vao)) = (self.aoprog, self.ao_vao) else { return false };
        if self.ao_failed {
            return false;
        }
        let prev_fb = gl.get_parameter_framebuffer(glow::FRAMEBUFFER_BINDING);
        let mut vp = [0i32; 4];
        gl.get_parameter_i32_slice(glow::VIEWPORT, &mut vp);
        let (x, y, w, h) = (vp[0], vp[1], vp[2].max(1), vp[3].max(1));
        let scissor = gl.is_enabled(glow::SCISSOR_TEST);
        let mut sc = [0i32; 4];
        gl.get_parameter_i32_slice(glow::SCISSOR_BOX, &mut sc);
        let samples = gl.get_parameter_i32(glow::SAMPLES).clamp(0, 8);
        let Some(ti) = self.ao_target(gl, w, h, samples) else {
            self.ao_failed = true;
            gl.bind_framebuffer(glow::FRAMEBUFFER, prev_fb);
            return false;
        };
        let (ms, fbo, col, dep) = {
            let t = &self.ao_targets[ti];
            (t.ms.map(|m| m.0), t.fbo, t.col, t.dep)
        };
        // 1) scena in FBO
        gl.bind_framebuffer(glow::FRAMEBUFFER, Some(ms.unwrap_or(fbo)));
        gl.viewport(0, 0, w, h);
        gl.disable(glow::SCISSOR_TEST);
        gl.clear_color(0.0, 0.0, 0.0, 0.0);
        gl.clear_depth_f32(1.0);
        gl.depth_mask(true);
        gl.color_mask(true, true, true, true);
        gl.clear(glow::COLOR_BUFFER_BIT | glow::DEPTH_BUFFER_BIT);
        self.draw(gl, p, lists);
        // 2) MSAA -> texturi
        if let Some(m) = ms {
            gl.bind_framebuffer(glow::READ_FRAMEBUFFER, Some(m));
            gl.bind_framebuffer(glow::DRAW_FRAMEBUFFER, Some(fbo));
            gl.blit_framebuffer(0, 0, w, h, 0, 0, w, h, glow::COLOR_BUFFER_BIT, glow::NEAREST);
            gl.blit_framebuffer(0, 0, w, h, 0, 0, w, h, glow::DEPTH_BUFFER_BIT, glow::NEAREST);
        }
        // 3) compunere in framebuffer-ul initial, in aceeasi zona
        gl.bind_framebuffer(glow::FRAMEBUFFER, prev_fb);
        gl.viewport(x, y, w, h);
        if scissor {
            gl.enable(glow::SCISSOR_TEST);
            gl.scissor(sc[0], sc[1], sc[2], sc[3]);
        }
        gl.use_program(Some(aoprog));
        gl.active_texture(glow::TEXTURE2);
        gl.bind_texture(glow::TEXTURE_2D, Some(col));
        gl.active_texture(glow::TEXTURE3);
        gl.bind_texture(glow::TEXTURE_2D, Some(dep));
        gl.uniform_1_i32(gl.get_uniform_location(aoprog, "u_col").as_ref(), 2);
        gl.uniform_1_i32(gl.get_uniform_location(aoprog, "u_dep").as_ref(), 3);
        gl.uniform_2_f32(gl.get_uniform_location(aoprog, "u_texel").as_ref(), 1.0 / w as f32, 1.0 / h as f32);
        gl.uniform_4_f32(gl.get_uniform_location(aoprog, "u_k").as_ref(), p.depth_k[0], p.depth_k[1], p.depth_k[2], p.depth_k[3]);
        gl.uniform_1_f32(gl.get_uniform_location(aoprog, "u_h").as_ref(), h as f32);
        gl.uniform_1_f32(gl.get_uniform_location(aoprog, "u_str").as_ref(), p.ao.clamp(0.0, 1.0));
        gl.uniform_1_f32(gl.get_uniform_location(aoprog, "u_edge").as_ref(), p.edges.clamp(0.0, 1.0));
        gl.uniform_1_i32(gl.get_uniform_location(aoprog, "u_style").as_ref(), p.style.code());
        gl.disable(glow::DEPTH_TEST);
        gl.enable(glow::BLEND);
        gl.blend_func(glow::ONE, glow::ONE_MINUS_SRC_ALPHA);
        gl.bind_vertex_array(Some(vao));
        gl.draw_arrays(glow::TRIANGLES, 0, 3);
        gl.bind_vertex_array(None);
        gl.bind_texture(glow::TEXTURE_2D, None);
        gl.active_texture(glow::TEXTURE2);
        gl.bind_texture(glow::TEXTURE_2D, None);
        gl.active_texture(glow::TEXTURE0);
        gl.use_program(None);
        true
    }

    /// Harta de umbre a soarelui: adancimea scenei vazuta din directia soarelui (proiectie ortografica
    /// care cuprinde sfera scenei). Intoarce matricea luminii, marimea unui texel in lume si adancimea totala;
    /// None daca nu se poate (atunci fara umbre).
    unsafe fn shadow_pass(&mut self, gl: &glow::Context, p: &FrameParams, lists: Option<&HashMap<u32, DrawList>>) -> Option<(Mat4, f32, f32)> {
        let sprog = self.sprog?;
        if self.shadow_failed || p.scene.w <= 0.0 || p.sun.z <= 0.02 {
            return None;
        }
        if self.shadow_fbo.is_none() {
            let t = gl.create_texture().ok()?;
            gl.bind_texture(glow::TEXTURE_2D, Some(t));
            gl.tex_image_2d(glow::TEXTURE_2D, 0, glow::DEPTH_COMPONENT24 as i32, SHADOW_SIZE, SHADOW_SIZE, 0, glow::DEPTH_COMPONENT, glow::UNSIGNED_INT, None);
            gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MIN_FILTER, glow::NEAREST as i32);
            gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MAG_FILTER, glow::NEAREST as i32);
            gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_S, glow::CLAMP_TO_EDGE as i32);
            gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_T, glow::CLAMP_TO_EDGE as i32);
            gl.bind_texture(glow::TEXTURE_2D, None);
            let f = gl.create_framebuffer().ok()?;
            let prev = gl.get_parameter_framebuffer(glow::FRAMEBUFFER_BINDING);
            gl.bind_framebuffer(glow::FRAMEBUFFER, Some(f));
            gl.framebuffer_texture_2d(glow::FRAMEBUFFER, glow::DEPTH_ATTACHMENT, glow::TEXTURE_2D, Some(t), 0);
            gl.draw_buffers(&[glow::NONE]);
            gl.read_buffer(glow::NONE);
            let ok = gl.check_framebuffer_status(glow::FRAMEBUFFER) == glow::FRAMEBUFFER_COMPLETE;
            gl.bind_framebuffer(glow::FRAMEBUFFER, prev);
            if !ok {
                gl.delete_framebuffer(f);
                gl.delete_texture(t);
                self.shadow_failed = true;
                return None;
            }
            self.shadow_fbo = Some((f, t));
        }
        let (fbo, _) = self.shadow_fbo?;
        // lumina: ortografica pe directia soarelui, cuprinde sfera scenei
        let c = Vec3::new(p.scene.x, p.scene.y, p.scene.z);
        let r = p.scene.w;
        let dir = p.sun.normalize();
        let up = if dir.z.abs() > 0.99 { Vec3::Y } else { Vec3::Z };
        let view = Mat4::look_at_rh(c + dir * r * 2.0, c, up);
        let proj = Mat4::orthographic_rh_gl(-r, r, -r, r, r * 0.9, r * 3.1);
        let svp = proj * view;
        let prev_fb = gl.get_parameter_framebuffer(glow::FRAMEBUFFER_BINDING);
        let mut vp = [0i32; 4];
        gl.get_parameter_i32_slice(glow::VIEWPORT, &mut vp);
        let scissor = gl.is_enabled(glow::SCISSOR_TEST);
        gl.bind_framebuffer(glow::FRAMEBUFFER, Some(fbo));
        gl.viewport(0, 0, SHADOW_SIZE, SHADOW_SIZE);
        gl.disable(glow::SCISSOR_TEST);
        gl.disable(glow::BLEND);
        gl.enable(glow::DEPTH_TEST);
        gl.depth_func(glow::LEQUAL);
        gl.depth_mask(true);
        gl.clear_depth_f32(1.0);
        gl.clear(glow::DEPTH_BUFFER_BIT);
        gl.disable(glow::CULL_FACE);
        // un mic decalaj de adancime: fara "acnee" pe suprafetele luminate
        gl.enable(glow::POLYGON_OFFSET_FILL);
        gl.polygon_offset(1.5, 2.0);
        gl.use_program(Some(sprog));
        gl.uniform_matrix_4_f32_slice(gl.get_uniform_location(sprog, "u_vp").as_ref(), false, &svp.to_cols_array());
        let mut pl = [0f32; 24];
        for (i, v) in p.planes.iter().enumerate() {
            pl[i * 4..i * 4 + 4].copy_from_slice(&v.to_array());
        }
        gl.uniform_4_f32_slice(gl.get_uniform_location(sprog, "u_planes").as_ref(), &pl);
        gl.uniform_1_i32(gl.get_uniform_location(sprog, "u_nplanes").as_ref(), p.nplanes);
        for (slot, m) in &self.meshes {
            gl.bind_vertex_array(Some(m.vao));
            draw_ranges(gl, m.count, lists.and_then(|l| l.get(slot)).map(|d| &d.all));
        }
        gl.bind_vertex_array(None);
        gl.disable(glow::POLYGON_OFFSET_FILL);
        gl.use_program(None);
        gl.bind_framebuffer(glow::FRAMEBUFFER, prev_fb);
        gl.viewport(vp[0], vp[1], vp[2], vp[3]);
        if scissor {
            gl.enable(glow::SCISSOR_TEST);
        }
        Some((svp, 2.0 * r / SHADOW_SIZE as f32, r * 2.2))
    }

    /// Tintele SSAO pentru marimea data (cel mult doua: fereastra + randare), create la nevoie.
    unsafe fn ao_target(&mut self, gl: &glow::Context, w: i32, h: i32, samples: i32) -> Option<usize> {
        if let Some(i) = self.ao_targets.iter().position(|t| t.w == w && t.h == h && t.samples == samples) {
            return Some(i);
        }
        if self.ao_targets.len() >= 2 {
            let old = self.ao_targets.remove(0);
            if let Some((f, c, d)) = old.ms {
                gl.delete_framebuffer(f);
                gl.delete_renderbuffer(c);
                gl.delete_renderbuffer(d);
            }
            gl.delete_framebuffer(old.fbo);
            gl.delete_texture(old.col);
            gl.delete_texture(old.dep);
        }
        let tex = |internal: u32, format: u32, ty: u32| -> Option<glow::Texture> {
            let t = gl.create_texture().ok()?;
            gl.bind_texture(glow::TEXTURE_2D, Some(t));
            gl.tex_image_2d(glow::TEXTURE_2D, 0, internal as i32, w, h, 0, format, ty, None);
            gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MIN_FILTER, glow::NEAREST as i32);
            gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MAG_FILTER, glow::NEAREST as i32);
            gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_S, glow::CLAMP_TO_EDGE as i32);
            gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_T, glow::CLAMP_TO_EDGE as i32);
            Some(t)
        };
        let col = tex(glow::RGBA8, glow::RGBA, glow::UNSIGNED_BYTE)?;
        let dep = tex(glow::DEPTH_COMPONENT24, glow::DEPTH_COMPONENT, glow::UNSIGNED_INT)?;
        gl.bind_texture(glow::TEXTURE_2D, None);
        let fbo = gl.create_framebuffer().ok()?;
        gl.bind_framebuffer(glow::FRAMEBUFFER, Some(fbo));
        gl.framebuffer_texture_2d(glow::FRAMEBUFFER, glow::COLOR_ATTACHMENT0, glow::TEXTURE_2D, Some(col), 0);
        gl.framebuffer_texture_2d(glow::FRAMEBUFFER, glow::DEPTH_ATTACHMENT, glow::TEXTURE_2D, Some(dep), 0);
        let mut ok = gl.check_framebuffer_status(glow::FRAMEBUFFER) == glow::FRAMEBUFFER_COMPLETE;
        let mut ms = None;
        if ok && samples > 1 {
            let (Ok(f), Ok(c), Ok(d)) = (gl.create_framebuffer(), gl.create_renderbuffer(), gl.create_renderbuffer()) else { return None };
            gl.bind_renderbuffer(glow::RENDERBUFFER, Some(c));
            gl.renderbuffer_storage_multisample(glow::RENDERBUFFER, samples, glow::RGBA8, w, h);
            gl.bind_renderbuffer(glow::RENDERBUFFER, Some(d));
            gl.renderbuffer_storage_multisample(glow::RENDERBUFFER, samples, glow::DEPTH_COMPONENT24, w, h);
            gl.bind_renderbuffer(glow::RENDERBUFFER, None);
            gl.bind_framebuffer(glow::FRAMEBUFFER, Some(f));
            gl.framebuffer_renderbuffer(glow::FRAMEBUFFER, glow::COLOR_ATTACHMENT0, glow::RENDERBUFFER, Some(c));
            gl.framebuffer_renderbuffer(glow::FRAMEBUFFER, glow::DEPTH_ATTACHMENT, glow::RENDERBUFFER, Some(d));
            ok = gl.check_framebuffer_status(glow::FRAMEBUFFER) == glow::FRAMEBUFFER_COMPLETE;
            ms = Some((f, c, d));
        }
        if !ok {
            return None;
        }
        self.ao_targets.push(AoTargets { w, h, samples, ms, fbo, col, dep });
        Some(self.ao_targets.len() - 1)
    }
}

unsafe fn delete_points(gl: &glow::Context, p: GpuPoints) {
    gl.delete_vertex_array(p.vao);
    gl.delete_buffer(p.vbo_pos);
    gl.delete_buffer(p.vbo_col);
}

unsafe fn compile_program(gl: &glow::Context, header: &str, vs: &str, fs: &str) -> Result<glow::Program, String> {
    let prog = gl.create_program()?;
    let mut shaders = Vec::new();
    for (ty, src) in [(glow::VERTEX_SHADER, vs), (glow::FRAGMENT_SHADER, fs)] {
        let s = gl.create_shader(ty)?;
        gl.shader_source(s, &format!("{header}{src}"));
        gl.compile_shader(s);
        if !gl.get_shader_compile_status(s) {
            return Err(gl.get_shader_info_log(s));
        }
        gl.attach_shader(prog, s);
        shaders.push(s);
    }
    gl.link_program(prog);
    if !gl.get_program_link_status(prog) {
        return Err(gl.get_program_info_log(prog));
    }
    for s in shaders {
        gl.detach_shader(prog, s);
        gl.delete_shader(s);
    }
    Ok(prog)
}

unsafe fn delete_mesh(gl: &glow::Context, m: GpuMesh) {
    gl.delete_vertex_array(m.vao);
    gl.delete_buffer(m.vbo_pos);
    gl.delete_buffer(m.vbo_nrm);
    gl.delete_buffer(m.vbo_col);
    gl.delete_buffer(m.vbo_mat);
}

fn bytes_f32(v: &[f32]) -> &[u8] {
    unsafe { std::slice::from_raw_parts(v.as_ptr() as *const u8, v.len() * 4) }
}

/// Date de varf (pozitii + normale) pentru un model.
pub fn mesh_data(m: &Model) -> (Vec<f32>, Vec<i8>) {
    let mut pos = Vec::with_capacity(m.pos.len() * 3);
    let mut nrm = Vec::with_capacity(m.pos.len() * 4);
    for t in m.pos.chunks_exact(3) {
        let n = (t[1] - t[0]).cross(t[2] - t[0]).normalize_or_zero();
        let q = [(n.x * 127.0) as i8, (n.y * 127.0) as i8, (n.z * 127.0) as i8, 0];
        for p in t {
            pos.extend_from_slice(&[p.x, p.y, p.z]);
            nrm.extend_from_slice(&q);
        }
    }
    (pos, nrm)
}

/// Deseneaza intervalele (varf de start, numar de varfuri), limitate la marimea bufferului; None = tot bufferul.
unsafe fn draw_ranges(gl: &glow::Context, total: i32, ranges: Option<&Vec<(i32, i32)>>) {
    match ranges {
        None => gl.draw_arrays(glow::TRIANGLES, 0, total),
        Some(r) => {
            for &(first, count) in r {
                if first < 0 || count <= 0 || first >= total {
                    continue;
                }
                let n = count.min(total - first);
                gl.draw_arrays(glow::TRIANGLES, first, n - n % 3);
            }
        }
    }
}

/// Stilul de afisare al modelului (ca in Navisworks / Revit).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, serde::Serialize, serde::Deserialize)]
pub enum VisualStyle {
    /// materiale, texturi, reflexii, umbre de contact
    #[default]
    Realistic,
    /// culorile din fisier, lumina, fara texturi
    Shaded,
    /// macheta: aproape alb, mat, margini accentuate
    Concept,
    /// doar muchiile triunghiurilor
    Wireframe,
    /// hidden-line: suprafete neutre, muchiile ascunse de geometrie nu se vede (ca in AutoCAD / Navisworks)
    Hidden,
}

impl VisualStyle {
    pub const ALL: [VisualStyle; 5] = [VisualStyle::Realistic, VisualStyle::Shaded, VisualStyle::Concept, VisualStyle::Wireframe, VisualStyle::Hidden];
    pub fn label(self) -> &'static str {
        match self {
            VisualStyle::Realistic => "Realistic",
            VisualStyle::Shaded => "Shaded",
            VisualStyle::Concept => "Concept",
            VisualStyle::Wireframe => "Wireframe",
            VisualStyle::Hidden => "Hidden line",
        }
    }
    pub fn code(self) -> i32 {
        match self {
            VisualStyle::Realistic => 0,
            VisualStyle::Shaded => 1,
            VisualStyle::Concept => 2,
            VisualStyle::Wireframe => 3,
            VisualStyle::Hidden => 4,
        }
    }
}
