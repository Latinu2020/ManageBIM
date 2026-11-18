//! Quantification (ca in Navisworks): cantitati grupate dupa orice proprietati, cu totaluri.
//! Surse: geometria elementului (volum, arii, lungime, inaltime) sau proprietati numerice IFC
//! (Qto_*BaseQuantities: NetVolume, NetSideArea, Length...), adunate pe grupuri.
use crate::app::TreeLevel;
use crate::model::{ElemId, Element, Model};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum QtyColumn {
    Count,
    /// volumul inchis de mesh (m³)
    Volume,
    /// suma ariilor tuturor fetelor (m²)
    Surface,
    /// aria fetelor orientate in sus, proiectata in plan (m²) - placi, pardoseli, acoperisuri
    PlanArea,
    /// cea mai mare dimensiune orizontala a cutiei (m) - pereti, grinzi, conducte
    Length,
    /// inaltimea cutiei (m)
    Height,
    /// suma unei proprietati numerice (ex. Qto_WallBaseQuantities / NetVolume)
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

/// Configuratia unei cantitati: refolosibila ca sablon (.bimqto) si salvata in proiect.
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

// ====================================================================== geometrie

/// Cantitatile geometrice ale unui element (in metri, coordonatele modelului).
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
    // volum prin tetraedre cu varful in centrul elementului (stabil numeric fata de originea scenei)
    let c = e.aabb.center().as_dvec3();
    for tri in t.chunks_exact(3) {
        let (a, b, d) = (tri[0].as_dvec3() - c, tri[1].as_dvec3() - c, tri[2].as_dvec3() - c);
        vol += a.dot(b.cross(d)) / 6.0;
        let n = (b - a).cross(d - a);
        surf += n.length() * 0.5;
        // fata orientata in sus (normala cu z > 0.5): aria proiectata in plan
        let len = n.length();
        if len > 0.0 && n.z / len > 0.5 {
            plan += n.z * 0.5;
        }
    }
    let s = e.aabb.size();
    GeoQty { volume: vol.abs(), surface: surf, plan, length: s.x.max(s.y) as f64, height: s.z as f64 }
}

// ====================================================================== rezultat

pub struct QtoRow {
    /// valorile de grupare (cate una pentru fiecare nivel din group_by)
    pub keys: Vec<String>,
    pub ids: Vec<ElemId>,
    /// valorile fiecarui element (paralel cu ids) - pentru foaia detaliata din Excel
    pub item_values: Vec<Vec<Option<f64>>>,
    /// suma pe coloana; None = niciun element din grup nu are valoarea
    pub values: Vec<Option<f64>>,
    /// cate elemente din grup nu au valoarea coloanei (proprietate lipsa)
    pub missing: Vec<u32>,
}

pub struct QtoResult {
    pub setup: QtoSetup,
    pub rows: Vec<QtoRow>,
    pub totals: Vec<Option<f64>>,
    pub missing: Vec<u32>,
    pub count: usize,
    /// unitatea pentru coloanele de tip proprietate (din prima valoare gasita, ex. "m³")
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

    /// Rezumat pe grupuri (BOM UTF-8, separator ';' - se deschide direct in Excel).
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

/// numar pentru CSV: cu virgula zecimala (Excel in romana / europa) - fara separator de mii
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
    /// rand TOTAL cu formule SUM pe coloanele numerice incepand cu aceasta coloana
    pub total_from: Option<usize>,
}

