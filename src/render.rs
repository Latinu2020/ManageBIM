//! OpenGL rendering (glow) of the federated models, inside an egui callback.
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
    /// only a portion of the colors (the elements that changed), starting at vertex `first`
    ColorRange { slot: u32, first: u32, col: Vec<u8>, mat: Vec<u8> },
    Remove { slot: u32 },
    /// the points of a cloud (E57); None = the slot has no cloud
    Points { slot: u32, cloud: Option<std::sync::Arc<crate::model::PointCloud>> },
    /// all texture layers (TEX_SIZE x TEX_SIZE RGBA each)
    Textures(Vec<Vec<u8>>),
}

/// What is drawn from a model in this frame (vertex ranges): all = the visible elements (opaque pass),
/// transp = only those with something transparent (transparent pass). No list = the whole model (as before).
pub struct DrawList {
    pub all: Vec<(i32, i32)>,
    pub transp: Vec<(i32, i32)>,
    /// point ranges (point cloud): the visible scans, the first N% of each
    pub points: Vec<(i32, i32)>,
}

/// What the last frame actually drew, after the frustum / section / LOD culling. Exposed through
/// MCP `get_performance`, so the effect of a change can be measured instead of guessed.
#[derive(Clone, Copy, Debug, Default, serde::Serialize)]
pub struct DrawStats {
    pub elements: u64,
    pub elements_drawn: u64,
    pub triangles: u64,
    pub triangles_drawn: u64,
    pub hidden: u64,
    pub culled_frustum: u64,
    pub culled_section: u64,
    pub culled_lod: u64,
    /// merged vertex ranges = draw calls for the opaque + transparent pass
    pub ranges: u64,
    pub points: u64,
    pub points_drawn: u64,
}

struct GpuPoints {
    vao: glow::VertexArray,
    vbo_pos: glow::Buffer,
    vbo_col: glow::Buffer,
    count: i32,
}

pub struct Renderer {
    /// draw lists per slot, computed by the app for the current view (None = everything)
    pub lists: Option<HashMap<u32, DrawList>>,
    prog: Option<glow::Program>,
    meshes: HashMap<u32, GpuMesh>,
    pprog: Option<glow::Program>,
    points: HashMap<u32, GpuPoints>,
    pub pending: Vec<Upload>,
    pub error: Option<String>,
    /// off-screen render request (Render tab) and its result
    pub capture: Option<CaptureReq>,
    /// interactive Ray Trace view: the frame to refine this paint (set by the viewport callback)
    pub pt_live: Option<crate::gpu_pt::PtFrame>,
    pub gpu_live: crate::gpu_pt::GpuPt,
    /// final render on the GPU (Render › Path Trace), advanced a few samples per paint
    pub pt_job: Option<GpuJob>,
    gpu_off: crate::gpu_pt::GpuPt,
    pub captured: Option<Result<Frame, String>>,
    fbo: Option<(glow::Framebuffer, glow::Renderbuffer, glow::Renderbuffer, u32, u32)>,
    tex: Option<glow::Texture>,
    /// contact shading (SSAO): the program, the targets (cached by size) and an empty VAO for the full-screen triangle
    aoprog: Option<glow::Program>,
    ao_targets: Vec<AoTargets>,
    ao_vao: Option<glow::VertexArray>,
    /// something failed (incomplete FBO etc.): normal drawing until restart
    pub ao_failed: bool,
    /// shadows: the depth program, FBO + depth texture, and the matrix / parameters of the current frame
    sprog: Option<glow::Program>,
    shadow_fbo: Option<(glow::Framebuffer, glow::Texture)>,
    shadow_state: Option<(Mat4, f32, f32)>,
    shadow_failed: bool,
}

/// shadow map size (pixels)
const SHADOW_SIZE: i32 = 2048;

struct AoTargets {
    w: i32,
    h: i32,
    samples: i32,
    /// multisample FBO (antialiasing) - None when samples <= 1
    ms: Option<(glow::Framebuffer, glow::Renderbuffer, glow::Renderbuffer)>,
    fbo: glow::Framebuffer,
    col: glow::Texture,
    dep: glow::Texture,
}

#[derive(Clone)]
/// A final path-traced render running on the GPU; its progress and result go to `job` (as for the CPU render).
pub struct GpuJob {
    pub frame: crate::gpu_pt::PtFrame,
    pub job: std::sync::Arc<crate::pathtrace::PathTraceJob>,
    pub last_read: std::time::Instant,
    /// samples per paint (adapted to keep the interface responsive)
    pub step: u32,
}

pub struct CaptureReq {
    pub params: FrameParams,
    pub w: u32,
    pub h: u32,
    pub bg_top: [u8; 3],
    pub bg_bottom: [u8; 3],
    /// what is drawn (hidden / dimmed as in the viewport); None = the whole model (renders, video)
    pub lists: Option<std::sync::Arc<HashMap<u32, DrawList>>>,
    /// path traced on the GPU instead of rasterised (Ray Trace view on): (frame, samples per pixel)
    pub pt: Option<(crate::gpu_pt::PtFrame, u32)>,
    /// read back the accumulated interactive Ray Trace image instead of rendering a new one
    pub live: bool,
    /// a path-traced image was requested: never fall back to the raster view silently
    pub require_pt: bool,
}

