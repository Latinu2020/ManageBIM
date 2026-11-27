//! Quantification (as in Navisworks): quantities grouped by any properties, with totals.
//! Sources: the element geometry (volume, areas, length, height) or IFC numeric properties
//! (Qto_*BaseQuantities: NetVolume, NetSideArea, Length...), summed per group.
use crate::app::TreeLevel;
use crate::model::{ElemId, Element, Model};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum QtyColumn {
    Count,
    /// the volume enclosed by the mesh (m³)
    Volume,
    /// the sum of the areas of all faces (m²)
    Surface,
    /// the area of the upward-facing faces, projected in plan (m²) - slabs, floors, roofs
    PlanArea,
    /// the largest horizontal dimension of the box (m) - walls, beams, pipes
    Length,
    /// the height of the box (m)
    Height,
    /// the sum of a numeric property (e.g. Qto_WallBaseQuantities / NetVolume)
    Prop { cat: String, prop: String },
}

impl QtyColumn {
    pub const GEOMETRIC: [QtyColumn; 6] = [QtyColumn::Count, QtyColumn::Volume, QtyColumn::Surface, QtyColumn::PlanArea, QtyColumn::Length, QtyColumn::Height];
    pub fn title(&self) -> String {
        match self {
            QtyColumn::Count => "Count".into(),
            QtyColumn::Volume => "Volume (m³)".into(),
            QtyColumn::Surface => "Surface area (m²)".into(),
            QtyColumn::PlanArea => "Plan area (m²)".into(),
            QtyColumn::Length => "Length (m)".into(),
            QtyColumn::Height => "Height (m)".into(),
            QtyColumn::Prop { prop, .. } => prop.clone(),
        }
    }
    pub fn hint(&self) -> &'static str {
        match self {
            QtyColumn::Count => "Number of items",
            QtyColumn::Volume => "Volume enclosed by the 3D geometry (closed solids)",
            QtyColumn::Surface => "Total area of all faces of the 3D geometry",
            QtyColumn::PlanArea => "Area seen from above (upward faces) - slabs, floors, roofs",
            QtyColumn::Length => "Longest horizontal dimension of the bounding box - walls, beams, pipes, ducts",
            QtyColumn::Height => "Height of the bounding box",
            QtyColumn::Prop { .. } => "Sum of this numeric property (e.g. IFC base quantities)",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub enum QtoScope {
    #[default]
    All,
    Visible,
    Selection,
}

impl QtoScope {
    pub const ALL: [QtoScope; 3] = [QtoScope::All, QtoScope::Visible, QtoScope::Selection];
    pub fn label(self) -> &'static str {
        match self {
            QtoScope::All => "All items",
            QtoScope::Visible => "Visible items",
            QtoScope::Selection => "Selected items",
        }
    }
}

/// The configuration of a quantity: reusable as a template (.bimqto) and saved in the project.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct QtoSetup {
    pub name: String,
    pub group_by: Vec<TreeLevel>,
    pub columns: Vec<QtyColumn>,
    #[serde(default)]
    pub scope: QtoScope,
}

