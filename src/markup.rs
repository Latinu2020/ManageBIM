//! Redline markups on viewpoints + issue data, and report exports (HTML, Excel .xlsx, BCF 2.1).
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum RedKind {
    Freehand,
    Line,
    Arrow,
    Rect,
    Ellipse,
    Cloud,
    Text,
}

impl RedKind {
    pub const ALL: [RedKind; 7] = [RedKind::Freehand, RedKind::Line, RedKind::Arrow, RedKind::Cloud, RedKind::Rect, RedKind::Ellipse, RedKind::Text];
    pub fn label(self) -> &'static str {
        match self {
            RedKind::Freehand => "Draw",
            RedKind::Line => "Line",
            RedKind::Arrow => "Arrow",
            RedKind::Rect => "Rectangle",
            RedKind::Ellipse => "Ellipse",
            RedKind::Cloud => "Cloud",
            RedKind::Text => "Text",
        }
    }
    pub fn icon(self) -> &'static str {
        match self {
            RedKind::Freehand => "✏",
            RedKind::Line => "╱",
            RedKind::Arrow => "↗",
            RedKind::Rect => "▭",
            RedKind::Ellipse => "⭕",
            RedKind::Cloud => "☁",
            RedKind::Text => "T",
        }
    }
}

/// Points are in viewport units: (pixel - viewport centre) / viewport height,
/// so redlines keep their place when the window is resized.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Redline {
    pub kind: RedKind,
    pub pts: Vec<[f32; 2]>,
    pub color: [u8; 3],
    pub width: f32,
    #[serde(default)]
    pub text: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct VpComment {
    pub author: String,
    pub date: String,
    pub text: String,
}

pub const STATUSES: [&str; 4] = ["Open", "In Progress", "Resolved", "Closed"];
pub const PRIORITIES: [&str; 4] = ["Low", "Normal", "High", "Critical"];

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Issue {
    pub guid: String,
    pub status: String,
    pub priority: String,
    pub assigned: String,
    pub author: String,
    pub created: String,
}

impl Default for Issue {
    fn default() -> Self {
        Issue { guid: new_guid(), status: "Open".into(), priority: "Normal".into(), assigned: String::new(), author: user_name(), created: now_iso() }
    }
}

pub fn user_name() -> String {
    std::env::var("USERNAME").or_else(|_| std::env::var("USER")).unwrap_or_else(|_| "user".into())
}

/// RFC 4122 v4 style GUID (random enough for BCF topic ids)
pub fn new_guid() -> String {
    use std::hash::{BuildHasher, Hasher};
    let mut b = [0u8; 16];
    for (i, c) in b.chunks_mut(8).enumerate() {
        let mut h = std::collections::hash_map::RandomState::new().build_hasher();
        h.write_u128(std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0));
        h.write_usize(i);
        c.copy_from_slice(&h.finish().to_le_bytes());
    }
    b[6] = (b[6] & 0x0f) | 0x40;
    b[8] = (b[8] & 0x3f) | 0x80;
    let x: Vec<String> = b.iter().map(|v| format!("{v:02x}")).collect();
    format!("{}-{}-{}-{}-{}", x[0..4].concat(), x[4..6].concat(), x[6..8].concat(), x[8..10].concat(), x[10..16].concat())
}

pub fn now_iso() -> String {
    let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0);
    let days = secs.div_euclid(86400);
    let tod = secs.rem_euclid(86400);
    // civil from days (Howard Hinnant)
    let z = days + 719468;
    let era = z.div_euclid(146097);
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!("{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z", tod / 3600, tod % 3600 / 60, tod % 60)
}

pub fn pretty_date(iso: &str) -> String {
    iso.replace('T', " ").trim_end_matches('Z').chars().take(16).collect()
}

// ---------------------------------------------------------------------------- report data
pub struct ReportItem {
    pub index: usize,
    pub name: String,
    pub description: String,
    pub issue: Issue,
    pub comments: Vec<VpComment>,
    pub redlines: usize,
    /// world coordinates
    pub eye: [f64; 3],
    pub dir: [f64; 3],
    pub up: [f64; 3],
    pub fov_deg: f32,
    pub ortho_height: Option<f64>,
    pub selected_guids: Vec<String>,
    pub hidden_guids: Vec<String>,
    /// PNG
    pub image: Vec<u8>,
    pub image_w: u32,
    pub image_h: u32,
}

