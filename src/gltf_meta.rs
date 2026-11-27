//! glTF: EXT_structural_metadata (property tables) + EXT_mesh_features (object ID per vertex).
//! Used by Cesium / 3D Tiles 1.1, CityGML -> glTF exports, iTwin etc.: a single mesh contains several
//! objects (buildings, walls, windows), each with its own row of properties. We read the raw JSON
//! (the `gltf` crate does not expose the extensions) and the binary data from the bufferViews.
use serde_json::Value;

/// The property tables: (class name, rows [(property, value)]).
#[derive(Default, Debug)]
pub struct Meta {
    pub tables: Vec<(String, Vec<Vec<(String, String)>>)>,
}

/// The object IDs of a primitive: the per-vertex value, the associated table, the "no object" value.
pub struct FeatureIds {
    pub per_vertex: Vec<u32>,
    pub table: Option<usize>,
    pub null: Option<u32>,
}

/// The JSON of a .gltf or of the first chunk of a .glb.
pub fn json_of(bytes: &[u8]) -> Option<Value> {
    if bytes.len() >= 20 && &bytes[..4] == b"glTF" {
        let len = u32::from_le_bytes(bytes[12..16].try_into().ok()?) as usize;
        if &bytes[16..20] != b"JSON" || 20 + len > bytes.len() {
            return None;
        }
        return serde_json::from_slice(&bytes[20..20 + len]).ok();
    }
    serde_json::from_slice(bytes).ok()
}

fn view<'a>(json: &Value, buffers: &'a [Vec<u8>], idx: usize) -> Option<(&'a [u8], usize)> {
    let bv = json.get("bufferViews")?.get(idx)?;
    let buf = buffers.get(bv.get("buffer")?.as_u64()? as usize)?;
    let off = bv.get("byteOffset").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
    let len = bv.get("byteLength")?.as_u64()? as usize;
    let stride = bv.get("byteStride").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
    Some((buf.get(off..off + len)?, stride))
}

fn comp_size(t: &str) -> usize {
    match t {
        "INT8" | "UINT8" => 1,
        "INT16" | "UINT16" => 2,
        "INT32" | "UINT32" | "FLOAT32" => 4,
        _ => 8,
    }
}

fn read_num(t: &str, b: &[u8]) -> f64 {
    // a value cut short by a damaged file counts as 0 instead of stopping the app
    let mut pad = [0u8; 8];
    let n = b.len().min(8);
    pad[..n].copy_from_slice(&b[..n]);
    let b = &pad[..];
    match t {
        "INT8" => b[0] as i8 as f64,
        "UINT8" => b[0] as f64,
        "INT16" => i16::from_le_bytes([b[0], b[1]]) as f64,
        "UINT16" => u16::from_le_bytes([b[0], b[1]]) as f64,
        "INT32" => i32::from_le_bytes(b[..4].try_into().unwrap()) as f64,
        "UINT32" => u32::from_le_bytes(b[..4].try_into().unwrap()) as f64,
        "FLOAT32" => f32::from_le_bytes(b[..4].try_into().unwrap()) as f64,
        "INT64" => i64::from_le_bytes(b[..8].try_into().unwrap()) as f64,
        "UINT64" => u64::from_le_bytes(b[..8].try_into().unwrap()) as f64,
        _ => f64::from_le_bytes(b[..8].try_into().unwrap()),
    }
}

fn fmt_num(v: f64) -> String {
    if v.fract() == 0.0 && v.abs() < 1e15 {
        format!("{}", v as i64)
    } else {
        let s = format!("{v:.6}");
        s.trim_end_matches('0').trim_end_matches('.').to_string()
    }
}

fn components(ty: &str) -> usize {
    match ty {
        "VEC2" => 2,
        "VEC3" => 3,
        "VEC4" | "MAT2" => 4,
        "MAT3" => 9,
        "MAT4" => 16,
        _ => 1,
    }
}