#[derive(Clone, Copy)]
pub struct FrameParams {
    pub view_proj: Mat4,
    pub eye: Vec3,
    pub planes: [Vec4; 6],
    pub nplanes: i32,
    /// direction towards the sun (world)
    pub sun: Vec3,
    /// lighting: x = exposure, y = ambient, z = sun intensity (1 = default)
    pub light: Vec3,
    /// point size of the clouds (pixels)
    pub point_size: f32,
    /// point clouds: (color mode, min elevation, max elevation, round) and (real size m, pixels per meter)
    pub cloud: Vec4,
    pub cloud_world: [f32; 2],
    /// faces cut by the section drawn solid (caps)
    pub caps: bool,
    /// contact shading: 0 = off; otherwise the strength (0..1)
    pub ao: f32,
    /// projection: (P22, P32, orthographic) for linear depth and P11 (Y scale)
    /// fine edges (0 = off; otherwise the strength)
    pub edges: f32,
    pub depth_k: [f32; 4],
    /// display style
    pub style: VisualStyle,
    /// shadows cast by the sun + the sphere enclosing the scene (center, radius) for the shadow map
    pub shadows: bool,
    /// lamps in the model: xyz = position, w = power (lux / 20)
    pub lamps: [Vec4; 8],
    /// rgb = color, w = radius (m)
    pub lamp_data: [Vec4; 8],
    /// xyz = light direction (toward the target), w = type: -2 point, 2 distant (sun), otherwise cos(half cone angle) for spot
    pub lamp_dir: [Vec4; 8],
    pub nlamps: i32,
    /// night scene: sky and ambient light almost off, only the lamps give light
    pub night: bool,
    pub scene: Vec4,
}

const VS_P: &str = r#"
layout(location=0) in vec3 a_pos;
layout(location=1) in vec4 a_col;
uniform mat4 u_vp;
uniform float u_size;
// x = real point size (m, 0 = pixels only), y = pixels per meter at distance 1
uniform vec2 u_world;
out vec3 v_pos;
out vec4 v_col;
void main() {
    v_pos = a_pos;
    v_col = a_col;
    gl_Position = u_vp * vec4(a_pos, 1.0);
    // close to the cloud the points grow (fill the gaps, continuous look), far away they stay at u_size
    float ws = u_world.x * u_world.y / max(gl_Position.w, 1e-3);
    gl_PointSize = clamp(max(u_size, ws), 1.0, 14.0);
}
"#;