pub fn esc(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

// ---------------------------------------------------------------------------- zip
fn crc32(data: &[u8]) -> u32 {
    let mut table = [0u32; 256];
    for i in 0..256u32 {
        let mut c = i;
        for _ in 0..8 {
            c = if c & 1 != 0 { 0xEDB88320 ^ (c >> 1) } else { c >> 1 };
        }
        table[i as usize] = c;
    }
    let mut c = 0xFFFF_FFFFu32;
    for &b in data {
        c = table[((c ^ b as u32) & 0xff) as usize] ^ (c >> 8);
    }
    c ^ 0xFFFF_FFFF
}

#[derive(Default)]
pub struct Zip {
    out: Vec<u8>,
    central: Vec<u8>,
    n: u16,
}

impl Zip {
    pub fn add(&mut self, name: &str, data: &[u8]) {
        let crc = crc32(data);
        let comp = miniz_oxide::deflate::compress_to_vec(data, 6);
        let (method, body): (u16, &[u8]) = if comp.len() < data.len() { (8, &comp) } else { (0, data) };
        let off = self.out.len() as u32;
        let nb = name.as_bytes();
        let le16 = |v: u16| v.to_le_bytes();
        let le32 = |v: u32| v.to_le_bytes();
        // local header
        self.out.extend_from_slice(&le32(0x04034b50));
        self.out.extend_from_slice(&le16(20));
        self.out.extend_from_slice(&le16(0x0800)); // UTF-8 names
        self.out.extend_from_slice(&le16(method));
        self.out.extend_from_slice(&le16(0));
        self.out.extend_from_slice(&le16(0x5821));
        self.out.extend_from_slice(&le32(crc));
        self.out.extend_from_slice(&le32(body.len() as u32));
        self.out.extend_from_slice(&le32(data.len() as u32));
        self.out.extend_from_slice(&le16(nb.len() as u16));
        self.out.extend_from_slice(&le16(0));
        self.out.extend_from_slice(nb);
        self.out.extend_from_slice(body);
        // central directory
        let c = &mut self.central;
        c.extend_from_slice(&le32(0x02014b50));
        c.extend_from_slice(&le16(20));
        c.extend_from_slice(&le16(20));
        c.extend_from_slice(&le16(0x0800));
        c.extend_from_slice(&le16(method));
        c.extend_from_slice(&le16(0));
        c.extend_from_slice(&le16(0x5821));
        c.extend_from_slice(&le32(crc));
        c.extend_from_slice(&le32(body.len() as u32));
        c.extend_from_slice(&le32(data.len() as u32));
        c.extend_from_slice(&le16(nb.len() as u16));
        c.extend_from_slice(&[0; 12]);
        c.extend_from_slice(&le32(off));
        c.extend_from_slice(nb);
        self.n += 1;
    }
    pub fn finish(mut self) -> Vec<u8> {
        let cd_off = self.out.len() as u32;
        let cd_len = self.central.len() as u32;
        self.out.extend_from_slice(&self.central);
        self.out.extend_from_slice(&0x06054b50u32.to_le_bytes());
        self.out.extend_from_slice(&[0; 4]);
        self.out.extend_from_slice(&self.n.to_le_bytes());
        self.out.extend_from_slice(&self.n.to_le_bytes());
        self.out.extend_from_slice(&cd_len.to_le_bytes());
        self.out.extend_from_slice(&cd_off.to_le_bytes());
        self.out.extend_from_slice(&[0; 2]);
        self.out
    }
}

// ---------------------------------------------------------------------------- HTML
pub fn html_report(project: &str, items: &[ReportItem]) -> String {
    use base64::Engine;
    let mut h = String::new();
    h.push_str(&format!(
        "<!DOCTYPE html><html><head><meta charset=\"utf-8\"><title>{0} - Viewpoints & Markups</title><style>\
body{{font-family:Segoe UI,Arial,sans-serif;margin:24px;color:#1e293b;background:#f8fafc}}h1{{margin:0 0 4px}}\
.sub{{color:#64748b;margin-bottom:18px}}table{{border-collapse:collapse;width:100%;margin-bottom:28px;background:#fff}}\
th,td{{border:1px solid #e2e8f0;padding:6px 8px;text-align:left;font-size:13px;vertical-align:top}}th{{background:#3b82f6;color:#fff}}\
.card{{background:#fff;border:1px solid #e2e8f0;border-radius:10px;padding:16px;margin-bottom:22px;page-break-inside:avoid}}\
.card img{{max-width:100%;border-radius:6px;border:1px solid #cbd5e1}}.badge{{display:inline-block;padding:2px 8px;border-radius:10px;color:#fff;font-size:12px}}\
.Open{{background:#ef4444}}.In.Progress,.InProgress{{background:#f59e0b}}.Resolved{{background:#22c55e}}.Closed{{background:#64748b}}\
.meta{{color:#475569;font-size:13px;margin:6px 0}}.c{{border-left:3px solid #3b82f6;padding:4px 10px;margin:6px 0;background:#f1f5f9;font-size:13px}}\
</style></head><body><h1>{0}</h1><div class=\"sub\">Viewpoints &amp; markups report · {1} · {2} viewpoints · generated by ManageBIM</div>",
        esc(project),
        pretty_date(&now_iso()),
        items.len()
    ));
    h.push_str("<table><tr><th>#</th><th>Name</th><th>Status</th><th>Priority</th><th>Assigned to</th><th>Author</th><th>Created</th><th>Redlines</th><th>Comments</th></tr>");
    for it in items {
        h.push_str(&format!(
            "<tr><td>{}</td><td><a href=\"#vp{}\">{}</a></td><td><span class=\"badge {}\">{}</span></td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td></tr>",
            it.index,
            it.index,
            esc(&it.name),
            it.issue.status.replace(' ', ""),
            esc(&it.issue.status),
            esc(&it.issue.priority),
            esc(&it.issue.assigned),
            esc(&it.issue.author),
            pretty_date(&it.issue.created),
            it.redlines,
            it.comments.len()
        ));
    }
    h.push_str("</table>");
    for it in items {
        h.push_str(&format!("<div class=\"card\" id=\"vp{}\"><h2>{}. {} <span class=\"badge {}\">{}</span></h2>", it.index, it.index, esc(&it.name), it.issue.status.replace(' ', ""), esc(&it.issue.status)));
        h.push_str(&format!(
            "<div class=\"meta\">Priority: <b>{}</b> · Assigned to: <b>{}</b> · Author: {} · Created: {} · Camera: {:.2}, {:.2}, {:.2}</div>",
            esc(&it.issue.priority),
            esc(if it.issue.assigned.is_empty() { "-" } else { &it.issue.assigned }),
            esc(&it.issue.author),
            pretty_date(&it.issue.created),
            it.eye[0],
            it.eye[1],
            it.eye[2]
        ));
        if !it.description.is_empty() {
            h.push_str(&format!("<p>{}</p>", esc(&it.description).replace('\n', "<br>")));
        }
        if !it.image.is_empty() {
            h.push_str(&format!("<img src=\"data:image/png;base64,{}\">", base64::engine::general_purpose::STANDARD.encode(&it.image)));
        }
        for c in &it.comments {
            h.push_str(&format!("<div class=\"c\"><b>{}</b> · {}<br>{}</div>", esc(&c.author), pretty_date(&c.date), esc(&c.text).replace('\n', "<br>")));
        }
        h.push_str("</div>");
    }
    h.push_str("</body></html>");
    h
}

// ---------------------------------------------------------------------------- Excel
pub fn col_name(c: usize) -> String {
    let mut s = String::new();
    let mut n = c + 1;
    while n > 0 {
        let r = (n - 1) % 26;
        s.insert(0, (b'A' + r as u8) as char);
        n = (n - 1) / 26;
    }
    s
}

/// .xlsx with one row per viewpoint and the (thumbnail) snapshot embedded in column B.
pub fn xlsx_report(project: &str, items: &[ReportItem], thumbs: &[(Vec<u8>, u32, u32)]) -> Vec<u8> {
    let headers = ["#", "Snapshot", "Viewpoint", "Status", "Priority", "Assigned to", "Author", "Created", "Description", "Comments", "Redlines", "Camera X", "Camera Y", "Camera Z"];
    let widths = [5.0, 36.0, 26.0, 12.0, 10.0, 16.0, 14.0, 17.0, 45.0, 50.0, 9.0, 12.0, 12.0, 12.0];
    let mut sheet = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><worksheet xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\" xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\"><sheetViews><sheetView workbookViewId=\"0\"><pane ySplit=\"2\" topLeftCell=\"A3\" activePane=\"bottomLeft\" state=\"frozen\"/></sheetView></sheetViews><cols>");
    for (i, w) in widths.iter().enumerate() {
        sheet.push_str(&format!("<col min=\"{0}\" max=\"{0}\" width=\"{1}\" customWidth=\"1\"/>", i + 1, w));
    }
    sheet.push_str("</cols><sheetData>");
    let cell_s = |r: usize, c: usize, v: &str, style: u32| format!("<c r=\"{}{}\" t=\"inlineStr\" s=\"{}\"><is><t xml:space=\"preserve\">{}</t></is></c>", col_name(c), r, style, esc(v));
    let cell_n = |r: usize, c: usize, v: f64, style: u32| format!("<c r=\"{}{}\" s=\"{}\"><v>{}</v></c>", col_name(c), r, style, v);
    sheet.push_str(&format!("<row r=\"1\" ht=\"24\" customHeight=\"1\">{}</row>", cell_s(1, 0, &format!("{project} - Viewpoints & Markups ({})", pretty_date(&now_iso())), 3)));
    sheet.push_str("<row r=\"2\">");
    for (i, hname) in headers.iter().enumerate() {
        sheet.push_str(&cell_s(2, i, hname, 1));
    }
    sheet.push_str("</row>");
    for (k, it) in items.iter().enumerate() {
        let r = k + 3;
        let th = thumbs.get(k).map(|t| t.2).unwrap_or(0);
        let ht = if th > 0 { (th as f64 + 8.0) * 0.75 } else { 18.0 };
        let comments: Vec<String> = it.comments.iter().map(|c| format!("{} ({}): {}", c.author, pretty_date(&c.date), c.text)).collect();
        sheet.push_str(&format!("<row r=\"{r}\" ht=\"{ht:.1}\" customHeight=\"1\">"));
        sheet.push_str(&cell_n(r, 0, it.index as f64, 2));
        sheet.push_str(&cell_s(r, 1, "", 2));
        sheet.push_str(&cell_s(r, 2, &it.name, 2));
        sheet.push_str(&cell_s(r, 3, &it.issue.status, 2));
        sheet.push_str(&cell_s(r, 4, &it.issue.priority, 2));
        sheet.push_str(&cell_s(r, 5, &it.issue.assigned, 2));
        sheet.push_str(&cell_s(r, 6, &it.issue.author, 2));
        sheet.push_str(&cell_s(r, 7, &pretty_date(&it.issue.created), 2));
        sheet.push_str(&cell_s(r, 8, &it.description, 2));
        sheet.push_str(&cell_s(r, 9, &comments.join("\n"), 2));
        sheet.push_str(&cell_n(r, 10, it.redlines as f64, 2));
        for a in 0..3 {
            sheet.push_str(&cell_n(r, 11 + a, (it.eye[a] * 1000.0).round() / 1000.0, 2));
        }
        sheet.push_str("</row>");
    }
    sheet.push_str(&format!("</sheetData><autoFilter ref=\"A2:{}{}\"/>", col_name(headers.len() - 1), items.len() + 2));
    let has_img = thumbs.iter().any(|t| !t.0.is_empty());
    if has_img {
        sheet.push_str("<drawing r:id=\"rId1\"/>");
    }
    sheet.push_str("</worksheet>");

    let mut drawing = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><xdr:wsDr xmlns:xdr=\"http://schemas.openxmlformats.org/drawingml/2006/spreadsheetDrawing\" xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\" xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\">");
    let mut drels = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">");
    let mut zip = Zip::default();
    let mut id = 0;
    for (k, t) in thumbs.iter().enumerate() {
        if t.0.is_empty() {
            continue;
        }
        id += 1;
        zip.add(&format!("xl/media/image{id}.png"), &t.0);
        drels.push_str(&format!("<Relationship Id=\"rId{id}\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/image\" Target=\"../media/image{id}.png\"/>"));
        let (cx, cy) = (t.1 as u64 * 9525, t.2 as u64 * 9525);
        drawing.push_str(&format!(
            "<xdr:oneCellAnchor><xdr:from><xdr:col>1</xdr:col><xdr:colOff>38100</xdr:colOff><xdr:row>{}</xdr:row><xdr:rowOff>38100</xdr:rowOff></xdr:from><xdr:ext cx=\"{cx}\" cy=\"{cy}\"/>\
<xdr:pic><xdr:nvPicPr><xdr:cNvPr id=\"{}\" name=\"Snapshot {}\"/><xdr:cNvPicPr><a:picLocks noChangeAspect=\"1\"/></xdr:cNvPicPr></xdr:nvPicPr>\
<xdr:blipFill><a:blip r:embed=\"rId{id}\"/><a:stretch><a:fillRect/></a:stretch></xdr:blipFill>\
<xdr:spPr><a:xfrm><a:off x=\"0\" y=\"0\"/><a:ext cx=\"{cx}\" cy=\"{cy}\"/></a:xfrm><a:prstGeom prst=\"rect\"><a:avLst/></a:prstGeom></xdr:spPr></xdr:pic><xdr:clientData/></xdr:oneCellAnchor>",
            k + 2,
            id + 1,
            k + 1
        ));
    }
    drawing.push_str("</xdr:wsDr>");
    drels.push_str("</Relationships>");

    zip.add("[Content_Types].xml", format!("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\"><Default Extension=\"rels\" ContentType=\"application/vnd.openxmlformats-package.relationships+xml\"/><Default Extension=\"xml\" ContentType=\"application/xml\"/><Default Extension=\"png\" ContentType=\"image/png\"/><Override PartName=\"/xl/workbook.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml\"/><Override PartName=\"/xl/worksheets/sheet1.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml\"/><Override PartName=\"/xl/styles.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.styles+xml\"/>{}</Types>", if has_img { "<Override PartName=\"/xl/drawings/drawing1.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.drawing+xml\"/>" } else { "" }).as_bytes());
    zip.add("_rels/.rels", b"<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\"><Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument\" Target=\"xl/workbook.xml\"/></Relationships>");
    zip.add("xl/workbook.xml", b"<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><workbook xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\" xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\"><sheets><sheet name=\"Viewpoints\" sheetId=\"1\" r:id=\"rId1\"/></sheets><definedNames><definedName name=\"_xlnm._FilterDatabase\" localSheetId=\"0\" hidden=\"1\">Viewpoints!$A$2:$N$2</definedName></definedNames></workbook>");
    zip.add("xl/_rels/workbook.xml.rels", b"<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\"><Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet\" Target=\"worksheets/sheet1.xml\"/><Relationship Id=\"rId2\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles\" Target=\"styles.xml\"/></Relationships>");
    zip.add("xl/styles.xml", b"<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><styleSheet xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\"><fonts count=\"3\"><font><sz val=\"10\"/><name val=\"Calibri\"/></font><font><b/><sz val=\"10\"/><color rgb=\"FFFFFFFF\"/><name val=\"Calibri\"/></font><font><b/><sz val=\"14\"/><color rgb=\"FF1E3A8A\"/><name val=\"Calibri\"/></font></fonts><fills count=\"3\"><fill><patternFill patternType=\"none\"/></fill><fill><patternFill patternType=\"gray125\"/></fill><fill><patternFill patternType=\"solid\"><fgColor rgb=\"FF3B82F6\"/><bgColor indexed=\"64\"/></patternFill></fill></fills><borders count=\"2\"><border><left/><right/><top/><bottom/><diagonal/></border><border><left style=\"thin\"><color rgb=\"FFCBD5E1\"/></left><right style=\"thin\"><color rgb=\"FFCBD5E1\"/></right><top style=\"thin\"><color rgb=\"FFCBD5E1\"/></top><bottom style=\"thin\"><color rgb=\"FFCBD5E1\"/></bottom><diagonal/></border></borders><cellStyleXfs count=\"1\"><xf numFmtId=\"0\" fontId=\"0\" fillId=\"0\" borderId=\"0\"/></cellStyleXfs><cellXfs count=\"4\"><xf numFmtId=\"0\" fontId=\"0\" fillId=\"0\" borderId=\"0\" xfId=\"0\"/><xf numFmtId=\"0\" fontId=\"1\" fillId=\"2\" borderId=\"1\" xfId=\"0\" applyFont=\"1\" applyFill=\"1\" applyBorder=\"1\"/><xf numFmtId=\"0\" fontId=\"0\" fillId=\"0\" borderId=\"1\" xfId=\"0\" applyBorder=\"1\" applyAlignment=\"1\"><alignment vertical=\"top\" wrapText=\"1\"/></xf><xf numFmtId=\"0\" fontId=\"2\" fillId=\"0\" borderId=\"0\" xfId=\"0\" applyFont=\"1\"/></cellXfs><cellStyles count=\"1\"><cellStyle name=\"Normal\" xfId=\"0\" builtinId=\"0\"/></cellStyles></styleSheet>");
    zip.add("xl/worksheets/sheet1.xml", sheet.as_bytes());
    if has_img {
        zip.add("xl/worksheets/_rels/sheet1.xml.rels", b"<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\"><Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/drawing\" Target=\"../drawings/drawing1.xml\"/></Relationships>");
        zip.add("xl/drawings/drawing1.xml", drawing.as_bytes());
        zip.add("xl/drawings/_rels/drawing1.xml.rels", drels.as_bytes());
    }
    zip.finish()
}