impl Default for QtoSetup {
    fn default() -> Self {
        QtoSetup {
            name: "Quantities".into(),
            group_by: vec![TreeLevel::new(crate::app::ITEM_CAT, "IFC Type")],
            columns: vec![QtyColumn::Count, QtyColumn::Volume, QtyColumn::PlanArea, QtyColumn::Length],
            scope: QtoScope::All,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct QtoFile {
    pub app: String,
    pub kind: String,
    pub version: u32,
    pub setup: QtoSetup,
}

pub const FILE_KIND: &str = "quantification";

impl QtoSetup {
    pub fn to_file_json(&self) -> String {
        serde_json::to_string_pretty(&QtoFile { app: "ManageBIM".into(), kind: FILE_KIND.into(), version: 1, setup: self.clone() }).unwrap_or_default()
    }
    pub fn from_file_json(text: &str) -> Result<QtoSetup, String> {
        let f: QtoFile = serde_json::from_str(text).map_err(|e| format!("Invalid quantification template: {e}"))?;
        if f.kind != FILE_KIND {
            return Err(format!("This is not a quantification template (kind \"{}\").", f.kind));
        }
        Ok(f.setup)
    }
}

// ====================================================================== geometry

/// The geometric quantities of an element (in metres, model coordinates).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct GeoQty {
    pub volume: f64,
    pub surface: f64,
    pub plan: f64,
    pub length: f64,
    pub height: f64,
}

pub fn geo_qty(m: &Model, e: &Element) -> GeoQty {
    let t = m.tris(e);
    let (mut vol, mut surf, mut plan) = (0.0f64, 0.0f64, 0.0f64);
    // volume through tetrahedra with the apex at the element centre (numerically stable relative to the scene origin)
    let c = e.aabb.center().as_dvec3();
    for tri in t.chunks_exact(3) {
        let (a, b, d) = (tri[0].as_dvec3() - c, tri[1].as_dvec3() - c, tri[2].as_dvec3() - c);
        vol += a.dot(b.cross(d)) / 6.0;
        let n = (b - a).cross(d - a);
        surf += n.length() * 0.5;
        // upward-facing face (normal with z > 0.5): the area projected in plan
        let len = n.length();
        if len > 0.0 && n.z / len > 0.5 {
            plan += n.z * 0.5;
        }
    }
    let s = e.aabb.size();
    // the length along the element direction (walls / beams / pipes on a diagonal), not along the X / Y axes
    let length = plan_length(t).unwrap_or(s.x.max(s.y) as f64);
    GeoQty { volume: vol.abs(), surface: surf, plan, length, height: s.z as f64 }
}

/// The largest dimension in plan measured along the principal axes of the points (2D PCA).
/// A 10 m wall rotated by 45° has 10 m (the box aligned to X / Y would give 7.07 m on each axis).
pub fn plan_length(pts: &[glam::Vec3]) -> Option<f64> {
    if pts.len() < 3 {
        return None;
    }
    // vertices appear several times (coplanar on faces); unequal weighting tilts the PCA axes
    // (a 3x2 box would give 3.23 m); we remove duplicates at 1 mm before the calculation
    let seen: std::collections::HashSet<(i64, i64)> = pts.iter().map(|p| ((p.x as f64 * 1000.0).round() as i64, (p.y as f64 * 1000.0).round() as i64)).collect();
    if seen.len() < 3 {
        return None;
    }
    let n = seen.len() as f64;
    let (mx, my) = seen.iter().fold((0.0f64, 0.0f64), |(x, y), (px, py)| (x + *px as f64, y + *py as f64));
    let (mx, my) = (mx / n / 1000.0, my / n / 1000.0);
    let (mut sxx, mut syy, mut sxy) = (0.0f64, 0.0f64, 0.0f64);
    for (px, py) in &seen {
        let (dx, dy) = (*px as f64 / 1000.0 - mx, *py as f64 / 1000.0 - my);
        sxx += dx * dx;
        syy += dy * dy;
        sxy += dx * dy;
    }
    // the main eigen-direction of the 2x2 covariance matrix
    let ang = 0.5 * (2.0 * sxy).atan2(sxx - syy);
    let (u, v) = ((ang.cos(), ang.sin()), (-ang.sin(), ang.cos()));
    let ext = |d: (f64, f64)| {
        let (lo, hi) = pts.iter().fold((f64::MAX, f64::MIN), |(lo, hi), p| {
            let k = (p.x as f64 - mx) * d.0 + (p.y as f64 - my) * d.1;
            (lo.min(k), hi.max(k))
        });
        hi - lo
    };
    let l = ext(u).max(ext(v));
    l.is_finite().then_some(l)
}

#[cfg(test)]
mod length_tests {
    use glam::Vec3;

    fn wall(len: f32, thick: f32, ang_deg: f32) -> Vec<Vec3> {
        let (c, s) = (ang_deg.to_radians().cos(), ang_deg.to_radians().sin());
        let mut v = Vec::new();
        for (x, y) in [(0.0, 0.0), (len, 0.0), (len, thick), (0.0, thick)] {
            for z in [0.0, 3.0] {
                v.push(Vec3::new(x * c - y * s, x * s + y * c, z));
            }
        }
        v
    }

    #[test]
    fn diagonal_wall_keeps_its_length() {
        for ang in [0.0, 30.0, 45.0, 90.0, 137.0] {
            let l = super::plan_length(&wall(10.0, 0.25, ang)).unwrap();
            assert!((l - 10.0).abs() < 1e-3, "angle {ang}: {l}");
        }
    }
}

// ====================================================================== result

pub struct QtoRow {
    /// the grouping values (one for each level in group_by)
    pub keys: Vec<String>,
    pub ids: Vec<ElemId>,
    /// the values of each element (parallel to ids) - for the detailed sheet in Excel
    pub item_values: Vec<Vec<Option<f64>>>,
    /// the column sum; None = no element in the group has the value
    pub values: Vec<Option<f64>>,
    /// how many elements in the group do not have the column value (missing property)
    pub missing: Vec<u32>,
}

pub struct QtoResult {
    pub setup: QtoSetup,
    pub rows: Vec<QtoRow>,
    pub totals: Vec<Option<f64>>,
    pub missing: Vec<u32>,
    pub count: usize,
    /// the unit for the property-type columns (from the first value found, e.g. "m³")
    pub units: Vec<String>,
}

pub fn fmt(v: Option<f64>, col: &QtyColumn) -> String {
    match v {
        None => "–".into(),
        Some(x) if *col == QtyColumn::Count => format!("{x:.0}"),
        Some(x) => format!("{x:.3}"),
    }
}

impl QtoResult {
    pub fn column_title(&self, i: usize) -> String {
        let c = &self.setup.columns[i];
        match c {
            QtyColumn::Prop { .. } if !self.units[i].is_empty() => format!("{} ({})", c.title(), self.units[i]),
            _ => c.title(),
        }
    }

    /// Summary per group (UTF-8 BOM, separator ';' - opens directly in Excel).
    pub fn summary_csv(&self) -> String {
        let q = |s: &str| format!("\"{}\"", s.replace('"', "\"\""));
        let mut s = String::from("\u{feff}");
        let mut head: Vec<String> = self.setup.group_by.iter().map(|g| q(&g.prop)).collect();
        head.extend((0..self.setup.columns.len()).map(|i| q(&self.column_title(i))));
        s.push_str(&head.join(";"));
        s.push('\n');
        for r in &self.rows {
            let mut cells: Vec<String> = r.keys.iter().map(|k| q(k)).collect();
            cells.extend(r.values.iter().zip(&self.setup.columns).map(|(v, c)| csv_num(*v, c)));
            s.push_str(&cells.join(";"));
            s.push('\n');
        }
        let mut cells: Vec<String> = vec![q("TOTAL")];
        cells.extend((1..self.setup.group_by.len()).map(|_| String::new()));
        cells.extend(self.totals.iter().zip(&self.setup.columns).map(|(v, c)| csv_num(*v, c)));
        s.push_str(&cells.join(";"));
        s.push('\n');
        s
    }
}

/// number for CSV: with a decimal comma (Excel in Romanian / Europe) - no thousands separator
pub fn csv_num(v: Option<f64>, c: &QtyColumn) -> String {
    fmt(v, c).replace('–', "").replace('.', ",")
}

// ====================================================================== Excel (.xlsx)

pub enum Cell {
    S(String),
    N(f64),
    Empty,
}

pub struct Sheet {
    pub name: String,
    pub title: String,
    pub headers: Vec<String>,
    pub widths: Vec<f64>,
    pub rows: Vec<Vec<Cell>>,
    /// TOTAL row with SUM formulas on the numeric columns starting with this column
    pub total_from: Option<usize>,
}

/// .xlsx file with several sheets: title, blue header, filters, a total row with SUM formulas.
pub fn xlsx(sheets: &[Sheet]) -> Vec<u8> {
    use crate::markup::{col_name, esc, Zip};
    let mut zip = Zip::default();
    let mut ct = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\"><Default Extension=\"rels\" ContentType=\"application/vnd.openxmlformats-package.relationships+xml\"/><Default Extension=\"xml\" ContentType=\"application/xml\"/><Override PartName=\"/xl/workbook.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml\"/><Override PartName=\"/xl/styles.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.styles+xml\"/>");
    let mut wb = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><workbook xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\" xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\"><sheets>");
    let mut rels = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">");
    for (k, sh) in sheets.iter().enumerate() {
        let n = k + 1;
        // the sheet name: max 31 characters, without []:*?/\
        let sname: String = sh.name.chars().filter(|c| !"[]:*?/\\".contains(*c)).take(31).collect();
        ct.push_str(&format!("<Override PartName=\"/xl/worksheets/sheet{n}.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml\"/>"));
        wb.push_str(&format!("<sheet name=\"{}\" sheetId=\"{n}\" r:id=\"rId{n}\"/>", esc(&sname)));
        rels.push_str(&format!("<Relationship Id=\"rId{n}\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet\" Target=\"worksheets/sheet{n}.xml\"/>"));

        let ncol = sh.headers.len().max(1);
        let mut x = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><worksheet xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\"><sheetViews><sheetView workbookViewId=\"0\"><pane ySplit=\"2\" topLeftCell=\"A3\" activePane=\"bottomLeft\" state=\"frozen\"/></sheetView></sheetViews><cols>");
        for c in 0..ncol {
            let w = sh.widths.get(c).copied().unwrap_or(14.0);
            x.push_str(&format!("<col min=\"{0}\" max=\"{0}\" width=\"{1}\" customWidth=\"1\"/>", c + 1, w));
        }
        x.push_str("</cols><sheetData>");
        let s_cell = |r: usize, c: usize, v: &str, st: u32| format!("<c r=\"{}{r}\" t=\"inlineStr\" s=\"{st}\"><is><t xml:space=\"preserve\">{}</t></is></c>", col_name(c), esc(v));
        x.push_str(&format!("<row r=\"1\" ht=\"22\" customHeight=\"1\">{}</row><row r=\"2\">", s_cell(1, 0, &sh.title, 3)));
        for (c, h) in sh.headers.iter().enumerate() {
            x.push_str(&s_cell(2, c, h, 1));
        }
        x.push_str("</row>");
        for (i, row) in sh.rows.iter().enumerate() {
            let r = i + 3;
            x.push_str(&format!("<row r=\"{r}\">"));
            for (c, cell) in row.iter().enumerate() {
                match cell {
                    Cell::S(v) => x.push_str(&s_cell(r, c, v, 2)),
                    Cell::N(v) if v.is_finite() => x.push_str(&format!("<c r=\"{}{r}\" s=\"5\"><v>{v}</v></c>", col_name(c))),
                    _ => x.push_str(&format!("<c r=\"{}{r}\" s=\"2\"/>", col_name(c))),
                }
            }
            x.push_str("</row>");
        }
        let last = sh.rows.len() + 2;
        if let Some(from) = sh.total_from {
            let r = last + 1;
            x.push_str(&format!("<row r=\"{r}\">{}", s_cell(r, 0, "TOTAL", 4)));
            for c in 1..ncol {
                if c >= from {
                    let col = col_name(c);
                    x.push_str(&format!("<c r=\"{col}{r}\" s=\"6\"><f>SUBTOTAL(9,{col}3:{col}{last})</f></c>"));
                } else {
                    x.push_str(&s_cell(r, c, "", 4));
                }
            }
            x.push_str("</row>");
        }
        x.push_str(&format!("</sheetData><autoFilter ref=\"A2:{}{}\"/></worksheet>", col_name(ncol - 1), last.max(2)));
        zip.add(&format!("xl/worksheets/sheet{n}.xml"), x.as_bytes());
    }
    ct.push_str("</Types>");
    wb.push_str("</sheets></workbook>");
    let ns = sheets.len() + 1;
    rels.push_str(&format!("<Relationship Id=\"rId{ns}\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles\" Target=\"styles.xml\"/></Relationships>"));
    zip.add("[Content_Types].xml", ct.as_bytes());
    zip.add("_rels/.rels", b"<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\"><Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument\" Target=\"xl/workbook.xml\"/></Relationships>");
    zip.add("xl/workbook.xml", wb.as_bytes());
    zip.add("xl/_rels/workbook.xml.rels", rels.as_bytes());
    // styles: 0 normal, 1 header, 2 bordered text, 3 title, 4 TOTAL text, 5 number 0.000, 6 TOTAL number
    zip.add("xl/styles.xml", b"<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><styleSheet xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\"><numFmts count=\"1\"><numFmt numFmtId=\"164\" formatCode=\"#,##0.000\"/></numFmts><fonts count=\"4\"><font><sz val=\"10\"/><name val=\"Calibri\"/></font><font><b/><sz val=\"10\"/><color rgb=\"FFFFFFFF\"/><name val=\"Calibri\"/></font><font><b/><sz val=\"14\"/><color rgb=\"FF1E3A8A\"/><name val=\"Calibri\"/></font><font><b/><sz val=\"10\"/><name val=\"Calibri\"/></font></fonts><fills count=\"4\"><fill><patternFill patternType=\"none\"/></fill><fill><patternFill patternType=\"gray125\"/></fill><fill><patternFill patternType=\"solid\"><fgColor rgb=\"FF3B82F6\"/><bgColor indexed=\"64\"/></patternFill></fill><fill><patternFill patternType=\"solid\"><fgColor rgb=\"FFE2E8F0\"/><bgColor indexed=\"64\"/></patternFill></fill></fills><borders count=\"2\"><border><left/><right/><top/><bottom/><diagonal/></border><border><left style=\"thin\"><color rgb=\"FFCBD5E1\"/></left><right style=\"thin\"><color rgb=\"FFCBD5E1\"/></right><top style=\"thin\"><color rgb=\"FFCBD5E1\"/></top><bottom style=\"thin\"><color rgb=\"FFCBD5E1\"/></bottom><diagonal/></border></borders><cellStyleXfs count=\"1\"><xf numFmtId=\"0\" fontId=\"0\" fillId=\"0\" borderId=\"0\"/></cellStyleXfs><cellXfs count=\"7\"><xf numFmtId=\"0\" fontId=\"0\" fillId=\"0\" borderId=\"0\" xfId=\"0\"/><xf numFmtId=\"0\" fontId=\"1\" fillId=\"2\" borderId=\"1\" xfId=\"0\" applyFont=\"1\" applyFill=\"1\" applyBorder=\"1\"/><xf numFmtId=\"0\" fontId=\"0\" fillId=\"0\" borderId=\"1\" xfId=\"0\" applyBorder=\"1\"/><xf numFmtId=\"0\" fontId=\"2\" fillId=\"0\" borderId=\"0\" xfId=\"0\" applyFont=\"1\"/><xf numFmtId=\"0\" fontId=\"3\" fillId=\"3\" borderId=\"1\" xfId=\"0\" applyFont=\"1\" applyFill=\"1\" applyBorder=\"1\"/><xf numFmtId=\"164\" fontId=\"0\" fillId=\"0\" borderId=\"1\" xfId=\"0\" applyNumberFormat=\"1\" applyBorder=\"1\"/><xf numFmtId=\"164\" fontId=\"3\" fillId=\"3\" borderId=\"1\" xfId=\"0\" applyNumberFormat=\"1\" applyFont=\"1\" applyFill=\"1\" applyBorder=\"1\"/></cellXfs><cellStyles count=\"1\"><cellStyle name=\"Normal\" xfId=\"0\" builtinId=\"0\"/></cellStyles></styleSheet>");
    zip.finish()
}