/// Fisier .xlsx cu mai multe foi: titlu, antet albastru, filtre, rand de total cu formule SUM.
pub fn xlsx(sheets: &[Sheet]) -> Vec<u8> {
    use crate::markup::{col_name, esc, Zip};
    let mut zip = Zip::default();
    let mut ct = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\"><Default Extension=\"rels\" ContentType=\"application/vnd.openxmlformats-package.relationships+xml\"/><Default Extension=\"xml\" ContentType=\"application/xml\"/><Override PartName=\"/xl/workbook.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml\"/><Override PartName=\"/xl/styles.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.styles+xml\"/>");
    let mut wb = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><workbook xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\" xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\"><sheets>");
    let mut rels = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">");
    for (k, sh) in sheets.iter().enumerate() {
        let n = k + 1;
        // numele foii: max 31 caractere, fara []:*?/\
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
    // stiluri: 0 normal, 1 antet, 2 text cu chenar, 3 titlu, 4 TOTAL text, 5 numar 0.000, 6 TOTAL numar
    zip.add("xl/styles.xml", b"<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><styleSheet xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\"><numFmts count=\"1\"><numFmt numFmtId=\"164\" formatCode=\"#,##0.000\"/></numFmts><fonts count=\"4\"><font><sz val=\"10\"/><name val=\"Calibri\"/></font><font><b/><sz val=\"10\"/><color rgb=\"FFFFFFFF\"/><name val=\"Calibri\"/></font><font><b/><sz val=\"14\"/><color rgb=\"FF1E3A8A\"/><name val=\"Calibri\"/></font><font><b/><sz val=\"10\"/><name val=\"Calibri\"/></font></fonts><fills count=\"4\"><fill><patternFill patternType=\"none\"/></fill><fill><patternFill patternType=\"gray125\"/></fill><fill><patternFill patternType=\"solid\"><fgColor rgb=\"FF3B82F6\"/><bgColor indexed=\"64\"/></patternFill></fill><fill><patternFill patternType=\"solid\"><fgColor rgb=\"FFE2E8F0\"/><bgColor indexed=\"64\"/></patternFill></fill></fills><borders count=\"2\"><border><left/><right/><top/><bottom/><diagonal/></border><border><left style=\"thin\"><color rgb=\"FFCBD5E1\"/></left><right style=\"thin\"><color rgb=\"FFCBD5E1\"/></right><top style=\"thin\"><color rgb=\"FFCBD5E1\"/></top><bottom style=\"thin\"><color rgb=\"FFCBD5E1\"/></bottom><diagonal/></border></borders><cellStyleXfs count=\"1\"><xf numFmtId=\"0\" fontId=\"0\" fillId=\"0\" borderId=\"0\"/></cellStyleXfs><cellXfs count=\"7\"><xf numFmtId=\"0\" fontId=\"0\" fillId=\"0\" borderId=\"0\" xfId=\"0\"/><xf numFmtId=\"0\" fontId=\"1\" fillId=\"2\" borderId=\"1\" xfId=\"0\" applyFont=\"1\" applyFill=\"1\" applyBorder=\"1\"/><xf numFmtId=\"0\" fontId=\"0\" fillId=\"0\" borderId=\"1\" xfId=\"0\" applyBorder=\"1\"/><xf numFmtId=\"0\" fontId=\"2\" fillId=\"0\" borderId=\"0\" xfId=\"0\" applyFont=\"1\"/><xf numFmtId=\"0\" fontId=\"3\" fillId=\"3\" borderId=\"1\" xfId=\"0\" applyFont=\"1\" applyFill=\"1\" applyBorder=\"1\"/><xf numFmtId=\"164\" fontId=\"0\" fillId=\"0\" borderId=\"1\" xfId=\"0\" applyNumberFormat=\"1\" applyBorder=\"1\"/><xf numFmtId=\"164\" fontId=\"3\" fillId=\"3\" borderId=\"1\" xfId=\"0\" applyNumberFormat=\"1\" applyFont=\"1\" applyFill=\"1\" applyBorder=\"1\"/></cellXfs><cellStyles count=\"1\"><cellStyle name=\"Normal\" xfId=\"0\" builtinId=\"0\"/></cellStyles></styleSheet>");
    zip.finish()
}

impl QtoResult {
    /// Excel: foaia "Summary" (un rand pe grup + TOTAL) si foaia "Items" (un rand pe element).
    /// `info(id)` -> (model, nume, tip IFC, nivel, GlobalId) pentru foaia detaliata.
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