// ---------------------------------------------------------------------------- BCF 2.1
fn xyz(tag: &str, v: [f64; 3]) -> String {
    format!("<{tag}><X>{:.6}</X><Y>{:.6}</Y><Z>{:.6}</Z></{tag}>", v[0], v[1], v[2])
}

/// Componenta BCF: GUID IFC (22 caractere) -> IfcGuid; altfel (Revit UniqueId, Navisworks, DWG handle) -> AuthoringToolId.
fn bcf_component(g: &str) -> String {
    if g.len() == 22 && !g.contains(':') {
        format!("<Component IfcGuid=\"{}\"/>", esc(g))
    } else {
        format!("<Component AuthoringToolId=\"{}\"/>", esc(g))
    }
}

pub fn bcf_zip(project: &str, items: &[ReportItem]) -> Vec<u8> {
    bcf_zip_version(project, items, 2)
}

/// Export BCF 2.1 sau 3.0. In 3.0 comentariile stau in interiorul `<Topic>`.
pub fn bcf_zip_version(project: &str, items: &[ReportItem], version: u8) -> Vec<u8> {
    let ver = if version == 3 { "3.0" } else { "2.1" };
    let mut z = Zip::default();
    z.add("bcf.version", format!("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<Version VersionId=\"{ver}\" xmlns:xsi=\"http://www.w3.org/2001/XMLSchema-instance\"><DetailedVersion>{ver}</DetailedVersion></Version>").as_bytes());
    z.add("project.bcfp", format!("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<ProjectExtension xmlns:xsi=\"http://www.w3.org/2001/XMLSchema-instance\"><Project ProjectId=\"{}\"><Name>{}</Name></Project><ExtensionSchema></ExtensionSchema></ProjectExtension>", new_guid(), esc(project)).as_bytes());
    for it in items {
        let tg = if it.issue.guid.len() == 36 { it.issue.guid.clone() } else { new_guid() };
        let vg = new_guid();
        let mut m = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<Markup xmlns:xsi=\"http://www.w3.org/2001/XMLSchema-instance\">");
        m.push_str(&format!("<Topic Guid=\"{tg}\" TopicType=\"Issue\" TopicStatus=\"{}\">", esc(&it.issue.status)));
        m.push_str(&format!("<Title>{}</Title><Priority>{}</Priority><Index>{}</Index>", esc(&it.name), esc(&it.issue.priority), it.index));
        m.push_str(&format!("<CreationDate>{}</CreationDate><CreationAuthor>{}</CreationAuthor>", esc(&it.issue.created), esc(&it.issue.author)));
        if !it.issue.assigned.is_empty() {
            m.push_str(&format!("<AssignedTo>{}</AssignedTo>", esc(&it.issue.assigned)));
        }
        if !it.description.is_empty() {
            m.push_str(&format!("<Description>{}</Description>", esc(&it.description)));
        }
        let comments: String = it.comments.iter().map(|c| format!(
            "<Comment Guid=\"{}\"><Date>{}</Date><Author>{}</Author><Comment>{}</Comment><Viewpoint Guid=\"{vg}\"/></Comment>",
            new_guid(), esc(&c.date), esc(&c.author), esc(&c.text)
        )).collect();
        if version == 3 && !comments.is_empty() {
            m.push_str(&format!("<Comments>{comments}</Comments>"));
        }
        m.push_str("</Topic>");
        if version != 3 {
            m.push_str(&comments);
        }
        let snap = if it.image.is_empty() { "" } else { "<Snapshot>snapshot.png</Snapshot>" };
        m.push_str(&format!("<Viewpoints Guid=\"{vg}\"><Viewpoint>viewpoint.bcfv</Viewpoint>{snap}</Viewpoints></Markup>"));
        let mut v = format!("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<VisualizationInfo Guid=\"{vg}\" xmlns:xsi=\"http://www.w3.org/2001/XMLSchema-instance\"><Components>");
        if !it.selected_guids.is_empty() {
            v.push_str("<Selection>");
            for g in &it.selected_guids {
                v.push_str(&bcf_component(g));
            }
            v.push_str("</Selection>");
        }
        v.push_str("<Visibility DefaultVisibility=\"true\">");
        if !it.hidden_guids.is_empty() {
            v.push_str("<Exceptions>");
            for g in &it.hidden_guids {
                v.push_str(&bcf_component(g));
            }
            v.push_str("</Exceptions>");
        }
        v.push_str("</Visibility></Components>");
        match it.ortho_height {
            Some(hgt) => v.push_str(&format!(
                "<OrthogonalCamera>{}{}{}<ViewToWorldScale>{:.6}</ViewToWorldScale></OrthogonalCamera>",
                xyz("CameraViewPoint", it.eye),
                xyz("CameraDirection", it.dir),
                xyz("CameraUpVector", it.up),
                hgt
            )),
            None => v.push_str(&format!(
                "<PerspectiveCamera>{}{}{}<FieldOfView>{:.3}</FieldOfView></PerspectiveCamera>",
                xyz("CameraViewPoint", it.eye),
                xyz("CameraDirection", it.dir),
                xyz("CameraUpVector", it.up),
                it.fov_deg
            )),
        }
        v.push_str("</VisualizationInfo>");
        z.add(&format!("{tg}/markup.bcf"), m.as_bytes());
        z.add(&format!("{tg}/viewpoint.bcfv"), v.as_bytes());
        if !it.image.is_empty() {
            z.add(&format!("{tg}/snapshot.png"), &it.image);
        }
    }
    z.finish()
}

