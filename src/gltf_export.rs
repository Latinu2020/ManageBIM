//! Binary glTF 2.0 export (.glb): the geometry, the colors / materials (PBR) and the BIM properties.
//! Opens directly in Blender, Rhino 8, Twinmotion, Unreal, Enscape, 3ds Max, Windows 3D Viewer.
//! Hierarchy: Model › Level › Element; the properties in `extras` (Blender: Custom Properties),
//! with the keys "Group|Name" - the ManageBIM glTF import reads them back too.

use glam::Vec3;
use serde_json::{json, Map, Value};
use std::collections::HashMap;

/// An element to export.
pub struct Item<'a> {
    pub model: &'a str,
    pub level: &'a str,
    pub name: &'a str,
    pub guid: &'a str,
    pub ifc_type: &'a str,
    /// (group, name, value)
    pub props: Vec<(&'a str, &'a str, &'a str)>,
    /// 3 vertices per triangle, local coordinates Z-up (meters)
    pub tris: &'a [Vec3],
    /// the RGBA color per triangle (sRGB)
    pub colors: Vec<[u8; 4]>,
    /// gloss and metal per triangle (0..255), as in the ManageBIM materials
    pub gloss_metal: Vec<[u8; 2]>,
}

#[derive(Default, Debug)]
pub struct Stats {
    pub elements: usize,
    pub triangles: usize,
    pub materials: usize,
}

fn srgb_to_linear(c: u8) -> f32 {
    let x = c as f32 / 255.0;
    if x <= 0.04045 { x / 12.92 } else { ((x + 0.055) / 1.055).powf(2.4) }
}

/// Z-up (BIM) -> Y-up (glTF)
fn yup(p: Vec3) -> [f32; 3] {
    [p.x, p.z, -p.y]
}

