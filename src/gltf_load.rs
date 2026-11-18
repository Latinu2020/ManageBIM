//! Incarcator glTF 2.0 (.gltf / .glb) - doar geometrie + culori de material.
use crate::model::{Prop, RawElement, RawModel};
use base64::Engine;
use glam::{DMat4, DVec3};
use std::path::Path;

pub fn load_gltf(path: &str, progress: &dyn Fn(f32, &str)) -> Result<RawModel, String> {
    progress(0.05, "Reading file");
    let bytes = std::fs::read(path).map_err(|e| format!("Cannot read the file: {e}"))?;
    let g = gltf::Gltf::from_slice(&bytes).map_err(|e| format!("Invalid glTF: {e}"))?;
    let base = Path::new(path).parent().map(|p| p.to_path_buf()).unwrap_or_default();
    let mut buffers: Vec<Vec<u8>> = Vec::new();
    for b in g.buffers() {
        let data = match b.source() {
            gltf::buffer::Source::Bin => g.blob.clone().ok_or("GLB without a binary chunk")?,
            gltf::buffer::Source::Uri(u) => {
                if let Some(rest) = u.strip_prefix("data:") {
                    let b64 = rest.split_once(";base64,").map(|x| x.1).ok_or("Unsupported data: URI")?;
                    base64::engine::general_purpose::STANDARD.decode(b64).map_err(|e| format!("Invalid base64: {e}"))?
                } else {
                    let decoded = u.replace("%20", " ");
                    std::fs::read(base.join(&decoded)).map_err(|e| format!("Cannot read the external buffer '{decoded}': {e}"))?
                }
            }
        };
        buffers.push(data);
    }
    progress(0.3, "Generating geometry");
    let mut elements = Vec::new();
    let mut warnings = Vec::new();
    // Y-sus (glTF) -> Z-sus (BIM)
    let yup = DMat4::from_cols_array(&[1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, -1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0]);
    let scenes: Vec<gltf::Scene> = match g.default_scene() {
        Some(s) => vec![s],
        None => g.scenes().collect(),
    };
    let mut stack: Vec<(gltf::Node, DMat4, String)> = Vec::new();
    for s in &scenes {
        for n in s.nodes() {
            stack.push((n, yup, String::new()));
        }
    }
    while let Some((node, parent, group)) = stack.pop() {
        let m = parent * DMat4::from_cols_array_2d(&node.transform().matrix().map(|c| c.map(|v| v as f64)));
        let nname = node.name().unwrap_or("").to_string();
        let child_group = if group.is_empty() { nname.clone() } else { group.clone() };
        for ch in node.children() {
            stack.push((ch, m, child_group.clone()));
        }
        let Some(mesh) = node.mesh() else { continue };
        let mut tris = Vec::new();
        let mut colors = Vec::new();
        let mut mats: Vec<String> = Vec::new();
        for prim in mesh.primitives() {
            if prim.mode() != gltf::mesh::Mode::Triangles {
                continue;
            }
            if prim.get(&gltf::Semantic::Positions).is_none() {
                warnings.push("Primitives without positions (possibly Draco-compressed) were ignored.".to_string());
                continue;
            }
            let reader = prim.reader(|b| buffers.get(b.index()).map(|v| v.as_slice()));
            let Some(pos) = reader.read_positions() else { continue };
            let pos: Vec<DVec3> = pos.map(|p| m.transform_point3(DVec3::new(p[0] as f64, p[1] as f64, p[2] as f64))).collect();
            let idx: Vec<u32> = match reader.read_indices() {
                Some(i) => i.into_u32().collect(),
                None => (0..pos.len() as u32).collect(),
            };
            let mat = prim.material();
            let c = mat.pbr_metallic_roughness().base_color_factor();
            let col = [(c[0].powf(1.0 / 2.2) * 255.0) as u8, (c[1].powf(1.0 / 2.2) * 255.0) as u8, (c[2].powf(1.0 / 2.2) * 255.0) as u8, ((c[3].max(0.15)) * 255.0) as u8];
            if let Some(n) = mat.name() {
                if !mats.iter().any(|x| x == n) {
                    mats.push(n.to_string());
                }
            }
            for t in idx.chunks_exact(3) {
                let (Some(a), Some(b), Some(cc)) = (pos.get(t[0] as usize), pos.get(t[1] as usize), pos.get(t[2] as usize)) else { continue };
                if (*b - *a).cross(*cc - *a).length_squared() < 1e-18 {
                    continue;
                }
                tris.push([*a, *b, *cc]);
                colors.push(col);
            }
        }
        if tris.is_empty() {
            continue;
        }
        let name = if !nname.is_empty() { nname } else { mesh.name().map(|s| s.to_string()).unwrap_or_else(|| format!("Mesh {}", mesh.index())) };
        let mut props = vec![
            Prop { group: "Element".into(), name: "Name".into(), value: name.clone() },
            Prop { group: "Element".into(), name: "Type".into(), value: "glTF Mesh".into() },
            Prop { group: "Element".into(), name: "Triangles".into(), value: tris.len().to_string() },
        ];
        if !mats.is_empty() {
            props.push(Prop { group: "Material".into(), name: "Material".into(), value: mats.join(", ") });
        }
        elements.push(RawElement { name, ifc_type: "glTF Mesh".into(), guid: format!("node-{}", node.index()), storey: group, props, tris, colors, default_hidden: false });
    }
    if elements.is_empty() {
        return Err("The glTF file contains no triangle geometry.".into());
    }
    warnings.sort();
    warnings.dedup();
    let mut info = vec![("Format".to_string(), "glTF 2.0".to_string())];
    if let Some(gen) = g.document.as_json().asset.generator.as_ref() {
        info.push(("Generator".into(), gen.to_string()));
    }
    Ok(RawModel { elements, info, warnings })
}