impl QtoResult {
    /// Excel: the "Summary" sheet (one row per group + TOTAL) and the "Items" sheet (one row per element).
    /// `info(id)` -> (model, name, IFC type, level, GlobalId) for the detailed sheet.
    pub fn to_xlsx(&self, title: &str, info: &dyn Fn(ElemId) -> [String; 5]) -> Vec<u8> {
        let nk = self.setup.group_by.len().max(1);
        let ncols = self.setup.columns.len();
        let mut headers: Vec<String> = if self.setup.group_by.is_empty() { vec!["Group".into()] } else { self.setup.group_by.iter().map(|g| g.prop.clone()).collect() };
        headers.extend((0..ncols).map(|i| self.column_title(i)));
        let num = |v: &Option<f64>| v.map(Cell::N).unwrap_or(Cell::Empty);
        let rows: Vec<Vec<Cell>> = self
            .rows
            .iter()
            .map(|r| {
                let mut c: Vec<Cell> = if r.keys.is_empty() { vec![Cell::S("All".into())] } else { r.keys.iter().map(|k| Cell::S(k.clone())).collect() };
                c.extend(r.values.iter().map(num));
                c
            })
            .collect();
        let mut widths = vec![26.0; nk];
        widths.extend((0..ncols).map(|_| 16.0));
        let summary = Sheet { name: "Summary".into(), title: format!("{title} - {}", self.setup.name), headers, widths, rows, total_from: Some(nk) };

        let mut ih: Vec<String> = vec!["Model".into(), "Name".into(), "IFC Type".into(), "Level".into(), "GlobalId".into()];
        ih.extend(self.setup.group_by.iter().map(|g| g.prop.clone()));
        ih.extend((0..ncols).map(|i| self.column_title(i)));
        let mut items = Vec::new();
        for r in &self.rows {
            for (id, vals) in r.ids.iter().zip(&r.item_values) {
                let mut c: Vec<Cell> = info(*id).into_iter().map(Cell::S).collect();
                c.extend(r.keys.iter().map(|k| Cell::S(k.clone())));
                c.extend(vals.iter().map(num));
                items.push(c);
            }
        }
        let mut iw = vec![18.0, 30.0, 18.0, 16.0, 26.0];
        iw.extend(self.setup.group_by.iter().map(|_| 20.0));
        iw.extend((0..ncols).map(|_| 14.0));
        let from = 5 + self.setup.group_by.len();
        let detail = Sheet { name: "Items".into(), title: format!("{title} - items"), headers: ih, widths: iw, rows: items, total_from: Some(from) };
        xlsx(&[summary, detail])
    }
}