pub fn write_glb(items: &[Item], generator: &str) -> (Vec<u8>, Stats) {
    let mut st = Stats::default();
    let mut pos: Vec<u8> = Vec::new();
    let mut nrm: Vec<u8> = Vec::new();
    let mut accessors: Vec<Value> = Vec::new();
    let mut meshes: Vec<Value> = Vec::new();
    let mut materials: Vec<Value> = Vec::new();
    let mut mat_ix: HashMap<([u8; 4], [u8; 2]), usize> = HashMap::new();
    let mut nodes: Vec<Value> = Vec::new();
    // model -> level -> the children (node indices)
    let mut tree: Vec<(String, Vec<(String, Vec<usize>)>)> = Vec::new();

    for it in items {
        // the triangles grouped by material (one primitive per material)
        let mut groups: Vec<(([u8; 4], [u8; 2]), Vec<usize>)> = Vec::new();
        for t in 0..it.tris.len() / 3 {
            let (a, b, c) = (it.tris[t * 3], it.tris[t * 3 + 1], it.tris[t * 3 + 2]);
            if (b - a).cross(c - a).length_squared() < 1e-14 || !a.is_finite() || !b.is_finite() || !c.is_finite() {
                continue;
            }
            let key = (it.colors.get(t).copied().unwrap_or([200, 200, 200, 255]), it.gloss_metal.get(t).copied().unwrap_or([0, 0]));
            match groups.iter_mut().find(|g| g.0 == key) {
                Some(g) => g.1.push(t),
                None => groups.push((key, vec![t])),
            }
        }
        if groups.is_empty() {
            continue;
        }
        let mut prims = Vec::new();
        for (key, tris) in &groups {
            let mi = *mat_ix.entry(*key).or_insert_with(|| {
                let ([r, g, b, a], [gl, me]) = *key;
                let mut m = json!({
                    "name": format!("RGB {r} {g} {b}"),
                    "pbrMetallicRoughness": {
                        "baseColorFactor": [srgb_to_linear(r), srgb_to_linear(g), srgb_to_linear(b), a as f32 / 255.0],
                        "metallicFactor": me as f32 / 255.0,
                        "roughnessFactor": (1.0 - gl as f32 / 255.0).clamp(0.05, 1.0),
                    },
                    // BIM models often have inconsistently oriented faces
                    "doubleSided": true,
                });
                if a < 250 {
                    m["alphaMode"] = json!("BLEND");
                }
                materials.push(m);
                materials.len() - 1
            });
            let (mut mn, mut mx) = ([f32::MAX; 3], [f32::MIN; 3]);
            let p0 = pos.len();
            for &t in tris {
                let (a, b, c) = (it.tris[t * 3], it.tris[t * 3 + 1], it.tris[t * 3 + 2]);
                let n = yup((b - a).cross(c - a).normalize_or_zero());
                for v in [a, b, c] {
                    let q = yup(v);
                    for k in 0..3 {
                        mn[k] = mn[k].min(q[k]);
                        mx[k] = mx[k].max(q[k]);
                        pos.extend_from_slice(&q[k].to_le_bytes());
                        nrm.extend_from_slice(&n[k].to_le_bytes());
                    }
                }
            }
            let count = tris.len() * 3;
            st.triangles += tris.len();
            accessors.push(json!({ "bufferView": 0, "byteOffset": p0, "componentType": 5126, "count": count, "type": "VEC3", "min": mn, "max": mx }));
            accessors.push(json!({ "bufferView": 1, "byteOffset": p0, "componentType": 5126, "count": count, "type": "VEC3" }));
            let ai = accessors.len() - 2;
            prims.push(json!({ "attributes": { "POSITION": ai, "NORMAL": ai + 1 }, "material": mi, "mode": 4 }));
        }
        meshes.push(json!({ "name": it.name, "primitives": prims }));
        // the properties: Blender shows them as Custom Properties; the ManageBIM import reads them back
        let mut extras = Map::new();
        extras.insert("GlobalId".into(), json!(it.guid));
        extras.insert("IfcType".into(), json!(it.ifc_type));
        extras.insert("Level".into(), json!(it.level));
        extras.insert("Model".into(), json!(it.model));
        for (g, n, v) in &it.props {
            if v.is_empty() {
                continue;
            }
            let key = if g.is_empty() { n.to_string() } else { format!("{g}|{n}") };
            extras.entry(key).or_insert_with(|| json!(v));
        }
        nodes.push(json!({ "name": it.name, "mesh": meshes.len() - 1, "extras": Value::Object(extras) }));
        let ni = nodes.len() - 1;
        st.elements += 1;
        let mpos = match tree.iter().position(|(m, _)| m == it.model) {
            Some(i) => i,
            None => {
                tree.push((it.model.to_string(), Vec::new()));
                tree.len() - 1
            }
        };
        let lvl = if it.level.is_empty() { "(no level)" } else { it.level };
        let levels = &mut tree[mpos].1;
        match levels.iter_mut().find(|(l, _)| l == lvl) {
            Some(l) => l.1.push(ni),
            None => levels.push((lvl.to_string(), vec![ni])),
        }
    }
    // the group nodes: model and level
    let mut roots = Vec::new();
    for (model, levels) in tree {
        let mut kids = Vec::new();
        for (lvl, ch) in levels {
            nodes.push(json!({ "name": lvl, "children": ch }));
            kids.push(nodes.len() - 1);
        }
        nodes.push(json!({ "name": model, "children": kids }));
        roots.push(nodes.len() - 1);
    }
    st.materials = materials.len();
    let bin_len = pos.len() + nrm.len();
    let mut gltf = json!({
        "asset": { "version": "2.0", "generator": generator },
        "scene": 0,
        "scenes": [{ "name": "ManageBIM", "nodes": roots }],
        "nodes": nodes,
        "meshes": meshes,
        "materials": materials,
        "accessors": accessors,
        "bufferViews": [
            { "buffer": 0, "byteOffset": 0, "byteLength": pos.len(), "target": 34962 },
            { "buffer": 0, "byteOffset": pos.len(), "byteLength": nrm.len(), "target": 34962 },
        ],
        "buffers": [{ "byteLength": bin_len }],
    });
    if meshes_empty(&gltf) {
        // glTF without geometry: no zero-length bufferViews (invalid)
        gltf.as_object_mut().map(|o| {
            o.remove("bufferViews");
            o.remove("buffers");
            o.remove("accessors");
        });
    }
    let mut js = serde_json::to_vec(&gltf).unwrap_or_default();
    while !js.len().is_multiple_of(4) {
        js.push(b' ');
    }
    let mut bin = pos;
    bin.extend_from_slice(&nrm);
    while !bin.len().is_multiple_of(4) {
        bin.push(0);
    }
    let has_bin = !bin.is_empty();
    let total = 12 + 8 + js.len() + if has_bin { 8 + bin.len() } else { 0 };
    let mut out = Vec::with_capacity(total);
    out.extend_from_slice(b"glTF");
    out.extend_from_slice(&2u32.to_le_bytes());
    out.extend_from_slice(&(total as u32).to_le_bytes());
    out.extend_from_slice(&(js.len() as u32).to_le_bytes());
    out.extend_from_slice(b"JSON");
    out.extend_from_slice(&js);
    if has_bin {
        out.extend_from_slice(&(bin.len() as u32).to_le_bytes());
        out.extend_from_slice(b"BIN\0");
        out.extend_from_slice(&bin);
    }
    (out, st)
}

