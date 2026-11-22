//! Export COBie (Construction Operations Building Information Exchange) in XLSX.
//! Foiile sint cele din specificatia COBie 2.4: Contact, Facility, Floor,
//! Type, Component, System. Datele vin din modelele incarcate (IFC / user data).

use crate::qto::{Cell, Sheet};

fn s(v: impl Into<String>) -> Cell {
    Cell::S(v.into())
}

/// Genereaza registrul COBie pentru modelele deschise.
pub fn workbook(app: &crate::app::App) -> Vec<u8> {
    let project = app
        .project_path
        .as_ref()
        .map(|p| crate::file_name(p))
        .unwrap_or_else(|| "ManageBIM project".to_string());

    let mut contacts: Vec<Vec<Cell>> = Vec::new();
    let mut floors: Vec<Vec<Cell>> = Vec::new();
    let mut types: Vec<Vec<Cell>> = Vec::new();
    let mut components: Vec<Vec<Cell>> = Vec::new();
    let mut systems: Vec<Vec<Cell>> = Vec::new();
    let mut seen_floors = std::collections::BTreeSet::new();
    let mut seen_types = std::collections::BTreeSet::new();

    for (_, m) in app.loaded_models() {
        for e in &m.elements {
            let level = if e.storey.is_empty() { "Level 0".to_string() } else { e.storey.clone() };
            if seen_floors.insert(level.clone()) {
                floors.push(vec![s(level.clone()), s(e.storey.clone())]);
            }
            let ty = if e.ifc_type.is_empty() { "IfcBuildingElementProxy".to_string() } else { e.ifc_type.clone() };
            if seen_types.insert(ty.clone()) {
                types.push(vec![s(ty.clone()), s(ty.clone()), s(&e.name)]);
                systems.push(vec![s(format!("System - {ty}")), s(&ty)]);
            }
            components.push(vec![
                s(if e.name.is_empty() { e.guid.clone() } else { e.name.clone() }),
                s(&ty),
                s(&level),
                s(e.guid.clone()),
                s(&m.name),
            ]);
        }
    }

    contacts.push(vec![s("ManageBIM"), s(""), s("Author")]);

    let sheets = vec![
        Sheet {
            name: "Contact".into(),
            title: "COBie Contact".into(),
            headers: ["Name", "Email", "Role"].iter().map(|x| x.to_string()).collect(),
            widths: vec![26.0, 34.0, 20.0],
            rows: contacts,
            total_from: None,
        },
        Sheet {
            name: "Facility".into(),
            title: "COBie Facility".into(),
            headers: ["Name", "SiteName", "ProjectName"].iter().map(|x| x.to_string()).collect(),
            widths: vec![30.0, 26.0, 34.0],
            rows: vec![vec![s(&project), s("Site"), s(&project)]],
            total_from: None,
        },
        Sheet {
            name: "Floor".into(),
            title: "COBie Floor".into(),
            headers: ["Name", "Elevation"].iter().map(|x| x.to_string()).collect(),
            widths: vec![28.0, 18.0],
            rows: floors,
            total_from: None,
        },
        Sheet {
            name: "Type".into(),
            title: "COBie Type".into(),
            headers: ["Name", "Category", "Description"].iter().map(|x| x.to_string()).collect(),
            widths: vec![30.0, 30.0, 44.0],
            rows: types,
            total_from: None,
        },
        Sheet {
            name: "Component".into(),
            title: "COBie Component".into(),
            headers: ["Name", "TypeName", "Space", "GlobalId", "Model"].iter().map(|x| x.to_string()).collect(),
            widths: vec![32.0, 28.0, 20.0, 26.0, 26.0],
            rows: components,
            total_from: None,
        },
        Sheet {
            name: "System".into(),
            title: "COBie System".into(),
            headers: ["Name", "Category"].iter().map(|x| x.to_string()).collect(),
            widths: vec![32.0, 30.0],
            rows: systems,
            total_from: None,
        },
    ];
    crate::qto::xlsx(&sheets)
}
