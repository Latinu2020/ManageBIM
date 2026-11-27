//! The open formats: CityJSON / CityJSONSeq, 3D Tiles (tileset + b3dm with a batch table).
use bimrust::*;

fn tmp(name: &str) -> std::path::PathBuf {
    let d = std::env::temp_dir().join(format!("mb_open_{}_{name}", std::process::id()));
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// 1x1x1 cube as a CityJSON Solid (6 faces), the roof with RoofSurface semantics
fn cube_solid() -> serde_json::Value {
    serde_json::json!({
        "type": "Solid", "lod": "2",
        "boundaries": [[[[0,3,2,1]], [[4,5,6,7]], [[0,1,5,4]], [[1,2,6,5]], [[2,3,7,6]], [[3,0,4,7]]]],
        "semantics": {"surfaces": [{"type": "GroundSurface"}, {"type": "RoofSurface"}, {"type": "WallSurface"}],
                      "values": [[0, 1, 2, 2, 2, 2]]}
    })
}

#[test]
fn cityjson_building_with_attributes_semantics_and_parts() {
    let doc = serde_json::json!({
        "type": "CityJSON", "version": "2.0",
        "transform": {"scale": [0.001, 0.001, 0.001], "translate": [100000.0, 400000.0, 0.0]},
        "metadata": {"referenceSystem": "https://www.opengis.net/def/crs/EPSG/0/7415"},
        "CityObjects": {
            "B1": {"type": "Building", "attributes": {"name": "Town hall", "yearOfConstruction": 1910, "address": {"city": "Delft"}},
                   "children": ["B1-p"], "geometry": [{"type": "MultiSurface", "lod": "1", "boundaries": [[[0,1,2,3]]]}, cube_solid()]},
            "B1-p": {"type": "BuildingPart", "parents": ["B1"], "geometry": [cube_solid()]}
        },
        "vertices": [[0,0,0],[1000,0,0],[1000,1000,0],[0,1000,0],[0,0,1000],[1000,0,1000],[1000,1000,1000],[0,1000,1000]]
    });
    let d = tmp("cj");
    let p = d.join("city.city.json");
    std::fs::write(&p, serde_json::to_vec(&doc).unwrap()).unwrap();
    let m = load_any(&p.to_string_lossy(), &|_, _| {}).unwrap();
    let _ = std::fs::remove_dir_all(&d);
    assert_eq!(m.elements.len(), 2);
    let b = m.elements.iter().find(|e| e.guid == "B1").unwrap();
    assert_eq!(b.name, "Building Town hall");
    assert_eq!(b.tris.len(), 12, "only the best LoD (2), 6 quads");
    assert!(b.props.iter().any(|p| p.name == "address.city" && p.value == "Delft"));
    assert!(b.props.iter().any(|p| p.name == "yearOfConstruction" && p.value == "1910"));
    assert!(b.props.iter().any(|p| p.name == "Surfaces" && p.value.contains("RoofSurface")));
    assert!(b.colors.contains(&[178, 84, 64, 255]), "roof colour");
    assert!((b.tris.iter().flat_map(|t| t.iter()).map(|p| p.x).fold(0.0, f64::max) - 100001.0).abs() < 1e-6, "transform");
    let part = m.elements.iter().find(|e| e.guid == "B1-p").unwrap();
    assert!(part.props.iter().any(|p| p.name == "Root object" && p.value == "B1"), "parts linked to their building");
}

#[test]
fn cityjson_seq_features() {
    let head = serde_json::json!({"type": "CityJSON", "version": "2.0", "transform": {"scale": [1.0, 1.0, 1.0], "translate": [0.0, 0.0, 0.0]}, "CityObjects": {}, "vertices": []});
    let feat = |id: &str, dx: i64| {
        serde_json::json!({"type": "CityJSONFeature", "id": id,
            "CityObjects": {id: {"type": "Building", "attributes": {"h": dx}, "geometry": [cube_solid()]}},
            "vertices": [[dx,0,0],[dx+1,0,0],[dx+1,1,0],[dx,1,0],[dx,0,1],[dx+1,0,1],[dx+1,1,1],[dx,1,1]]})
    };
    let text = format!("{}\n{}\n{}\n", head, feat("A", 0), feat("B", 10));
    let d = tmp("cjseq");
    let p = d.join("city.jsonl");
    std::fs::write(&p, text).unwrap();
    let m = load_any(&p.to_string_lossy(), &|_, _| {}).unwrap();
    let _ = std::fs::remove_dir_all(&d);
    assert_eq!(m.elements.len(), 2);
    let b = m.elements.iter().find(|e| e.guid == "B").unwrap();
    assert!(b.tris.iter().all(|t| t.iter().all(|p| p.x >= 10.0)), "each feature has its own vertices");
}

fn wall_of(m: &model::RawModel) -> &model::RawElement {
    m.elements.iter().find(|e| e.ifc_type.to_ascii_lowercase().contains("wall")).unwrap_or_else(|| panic!("no wall: {:?} {:?}", m.elements.iter().map(|e| &e.ifc_type).collect::<Vec<_>>(), m.warnings))
}

fn extent(e: &model::RawElement) -> glam::DVec3 {
    let (mut lo, mut hi) = (glam::DVec3::splat(f64::INFINITY), glam::DVec3::splat(f64::NEG_INFINITY));
    for t in &e.tris {
        for p in t {
            lo = lo.min(*p);
            hi = hi.max(*p);
        }
    }
    hi - lo
}

/// ifcJSON (buildingSMART): extruded wall 4 x 0.2 x 3 m, Pset_WallCommon written as an inverse relationship (isDefinedBy)
#[test]
fn ifcjson_wall_geometry_and_pset() {
    let doc = serde_json::json!({
        "type": "ifcJSON", "version": "0.0.1", "schemaIdentifier": "IFC4",
        "data": [
            {"type": "IfcProject", "globalId": "0YvctVUKr0kugbFTf53O9L", "name": "Demo",
             "unitsInContext": {"type": "IfcUnitAssignment", "units": [{"type": "IfcSIUnit", "unitType": "LENGTHUNIT", "name": "METRE"}]}},
            {"type": "IfcWall", "globalId": "2O2Fr$t4X7Zf8NOew3FLOH", "name": "Wall A", "predefinedType": "STANDARD",
             "objectPlacement": {"type": "IfcLocalPlacement", "relativePlacement": {"type": "IfcAxis2Placement3D",
                 "location": {"type": "IfcCartesianPoint", "coordinates": [10.0, 0.0, 0.0]}}},
             "representation": {"type": "IfcProductDefinitionShape", "representations": [{"type": "IfcShapeRepresentation",
                 "representationIdentifier": "Body", "representationType": "SweptSolid",
                 "items": [{"type": "IfcExtrudedAreaSolid", "depth": 3.0,
                     "sweptArea": {"type": "IfcRectangleProfileDef", "profileType": "AREA", "xDim": 4.0, "yDim": 0.2},
                     "extrudedDirection": {"type": "IfcDirection", "directionRatios": [0.0, 0.0, 1.0]}}]}]},
             "isDefinedBy": [{"type": "IfcRelDefinesByProperties", "globalId": "1kTvXnbbzCWw8lcMd1dR4o",
                 "relatingPropertyDefinition": {"type": "IfcPropertySet", "globalId": "3ZYW59sxj8lei475l7EhLU", "name": "Pset_WallCommon",
                     "hasProperties": [
                         {"type": "IfcPropertySingleValue", "name": "FireRating", "nominalValue": {"type": "IfcLabel", "value": "REI 60"}},
                         {"type": "IfcPropertySingleValue", "name": "IsExternal", "nominalValue": {"type": "IfcBoolean", "value": true}}]}}]}
        ]
    });
    let (step, _) = ifc_convert::ifcjson_to_step(&doc).unwrap();
    assert!(step.contains("IFCRELDEFINESBYPROPERTIES("), "{step}");
    assert!(step.contains("IFCLABEL('REI 60')") && step.contains("IFCBOOLEAN(.T.)"), "{step}");
    let d = tmp("ifcjson");
    let p = d.join("wall.json");
    std::fs::write(&p, serde_json::to_vec(&doc).unwrap()).unwrap();
    let m = load_any(&p.to_string_lossy(), &|_, _| {}).unwrap();
    let _ = std::fs::remove_dir_all(&d);
    let w = wall_of(&m);
    assert_eq!(w.guid, "2O2Fr$t4X7Zf8NOew3FLOH");
    let s = extent(w);
    assert!((s.x - 4.0).abs() < 1e-6 && (s.z - 3.0).abs() < 1e-6, "{s:?}");
    assert!(w.props.iter().any(|p| p.name == "FireRating" && p.value.contains("REI 60")), "{:?}", w.props);
}

/// ifcXML4: the same wall, with lists in attributes ("0 0 1"), lowercase enumerations and IfcLabel-wrapper
#[test]
fn ifcxml_wall_geometry_and_pset() {
    let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<ifcXML xmlns="http://www.buildingsmart-tech.org/ifcXML/IFC4/Add2" xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance">
 <IfcProject id="p1" GlobalId="0YvctVUKr0kugbFTf53O9L" Name="Demo">
  <UnitsInContext><IfcUnitAssignment><Units><IfcSIUnit UnitType="lengthunit" Name="metre"/></Units></IfcUnitAssignment></UnitsInContext>
 </IfcProject>
 <IfcWall id="w1" GlobalId="1hOSvn6df7F8_7GcBWlRGQ" Name="Wall X" PredefinedType="standard">
  <ObjectPlacement><IfcLocalPlacement><RelativePlacement><IfcAxis2Placement3D>
   <Location><IfcCartesianPoint Coordinates="0 0 0"/></Location></IfcAxis2Placement3D></RelativePlacement></IfcLocalPlacement></ObjectPlacement>
  <Representation><IfcProductDefinitionShape><Representations>
   <IfcShapeRepresentation RepresentationIdentifier="Body" RepresentationType="SweptSolid"><Items>
    <IfcExtrudedAreaSolid Depth="2.5"><SweptArea><IfcRectangleProfileDef ProfileType="area" XDim="6" YDim="0.3"/></SweptArea>
     <ExtrudedDirection><IfcDirection DirectionRatios="0 0 1"/></ExtrudedDirection></IfcExtrudedAreaSolid>
   </Items></IfcShapeRepresentation></Representations></IfcProductDefinitionShape></Representation>
  <IsDefinedBy><IfcRelDefinesByProperties GlobalId="2ZYW59sxj8lei475l7EhLU"><RelatingPropertyDefinition>
   <IfcPropertySet GlobalId="3ZYW59sxj8lei475l7EhLV" Name="Pset_WallCommon"><HasProperties>
    <IfcPropertySingleValue Name="FireRating"><NominalValue><IfcLabel-wrapper>REI 90</IfcLabel-wrapper></NominalValue></IfcPropertySingleValue>
   </HasProperties></IfcPropertySet></RelatingPropertyDefinition></IfcRelDefinesByProperties></IsDefinedBy>
 </IfcWall>
</ifcXML>"#;
    let d = tmp("ifcxml");
    let p = d.join("wall.ifcxml");
    std::fs::write(&p, xml).unwrap();
    let m = load_any(&p.to_string_lossy(), &|_, _| {}).unwrap();
    let _ = std::fs::remove_dir_all(&d);
    let w = wall_of(&m);
    let s = extent(w);
    assert!((s.x - 6.0).abs() < 1e-6 && (s.z - 2.5).abs() < 1e-6, "{s:?}");
    assert!(w.props.iter().any(|p| p.name == "FireRating" && p.value.contains("REI 90")), "{:?}", w.props);
}

/// minimal GLB: one triangle with _BATCHID = 0 (for b3dm)
fn glb_triangle() -> Vec<u8> {
    let mut bin = Vec::new();
    for v in [0f32, 0., 0., 1., 0., 0., 0., 1., 0.] {
        bin.extend_from_slice(&v.to_le_bytes());
    }
    for _ in 0..3 {
        bin.extend_from_slice(&0f32.to_le_bytes()); // _BATCHID as float
    }
    let json = serde_json::json!({
        "asset": {"version": "2.0"},
        "buffers": [{"byteLength": bin.len()}],
        "bufferViews": [{"buffer": 0, "byteOffset": 0, "byteLength": 36}, {"buffer": 0, "byteOffset": 36, "byteLength": 12}],
        "accessors": [{"bufferView": 0, "componentType": 5126, "count": 3, "type": "VEC3", "min": [0,0,0], "max": [1,1,0]},
                      {"bufferView": 1, "componentType": 5126, "count": 3, "type": "SCALAR"}],
        "meshes": [{"primitives": [{"attributes": {"POSITION": 0, "_BATCHID": 1}}]}],
        "nodes": [{"mesh": 0, "name": "Tile mesh"}], "scenes": [{"nodes": [0]}], "scene": 0
    });
    let mut j = serde_json::to_vec(&json).unwrap();
    while !j.len().is_multiple_of(4) {
        j.push(b' ');
    }
    let mut out = Vec::new();
    out.extend_from_slice(b"glTF");
    out.extend_from_slice(&2u32.to_le_bytes());
    out.extend_from_slice(&((12 + 8 + j.len() + 8 + bin.len()) as u32).to_le_bytes());
    out.extend_from_slice(&(j.len() as u32).to_le_bytes());
    out.extend_from_slice(b"JSON");
    out.extend_from_slice(&j);
    out.extend_from_slice(&(bin.len() as u32).to_le_bytes());
    out.extend_from_slice(b"BIN\0");
    out.extend_from_slice(&bin);
    out
}

#[test]
fn tiles_tileset_with_b3dm_batch_table() {
    let glb = glb_triangle();
    let mut ft = serde_json::to_vec(&serde_json::json!({"BATCH_LENGTH": 1})).unwrap();
    while !ft.len().is_multiple_of(8) {
        ft.push(b' ');
    }
    let mut bt = serde_json::to_vec(&serde_json::json!({"name": ["Pump P-1"], "height": [2.5]})).unwrap();
    while !bt.len().is_multiple_of(8) {
        bt.push(b' ');
    }
    let mut b3dm = Vec::new();
    b3dm.extend_from_slice(b"b3dm");
    b3dm.extend_from_slice(&1u32.to_le_bytes());
    b3dm.extend_from_slice(&((28 + ft.len() + bt.len() + glb.len()) as u32).to_le_bytes());
    for v in [ft.len(), 0, bt.len(), 0] {
        b3dm.extend_from_slice(&(v as u32).to_le_bytes());
    }
    b3dm.extend_from_slice(&ft);
    b3dm.extend_from_slice(&bt);
    b3dm.extend_from_slice(&glb);
    let d = tmp("tiles");
    std::fs::write(d.join("t0.b3dm"), &b3dm).unwrap();
    // REPLACE: the parent (with no own content used) is replaced by the child; the child's transform = +100 on X
    let ts = serde_json::json!({
        "asset": {"version": "1.1"}, "geometricError": 100,
        "root": {"geometricError": 10, "refine": "REPLACE", "boundingVolume": {"box": [0,0,0,1,0,0,0,1,0,0,0,1]},
                 "content": {"uri": "missing_parent.glb"},
                 "children": [{"geometricError": 0, "boundingVolume": {"box": [0,0,0,1,0,0,0,1,0,0,0,1]},
                               "transform": [1,0,0,0, 0,1,0,0, 0,0,1,0, 100,0,0,1],
                               "content": {"uri": "t0.b3dm"}}]}
    });
    let p = d.join("tileset.json");
    std::fs::write(&p, serde_json::to_vec(&ts).unwrap()).unwrap();
    let m = load_any(&p.to_string_lossy(), &|_, _| {}).unwrap();
    let _ = std::fs::remove_dir_all(&d);
    assert_eq!(m.elements.len(), 1, "{:?}", m.warnings);
    let e = &m.elements[0];
    assert!(e.props.iter().any(|p| p.name == "name" && p.value == "Pump P-1"), "{:?}", e.props);
    assert!(e.props.iter().any(|p| p.name == "height" && p.value == "2.5"));
    assert!(e.tris[0].iter().all(|p| p.x >= 100.0), "tile transform");
    assert!(m.warnings.iter().all(|w| !w.contains("missing_parent")), "REPLACE skips the parent content");
}
