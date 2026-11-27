//! "Web viewer" export: a single .html file containing the geometry, colours and properties
//! of the model and its own WebGL 2 viewer (orbit / pan / zoom, selection, properties, tree, search,
//! hide / isolate, X-ray). It opens in any modern browser, with no internet and no installation.

use crate::model::{Element, Model};
use base64::Engine;

const TEMPLATE: &str = include_str!("web_viewer.html");

pub struct WebStats {
    pub elements: usize,
    pub triangles: usize,
    pub bytes: usize,
}

/// `items`: the elements to export (model, element); `origin` is not needed - the positions are already local.
pub fn html(items: &[(&Model, &Element)], title: &str, date: &str) -> (String, WebStats) {
    // the models, in order of appearance
    let mut models: Vec<*const Model> = Vec::new();
    let mut model_names: Vec<String> = Vec::new();
    let mut tris = 0usize;
    for (m, e) in items {
        if !models.contains(&(*m as *const Model)) {
            models.push(*m as *const Model);
            model_names.push(crate::app::model_label(&m.name));
        }
        tris += e.tri_count as usize;
    }
    let vc = tris * 3;
    let mut pos: Vec<u8> = Vec::with_capacity(vc * 12);
    let mut col: Vec<u8> = Vec::with_capacity(vc * 4);
    let mut ids: Vec<u8> = Vec::with_capacity(vc * 4);
    let mut elems: Vec<serde_json::Value> = Vec::with_capacity(items.len());
    for (k, (m, e)) in items.iter().enumerate() {
        let mi = models.iter().position(|p| std::ptr::eq(*p, *m)).unwrap_or(0);
        for t in e.tri_start..e.tri_start + e.tri_count {
            let c = m.tri_color.get(t as usize).copied().unwrap_or([200, 200, 200, 255]);
            for v in 0..3 {
                let p = m.pos.get(t as usize * 3 + v).copied().unwrap_or_default();
                for x in [p.x, p.y, p.z] {
                    pos.extend_from_slice(&x.to_le_bytes());
                }
                col.extend_from_slice(&c);
                ids.extend_from_slice(&(k as u32).to_le_bytes());
            }
        }
        let props: Vec<[&str; 3]> = e.props.iter().map(|p| [p.group.as_str(), p.name.as_str(), p.value.as_str()]).collect();
        elems.push(serde_json::json!([e.name, e.ifc_type, e.guid, e.storey, mi, props]));
    }
    let meta = serde_json::json!({ "title": title, "date": date, "models": model_names, "elems": elems }).to_string();
    let mut blob: Vec<u8> = Vec::with_capacity(12 + vc * 20 + meta.len());
    blob.extend_from_slice(&(vc as u32).to_le_bytes());
    blob.extend_from_slice(&(items.len() as u32).to_le_bytes());
    blob.extend_from_slice(&pos);
    blob.extend_from_slice(&col);
    blob.extend_from_slice(&ids);
    blob.extend_from_slice(&(meta.len() as u32).to_le_bytes());
    blob.extend_from_slice(meta.as_bytes());
    drop((pos, col, ids));
    let z = miniz_oxide::deflate::compress_to_vec_zlib(&blob, 6);
    let b64 = base64::engine::general_purpose::STANDARD.encode(&z);
    let t = esc(title);
    let out = TEMPLATE.replace("__TITLE__", &t).replacen("__DATA__", &b64, 1);
    let bytes = out.len();
    (out, WebStats { elements: items.len(), triangles: tris, bytes })
}

/// the compressed data from the exported file (for tests): (number of vertices, number of elements, JSON metadata)
pub fn read_back(html: &str) -> Option<(u32, u32, serde_json::Value)> {
    let start = html.find("const DATA=\"")? + 12;
    let end = start + html[start..].find('"')?;
    let z = base64::engine::general_purpose::STANDARD.decode(&html[start..end]).ok()?;
    let b = miniz_oxide::inflate::decompress_to_vec_zlib(&z).ok()?;
    let u = |o: usize| u32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]]);
    let vc = u(0);
    let ec = u(4);
    let off = 8 + vc as usize * 20;
    let jl = u(off) as usize;
    let meta = serde_json::from_slice(&b[off + 4..off + 4 + jl]).ok()?;
    Some((vc, ec, meta))
}

fn esc(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}