/// Reads all property tables (the rows as text).
pub fn read_meta(json: &Value, buffers: &[Vec<u8>]) -> Meta {
    let mut meta = Meta::default();
    let Some(ext) = json.get("extensions").and_then(|e| e.get("EXT_structural_metadata")) else { return meta };
    let schema = ext.get("schema");
    let classes = schema.and_then(|s| s.get("classes"));
    let enums = schema.and_then(|s| s.get("enums"));
    let Some(tables) = ext.get("propertyTables").and_then(|t| t.as_array()) else { return meta };
    for t in tables {
        let class = t.get("class").and_then(|c| c.as_str()).unwrap_or("");
        let count = t.get("count").and_then(|c| c.as_u64()).unwrap_or(0).min(5_000_000) as usize;
        let cdef = classes.and_then(|c| c.get(class));
        let title = cdef.and_then(|c| c.get("name")).and_then(|n| n.as_str()).unwrap_or(class).to_string();
        let mut rows: Vec<Vec<(String, String)>> = vec![Vec::new(); count];
        if let Some(props) = t.get("properties").and_then(|p| p.as_object()) {
            for (pname, pv) in props {
                let pdef = cdef.and_then(|c| c.get("properties")).and_then(|p| p.get(pname));
                let label = pdef.and_then(|d| d.get("name")).and_then(|n| n.as_str()).unwrap_or(pname).to_string();
                let ty = pdef.and_then(|d| d.get("type")).and_then(|v| v.as_str()).unwrap_or("SCALAR");
                let ct = pdef.and_then(|d| d.get("componentType")).and_then(|v| v.as_str()).unwrap_or("FLOAT32");
                let is_array = pdef.and_then(|d| d.get("array")).and_then(|v| v.as_bool()).unwrap_or(false);
                let fixed = pdef.and_then(|d| d.get("count")).and_then(|v| v.as_u64()).map(|v| v as usize);
                let Some(values) = pv.get("values").and_then(|v| v.as_u64()).and_then(|i| view(json, buffers, i as usize)).map(|v| v.0) else { continue };
                let offsets = |key: &str, tkey: &str| -> Option<Vec<usize>> {
                    let (b, _) = view(json, buffers, pv.get(key)?.as_u64()? as usize)?;
                    let ot = pv.get(tkey).and_then(|v| v.as_str()).unwrap_or("UINT32");
                    let s = comp_size(ot);
                    Some(b.chunks_exact(s).map(|c| read_num(ot, c) as usize).collect())
                };
                let str_offs = offsets("stringOffsets", "stringOffsetType");
                let arr_offs = offsets("arrayOffsets", "arrayOffsetType");
                for (r, row) in rows.iter_mut().enumerate() {
                    let v: Option<String> = match ty {
                        "STRING" => {
                            let so = str_offs.as_ref();
                            if is_array {
                                Some("(list)".into())
                            } else {
                                so.and_then(|o| Some(String::from_utf8_lossy(values.get(*o.get(r)?..*o.get(r + 1)?)?).to_string()))
                            }
                        }
                        "BOOLEAN" => {
                            let bit = r;
                            values.get(bit / 8).map(|b| if b >> (bit % 8) & 1 == 1 { "Yes".to_string() } else { "No".to_string() })
                        }
                        "ENUM" => {
                            let vt = enums.and_then(|e| e.get(pdef.and_then(|d| d.get("enumType")).and_then(|v| v.as_str()).unwrap_or(""))).and_then(|e| e.get("valueType")).and_then(|v| v.as_str()).unwrap_or("UINT16");
                            let s = comp_size(vt);
                            values.get(r * s..r * s + s).map(|b| {
                                let n = read_num(vt, b) as i64;
                                enums
                                    .and_then(|e| e.get(pdef.and_then(|d| d.get("enumType")).and_then(|v| v.as_str()).unwrap_or("")))
                                    .and_then(|e| e.get("values"))
                                    .and_then(|v| v.as_array())
                                    .and_then(|vs| vs.iter().find(|x| x.get("value").and_then(|v| v.as_i64()) == Some(n)))
                                    .and_then(|x| x.get("name"))
                                    .and_then(|n| n.as_str())
                                    .map(|s| s.to_string())
                                    .unwrap_or_else(|| n.to_string())
                            })
                        }
                        _ => {
                            // SCALAR / VECn / MATn, possibly a list (fixed or with offsets)
                            let k = components(ty);
                            let s = comp_size(ct);
                            let (start, n) = if is_array {
                                match (fixed, arr_offs.as_ref()) {
                                    (Some(c), _) => (r * c * k, c * k),
                                    (None, Some(o)) => match (o.get(r), o.get(r + 1)) {
                                        (Some(a), Some(b)) => (a / s, (b - a) / s),
                                        _ => (0, 0),
                                    },
                                    _ => (0, 0),
                                }
                            } else {
                                (r * k, k)
                            };
                            let nums: Vec<String> = (0..n).filter_map(|i| values.get((start + i) * s..(start + i + 1) * s).map(|b| read_num(ct, b))).map(|x| {
                                // offset / scale from the schema (normalized or quantized values)
                                let sc = pdef.and_then(|d| d.get("scale")).and_then(|v| v.as_f64()).unwrap_or(1.0);
                                let of = pdef.and_then(|d| d.get("offset")).and_then(|v| v.as_f64()).unwrap_or(0.0);
                                fmt_num(x * sc + of)
                            }).collect();
                            (!nums.is_empty()).then(|| nums.join(", "))
                        }
                    };
                    if let Some(v) = v.filter(|v| !v.is_empty()) {
                        row.push((label.clone(), v));
                    }
                }
            }
        }
        meta.tables.push((title, rows));
    }
    meta
}