fn meshes_empty(g: &Value) -> bool {
    g["meshes"].as_array().is_none_or(|m| m.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// GLB -> the ManageBIM glTF import: the same geometry (Z-up), colors, names, GUID, type, level and properties
    #[test]
    fn glb_roundtrip_keeps_geometry_colours_and_properties() {
        let tris = vec![Vec3::new(0., 0., 0.), Vec3::new(2., 0., 0.), Vec3::new(0., 3., 0.), Vec3::new(0., 0., 0.), Vec3::new(0., 0., 4.), Vec3::new(2., 0., 0.)];
        let items = vec![Item {
            model: "Arch",
            level: "Level 1",
            name: "Wall A",
            guid: "2O2Fr$t4X7Zf8NOew3FLOH",
            ifc_type: "IfcWall",
            props: vec![("Pset_WallCommon", "FireRating", "EI30"), ("Element", "Name", "Wall A")],
            tris: &tris,
            colors: vec![[200, 30, 30, 255], [30, 30, 200, 128]],
            gloss_metal: vec![[0, 0], [200, 255]],
        }];
        let (glb, st) = write_glb(&items, "ManageBIM test");
        assert_eq!((st.elements, st.triangles, st.materials), (1, 2, 2));
        assert_eq!(&glb[0..4], b"glTF");
        assert_eq!(u32::from_le_bytes([glb[8], glb[9], glb[10], glb[11]]) as usize, glb.len());
        let path = std::env::temp_dir().join("mb_glb_roundtrip.glb");
        std::fs::write(&path, &glb).unwrap();
        let m = crate::gltf_load::load_gltf(&path.to_string_lossy(), &|_, _| {}).expect("load");
        let _ = std::fs::remove_file(&path);
        assert_eq!(m.elements.len(), 1);
        let e = &m.elements[0];
        assert_eq!(e.name, "Wall A");
        assert_eq!(e.guid, "2O2Fr$t4X7Zf8NOew3FLOH");
        assert_eq!(e.ifc_type, "IfcWall");
        assert_eq!(e.storey, "Level 1");
        assert!(e.props.iter().any(|p| p.group == "Pset_WallCommon" && p.name == "FireRating" && p.value == "EI30"));
        assert_eq!(e.tris.len(), 2);
        // the geometry comes back in Z-up, at the same coordinates
        let all: Vec<glam::DVec3> = e.tris.iter().flatten().copied().collect();
        assert!(all.iter().any(|p| (p.z - 4.0).abs() < 1e-5), "Z-up preserved");
        assert!(all.iter().any(|p| (p.y - 3.0).abs() < 1e-5));
        // the colors (sRGB) come back almost identical
        let reds = e.colors.iter().filter(|c| c[0] > 190 && c[1] < 45).count();
        assert_eq!(reds, 1, "{:?}", e.colors);
    }

    #[test]
    fn percent_decoding() {
        assert_eq!(crate::gltf_load::percent_decode("My%20Folder/%C8%99.bin"), "My Folder/ș.bin");
        assert_eq!(crate::gltf_load::percent_decode("100%"), "100%");
        assert_eq!(crate::gltf_load::percent_decode("a%zzb"), "a%zzb");
    }
}