pub fn encode_png(rgb: &[u8], w: u32, h: u32) -> Vec<u8> {
    let mut out = Vec::new();
    {
        let mut e = png::Encoder::new(&mut out, w, h);
        e.set_color(png::ColorType::Rgb);
        e.set_depth(png::BitDepth::Eight);
        if let Ok(mut wr) = e.write_header() {
            let _ = wr.write_image_data(rgb);
        }
    }
    out
}

/// box-filter downscale of an RGB image to fit `max_w`
pub fn downscale_rgb(rgb: &[u8], w: u32, h: u32, max_w: u32) -> (Vec<u8>, u32, u32) {
    if w <= max_w {
        return (rgb.to_vec(), w, h);
    }
    let nw = max_w;
    let nh = ((h as f64) * nw as f64 / w as f64).round().max(1.0) as u32;
    let mut out = vec![0u8; (nw * nh * 3) as usize];
    for y in 0..nh {
        let y0 = (y as u64 * h as u64 / nh as u64) as u32;
        let y1 = (((y + 1) as u64 * h as u64 / nh as u64) as u32).max(y0 + 1).min(h);
        for x in 0..nw {
            let x0 = (x as u64 * w as u64 / nw as u64) as u32;
            let x1 = (((x + 1) as u64 * w as u64 / nw as u64) as u32).max(x0 + 1).min(w);
            let mut acc = [0u32; 3];
            let mut n = 0;
            for yy in y0..y1 {
                for xx in x0..x1 {
                    let i = ((yy * w + xx) * 3) as usize;
                    acc[0] += rgb[i] as u32;
                    acc[1] += rgb[i + 1] as u32;
                    acc[2] += rgb[i + 2] as u32;
                    n += 1;
                }
            }
            let o = ((y * nw + x) * 3) as usize;
            for c in 0..3 {
                out[o + c] = (acc[c] / n.max(1)) as u8;
            }
        }
    }
    (out, nw, nh)
}

// ---------------------------------------------------------------------------- Navisworks viewpoints XML
// Format "exchange" al Navisworks (schemas\nw-exchange-12.0.xsd din instalarea Navisworks):
// Viewpoints > (click dreapta) Import Viewpoints... Camera, sectiunea (box), comentariile si redline-urile.

/// Un viewpoint pentru Navisworks, in coordonate WORLD (metri).
pub struct NwView {
    pub name: String,
    pub guid: String,
    pub eye: [f64; 3],
    pub fwd: [f64; 3],
    pub up: [f64; 3],
    pub fov_deg: f32,
    pub ortho_height: Option<f64>,
    pub aspect: f32,
    pub focal: f64,
    pub near: f64,
    pub far: f64,
    pub scene_min: [f64; 3],
    pub scene_max: [f64; 3],
    pub section_box: Option<([f64; 3], [f64; 3])>,
    /// (autor, text, data ISO, stare Navisworks: new / active / approved / resolved)
    pub comments: Vec<(String, String, String, &'static str)>,
    pub redlines: Vec<Redline>,
    /// folderul de Saved Viewpoints ("A/B" = subfolder); gol = radacina
    pub folder: String,
}

/// Stare ManageBIM -> stare comentariu Navisworks
pub fn nw_status(s: &str) -> &'static str {
    match s {
        "In Progress" => "active",
        "Resolved" => "resolved",
        "Closed" => "approved",
        _ => "new",
    }
}

/// Rotatia camerei Navisworks (camera priveste spre -Z, sus = +Y) ca quaternion (a, b, c, d) = (x, y, z, w).
pub fn nw_quaternion(fwd: [f64; 3], up: [f64; 3]) -> [f64; 4] {
    let n = |v: [f64; 3]| {
        let l = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt().max(1e-12);
        [v[0] / l, v[1] / l, v[2] / l]
    };
    let cross = |a: [f64; 3], b: [f64; 3]| [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]];
    let z = n([-fwd[0], -fwd[1], -fwd[2]]);
    let x = n(cross(up, z));
    let y = cross(z, x);
    // matricea cu coloanele x, y, z -> quaternion (Shepperd)
    let (m00, m11, m22) = (x[0], y[1], z[2]);
    let tr = m00 + m11 + m22;
    let (qx, qy, qz, qw);
    if tr > 0.0 {
        let s = (tr + 1.0).sqrt() * 2.0;
        qw = 0.25 * s;
        qx = (y[2] - z[1]) / s;
        qy = (z[0] - x[2]) / s;
        qz = (x[1] - y[0]) / s;
    } else if m00 > m11 && m00 > m22 {
        let s = (1.0 + m00 - m11 - m22).sqrt() * 2.0;
        qw = (y[2] - z[1]) / s;
        qx = 0.25 * s;
        qy = (y[0] + x[1]) / s;
        qz = (z[0] + x[2]) / s;
    } else if m11 > m22 {
        let s = (1.0 + m11 - m00 - m22).sqrt() * 2.0;
        qw = (z[0] - x[2]) / s;
        qx = (y[0] + x[1]) / s;
        qy = 0.25 * s;
        qz = (z[1] + y[2]) / s;
    } else {
        let s = (1.0 + m22 - m00 - m11).sqrt() * 2.0;
        qw = (x[1] - y[0]) / s;
        qx = (z[0] + x[2]) / s;
        qy = (z[1] + y[2]) / s;
        qz = 0.25 * s;
    }
    [qx, qy, qz, qw]
}