/// The object IDs of primitive `mesh` / `prim` (the first set from EXT_mesh_features), per vertex.
pub fn feature_ids(json: &Value, buffers: &[Vec<u8>], mesh: usize, prim: usize, nverts: usize) -> Option<FeatureIds> {
    let p = json.get("meshes")?.get(mesh)?.get("primitives")?.get(prim)?;
    let set = p.get("extensions")?.get("EXT_mesh_features")?.get("featureIds")?.as_array()?.iter().find(|f| f.get("texture").is_none())?;
    let table = set.get("propertyTable").and_then(|v| v.as_u64()).map(|v| v as usize);
    let null = set.get("nullFeatureId").and_then(|v| v.as_u64()).map(|v| v as u32);
    let per_vertex = match set.get("attribute").and_then(|v| v.as_u64()) {
        // no attribute: default ID = the vertex index
        None => (0..nverts as u32).collect(),
        Some(a) => {
            let acc = p.get("attributes")?.get(format!("_FEATURE_ID_{a}"))?.as_u64()? as usize;
            let acc = json.get("accessors")?.get(acc)?;
            let ct = match acc.get("componentType")?.as_u64()? {
                5121 => "UINT8",
                5123 => "UINT16",
                5125 => "UINT32",
                _ => "FLOAT32",
            };
            let count = acc.get("count")?.as_u64()? as usize;
            let off = acc.get("byteOffset").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
            let (b, stride) = view(json, buffers, acc.get("bufferView")?.as_u64()? as usize)?;
            let s = comp_size(ct);
            let stride = if stride == 0 { s } else { stride };
            (0..count).map(|i| b.get(off + i * stride..off + i * stride + s).map(|x| read_num(ct, x) as u32).unwrap_or(0)).collect()
        }
    };
    Some(FeatureIds { per_vertex, table, null })
}

/// A scalar vertex attribute (e.g. _BATCHID, _FEATURE_ID_0) as integers.
fn read_scalar_attr(json: &Value, buffers: &[Vec<u8>], prim: &Value, name: &str) -> Option<Vec<u32>> {
    let acc = prim.get("attributes")?.get(name)?.as_u64()? as usize;
    let acc = json.get("accessors")?.get(acc)?;
    let ct = match acc.get("componentType")?.as_u64()? {
        5121 => "UINT8",
        5123 => "UINT16",
        5125 => "UINT32",
        _ => "FLOAT32",
    };
    let count = acc.get("count")?.as_u64()? as usize;
    let off = acc.get("byteOffset").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
    let (b, stride) = view(json, buffers, acc.get("bufferView")?.as_u64()? as usize)?;
    let s = comp_size(ct);
    let stride = if stride == 0 { s } else { stride };
    Some((0..count).map(|i| b.get(off + i * stride..off + i * stride + s).map(|x| read_num(ct, x) as u32).unwrap_or(0)).collect())
}

/// 3D Tiles 1.0 (.b3dm): the object of each vertex comes from the _BATCHID attribute, the rows from the batch table.
pub fn batch_ids(json: &Value, buffers: &[Vec<u8>], mesh: usize, prim: usize, table: Option<usize>) -> Option<FeatureIds> {
    let table = table?;
    let p = json.get("meshes")?.get(mesh)?.get("primitives")?.get(prim)?;
    let per_vertex = read_scalar_attr(json, buffers, p, "_BATCHID").or_else(|| read_scalar_attr(json, buffers, p, "BATCHID"))?;
    Some(FeatureIds { per_vertex, table: Some(table), null: None })
}