const FS_P: &str = r#"
in vec3 v_pos;
in vec4 v_col;
uniform vec4 u_planes[6];
uniform int u_nplanes;
uniform float u_exposure;
uniform vec4 u_cloud;
out vec4 o_col;
void main() {
    for (int i = 0; i < 6; i++) {
        if (i >= u_nplanes) break;
        if (dot(u_planes[i].xyz, v_pos) > u_planes[i].w) discard;
    }
    // x = color mode (0 scan RGB, 1 elevation, 2 gray), y/z = min/max elevation, w = round points with relief
    vec3 c = v_col.rgb;
    if (u_cloud.x > 0.5 && u_cloud.x < 1.5) {
        float t = clamp((v_pos.z - u_cloud.y) / max(u_cloud.z - u_cloud.y, 1e-3), 0.0, 1.0);
        // blue - cyan - green - yellow - red (as in Recap / CloudCompare)
        c = t < 0.25 ? mix(vec3(0.15, 0.25, 0.85), vec3(0.1, 0.75, 0.9), t / 0.25)
          : t < 0.5 ? mix(vec3(0.1, 0.75, 0.9), vec3(0.2, 0.8, 0.3), (t - 0.25) / 0.25)
          : t < 0.75 ? mix(vec3(0.2, 0.8, 0.3), vec3(0.95, 0.85, 0.2), (t - 0.5) / 0.25)
          : mix(vec3(0.95, 0.85, 0.2), vec3(0.9, 0.25, 0.15), (t - 0.75) / 0.25);
    } else if (u_cloud.x > 1.5) {
        c = vec3(dot(c, vec3(0.299, 0.587, 0.114)));
    }
    float shade = 1.0;
    if (u_cloud.w > 0.5) {
        vec2 q = gl_PointCoord * 2.0 - 1.0;
        float r2 = dot(q, q);
        if (r2 > 1.0) discard;
        // each point as a small sphere lit from the top-left: relief, no „square noise"
        vec3 n = vec3(q.x, -q.y, sqrt(1.0 - r2));
        shade = 0.62 + 0.38 * max(dot(n, normalize(vec3(-0.4, 0.5, 0.75))), 0.0);
        // depth on the sphere: the points overlap cleanly
        gl_FragDepth = gl_FragCoord.z - (1.0 - r2) * 2e-6;
    } else {
        gl_FragDepth = gl_FragCoord.z;
    }
    // alpha 0.98 = "point cloud" marker: the SSAO / edges pass does not touch it (otherwise every point becomes a black edge)
    o_col = vec4(min(c * u_exposure * shade, vec3(1.0)), 0.98);
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

// shadow pass: depth only; sections and transparent / hidden elements do not cast shadows
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
uniform int u_style;   // 0 realistic, 1 shaded, 2 concept, 3 wireframe, 4 hidden-line
out vec4 o_col;
uniform sampler2D u_shadow;
uniform mat4 u_svp;
uniform vec4 u_sp;     // x = size of a texel in the world, y = total depth (m), z = shadows on
uniform vec4 u_lamps[8];     // xyz = position, w = power
uniform vec4 u_lamp_data[8]; // rgb = linear color, w = radius (0 = unlimited)
uniform vec4 u_lamp_dir[8];  // xyz = direction (toward the target), w = -2 point / 2 distant / cos(half cone angle) spot
uniform int u_nlamps;
uniform int u_night;

// 1 = in light, 0 = in shadow (3x3 PCF: soft edges)
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
        // inner face seen through the cut: solid cap, hatched (only with an active section, opaque elements)
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
    // styles without materials: the colors from the file (shaded) or almost white, matte (concept)
    if (u_style == 1 || u_style == 3 || u_style == 4) pt = 0;
    if (u_style == 2) { pt = 0; base = mix(base, vec3(0.94, 0.94, 0.92), 0.78); }
    if (u_style == 4) { base = vec3(0.86, 0.88, 0.91); }
    float pbr_rough = -1.0;
    float pbr_metal = 0.0;
    vec3 an = abs(n);
    vec2 uv = an.z >= an.x && an.z >= an.y ? v_pos.xy : (an.x >= an.y ? v_pos.yz : v_pos.xz);
    uv /= (0.05 + v_mat.w * 4.0);
    if (pt == 200) {
        // PBR: two packed layers (albedo*AO + roughness / normal + metallic)
        // the layer is in byte 1 of v_mat (the gloss comes from the map, so the byte is free)
        int l = int(v_mat.y * 255.0 + 0.5);
        vec4 ca = texture(u_tex, vec3(uv, float(l)));
        vec4 cs = texture(u_tex, vec3(uv, float(l + 1)));
        base = ca.rgb;
        pbr_rough = ca.a;
        pbr_metal = cs.a;
        // tangent basis from the derivatives of the position and the UV (normal mapping without UV attributes)
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
    // hemispheric ambient (cool sky, warm ground) + warm sun + fill light from the side opposite the sun
    vec3 amb = mix(vec3(0.90, 0.86, 0.80), vec3(0.94, 0.97, 1.04), sky);
    vec3 fdir = normalize(vec3(-u_sun.x, -u_sun.y, 0.45));
    float fill = max(dot(n, fdir), 0.0);
    // at night: the sky, the sun and the fill light almost off - the lamps in the model give the light
    float dayf = u_night == 1 ? 0.0 : 1.0;
    float ambf = u_night == 1 ? 0.07 : 1.0;
    vec3 k = (amb * (0.21 + 0.14 * sky) * u_light.y + vec3(0.31 * head + 0.07 * fill * u_light.y)) * ambf + vec3(1.02, 0.99, 0.94) * (0.30 * sun * u_light.z) * dayf;
    // cast shadow: faces turned toward the sun but covered become visibly darker (as outdoors, where the sun dominates)
    k *= mix(1.0, mix(0.62, 1.0, shv), smoothstep(0.0, 0.25, dot(n, u_sun)) * step(0.5, u_sp.z) * dayf);
    // lamps in the model ( Interactive Ray Trace as in Navisworks ): light falls on surfaces in real time.
    // Point = in all directions; Spot = cone aimed at the target (rotating it from the gizmo shows immediately);
    // Distant = parallel rays along the light direction. Radius 0 = no falloff with distance.
    // Added AFTER the sun shadow: the lamps are not darkened by the sun shadow.
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
                // cone: the center brightest, the edge soft (as in offline rendering)
                float ca = dot(-L, ld.xyz);
                float cone = clamp((ca - ld.w) / max(1.0 - ld.w, 1e-4), 0.0, 1.0);
                att *= pow(cone, 1.5);
            }
        }
        float nl = max(dot(n, L), 0.0);
        klamp += u_lamp_data[li].rgb * nl * att;
        // lamp highlight: the bright spot shows on glossy surfaces
        vec3 hl = normalize(L + vd);
        lamp_spec += u_lamp_data[li].rgb * pow(max(dot(n, hl), 0.0), 48.0) * nl * att * 0.25;
    }
    k += klamp;
    // a small gloss for every material (surfaces no longer look flat / papery)
    // the roughness map (0 = mirror) is the inverse of gloss; the metalness map replaces the global value
    float gloss = pbr_rough >= 0.0 ? clamp(1.0 - pbr_rough, 0.03, 1.0) : (u_style == 0 ? max(v_mat.y, 0.07) : ((u_style == 2 || u_style == 4) ? 0.0 : 0.07));
    float metal = u_style == 0 ? (pbr_rough >= 0.0 ? pbr_metal : v_mat.z) : 0.0;
    if (u_style == 2 || u_style == 4) {
        // concept / hidden-line: soft light, no reflections
        // the lamps show here too (colored light spot), at night the background is dark
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
        // realistic: sky reflection (Fresnel) - glass, metal, glossy surfaces catch the sky and the ground
        vec3 rv = reflect(-vd, n);
        vec3 env = mix(vec3(0.34, 0.32, 0.29), mix(vec3(0.78, 0.84, 0.92), vec3(0.42, 0.58, 0.85), clamp(rv.z, 0.0, 1.0)), smoothstep(-0.15, 0.25, rv.z));
        vec3 f0 = mix(vec3(0.04), base, metal);
        vec3 fres = f0 + (1.0 - f0) * pow(1.0 - head, 5.0);
        bool glass = a < 0.95;
        float refl = glass ? 0.9 : gloss * 0.6;
        c = mix(c, env * u_light.z * 1.1 * mix(0.04, 1.0, dayf), clamp(fres * refl, 0.0, 1.0));
        // glass: more opaque at grazing angles (as in reality)
        if (glass) a = clamp(a + (1.0 - a) * pow(1.0 - head, 3.0) * 0.8, 0.0, 1.0);
        // filmic tones (ACES): natural contrast and saturation, highlights do not burn out
        // ACES lifts the midtones: compensated exposure (a mid gray stays almost unchanged)
        c *= u_light.x * 0.85;
        c = clamp((c * (2.51 * c + 0.03)) / (c * (2.43 * c + 0.59) + 0.14), 0.0, 1.0);
        o_col = vec4(c * a, a);
        return;
    }
    // soft shoulder instead of clipping to white: strong highlights do not burn out
    c *= u_light.x;
    vec3 e = max(c - 0.8, 0.0);
    c = min(c, vec3(0.8)) + e / (1.0 + e * 5.0);
    o_col = vec4(c * a, a);
}
"#;