fn nw_date(iso: &str) -> String {
    let g = |a: usize, b: usize| iso.get(a..b).and_then(|s| s.parse::<u32>().ok()).unwrap_or(0);
    let (y, mo, d) = (g(0, 4).max(1), g(5, 7).max(1), g(8, 10).max(1));
    format!("<createddate><date year=\"{y}\" month=\"{mo}\" day=\"{d}\" hour=\"{}\" minute=\"{}\" second=\"{}\"/></createddate>", g(11, 13), g(14, 16), g(17, 19))
}

fn p3(tag: &str, p: [f64; 3]) -> String {
    format!("<{tag}><pos3f x=\"{:.6}\" y=\"{:.6}\" z=\"{:.6}\"/></{tag}>", p[0], p[1], p[2])
}

/// Redline (coordonate ManageBIM: fata de centrul vederii, impartite la inaltime, y in jos) ->
/// coordonate Navisworks (-1..1 pe latime / inaltime, y in sus).
fn nw_pt(p: [f32; 2], aspect: f32) -> String {
    let x = 2.0 * p[0] / aspect.max(0.01);
    let y = -2.0 * p[1];
    format!("<pos2f x=\"{:.5}\" y=\"{:.5}\"/>", x.clamp(-1.0, 1.0), y.clamp(-1.0, 1.0))
}

/// Markup-ul ca polilinii (coordonate ManageBIM: fata de centru / inaltime, y in jos), pentru add-in-urile
/// Revit / Navisworks: linie, sageata (cu varf), dreptunghi, elipsa, nor cu festoane, desen liber. Textul: nimic.
pub fn redline_polylines(r: &Redline) -> Vec<Vec<[f32; 2]>> {
    let (Some(&a), Some(&b)) = (r.pts.first(), r.pts.last()) else { return Vec::new() };
    match r.kind {
        RedKind::Line => vec![vec![a, b]],
        RedKind::Arrow => {
            let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
            let len = (dx * dx + dy * dy).sqrt().max(1e-6);
            let (ux, uy) = (dx / len, dy / len);
            let h = (len * 0.25).min(0.04);
            let w = h * 0.5;
            let l = [b[0] - ux * h - uy * w, b[1] - uy * h + ux * w];
            let rr = [b[0] - ux * h + uy * w, b[1] - uy * h - ux * w];
            vec![vec![a, b], vec![l, b, rr]]
        }
        RedKind::Rect => vec![vec![a, [b[0], a[1]], b, [a[0], b[1]], a]],
        RedKind::Ellipse => {
            let (cx, cy, rx, ry) = ((a[0] + b[0]) * 0.5, (a[1] + b[1]) * 0.5, (b[0] - a[0]).abs() * 0.5, (b[1] - a[1]).abs() * 0.5);
            vec![(0..=48).map(|k| {
                let t = k as f32 / 48.0 * std::f32::consts::TAU;
                [cx + rx * t.cos(), cy + ry * t.sin()]
            })
            .collect()]
        }
        RedKind::Cloud => {
            let (x0, x1, y0, y1) = (a[0].min(b[0]), a[0].max(b[0]), a[1].min(b[1]), a[1].max(b[1]));
            let corners = [[x0, y0], [x1, y0], [x1, y1], [x0, y1]];
            let bump = (0.03 + r.width * 0.003).max(0.02);
            let mut pts: Vec<[f32; 2]> = Vec::new();
            for k in 0..4 {
                let (p, q) = (corners[k], corners[(k + 1) % 4]);
                let (dx, dy) = (q[0] - p[0], q[1] - p[1]);
                let len = (dx * dx + dy * dy).sqrt();
                if len < 1e-6 {
                    continue;
                }
                let n = ((len / bump).round() as usize).max(1);
                let (sx, sy) = (dx / n as f32, dy / n as f32);
                let l = (sx * sx + sy * sy).sqrt();
                let (ox, oy) = (sy / l, -sx / l);
                let rad = l * 0.5;
                for j in 0..n {
                    let (cx, cy) = (p[0] + sx * (j as f32 + 0.5), p[1] + sy * (j as f32 + 0.5));
                    for s in 0..=6 {
                        let t = s as f32 / 6.0 * std::f32::consts::PI;
                        pts.push([cx - sx / l * rad * t.cos() + ox * rad * t.sin(), cy - sy / l * rad * t.cos() + oy * rad * t.sin()]);
                    }
                }
            }
            vec![pts]
        }
        RedKind::Freehand => {
            let step = (r.pts.len() / 400).max(1);
            vec![r.pts.iter().step_by(step).copied().chain(std::iter::once(b)).collect()]
        }
        RedKind::Text => Vec::new(),
    }
}

fn nw_redline(r: &Redline, aspect: f32) -> String {
    let col = format!("<colour red=\"{:.3}\" green=\"{:.3}\" blue=\"{:.3}\"/>", r.color[0] as f32 / 255.0, r.color[1] as f32 / 255.0, r.color[2] as f32 / 255.0);
    let th = (r.width.round() as u32).max(1);
    // pattern 65535 = linie continua (cum scrie Navisworks)
    let seg = |tag: &str, a: [f32; 2], b: [f32; 2]| format!("<{tag} thickness=\"{th}\" pattern=\"65535\">{col}<start>{}</start><end>{}</end></{tag}>", nw_pt(a, aspect), nw_pt(b, aspect));
    let (Some(&a), Some(&b)) = (r.pts.first(), r.pts.last()) else { return String::new() };
    match r.kind {
        RedKind::Line => seg("rlline", a, b),
        RedKind::Arrow => seg("rlarrow", a, b),
        RedKind::Ellipse => seg("rlellipse", a, b),
        RedKind::Rect => {
            let (c, d) = ([b[0], a[1]], [a[0], b[1]]);
            [seg("rlline", a, c), seg("rlline", c, b), seg("rlline", b, d), seg("rlline", d, a)].concat()
        }
        RedKind::Cloud => {
            // Navisworks deseneaza un arc intre fiecare doua puncte consecutive: conturul dreptunghiului
            // impartit in pasi mici (~3% din inaltimea ecranului), in sensul acelor de ceas pe ecran - ca norii
            // desenati in Navisworks (4 colturi ar da 4 bucle uriase)
            // conturul cu festoane (semicercuri spre exterior), exact ca norul desenat in ManageBIM, esantionat
            // des: arata la fel fie ca Navisworks uneste punctele cu linii, fie cu arce mici
            let (x0, x1, y0, y1) = (a[0].min(b[0]), a[0].max(b[0]), a[1].min(b[1]), a[1].max(b[1]));
            let corners = [[x0, y0], [x1, y0], [x1, y1], [x0, y1]];
            let bump = (0.03 + r.width * 0.003).max(0.02);
            let mut pts: Vec<[f32; 2]> = Vec::new();
            for k in 0..4 {
                let (p, q) = (corners[k], corners[(k + 1) % 4]);
                let (dx, dy) = (q[0] - p[0], q[1] - p[1]);
                let len = (dx * dx + dy * dy).sqrt();
                if len < 1e-6 {
                    continue;
                }
                let n = ((len / bump).round() as usize).max(1);
                let (sx, sy) = (dx / n as f32, dy / n as f32);
                let l = (sx * sx + sy * sy).sqrt();
                // spre exterior pe ecran (y in jos, colturi in sensul acelor de ceas)
                let (ox, oy) = (sy / l, -sx / l);
                let rad = l * 0.5;
                for j in 0..n {
                    let (cx, cy) = (p[0] + sx * (j as f32 + 0.5), p[1] + sy * (j as f32 + 0.5));
                    for s in 0..6 {
                        let t = s as f32 / 6.0 * std::f32::consts::PI;
                        pts.push([cx - sx / l * rad * t.cos() + ox * rad * t.sin(), cy - sy / l * rad * t.cos() + oy * rad * t.sin()]);
                    }
                }
            }
            pts.push(corners[0]);
            format!("<rlcloud thickness=\"{th}\" pattern=\"65535\">{col}<points>{}</points></rlcloud>", pts.iter().map(|p| nw_pt(*p, aspect)).collect::<String>())
        }
        RedKind::Freehand => {
            // fara polilinie in schema: segmente (max ~80)
            let step = (r.pts.len() / 80).max(1);
            let pts: Vec<[f32; 2]> = r.pts.iter().step_by(step).copied().chain(std::iter::once(b)).collect();
            pts.windows(2).map(|w| seg("rlline", w[0], w[1])).collect()
        }
        RedKind::Text => format!("<rltext thickness=\"{th}\" pattern=\"65535\">{col}<pos>{}</pos><text>{}</text></rltext>", nw_pt(a, aspect), esc(&r.text)),
    }
}