/// The JSON batch table (3D Tiles 1.0): { "prop": [v0, v1, ...] } -> rows. The binary values
/// ({"byteOffset", "componentType"}) are read from `bin`.
pub fn batch_table_rows(json: &Value, bin: &[u8], count: usize) -> Vec<Vec<(String, String)>> {
    let mut rows = vec![Vec::new(); count];
    let Some(m) = json.as_object() else { return rows };
    for (k, v) in m {
        if k == "extensions" || k == "extras" {
            continue;
        }
        match v {
            Value::Array(a) => {
                for (r, x) in a.iter().enumerate().take(count) {
                    let s = match x {
                        Value::String(s) => s.clone(),
                        Value::Null => continue,
                        other => other.to_string(),
                    };
                    rows[r].push((k.clone(), s));
                }
            }
            Value::Object(o) => {
                // binary values: componentType + type (SCALAR / VECn)
                let off = o.get("byteOffset").and_then(|x| x.as_u64()).unwrap_or(0) as usize;
                let ct = match o.get("componentType").and_then(|x| x.as_str()).unwrap_or("FLOAT") {
                    "BYTE" => "INT8",
                    "UNSIGNED_BYTE" => "UINT8",
                    "SHORT" => "INT16",
                    "UNSIGNED_SHORT" => "UINT16",
                    "INT" => "INT32",
                    "UNSIGNED_INT" => "UINT32",
                    "DOUBLE" => "FLOAT64",
                    _ => "FLOAT32",
                };
                let k2 = components(o.get("type").and_then(|x| x.as_str()).unwrap_or("SCALAR"));
                let s = comp_size(ct);
                for (r, row) in rows.iter_mut().enumerate() {
                    let nums: Vec<String> = (0..k2).filter_map(|c| bin.get(off + (r * k2 + c) * s..off + (r * k2 + c + 1) * s).map(|b| fmt_num(read_num(ct, b)))).collect();
                    if !nums.is_empty() {
                        row.push((k.clone(), nums.join(", ")));
                    }
                }
            }
            _ => {}
        }
    }
    rows
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_with_strings_scalars_and_bools() {
        // 2 rows: name (STRING), height (FLOAT32), isPublic (BOOLEAN)
        let mut buf = Vec::new();
        let names = b"HallTower";
        buf.extend_from_slice(names); // view 0: 0..9
        let so: Vec<u8> = [0u32, 4, 9].iter().flat_map(|v| v.to_le_bytes()).collect(); // view 1: 9..21
        buf.extend_from_slice(&so);
        let h: Vec<u8> = [12.5f32, 40.0].iter().flat_map(|v| v.to_le_bytes()).collect(); // view 2: 21..29
        buf.extend_from_slice(&h);
        buf.push(0b10); // view 3: 29..30 -> row0 false, row1 true
        let json: Value = serde_json::json!({
            "bufferViews": [
                {"buffer": 0, "byteOffset": 0, "byteLength": 9},
                {"buffer": 0, "byteOffset": 9, "byteLength": 12},
                {"buffer": 0, "byteOffset": 21, "byteLength": 8},
                {"buffer": 0, "byteOffset": 29, "byteLength": 1}
            ],
            "extensions": {"EXT_structural_metadata": {
                "schema": {"classes": {"building": {"name": "Building", "properties": {
                    "name": {"type": "STRING"},
                    "height": {"type": "SCALAR", "componentType": "FLOAT32"},
                    "isPublic": {"type": "BOOLEAN"}
                }}}},
                "propertyTables": [{"class": "building", "count": 2, "properties": {
                    "name": {"values": 0, "stringOffsets": 1},
                    "height": {"values": 2},
                    "isPublic": {"values": 3}
                }}]
            }}
        });
        let m = read_meta(&json, &[buf]);
        assert_eq!(m.tables.len(), 1);
        let (title, rows) = &m.tables[0];
        assert_eq!(title, "Building");
        let get = |r: usize, k: &str| rows[r].iter().find(|(n, _)| n == k).map(|x| x.1.clone());
        assert_eq!(get(0, "name").as_deref(), Some("Hall"));
        assert_eq!(get(1, "name").as_deref(), Some("Tower"));
        assert_eq!(get(0, "height").as_deref(), Some("12.5"));
        assert_eq!(get(1, "isPublic").as_deref(), Some("Yes"));
        assert_eq!(get(0, "isPublic").as_deref(), Some("No"));
    }
}
