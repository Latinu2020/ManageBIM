//! glTF 2.0 loader (.gltf / .glb) - geometry + material colors only.
use crate::model::{Prop, RawElement, RawModel};
use base64::Engine;
use glam::{DMat4, DVec3};
use std::path::Path;

pub fn load_gltf(path: &str, progress: &dyn Fn(f32, &str)) -> Result<RawModel, String> {
    progress(0.05, "Reading file");
    let bytes = std::fs::read(path).map_err(|e| format!("Cannot read the file: {e}"))?;
    load_gltf_bytes(&bytes, path, progress, None)
}

/// Like load_gltf, from memory (the GLB from .b3dm / .i3dm); path gives the folder of external resources.
/// atch: the 3D Tiles 1.0 batch table (the rows by _BATCHID), if there is one.
pub fn load_gltf_bytes(bytes: &[u8], path: &str, progress: &dyn Fn(f32, &str), batch: Option<Vec<Vec<(String, String)>>>) -> Result<RawModel, String> {
    // the gltf crate can panic while validating a damaged file (an index into an empty list): a clear error instead
    let g = std::panic::catch_unwind(|| gltf::Gltf::from_slice(bytes))
        .map_err(|_| "The glTF file is damaged (its structure cannot be read).".to_string())?
        .map_err(|e| format!("Invalid glTF: {e}"))?;
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
                    let decoded = percent_decode(u);
                    std::fs::read(base.join(&decoded)).map_err(|e| format!("Cannot read the external buffer '{decoded}': {e}"))?
                }
            }
        };
        buffers.push(data);
    }
    progress(0.3, "Generating geometry");
    // EXT_structural_metadata / EXT_mesh_features (Cesium, 3D Tiles 1.1): one object = one row in the table
    let raw_json = crate::gltf_meta::json_of(bytes);
    let mut meta = raw_json.as_ref().map(|j| crate::gltf_meta::read_meta(j, &buffers)).unwrap_or_default();
    // 3D Tiles 1.0: the batch table becomes the last table; _BATCHID references it
    let batch_table = batch.map(|rows| {
        meta.tables.push(("Batch table".to_string(), rows));
        meta.tables.len() - 1
    });
    let mut elements = Vec::new();
    let mut warnings = Vec::new();
    // Y-up (glTF) -> Z-up (BIM)
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
        // the object (table, id) of each triangle, when the mesh has EXT_mesh_features
        let mut feats: Vec<Option<(usize, u32)>> = Vec::new();
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
            let fid = raw_json.as_ref().and_then(|j| crate::gltf_meta::feature_ids(j, &buffers, mesh.index(), prim.index(), pos.len()).or_else(|| crate::gltf_meta::batch_ids(j, &buffers, mesh.index(), prim.index(), batch_table)));
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
                feats.push(fid.as_ref().and_then(|f| {
                    let id = *f.per_vertex.get(t[0] as usize)?;
                    (Some(id) != f.null).then_some((f.table.unwrap_or(usize::MAX), id))
                }));
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
        // the user properties: Blender (Custom Properties), Rhino, or a GLB exported from ManageBIM
        let mut ifc_type = "glTF Mesh".to_string();
        let mut guid = format!("node-{}", node.index());
        let mut storey = group;
        for (k, v) in extras_props(node.extras()).into_iter().chain(extras_props(mesh.extras())) {
            match k.as_str() {
                "GlobalId" if !v.is_empty() => guid = v.clone(),
                "IfcType" if !v.is_empty() => ifc_type = v.clone(),
                "Level" if !v.is_empty() => storey = v.clone(),
                _ => {}
            }
            let (g, n) = k.split_once('|').map(|(g, n)| (g.to_string(), n.to_string())).unwrap_or(("Properties".to_string(), k.clone()));
            props.push(Prop { group: g, name: n, value: v });
        }
        if ifc_type != "glTF Mesh" {
            if let Some(p) = props.iter_mut().find(|p| p.group == "Element" && p.name == "Type") {
                p.value = ifc_type.clone();
            }
        }
        if feats.iter().any(|f| f.is_some()) {
            // one element per object (feature), with its row from the property table
            let mut groups: std::collections::BTreeMap<Option<(usize, u32)>, (Vec<[DVec3; 3]>, Vec<[u8; 4]>)> = std::collections::BTreeMap::new();
            for ((t, c), f) in tris.into_iter().zip(colors).zip(feats) {
                let g = groups.entry(f).or_default();
                g.0.push(t);
                g.1.push(c);
            }
            for (f, (tris, colors)) in groups {
                let mut p = props.clone();
                let mut ename = name.clone();
                let mut eguid = guid.clone();
                if let Some((tbl, id)) = f {
                    p.push(Prop { group: "Feature".into(), name: "Feature ID".into(), value: id.to_string() });
                    eguid = format!("{guid}#{id}");
                    if let Some((title, rows)) = meta.tables.get(tbl) {
                        if let Some(row) = rows.get(id as usize) {
                            for (k, v) in row {
                                p.push(Prop { group: title.clone(), name: k.clone(), value: v.clone() });
                            }
                            // the object name: the first name / id property in the row
                            if let Some((_, v)) = row.iter().find(|(k, _)| ["name", "Name", "NAME", "id", "ID", "gml_id", "objectid", "OBJECTID"].contains(&k.as_str())) {
                                ename = format!("{name} · {v}");
                            } else {
                                ename = format!("{name} · {id}");
                            }
                        }
                    } else {
                        ename = format!("{name} · {id}");
                    }
                }
                if let Some(x) = p.iter_mut().find(|x| x.group == "Element" && x.name == "Triangles") {
                    x.value = tris.len().to_string();
                }
                elements.push(RawElement { name: ename, ifc_type: ifc_type.clone(), guid: eguid, storey: storey.clone(), props: p, tris, colors, default_hidden: false });
            }
            continue;
        }
        elements.push(RawElement { name, ifc_type, guid, storey, props, tris, colors, default_hidden: false });
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

/// `extras` (JSON object) -> (key, text value) pairs. Nested objects become "group|key".
fn extras_props(x: &Option<Box<serde_json::value::RawValue>>) -> Vec<(String, String)> {
    let Some(raw) = x else { return Vec::new() };
    let Ok(serde_json::Value::Object(map)) = serde_json::from_str::<serde_json::Value>(raw.get()) else { return Vec::new() };
    let text = |v: &serde_json::Value| match v {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Null => String::new(),
        other => other.to_string(),
    };
    let mut out = Vec::new();
    for (k, v) in &map {
        match v {
            serde_json::Value::Object(inner) => {
                for (k2, v2) in inner {
                    out.push((format!("{k}|{k2}"), text(v2)));
                }
            }
            _ => out.push((k.clone(), text(v))),
        }
    }
    out
}

/// "My%20Folder/%C8%99.bin" -> "My Folder/ș.bin" (glTF URIs are percent-encoded, UTF-8)
pub fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    let hex = |c: u8| (c as char).to_digit(16).map(|d| d as u8);
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            if let (Some(h), Some(l)) = (hex(b[i + 1]), hex(b[i + 2])) {
                out.push(h * 16 + l);
                i += 3;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}