/// Unitati Navisworks (atributul `units` din XML) si metri pe unitate.
pub const NW_UNITS: [(&str, f64, &str); 7] = [("m", 1.0, "Meters"), ("cm", 0.01, "Centimeters"), ("mm", 0.001, "Millimeters"), ("ft", 0.3048, "Feet"), ("in", 0.0254, "Inches"), ("yrd", 0.9144, "Yards"), ("km", 1000.0, "Kilometers")];

/// Unitatea unui document Navisworks ("Inches", "Feet"...) -> codul din XML ("in", "ft"...).
pub fn nw_unit_code(navis_name: &str) -> Option<&'static str> {
    let n = navis_name.trim().to_lowercase();
    NW_UNITS.iter().find(|u| u.2.to_lowercase() == n || u.0 == n).map(|u| u.0)
}

/// `views` sunt in METRI; se scriu in unitatea `units` (cea a documentului Navisworks: Navisworks pune
/// pozitiile direct in unitatile documentului, deci asa ajung la locul lor indiferent cum trateaza atributul).
pub fn navis_viewpoints_xml(views: &[NwView], units: &str) -> String {
    let k = 1.0 / NW_UNITS.iter().find(|u| u.0 == units).map(|u| u.1).unwrap_or(1.0);
    let units = if NW_UNITS.iter().any(|u| u.0 == units) { units } else { "m" };
    let sc = |p: [f64; 3]| [p[0] * k, p[1] * k, p[2] * k];
    let mut head = format!("<?xml version=\"1.0\" encoding=\"UTF-8\" ?>\n<exchange xmlns:xsi=\"http://www.w3.org/2001/XMLSchema-instance\" xsi:noNamespaceSchemaLocation=\"http://download.autodesk.com/us/navisworks/schemas/nw-exchange-12.0.xsd\" units=\"{units}\" filename=\"\" filepath=\"\">\n<viewpoints>\n");
    // fiecare <view> separat; apoi asezate in <viewfolder> dupa folderul lor
    let mut parts: Vec<(String, String)> = Vec::new();
    for v in views {
        let mut s = String::new();
        let q = nw_quaternion(v.fwd, v.up);
        let (proj, height) = match v.ortho_height {
            Some(h) => ("ortho", h * k),
            None => ("persp", (v.fov_deg as f64).to_radians()),
        };
        s.push_str(&format!("<view name=\"{}\" guid=\"{}\">\n", esc(&v.name), esc(&v.guid)));
        s.push_str(&format!(
            "<viewpoint tool=\"none\" render=\"shaded\" lighting=\"headlight\" focal=\"{:.6}\" linear=\"1.0\" angular=\"0.785398\">\
             <camera projection=\"{proj}\" near=\"{:.6}\" far=\"{:.6}\" aspect=\"{:.6}\" height=\"{:.6}\">{}<rotation><quaternion a=\"{:.8}\" b=\"{:.8}\" c=\"{:.8}\" d=\"{:.8}\"/></rotation></camera>\
             <up><vec3f x=\"0\" y=\"0\" z=\"1\"/></up></viewpoint>\n",
            v.focal * k, v.near * k, v.far * k, v.aspect, height, p3("position", sc(v.eye)), q[0], q[1], q[2], q[3]
        ));
        if let Some((mn, mx)) = v.section_box {
            // ca exportul Navisworks: cele 6 planuri (fetele cutiei, normale spre interior), cutia si rotatia ei
            let (a, b) = (sc(mn), sc(mx));
            let plane = |al: &str, n: [f64; 3], d: f64| format!("<clipplane state=\"default\" alignment=\"{al}\"><plane distance=\"{d:.6}\"><vec3f x=\"{}\" y=\"{}\" z=\"{}\"/></plane></clipplane>", n[0], n[1], n[2]);
            let planes = [
                plane("top", [0.0, 0.0, -1.0], -b[2]),
                plane("bottom", [0.0, 0.0, 1.0], a[2]),
                plane("front", [0.0, 1.0, 0.0], a[1]),
                plane("back", [0.0, -1.0, 0.0], -b[1]),
                plane("left", [1.0, 0.0, 0.0], a[0]),
                plane("right", [-1.0, 0.0, 0.0], -b[0]),
            ]
            .concat();
            s.push_str(&format!(
                "<clipplaneset linked=\"0\" current=\"0\" mode=\"box\" enabled=\"1\"><range><box3f>{}{}</box3f></range><clipplanes>{planes}</clipplanes><box><box3f>{}{}</box3f></box><box-rotation><rotation><quaternion a=\"0\" b=\"0\" c=\"0\" d=\"1\"/></rotation></box-rotation></clipplaneset>\n",
                p3("min", sc(v.scene_min)),
                p3("max", sc(v.scene_max)),
                p3("min", a),
                p3("max", b)
            ));
        }
        if !v.comments.is_empty() {
            s.push_str("<comments>");
            for (i, (user, body, date, st)) in v.comments.iter().enumerate() {
                s.push_str(&format!("<comment id=\"{}\" status=\"{st}\"><user>{}</user><body>{}</body>{}</comment>", i + 1, esc(user), esc(body), nw_date(date)));
            }
            s.push_str("</comments>\n");
        }
        if !v.redlines.is_empty() {
            s.push_str("<redlines>");
            for r in &v.redlines {
                s.push_str(&nw_redline(r, v.aspect));
            }
            s.push_str("</redlines>\n");
        }
        s.push_str("</view>\n");
        parts.push((v.folder.trim_matches('/').to_string(), s));
    }
    emit_folder(&mut head, &parts, "");
    head.push_str("</viewpoints>\n</exchange>\n");
    head
}

/// viewpoint-urile din folderul `path`, apoi subfolderele lui ca <viewfolder> (in ordinea aparitiei)
fn emit_folder(out: &mut String, parts: &[(String, String)], path: &str) {
    for (f, xml) in parts {
        if f == path {
            out.push_str(xml);
        }
    }
    let mut children: Vec<String> = Vec::new();
    for (f, _) in parts {
        let rest = if path.is_empty() { Some(f.as_str()).filter(|f| !f.is_empty()) } else { f.strip_prefix(path).and_then(|r| r.strip_prefix('/')) };
        if let Some(r) = rest {
            let child = r.split('/').next().unwrap_or(r).to_string();
            if !children.contains(&child) {
                children.push(child);
            }
        }
    }
    for c in children {
        let p = if path.is_empty() { c.clone() } else { format!("{path}/{c}") };
        out.push_str(&format!("<viewfolder name=\"{}\" guid=\"{}\">\n", esc(&c), new_guid()));
        emit_folder(out, parts, &p);
        out.push_str("</viewfolder>\n");
    }
}

// ---------------------------------------------------------------------------- Navisworks viewpoints XML: import
fn xml_attr(tag: &str, name: &str) -> Option<String> {
    let k = format!(" {name}=\"");
    let i = tag.find(&k)? + k.len();
    let j = tag[i..].find('"')? + i;
    Some(unesc(&tag[i..j]))
}

fn unesc(s: &str) -> String {
    s.replace("&lt;", "<").replace("&gt;", ">").replace("&quot;", "\"").replace("&apos;", "'").replace("&amp;", "&")
}

fn xml_f(tag: &str, name: &str) -> f64 {
    xml_attr(tag, name).and_then(|v| v.trim().parse::<f64>().ok()).unwrap_or(0.0)
}

