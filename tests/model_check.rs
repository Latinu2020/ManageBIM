use bimrust::check::{parse_ids, run, Card, Facet, Kind, Pattern, Rule, Value};
use bimrust::model::{Element, Model, Prop};

fn el(name: &str, ty: &str, props: &[(&str, &str, &str)]) -> Element {
    Element {
        name: name.into(),
        ifc_type: ty.into(),
        guid: format!("guid-{name}"),
        storey: "Level 1".into(),
        props: props.iter().map(|(g, n, v)| Prop { group: g.to_string(), name: n.to_string(), value: v.to_string() }).collect(),
        tri_start: 0,
        tri_count: 1,
        aabb: bimrust::model::Aabb::EMPTY,
        default_hidden: false,
        bvh: Default::default(),
    }
}

fn model(elements: Vec<Element>) -> Model {
    Model { name: "m.ifc".into(), path: String::new(), elements, pos: Vec::new(), tri_color: Vec::new(), aabb: bimrust::model::Aabb::EMPTY, info: Vec::new(), warnings: Vec::new(), cloud: None }
}

#[test]
fn xsd_patterns_are_anchored_and_support_classes_and_counts() {
    let p = Pattern::new(r"EI\d{2,3}").unwrap();
    assert!(p.is_match("EI60"));
    assert!(p.is_match("EI120"));
    assert!(!p.is_match("EI6"));
    assert!(!p.is_match("xEI60"));
    let q = Pattern::new("(REI|EI)[0-9]+( min)?").unwrap();
    assert!(q.is_match("REI90 min"));
    assert!(!q.is_match("R90"));
    assert!(Pattern::new("[A-Z]{2}-[^ ]+").unwrap().is_match("WL-01a"));
    assert!(Pattern::new("a*").unwrap().is_match(""));
}

#[test]
fn values_compare_leniently() {
    assert!(Value::OneOf(vec!["TRUE".into()]).matches(".T."));
    assert!(Value::OneOf(vec!["ei60".into()]).matches("EI60"));
    assert!(Value::OneOf(vec!["12".into()]).matches("12.0"));
    assert!(Value::Range { min: Some(0.1), max: Some(0.5) }.matches("0,25 m"));
    assert!(!Value::Range { min: Some(0.1), max: Some(0.5) }.matches("abc"));
}

#[test]
fn rules_find_missing_and_wrong_values() {
    let m = model(vec![
        el("W1", "IfcWall", &[("Pset_WallCommon", "FireRating", "EI60")]),
        el("W2", "IfcWall", &[("Pset_WallCommon", "FireRating", "none")]),
        el("W3", "IfcWall", &[]),
        el("D1", "IfcDoor", &[]),
    ]);
    let mut r = Rule::new("walls fire rating");
    r.applicability.push(Facet::entity(&["IFCWALL"]));
    r.requirements.push(Facet::property("Pset_WallCommon", "FireRating", Value::Pattern(r"EI\d+".into())));
    let mut no_name = Rule::new("prohibited");
    no_name.requirements.push(Facet { kind: Kind::Property, set: Value::Any, name: Value::one("FireRating"), value: Value::Any, card: Card::Prohibited });
    let res = run(&[r, no_name], &[(0, &m)]);
    assert_eq!(res[0].checked, 3, "only walls are checked");
    assert_eq!(res[0].failed.len(), 2);
    assert!(res[0].failed.iter().any(|(id, why)| id.e == 1 && why.contains("'none'")));
    assert!(res[0].failed.iter().any(|(id, why)| id.e == 2 && why.contains("missing")));
    assert_eq!(res[1].checked, 4);
    assert_eq!(res[1].failed.len(), 2, "W1 and W2 have the prohibited property");
}

#[test]
fn ids_file_is_imported_as_rules() {
    let ids = r#"<?xml version="1.0" encoding="UTF-8"?>
<ids:ids xmlns:ids="http://standards.buildingsmart.org/IDS" xmlns:xs="http://www.w3.org/2001/XMLSchema">
  <ids:info><ids:title>Test</ids:title></ids:info>
  <ids:specifications>
    <ids:specification name="Doors" ifcVersion="IFC4">
      <ids:applicability minOccurs="0" maxOccurs="unbounded">
        <ids:entity><ids:name><ids:simpleValue>IFCDOOR</ids:simpleValue></ids:name></ids:entity>
      </ids:applicability>
      <ids:requirements>
        <ids:property dataType="IFCLABEL" cardinality="required">
          <ids:propertySet><ids:simpleValue>Pset_DoorCommon</ids:simpleValue></ids:propertySet>
          <ids:baseName><ids:simpleValue>FireRating</ids:simpleValue></ids:baseName>
          <ids:value><xs:restriction base="xs:string"><xs:enumeration value="EI30"/><xs:enumeration value="EI60"/></xs:restriction></ids:value>
        </ids:property>
        <ids:attribute cardinality="required"><ids:name><ids:simpleValue>Name</ids:simpleValue></ids:name></ids:attribute>
        <ids:partOf relation="IFCRELAGGREGATES"><ids:entity><ids:name><ids:simpleValue>IFCBUILDINGSTOREY</ids:simpleValue></ids:name></ids:entity></ids:partOf>
      </ids:requirements>
    </ids:specification>
    <ids:specification name="Widths">
      <ids:applicability><ids:entity><ids:name><ids:simpleValue>IFCDOOR</ids:simpleValue></ids:name></ids:entity></ids:applicability>
      <ids:requirements>
        <ids:property cardinality="optional"><ids:propertySet><ids:simpleValue>Qto_DoorBaseQuantities</ids:simpleValue></ids:propertySet><ids:baseName><ids:simpleValue>Width</ids:simpleValue></ids:baseName>
          <ids:value><xs:restriction base="xs:double"><xs:minInclusive value="0.8"/><xs:maxInclusive value="2.0"/></xs:restriction></ids:value></ids:property>
      </ids:requirements>
    </ids:specification>
  </ids:specifications>
</ids:ids>"#;
    let (rules, warn) = parse_ids(ids, "test.ids").unwrap();
    assert_eq!(rules.len(), 2);
    assert_eq!(rules[0].name, "Doors");
    assert_eq!(rules[0].requirements.len(), 2);
    assert_eq!(rules[0].requirements[0].value, Value::OneOf(vec!["EI30".into(), "EI60".into()]));
    assert_eq!(warn.len(), 1, "partOf is reported as not supported");
    assert_eq!(rules[1].requirements[0].card, Card::Optional);
    let m = model(vec![
        el("Door A", "IfcDoor", &[("Pset_DoorCommon", "FireRating", "EI60"), ("Qto_DoorBaseQuantities", "Width", "0.9 m")]),
        el("Door B", "IfcDoor", &[("Pset_DoorCommon", "FireRating", "EI90"), ("Qto_DoorBaseQuantities", "Width", "0.6 m")]),
        el("Door C", "IfcDoor", &[("Pset_DoorCommon", "FireRating", "EI30")]),
        el("Wall", "IfcWall", &[]),
    ]);
    let res = run(&rules, &[(0, &m)]);
    assert_eq!((res[0].checked, res[0].failed.len()), (3, 1));
    assert_eq!((res[1].checked, res[1].failed.len()), (3, 1), "only Door B has a wrong width; Door C has none (optional)");
    assert!(parse_ids("<foo/>", "x").is_err());
}