impl Default for Renderer {
    fn default() -> Self {
        Self::new()
    }
}

impl Renderer {
    pub fn new() -> Renderer {
        Renderer { lists: None, prog: None, meshes: HashMap::new(), pprog: None, points: HashMap::new(), aoprog: None, ao_targets: Vec::new(), ao_vao: None, ao_failed: false, sprog: None, shadow_fbo: None, shadow_state: None, shadow_failed: false, pending: Vec::new(), error: None, capture: None, captured: None, fbo: None, tex: None, pt_live: None, gpu_live: Default::default(), pt_job: None, gpu_off: Default::default() }
    }

    unsafe fn init(&mut self, gl: &glow::Context) {
        if self.prog.is_some() || self.error.is_some() {
            return;
        }
        // graphics card / driver too old (or Remote Desktop without acceleration): an explanation, not just the shader error
        let v = gl.version();
        let hint = format!(
            "The 3D view needs OpenGL 3.3; this graphics driver offers OpenGL {}.{} ({}). Update the graphics driver, run ManageBIM on the dedicated graphics card, or restart with software rendering.",
            v.major,
            v.minor,
            gl.get_parameter_string(glow::RENDERER)
        );
        if !v.is_embedded && (v.major, v.minor) < (3, 3) {
            self.error = Some(hint);
            return;
        }
        let header = if v.is_embedded { "#version 300 es\nprecision highp float;\nprecision highp sampler2DArray;\n" } else { "#version 330 core\n" };
        let Ok(prog) = gl.create_program() else {
            self.error = Some(hint);
            return;
        };
        let mut shaders = Vec::new();
        for (ty, src) in [(glow::VERTEX_SHADER, VS), (glow::FRAGMENT_SHADER, FS)] {
            let Ok(s) = gl.create_shader(ty) else {
                self.error = Some(hint);
                return;
            };
            gl.shader_source(s, &format!("{header}{src}"));
            gl.compile_shader(s);
            if !gl.get_shader_compile_status(s) {
                self.error = Some(format!("Shader error: {}\n{hint}", gl.get_shader_info_log(s)));
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
        // the program for point clouds; if it fails, the models still show (just without points)
        self.pprog = compile_program(gl, header, VS_P, FS_P).ok();
        // SSAO: without it, drawing stays the usual one
        let hdr_ao = if gl.version().is_embedded { "#version 300 es\nprecision highp float;\nprecision highp sampler2D;\n" } else { header };
        self.aoprog = compile_program(gl, hdr_ao, VS_AO, FS_AO).ok();
        self.ao_vao = gl.create_vertex_array().ok();
        // shadows: without them, drawing stays the usual one
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
                    // out of graphics memory: the model is skipped (it stays invisible) instead of closing ManageBIM
                    let (Ok(vao), Ok(vbo_pos), Ok(vbo_nrm), Ok(vbo_col), Ok(vbo_mat)) = (gl.create_vertex_array(), gl.create_buffer(), gl.create_buffer(), gl.create_buffer(), gl.create_buffer()) else {
                        eprintln!("render: could not create the GPU buffers for model {slot}");
                        continue;
                    };
                    gl.bind_vertex_array(Some(vao));
                    gl.bind_buffer(glow::ARRAY_BUFFER, Some(vbo_pos));
                    gl.buffer_data_u8_slice(glow::ARRAY_BUFFER, bytes_f32(&pos), glow::STATIC_DRAW);
                    gl.enable_vertex_attrib_array(0);
                    gl.vertex_attrib_pointer_f32(0, 3, glow::FLOAT, false, 12, 0);
                    gl.bind_buffer(glow::ARRAY_BUFFER, Some(vbo_nrm));
                    let nb: &[u8] = std::slice::from_raw_parts(nrm.as_ptr() as *const u8, nrm.len());
                    gl.buffer_data_u8_slice(glow::ARRAY_BUFFER, nb, glow::STATIC_DRAW);
                    gl.enable_vertex_attrib_array(1);
                    gl.vertex_attrib_pointer_f32(1, 4, glow::BYTE, true, 4, 0);
                    gl.bind_buffer(glow::ARRAY_BUFFER, Some(vbo_col));
                    gl.buffer_data_u8_slice(glow::ARRAY_BUFFER, &col, glow::DYNAMIC_DRAW);
                    gl.enable_vertex_attrib_array(2);
                    gl.vertex_attrib_pointer_f32(2, 4, glow::UNSIGNED_BYTE, true, 4, 0);
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
                        // 4 bytes per vertex; we ignore ranges outside the buffer (mesh replaced in the meantime)
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
        self.gpu_job_step(gl);
        // interactive Ray Trace: the path-traced image replaces the rasterised view
        if let Some(f) = self.pt_live.take() {
            let mut vp = [0i32; 4];
            gl.get_parameter_i32_slice(glow::VIEWPORT, &mut vp);
            if self.gpu_live.live(gl, &f, vp, self.tex, true) {
                if let Some(req) = self.capture.take() {
                    let r = self.capture_frame(gl, &req);
                    self.captured = Some(r);
                }
                return;
            }
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
            // egui draws with filled triangles after us
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
        // lamps (max 8), as in Navisworks Interactive Ray Trace
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
        // shadow map on unit 4
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
        // pass 1: opaque
        gl.disable(glow::BLEND);
        gl.uniform_1_i32(u_pass.as_ref(), 0);
        for (slot, m) in &self.meshes {
            gl.bind_vertex_array(Some(m.vao));
            draw_ranges(gl, m.count, lists.and_then(|l| l.get(slot)).map(|d| &d.all));
        }
        // the point clouds (opaque), with their own program
        if let (Some(pp), false) = (self.pprog, self.points.is_empty()) {
            gl.use_program(Some(pp));
            if !gl.version().is_embedded {
                gl.enable(glow::PROGRAM_POINT_SIZE);
            }
            gl.uniform_matrix_4_f32_slice(gl.get_uniform_location(pp, "u_vp").as_ref(), false, &p.view_proj.to_cols_array());
            gl.uniform_1_f32(gl.get_uniform_location(pp, "u_size").as_ref(), p.point_size.clamp(1.0, 16.0));
            gl.uniform_1_f32(gl.get_uniform_location(pp, "u_exposure").as_ref(), p.light.x);
            gl.uniform_4_f32(gl.get_uniform_location(pp, "u_cloud").as_ref(), p.cloud.x, p.cloud.y, p.cloud.z, p.cloud.w);
            gl.uniform_2_f32(gl.get_uniform_location(pp, "u_world").as_ref(), p.cloud_world[0], p.cloud_world[1]);
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
        // pass 2: transparent
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

    /// Advances the final GPU render: a few samples per paint (about 30 ms), a preview read back twice a
    /// second, and at the end the sums go to the job, which then denoises in the background like the CPU render.
    unsafe fn gpu_job_step(&mut self, gl: &glow::Context) {
        let Some(mut gj) = self.pt_job.take() else { return };
        let job = gj.job.clone();
        let target = job.samples_target.max(1);
        let n = job.count.load(std::sync::atomic::Ordering::Relaxed);
        let timed_out = job.time_limit_secs > 0.0 && job.started.elapsed().as_secs_f32() >= job.time_limit_secs;
        let cancel = job.cancel.load(std::sync::atomic::Ordering::Relaxed);
        let prev_fb = gl.get_parameter_framebuffer(glow::FRAMEBUFFER_BINDING);
        let mut vp = [0i32; 4];
        gl.get_parameter_i32_slice(glow::VIEWPORT, &mut vp);
        let scissor = gl.is_enabled(glow::SCISSOR_TEST);
        let mut sc = [0i32; 4];
        gl.get_parameter_i32_slice(glow::SCISSOR_BOX, &mut sc);
        let restore = |gl: &glow::Context| {
            gl.bind_framebuffer(glow::FRAMEBUFFER, prev_fb);
            gl.viewport(vp[0], vp[1], vp[2], vp[3]);
            if scissor {
                gl.scissor(sc[0], sc[1], sc[2], sc[3]);
                gl.enable(glow::SCISSOR_TEST);
            } else {
                gl.disable(glow::SCISSOR_TEST);
            }
        };
        let finished = n >= target || timed_out || cancel;
        if !finished {
            let k = gj.step.clamp(1, target - n);
            let t0 = std::time::Instant::now();
            let ok = self.gpu_off.accumulate(gl, &gj.frame, job.width as i32, job.height as i32, k, self.tex, Some(40.0));
            gl.finish();
            restore(gl);
            if !ok {
                *job.denoise_msg.lock().unwrap_or_else(|e| e.into_inner()) = self.gpu_off.error.clone().unwrap_or_else(|| "GPU path tracing failed".into());
                job.done.store(true, std::sync::atomic::Ordering::Relaxed);
                return;
            }
            let ms = t0.elapsed().as_secs_f32() * 1000.0;
            gj.step = if ms < 15.0 { (k * 2).min(64) } else if ms > 45.0 { (k / 2).max(1) } else { k };
            job.count.store(self.gpu_off.samples, std::sync::atomic::Ordering::Relaxed);
        }
        if finished || gj.last_read.elapsed().as_secs_f32() > 0.5 {
            gj.last_read = std::time::Instant::now();
            if let Some((col, aux)) = self.gpu_off.read_back(gl) {
                *job.buf.lock().unwrap_or_else(|e| e.into_inner()) = col;
                *job.aux.lock().unwrap_or_else(|e| e.into_inner()) = aux;
                job.count.store(self.gpu_off.samples, std::sync::atomic::Ordering::Relaxed);
            }
            restore(gl);
        }
        if finished {
            crate::pathtrace::finish_job(job);
        } else {
            self.pt_job = Some(gj);
        }
    }

    /// A path-traced image of `f` at w x h with `spp` samples (ACES tone map, as the renders); None if the
    /// GPU path tracer is not available.
    unsafe fn capture_pt(&mut self, gl: &glow::Context, f: &crate::gpu_pt::PtFrame, w: u32, h: u32, spp: u32) -> Option<Frame> {
        let prev_fb = gl.get_parameter_framebuffer(glow::FRAMEBUFFER_BINDING);
        let mut vp = [0i32; 4];
        gl.get_parameter_i32_slice(glow::VIEWPORT, &mut vp);
        let scissor = gl.is_enabled(glow::SCISSOR_TEST);
        let mut sc = [0i32; 4];
        gl.get_parameter_i32_slice(glow::SCISSOR_BOX, &mut sc);
        let mut ok = true;
        // a few samples per call so one call never blocks the GPU for long
        let mut left = spp.max(1);
        while ok && left > 0 {
            let k = left.min(16);
            ok = self.gpu_off.accumulate(gl, f, w as i32, h as i32, k, self.tex, None);
            gl.finish();
            left -= k;
        }
        let data = if ok { self.gpu_off.read_back(gl) } else { None };
        gl.bind_framebuffer(glow::FRAMEBUFFER, prev_fb);
        gl.viewport(vp[0], vp[1], vp[2], vp[3]);
        if scissor {
            gl.scissor(sc[0], sc[1], sc[2], sc[3]);
            gl.enable(glow::SCISSOR_TEST);
        } else {
            gl.disable(glow::SCISSOR_TEST);
        }
        let (col, _) = data?;
        let n = self.gpu_off.samples.max(1) as f32;
        let mut rgba = Vec::with_capacity((w * h * 4) as usize);
        for p in col.as_chunks::<3>().0 {
            let c = crate::pathtrace::to_srgb([p[0] / n, p[1] / n, p[2] / n]);
            rgba.extend_from_slice(&[c[0], c[1], c[2], 255]);
        }
        Some(Frame { w, h, rgba })
    }

    /// Tone maps the accumulated interactive Ray Trace image - exactly what the 3D view shows, including
    /// the quick filter and partial bands - so an assistant can inspect the live view, not a new render.
    unsafe fn capture_live(&self, gl: &glow::Context) -> Option<Frame> {
        let (col, _) = self.gpu_live.read_back(gl)?;
        let n = self.gpu_live.samples.max(1) as f32;
        let mut rgba = Vec::with_capacity(col.len() / 3 * 4);
        for p in col.as_chunks::<3>().0 {
            let c = crate::pathtrace::to_srgb([p[0] / n, p[1] / n, p[2] / n]);
            rgba.extend_from_slice(&[c[0], c[1], c[2], 255]);
        }
        Some(Frame { w: self.gpu_live.status.w.max(1) as u32, h: self.gpu_live.status.h.max(1) as u32, rgba })
    }

    /// Renders the scene into an off-screen framebuffer of any size and reads it back.
    unsafe fn capture_frame(&mut self, gl: &glow::Context, req: &CaptureReq) -> Result<Frame, String> {
        if req.live {
            if let Some(fr) = self.capture_live(gl) {
                return Ok(fr);
            }
        }
        // Ray Trace view: the image is path traced at the requested size (not while a final render uses the GPU)
        if let (Some((f, spp)), None) = (&req.pt, &self.pt_job) {
            if let Some(fr) = self.capture_pt(gl, f, req.w.max(16), req.h.max(16), *spp) {
                return Ok(fr);
            }
            if req.require_pt {
                let why = self.gpu_off.error.clone().unwrap_or_else(|| "unknown GPU error".into());
                return Err(format!("GPU path tracing failed: {why}. Turn it off in the Path trace window to use the processor."));
            }
        }
        let (w, h) = (req.w.max(16), req.h.max(16));
        let max = gl.get_parameter_i32(glow::MAX_RENDERBUFFER_SIZE).max(2048) as u32;
        if w > max || h > max {
            return Err(format!("The graphics card supports at most {max} px - lower the resolution or turn off supersampling."));
        }
        let prev_fb = gl.get_parameter_framebuffer(glow::FRAMEBUFFER_BINDING);
        let mut vp = [0i32; 4];
        gl.get_parameter_i32_slice(glow::VIEWPORT, &mut vp);
        let scissor = gl.is_enabled(glow::SCISSOR_TEST);
        if self.fbo.is_none_or(|f| f.3 != w || f.4 != h) {
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
        let Some((fb, ..)) = self.fbo else { return Err("Off-screen framebuffer not available.".into()) };
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
            // shadows for the whole model (the capture does not use the view lists)
            self.shadow_state = if req.params.shadows { self.shadow_pass(gl, &req.params, req.lists.as_deref()) } else { None };
            gl.bind_framebuffer(glow::FRAMEBUFFER, Some(fb));
            gl.viewport(0, 0, w as i32, h as i32);
            // captures (renders / video) draw the whole model, at maximum quality (and with SSAO)
            if !(req.params.ao > 0.0 && self.draw_ao(gl, &req.params, req.lists.as_deref())) {
                gl.bind_framebuffer(glow::FRAMEBUFFER, Some(fb));
                gl.viewport(0, 0, w as i32, h as i32);
                self.draw(gl, &req.params, req.lists.as_deref());
            }
            let mut buf = vec![0u8; (w * h * 4) as usize];
            gl.pixel_store_i32(glow::PACK_ALIGNMENT, 1);
            gl.read_pixels(0, 0, w as i32, h as i32, glow::RGBA, glow::UNSIGNED_BYTE, glow::PixelPackData::Slice(&mut buf));
            // vertical flip + compositing over the background (premultiplied colors)
            let mut out = vec![0u8; buf.len()];
            let row = (w * 4) as usize;
            for y in 0..h as usize {
                let src = &buf[(h as usize - 1 - y) * row..(h as usize - y) * row];
                let t = y as f32 / (h - 1).max(1) as f32;
                let bg: [f32; 3] = std::array::from_fn(|i| req.bg_top[i] as f32 * (1.0 - t) + req.bg_bottom[i] as f32 * t);
                let dst = &mut out[y * row..(y + 1) * row];
                for (s, d) in src.as_chunks::<4>().0.iter().zip(dst.as_chunks_mut::<4>().0.iter_mut()) {
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
    // a triangle covering the whole screen
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
uniform vec4 u_k;      // P22, P32, orthographic (1/0), P11
uniform float u_h;     // height in pixels
uniform float u_str;   // strength
uniform float u_edge;  // edges (0 = off)
uniform int u_style;   // 4 = hidden-line (technical drawing)
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
    // cloud points (alpha 0.98, MSAA-blended at edges 0.97..1): no contact shading and no edges
    if (c.a > 0.97 && c.a < 0.9995) { o_col = vec4(c.rgb, 1.0); return; }
    float z0 = lin(d0);
    // radius ~0.45 m in the world, projected on screen (limited to 3..48 px)
    float rw = 0.45;
    float pscale = 0.5 * u_h * u_k.w;
    float rpx = clamp(u_k.z > 0.5 ? rw * pscale : rw * pscale / max(z0, 1e-3), 3.0, 48.0);
    // per-pixel rotation (fine noise, no banding)
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
    // softer shadows: gentler attenuation
    float ao = 1.0 - u_str * 0.5 * occ / float(N);
    // fine edges: depth jumps (outline) and slope changes (creases). On a flat surface
    // 1/z (perspective) or z (orthographic) varies linearly on screen -> second derivative ~0.
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
        // orthographic: relative to the pixel size in the world; perspective: relative to 1/z
        float pix = 2.0 / max(u_k.w * u_h, 1e-3);
        float crease = ortho ? smoothstep(0.3, 1.0, lapAbs / pix) : smoothstep(0.0009, 0.004, lapAbs / max(abs(q0), 1e-6));
        // Hidden line: ONLY the general outline (silhouette / depth jump) - no creases.
        // Hidden line: silhouette AND the edges between faces (technical drawing); the other styles: more subtle edges
        edge = u_style == 4 ? max(smoothstep(0.015, 0.05, jump), crease) : max(smoothstep(0.02, 0.06, jump), crease * 0.8);
    }
    if (u_style == 4) {
        // HIDDEN LINE (technical drawing, like Rhino / Inventor): white surfaces with a very fine gray
        // tint by orientation (volumes read clearly), all edges of the shape in black ink.
        float ink = clamp(edge * 1.6, 0.0, 1.0);
        vec3 paper = vec3(0.985, 0.988, 0.992);
        // brightness of the Concept shading (depends on face orientation), brought into a narrow gray band
        float lum = dot(c.rgb / max(c.a, 1e-3), vec3(0.299, 0.587, 0.114));
        float tone = mix(0.80, 1.0, smoothstep(0.25, 0.85, lum));
        vec3 shade = paper * tone * mix(1.0, 0.86, clamp(1.0 - ao, 0.0, 1.0) * 2.0);
        vec3 col = mix(shade, vec3(0.08, 0.09, 0.11), ink);
        o_col = vec4(col, c.a);
        return;
    }
    o_col = vec4(c.rgb * ao * (1.0 - 0.45 * u_edge * edge), c.a);
}
"#;

impl Renderer {
    /// Scene with contact shading: drawn into an FBO (multisample when the current framebuffer has MSAA),
    /// resolved into color / depth textures, then composited into the current framebuffer with SSAO.
    /// Returns false if it cannot (then it draws normally).
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
        // 1) scene into the FBO
        gl.bind_framebuffer(glow::FRAMEBUFFER, Some(ms.unwrap_or(fbo)));
        gl.viewport(0, 0, w, h);
        gl.disable(glow::SCISSOR_TEST);
        gl.clear_color(0.0, 0.0, 0.0, 0.0);
        gl.clear_depth_f32(1.0);
        gl.depth_mask(true);
        gl.color_mask(true, true, true, true);
        gl.clear(glow::COLOR_BUFFER_BIT | glow::DEPTH_BUFFER_BIT);
        self.draw(gl, p, lists);
        // 2) MSAA -> textures
        if let Some(m) = ms {
            gl.bind_framebuffer(glow::READ_FRAMEBUFFER, Some(m));
            gl.bind_framebuffer(glow::DRAW_FRAMEBUFFER, Some(fbo));
            gl.blit_framebuffer(0, 0, w, h, 0, 0, w, h, glow::COLOR_BUFFER_BIT, glow::NEAREST);
            gl.blit_framebuffer(0, 0, w, h, 0, 0, w, h, glow::DEPTH_BUFFER_BIT, glow::NEAREST);
        }
        // 3) compositing into the initial framebuffer, in the same area
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

    /// Sun shadow map: the scene depth seen from the sun direction (orthographic projection
    /// enclosing the scene sphere). Returns the light matrix, the size of a texel in the world and the total depth;
    /// None if it cannot (then no shadows).
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
        // light: orthographic along the sun direction, encloses the scene sphere
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
        // a small depth offset: no "acne" on lit surfaces
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

    /// SSAO targets for the given size (at most two: window + render), created on demand.
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
        let Some(dep) = tex(glow::DEPTH_COMPONENT24, glow::DEPTH_COMPONENT, glow::UNSIGNED_INT) else {
            gl.delete_texture(col);
            return None;
        };
        gl.bind_texture(glow::TEXTURE_2D, None);
        let Ok(fbo) = gl.create_framebuffer() else {
            gl.delete_texture(col);
            gl.delete_texture(dep);
            return None;
        };
        gl.bind_framebuffer(glow::FRAMEBUFFER, Some(fbo));
        gl.framebuffer_texture_2d(glow::FRAMEBUFFER, glow::COLOR_ATTACHMENT0, glow::TEXTURE_2D, Some(col), 0);
        gl.framebuffer_texture_2d(glow::FRAMEBUFFER, glow::DEPTH_ATTACHMENT, glow::TEXTURE_2D, Some(dep), 0);
        let mut ok = gl.check_framebuffer_status(glow::FRAMEBUFFER) == glow::FRAMEBUFFER_COMPLETE;
        let mut ms = None;
        if ok && samples > 1 {
            let Ok(f) = gl.create_framebuffer() else { free_ao(gl, col, dep, fbo, None); return None };
            let Ok(c) = gl.create_renderbuffer() else {
                gl.delete_framebuffer(f);
                free_ao(gl, col, dep, fbo, None);
                return None;
            };
            let Ok(d) = gl.create_renderbuffer() else {
                gl.delete_framebuffer(f);
                gl.delete_renderbuffer(c);
                free_ao(gl, col, dep, fbo, None);
                return None;
            };
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
            free_ao(gl, col, dep, fbo, ms);
            return None;
        }
        self.ao_targets.push(AoTargets { w, h, samples, ms, fbo, col, dep });
        Some(self.ao_targets.len() - 1)
    }
}

/// Frees a half-built SSAO target (a failed allocation used to leak its textures / FBOs).
unsafe fn free_ao(gl: &glow::Context, col: glow::Texture, dep: glow::Texture, fbo: glow::Framebuffer, ms: Option<(glow::Framebuffer, glow::Renderbuffer, glow::Renderbuffer)>) {
    if let Some((f, c, d)) = ms {
        gl.delete_framebuffer(f);
        gl.delete_renderbuffer(c);
        gl.delete_renderbuffer(d);
    }
    gl.delete_framebuffer(fbo);
    gl.delete_texture(col);
    gl.delete_texture(dep);
}

unsafe fn delete_points(gl: &glow::Context, p: GpuPoints) {
    gl.delete_vertex_array(p.vao);
    gl.delete_buffer(p.vbo_pos);
    gl.delete_buffer(p.vbo_col);
}

pub(crate) unsafe fn compile_program(gl: &glow::Context, header: &str, vs: &str, fs: &str) -> Result<glow::Program, String> {
    let prog = gl.create_program()?;
    let cleanup = |gl: &glow::Context, shaders: &[glow::Shader]| {
        for s in shaders {
            gl.detach_shader(prog, *s);
            gl.delete_shader(*s);
        }
        gl.delete_program(prog);
    };
    let mut shaders = Vec::new();
    for (ty, src) in [(glow::VERTEX_SHADER, vs), (glow::FRAGMENT_SHADER, fs)] {
        let s = match gl.create_shader(ty) {
            Ok(s) => s,
            Err(e) => {
                cleanup(gl, &shaders);
                return Err(e);
            }
        };
        gl.shader_source(s, &format!("{header}{src}"));
        gl.compile_shader(s);
        if !gl.get_shader_compile_status(s) {
            let info = gl.get_shader_info_log(s);
            gl.delete_shader(s);
            cleanup(gl, &shaders);
            return Err(info);
        }
        gl.attach_shader(prog, s);
        shaders.push(s);
    }
    gl.link_program(prog);
    if !gl.get_program_link_status(prog) {
        let info = gl.get_program_info_log(prog);
        cleanup(gl, &shaders);
        return Err(info);
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

/// Vertex data (positions + normals) for a model.
pub fn mesh_data(m: &Model) -> (Vec<f32>, Vec<i8>) {
    let mut pos = Vec::with_capacity(m.pos.len() * 3);
    let mut nrm = Vec::with_capacity(m.pos.len() * 4);
    for t in m.pos.as_chunks::<3>().0 {
        let n = (t[1] - t[0]).cross(t[2] - t[0]).normalize_or_zero();
        let q = [(n.x * 127.0) as i8, (n.y * 127.0) as i8, (n.z * 127.0) as i8, 0];
        for p in t {
            pos.extend_from_slice(&[p.x, p.y, p.z]);
            nrm.extend_from_slice(&q);
        }
    }
    (pos, nrm)
}

/// Draws the ranges (start vertex, vertex count), limited to the buffer size; None = the whole buffer.
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

/// Model display style (as in Navisworks / Revit).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, serde::Serialize, serde::Deserialize)]
pub enum VisualStyle {
    /// materials, textures, reflections, contact shadows
    #[default]
    Realistic,
    /// the colors from the file, lighting, no textures
    Shaded,
    /// physical model look: almost white, matte, accentuated edges
    Concept,
    /// only the triangle edges
    Wireframe,
    /// hidden-line: neutral surfaces, edges hidden by geometry are not shown (as in AutoCAD / Navisworks)
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