/// Continutul primului element `<name ...>...</name>` (sau tag-ul gol) din `s`, si tag-ul de deschidere.
fn xml_elem<'a>(s: &'a str, name: &str) -> Option<(&'a str, &'a str)> {
    let mut from = 0;
    loop {
        let i = s[from..].find(&format!("<{name}"))? + from;
        let after = s.as_bytes().get(i + 1 + name.len()).copied().unwrap_or(b' ');
        if !(after == b' ' || after == b'>' || after == b'/' || after == b'\n' || after == b'\r' || after == b'\t') {
            from = i + 1;
            continue;
        }
        let open_end = s[i..].find('>')? + i;
        let open = &s[i..=open_end];
        if open.ends_with("/>") {
            return Some((open, ""));
        }
        let close = format!("</{name}>");
        let end = s[open_end..].find(&close).map(|k| k + open_end).unwrap_or(s.len());
        return Some((open, &s[open_end + 1..end]));
    }
}

/// Toate elementele `<name ...>` de pe acelasi nivel (cautare simpla, fara imbricare de acelasi nume).
fn xml_all<'a>(s: &'a str, name: &str) -> Vec<(&'a str, &'a str)> {
    let mut out = Vec::new();
    let mut rest = s;
    while let Some((open, body)) = xml_elem(rest, name) {
        let start = rest.find(open).unwrap_or(0);
        let consumed = start + open.len() + if body.is_empty() { 0 } else { body.len() + name.len() + 3 };
        out.push((open, body));
        if consumed == 0 || consumed >= rest.len() {
            break;
        }
        rest = &rest[consumed..];
    }
    out
}

fn xml_pos3(s: &str, name: &str) -> Option<[f64; 3]> {
    let (_, body) = xml_elem(s, name)?;
    let (p, _) = xml_elem(body, "pos3f")?;
    Some([xml_f(p, "x"), xml_f(p, "y"), xml_f(p, "z")])
}

fn xml_pos2(s: &str, name: &str) -> Option<[f32; 2]> {
    let (_, body) = xml_elem(s, name)?;
    let (p, _) = xml_elem(body, "pos2f")?;
    Some([xml_f(p, "x") as f32, xml_f(p, "y") as f32])
}

/// Coordonate Navisworks (-1..1, y in sus) -> redline ManageBIM (fata de centru / inaltime, y in jos).
fn from_nw_pt(p: [f32; 2], aspect: f32) -> [f32; 2] {
    [p[0] * aspect / 2.0, -p[1] / 2.0]
}

fn rot_q(q: [f64; 4], v: [f64; 3]) -> [f64; 3] {
    let (x, y, z, w) = (q[0], q[1], q[2], q[3]);
    let m = [
        [1.0 - 2.0 * (y * y + z * z), 2.0 * (x * y - z * w), 2.0 * (x * z + y * w)],
        [2.0 * (x * y + z * w), 1.0 - 2.0 * (x * x + z * z), 2.0 * (y * z - x * w)],
        [2.0 * (x * z - y * w), 2.0 * (y * z + x * w), 1.0 - 2.0 * (x * x + y * y)],
    ];
    [0, 1, 2].map(|i| m[i][0] * v[0] + m[i][1] * v[1] + m[i][2] * v[2])
}

/// Citeste viewpoints dintr-un XML Navisworks (exportat din Navisworks sau din ManageBIM).
/// `aspect` = latime / inaltime a ferestrei, pentru pozitia markup-urilor. Unitatile se convertesc in metri.
pub fn parse_navis_viewpoints(xml: &str, aspect: f32) -> Result<Vec<NwView>, String> {
    let (ex, _) = xml_elem(xml, "exchange").ok_or("Not a Navisworks exchange XML (no <exchange> element).")?;
    let k = match xml_attr(ex, "units").as_deref().unwrap_or("m") {
        "mm" => 0.001,
        "cm" => 0.01,
        "km" => 1000.0,
        "ft" => 0.3048,
        "in" => 0.0254,
        "yrd" => 0.9144,
        "mi" => 1609.344,
        "um" => 1e-6,
        "mils" => 2.54e-5,
        "uin" => 2.54e-8,
        _ => 1.0,
    };
    let sc = |p: [f64; 3]| [p[0] * k, p[1] * k, p[2] * k];
    let mut out = Vec::new();
    // <view> poate sta si in <viewfolder> (si subfoldere): calea folderului la fiecare pozitie din fisier
    let folders = folder_events(xml);
    let folder_at = |pos: usize| folders.iter().rev().find(|(p, _)| *p <= pos).map(|(_, f)| f.clone()).unwrap_or_default();
    let mut rest = xml;
    while let Some(i) = rest.find("<view") {
        let after = rest.as_bytes().get(i + 5).copied().unwrap_or(b'x');
        if !(after == b' ' || after == b'>') {
            rest = &rest[i + 5..];
            continue;
        }
        let folder = folder_at(xml.len() - rest.len() + i);
        let Some((open, body)) = xml_elem(&rest[i..], "view") else { break };
        rest = &rest[i + open.len() + body.len()..];
        let Some((vp_open, vp)) = xml_elem(body, "viewpoint") else { continue };
        let Some((cam_open, cam)) = xml_elem(vp, "camera") else { continue };
        let eye = sc(xml_pos3(cam, "position").unwrap_or([0.0; 3]));
        let q = xml_elem(cam, "quaternion").map(|(t, _)| [xml_f(t, "a"), xml_f(t, "b"), xml_f(t, "c"), xml_f(t, "d")]).unwrap_or([0.0, 0.0, 0.0, 1.0]);
        let fwd = rot_q(q, [0.0, 0.0, -1.0]);
        let up = rot_q(q, [0.0, 1.0, 0.0]);
        let height = xml_f(cam_open, "height");
        let ortho = xml_attr(cam_open, "projection").as_deref() == Some("ortho");
        let mut v = NwView {
            name: xml_attr(open, "name").unwrap_or_else(|| format!("Viewpoint {}", out.len() + 1)),
            guid: xml_attr(open, "guid").unwrap_or_default(),
            eye,
            fwd,
            up,
            fov_deg: if !ortho && height > 0.01 && height < 3.1 { height.to_degrees() as f32 } else { 50.0 },
            ortho_height: ortho.then_some(height * k),
            aspect,
            focal: xml_f(vp_open, "focal").max(0.05) * k,
            near: 0.0,
            far: 0.0,
            scene_min: [0.0; 3],
            scene_max: [0.0; 3],
            section_box: None,
            comments: Vec::new(),
            redlines: Vec::new(),
            folder,
        };
        if let Some((cs_open, cs)) = xml_elem(body, "clipplaneset") {
            if xml_attr(cs_open, "mode").as_deref() == Some("box") && xml_attr(cs_open, "enabled").as_deref() != Some("0") {
                if let Some((_, b)) = xml_elem(cs, "box") {
                    if let (Some(mn), Some(mx)) = (xml_pos3(b, "min"), xml_pos3(b, "max")) {
                        v.section_box = Some((sc(mn), sc(mx)));
                    }
                }
            }
        }
        if let Some((_, cm)) = xml_elem(body, "comments") {
            for (c_open, c) in xml_all(cm, "comment") {
                let user = xml_elem(c, "user").map(|x| unesc(x.1)).unwrap_or_default();
                let text = xml_elem(c, "body").map(|x| unesc(x.1)).unwrap_or_default();
                let date = xml_elem(c, "date").map(|(d, _)| format!("{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z", xml_f(d, "year") as u32, xml_f(d, "month") as u32, xml_f(d, "day") as u32, xml_f(d, "hour") as u32, xml_f(d, "minute") as u32, xml_f(d, "second") as u32)).unwrap_or_else(now_iso);
                let st = match xml_attr(c_open, "status").as_deref() {
                    Some("active") => "active",
                    Some("approved") => "approved",
                    Some("resolved") => "resolved",
                    _ => "new",
                };
                v.comments.push((user, text, date, st));
            }
        }
        if let Some((_, rl)) = xml_elem(body, "redlines") {
            let colour = |s: &str| {
                xml_elem(s, "colour").map(|(c, _)| [(xml_f(c, "red") * 255.0) as u8, (xml_f(c, "green") * 255.0) as u8, (xml_f(c, "blue") * 255.0) as u8]).unwrap_or([255, 0, 0])
            };
            let width = |t: &str| (xml_f(t, "thickness") as f32).max(1.0);
            for (tag, kind) in [("rlline", RedKind::Line), ("rlarrow", RedKind::Arrow), ("rlellipse", RedKind::Ellipse)] {
                for (o, b) in xml_all(rl, tag) {
                    if let (Some(a), Some(z)) = (xml_pos2(b, "start"), xml_pos2(b, "end")) {
                        v.redlines.push(Redline { kind, pts: vec![from_nw_pt(a, aspect), from_nw_pt(z, aspect)], color: colour(b), width: width(o), text: String::new() });
                    }
                }
            }
            for (o, b) in xml_all(rl, "rlcloud") {
                let pts: Vec<[f32; 2]> = xml_elem(b, "points").map(|(_, p)| xml_all(p, "pos2f").iter().map(|(t, _)| from_nw_pt([xml_f(t, "x") as f32, xml_f(t, "y") as f32], aspect)).collect()).unwrap_or_default();
                if pts.len() >= 2 {
                    // norul ManageBIM e definit de colturile dreptunghiului care il incadreaza
                    let (mut lo, mut hi) = ([f32::MAX; 2], [f32::MIN; 2]);
                    for p in &pts {
                        lo = [lo[0].min(p[0]), lo[1].min(p[1])];
                        hi = [hi[0].max(p[0]), hi[1].max(p[1])];
                    }
                    v.redlines.push(Redline { kind: RedKind::Cloud, pts: vec![lo, hi], color: colour(b), width: width(o), text: String::new() });
                }
            }
            for (o, b) in xml_all(rl, "rltext") {
                if let Some(p) = xml_pos2(b, "pos") {
                    let text = xml_elem(b, "text").map(|x| unesc(x.1)).unwrap_or_default();
                    v.redlines.push(Redline { kind: RedKind::Text, pts: vec![from_nw_pt(p, aspect)], color: colour(b), width: width(o), text });
                }
            }
        }
        out.push(v);
    }
    if out.is_empty() {
        return Err("No viewpoints in this XML file.".into());
    }
    Ok(out)
}

// ---------------------------------------------------------------------------- BCF import (2.1 / 3.0)
pub struct BcfTopic {
    pub guid: String,
    pub title: String,
    pub status: String,
    pub priority: String,
    pub assigned: String,
    pub author: String,
    pub created: String,
    pub description: String,
    pub comments: Vec<VpComment>,
    /// (ochi, directie, sus, fov grade sau None pentru ortho, inaltime ortho)
    pub camera: Option<([f64; 3], [f64; 3], [f64; 3], Option<f32>, f64)>,
    pub selected: Vec<String>,
}

fn xml_text(s: &str, name: &str) -> String {
    xml_elem(s, name).map(|(_, b)| unesc(b.trim())).unwrap_or_default()
}

fn bcf_xyz(s: &str, name: &str) -> Option<[f64; 3]> {
    let (_, b) = xml_elem(s, name)?;
    let g = |k: &str| xml_text(b, k).parse::<f64>().ok();
    Some([g("X")?, g("Y")?, g("Z")?])
}

/// Citeste un fisier .bcf / .bcfzip (BCF 2.1 sau 3.0): topic-uri, comentarii, camera, componente selectate.
pub fn parse_bcf(data: Vec<u8>) -> Result<Vec<BcfTopic>, String> {
    let zip = crate::fcstd::Zip::parse(data).map_err(|_| "Not a BCF file (ZIP archive expected).".to_string())?;
    let names: Vec<String> = zip.names().cloned().collect();
    let mut out = Vec::new();
    for mk in names.iter().filter(|n| n.to_lowercase().ends_with("markup.bcf")) {
        let dir = mk.rsplit_once('/').map(|x| x.0).unwrap_or("");
        let Some(m) = zip.text(mk) else { continue };
        let Some((topen, tbody)) = xml_elem(&m, "Topic") else { continue };
        let mut t = BcfTopic {
            guid: xml_attr(topen, "Guid").unwrap_or_else(new_guid),
            title: xml_text(tbody, "Title"),
            status: xml_attr(topen, "TopicStatus").filter(|s| !s.is_empty()).unwrap_or_else(|| xml_text(tbody, "TopicStatus")),
            priority: xml_text(tbody, "Priority"),
            assigned: xml_text(tbody, "AssignedTo"),
            author: xml_text(tbody, "CreationAuthor"),
            created: xml_text(tbody, "CreationDate"),
            description: xml_text(tbody, "Description"),
            comments: Vec::new(),
            camera: None,
            selected: Vec::new(),
        };
        // comentariile: in BCF 2.1 sub <Markup>, in 3.0 sub <Topic><Comments>
        for (_, c) in xml_all(&m, "Comment").into_iter().filter(|(o, _)| o.contains("Guid=")) {
            let text = xml_text(c, "Comment");
            if !text.is_empty() {
                t.comments.push(VpComment { author: xml_text(c, "Author"), date: xml_text(c, "Date"), text });
            }
        }
        // primul viewpoint al topic-ului
        let vp_name = xml_elem(&m, "Viewpoint").map(|(_, b)| b.trim().to_string()).filter(|s| s.ends_with(".bcfv"));
        let vp_path = vp_name
            .map(|n| if dir.is_empty() { n } else { format!("{dir}/{n}") })
            .filter(|p| names.contains(p))
            .or_else(|| names.iter().find(|n| n.starts_with(&format!("{dir}/")) && n.to_lowercase().ends_with(".bcfv")).cloned());
        if let Some(v) = vp_path.and_then(|p| zip.text(&p)) {
            if let Some((_, pc)) = xml_elem(&v, "PerspectiveCamera") {
                if let (Some(e), Some(d), Some(u)) = (bcf_xyz(pc, "CameraViewPoint"), bcf_xyz(pc, "CameraDirection"), bcf_xyz(pc, "CameraUpVector")) {
                    let fov = xml_text(pc, "FieldOfView").parse::<f32>().unwrap_or(60.0);
                    t.camera = Some((e, d, u, Some(fov), 0.0));
                }
            } else if let Some((_, oc)) = xml_elem(&v, "OrthogonalCamera") {
                if let (Some(e), Some(d), Some(u)) = (bcf_xyz(oc, "CameraViewPoint"), bcf_xyz(oc, "CameraDirection"), bcf_xyz(oc, "CameraUpVector")) {
                    let h = xml_text(oc, "ViewToWorldScale").parse::<f64>().unwrap_or(10.0);
                    t.camera = Some((e, d, u, None, h));
                }
            }
            if let Some((_, sel)) = xml_elem(&v, "Selection") {
                for (c, _) in xml_all(sel, "Component") {
                    if let Some(g) = xml_attr(c, "IfcGuid").or_else(|| xml_attr(c, "AuthoringToolId")) {
                        t.selected.push(g);
                    }
                }
            }
        }
        out.push(t);
    }
    if out.is_empty() {
        return Err("No BCF topics in this file.".into());
    }
    Ok(out)
}

/// (pozitie, calea folderului de acolo incolo) la fiecare <viewfolder ...> si </viewfolder> din fisier
fn folder_events(xml: &str) -> Vec<(usize, String)> {
    let mut ev = Vec::new();
    let mut stack: Vec<String> = Vec::new();
    let mut i = 0;
    while i < xml.len() {
        let open = xml[i..].find("<viewfolder").map(|p| p + i);
        let close = xml[i..].find("</viewfolder>").map(|p| p + i);
        match (open, close) {
            (Some(o), c) if c.is_none_or(|c| o < c) => {
                let end = xml[o..].find('>').map(|p| p + o).unwrap_or(xml.len());
                let tag = &xml[o..end];
                stack.push(xml_attr(tag, "name").unwrap_or_else(|| "Folder".into()));
                // <viewfolder .../> gol: se inchide imediat
                if tag.ends_with('/') {
                    stack.pop();
                }
                ev.push((o, stack.join("/")));
                i = end + 1;
            }
            (_, Some(c)) => {
                stack.pop();
                ev.push((c, stack.join("/")));
                i = c + 13;
            }
            _ => break,
        }
    }
    ev
}
