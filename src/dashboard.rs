//! Dashboard-ul proiectului: sumarul modelelor, clash-urilor, issue-urilor, verificarilor si al restului
//! datelor, afisat intr-o fereastra (ui/dashboard.rs) si exportat ca raport PDF (A4) sau Word (.docx).

use crate::markup::{esc, Zip};

#[derive(Clone, Debug, Default)]
pub struct ModelRow {
    pub name: String,
    pub format: String,
    pub elements: usize,
    pub triangles: usize,
    pub points: usize,
}

#[derive(Clone, Debug, Default)]
pub struct ClashRow {
    pub name: String,
    pub kind: String,
    pub total: usize,
    /// New, Active, Reviewed, Approved, Resolved
    pub by_status: [usize; 5],
    /// Critical, Major, Minor (rezultatele deschise)
    pub by_severity: [usize; 3],
    pub last_run: String,
}

#[derive(Clone, Debug, Default)]
pub struct Summary {
    pub project: String,
    pub author: String,
    pub date: String,
    pub models: Vec<ModelRow>,
    pub levels: Vec<(String, f64)>,
    /// cele mai frecvente categorii (nume, numar)
    pub categories: Vec<(String, usize)>,
    pub clash: Vec<ClashRow>,
    pub viewpoints: usize,
    /// status, priority, assignee -> numar
    pub issue_status: Vec<(String, usize)>,
    pub issue_priority: Vec<(String, usize)>,
    pub issue_assignee: Vec<(String, usize)>,
    /// (reguli, elemente verificate, erori) - None daca Model Checker nu a rulat
    pub check: Option<(usize, usize, usize)>,
    pub sets: usize,
    pub coord_objects: usize,
    pub materials: usize,
    pub tasks: usize,
    pub hidden: usize,
}

impl Summary {
    pub fn elements(&self) -> usize {
        self.models.iter().map(|m| m.elements).sum()
    }
    pub fn clashes_open(&self) -> usize {
        self.clash.iter().map(|c| c.by_status[0] + c.by_status[1] + c.by_status[2]).sum()
    }
    pub fn clashes_total(&self) -> usize {
        self.clash.iter().map(|c| c.total).sum()
    }
    pub fn issues_open(&self) -> usize {
        self.issue_status.iter().filter(|(s, _)| !matches!(s.to_ascii_lowercase().as_str(), "closed" | "resolved" | "done")).map(|(_, n)| n).sum()
    }
    /// indicatorii principali: (eticheta, valoare, detaliu)
    pub fn kpis(&self) -> Vec<(&'static str, String, String)> {
        let check = match self.check {
            Some((r, c, f)) => (format!("{f}"), format!("errors in {c} items, {r} rules")),
            None => ("-".into(), "Model Checker not run".into()),
        };
        vec![
            ("Models", self.models.len().to_string(), format!("{} elements", thousands(self.elements()))),
            ("Open clashes", self.clashes_open().to_string(), format!("of {} in {} tests", self.clashes_total(), self.clash.len())),
            ("Open issues", self.issues_open().to_string(), format!("{} viewpoints", self.viewpoints)),
            ("Check errors", check.0, check.1),
            ("Levels", self.levels.len().to_string(), format!("{} sets, {} coordination objects", self.sets, self.coord_objects)),
            ("4D tasks", self.tasks.to_string(), format!("{} material rules", self.materials)),
        ]
    }
}

pub fn thousands(n: usize) -> String {
    let s = n.to_string();
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i).is_multiple_of(3) {
            out.push(' ');
        }
        out.push(c);
    }
    out
}

pub const STATUS: [&str; 5] = ["New", "Active", "Reviewed", "Approved", "Resolved"];
pub const STATUS_RGB: [[u8; 3]; 5] = [[230, 70, 60], [240, 150, 40], [70, 130, 230], [60, 175, 90], [150, 150, 150]];
pub const SEVERITY: [&str; 3] = ["Critical", "Major", "Minor"];
pub const SEVERITY_RGB: [[u8; 3]; 3] = [[220, 60, 60], [240, 150, 40], [230, 200, 70]];

// ---------------------------------------------------------------- PDF (A4 portrait)

const PW: f32 = 595.28;
const PH: f32 = 841.89;
const M: f32 = 40.0;
const ACCENT: [u8; 3] = [37, 99, 222];

fn rgb(c: &mut Vec<u8>, col: [u8; 3], fill: bool) {
    c.extend_from_slice(format!("{:.3} {:.3} {:.3} {}\n", col[0] as f32 / 255.0, col[1] as f32 / 255.0, col[2] as f32 / 255.0, if fill { "rg" } else { "RG" }).as_bytes());
}

fn rect(c: &mut Vec<u8>, x: f32, y: f32, w: f32, h: f32, col: [u8; 3]) {
    rgb(c, col, true);
    c.extend_from_slice(format!("{x:.1} {y:.1} {w:.1} {h:.1} re f\n").as_bytes());
}

fn txt(c: &mut Vec<u8>, x: f32, y: f32, size: f32, bold: bool, col: [u8; 3], s: &str) {
    rgb(c, col, true);
    crate::pdf::text(c, x, y, size, bold, s);
}

struct Pdf {
    pages: Vec<crate::pdf::Page>,
    c: Vec<u8>,
    y: f32,
    title: String,
}

impl Pdf {
    fn new(title: &str) -> Pdf {
        let mut p = Pdf { pages: Vec::new(), c: Vec::new(), y: 0.0, title: title.to_string() };
        p.page();
        p
    }
    fn page(&mut self) {
        if !self.c.is_empty() {
            self.pages.push(crate::pdf::Page { content: std::mem::take(&mut self.c), image: None });
        }
        // banda de sus
        rect(&mut self.c, 0.0, PH - 6.0, PW, 6.0, ACCENT);
        txt(&mut self.c, M, PH - 28.0, 9.0, true, ACCENT, "ManageBIM");
        let t = self.title.clone();
        txt(&mut self.c, PW - M - crate::pdf::text_w(&t, 9.0), PH - 28.0, 9.0, false, [120, 120, 120], &t);
        self.y = PH - 50.0;
    }
    fn need(&mut self, h: f32) {
        if self.y - h < 50.0 {
            self.page();
        }
    }
    fn heading(&mut self, s: &str) {
        self.need(40.0);
        self.y -= 14.0;
        txt(&mut self.c, M, self.y, 13.0, true, [25, 30, 40], s);
        self.y -= 6.0;
        rect(&mut self.c, M, self.y, 40.0, 2.0, ACCENT);
        self.y -= 14.0;
    }
    fn table(&mut self, head: &[&str], widths: &[f32], rows: &[Vec<String>]) {
        let rh = 16.0;
        self.need(rh * 2.0);
        let draw_head = |p: &mut Pdf| {
            rect(&mut p.c, M, p.y - 4.0, PW - 2.0 * M, rh, [235, 240, 250]);
            let mut x = M + 4.0;
            for (h, w) in head.iter().zip(widths) {
                txt(&mut p.c, x, p.y + 1.0, 8.5, true, [40, 50, 70], h);
                x += w;
            }
            p.y -= rh;
        };
        draw_head(self);
        for (i, r) in rows.iter().enumerate() {
            if self.y - rh < 50.0 {
                self.page();
                draw_head(self);
            }
            if i % 2 == 1 {
                rect(&mut self.c, M, self.y - 4.0, PW - 2.0 * M, rh, [248, 249, 252]);
            }
            let mut x = M + 4.0;
            for (v, w) in r.iter().zip(widths) {
                // taiat la latimea coloanei
                let mut s = v.clone();
                if crate::pdf::text_w(&s, 8.5) > w - 6.0 {
                    // "..." (fontul standard al PDF-ului nu are caracterul ellipsis)
                    while s.chars().count() > 2 && crate::pdf::text_w(&format!("{s}..."), 8.5) > w - 6.0 {
                        s.pop();
                    }
                    s.push_str("...");
                }
                txt(&mut self.c, x, self.y + 1.0, 8.5, false, [30, 30, 35], &s);
                x += w;
            }
            self.y -= rh;
        }
        self.y -= 8.0;
    }
    /// bara orizontala impartita in segmente colorate, cu legenda
    fn stacked(&mut self, label: &str, parts: &[(String, usize, [u8; 3])]) {
        self.need(34.0);
        let total: usize = parts.iter().map(|p| p.1).sum();
        txt(&mut self.c, M, self.y, 9.0, true, [40, 40, 50], label);
        txt(&mut self.c, PW - M - 60.0, self.y, 9.0, false, [100, 100, 110], &format!("{total} total"));
        self.y -= 14.0;
        let w = PW - 2.0 * M;
        let mut x = M;
        if total == 0 {
            rect(&mut self.c, M, self.y, w, 8.0, [230, 230, 235]);
        } else {
            for (_, n, col) in parts {
                let sw = w * *n as f32 / total as f32;
                if sw > 0.0 {
                    rect(&mut self.c, x, self.y, sw, 8.0, *col);
                }
                x += sw;
            }
        }
        self.y -= 12.0;
        let mut lx = M;
        for (name, n, col) in parts {
            if *n == 0 {
                continue;
            }
            rect(&mut self.c, lx, self.y, 7.0, 7.0, *col);
            let s = format!("{name} {n}");
            txt(&mut self.c, lx + 10.0, self.y, 8.0, false, [70, 70, 80], &s);
            lx += 22.0 + crate::pdf::text_w(&s, 8.0);
        }
        self.y -= 16.0;
    }
    fn finish(mut self) -> Vec<u8> {
        let n = {
            self.pages.push(crate::pdf::Page { content: std::mem::take(&mut self.c), image: None });
            self.pages.len()
        };
        // numerotarea paginilor
        for (i, p) in self.pages.iter_mut().enumerate() {
            let s = format!("Page {} / {n}", i + 1);
            txt(&mut p.content, PW - M - crate::pdf::text_w(&s, 8.0), 28.0, 8.0, false, [140, 140, 140], &s);
        }
        crate::pdf::write_pdf(&self.pages, &self.title, PW, PH)
    }
}

pub fn pdf(s: &Summary) -> Vec<u8> {
    let mut p = Pdf::new(&format!("Project dashboard - {}", s.project));
    // titlu
    txt(&mut p.c, M, p.y - 14.0, 22.0, true, [20, 25, 35], "Project dashboard");
    p.y -= 34.0;
    txt(&mut p.c, M, p.y, 11.0, false, [90, 95, 105], &format!("{}  -  {}  -  {}", s.project, s.date, if s.author.is_empty() { "ManageBIM" } else { &s.author }));
    p.y -= 26.0;
    // indicatori (2 randuri x 3)
    let kpis = s.kpis();
    let cw = (PW - 2.0 * M - 20.0) / 3.0;
    let ch = 62.0;
    for (i, (label, value, detail)) in kpis.iter().enumerate() {
        let (col, row) = (i % 3, i / 3);
        let x = M + col as f32 * (cw + 10.0);
        let y = p.y - (row as f32 + 1.0) * (ch + 10.0);
        rect(&mut p.c, x, y, cw, ch, [244, 247, 253]);
        rect(&mut p.c, x, y, 3.0, ch, ACCENT);
        txt(&mut p.c, x + 12.0, y + ch - 16.0, 8.5, false, [100, 105, 115], label);
        txt(&mut p.c, x + 12.0, y + 20.0, 20.0, true, [20, 25, 35], value);
        txt(&mut p.c, x + 12.0, y + 8.0, 7.5, false, [120, 125, 135], detail);
    }
    p.y -= 2.0 * (ch + 10.0) + 14.0;

    // clash-uri
    p.heading("Clash Detective");
    if s.clash.is_empty() {
        txt(&mut p.c, M, p.y, 9.0, false, [110, 110, 120], "No clash tests.");
        p.y -= 16.0;
    } else {
        let mut all = [0usize; 5];
        for c in &s.clash {
            for k in 0..5 {
                all[k] += c.by_status[k];
            }
        }
        let parts: Vec<(String, usize, [u8; 3])> = (0..5).map(|k| (STATUS[k].to_string(), all[k], STATUS_RGB[k])).collect();
        p.stacked("All tests by status", &parts);
        let rows: Vec<Vec<String>> = s
            .clash
            .iter()
            .map(|c| {
                vec![c.name.clone(), c.kind.clone(), c.total.to_string(), c.by_status[0].to_string(), c.by_status[1].to_string(), c.by_status[2].to_string(), c.by_status[3].to_string(), c.by_status[4].to_string(), format!("{}/{}/{}", c.by_severity[0], c.by_severity[1], c.by_severity[2])]
            })
            .collect();
        p.table(&["Test", "Type", "Total", "New", "Active", "Reviewed", "Approved", "Resolved", "Crit/Maj/Min"], &[108.0, 82.0, 34.0, 32.0, 38.0, 48.0, 50.0, 48.0, 64.0], &rows);
    }

    // issues
    p.heading("Viewpoints & issues");
    let pal = [[230, 70, 60], [240, 150, 40], [70, 130, 230], [60, 175, 90], [150, 150, 150], [140, 90, 200]];
    let parts: Vec<(String, usize, [u8; 3])> = s.issue_status.iter().enumerate().map(|(i, (n, c))| (n.clone(), *c, pal[i % pal.len()])).collect();
    p.stacked(&format!("{} viewpoints by status", s.viewpoints), &parts);
    if !s.issue_assignee.is_empty() || !s.issue_priority.is_empty() {
        let n = s.issue_assignee.len().max(s.issue_priority.len());
        let rows: Vec<Vec<String>> = (0..n)
            .map(|i| {
                let a = s.issue_assignee.get(i).map(|(a, n)| (if a.is_empty() { "(unassigned)".to_string() } else { a.clone() }, n.to_string())).unwrap_or_default();
                let pr = s.issue_priority.get(i).map(|(a, n)| (a.clone(), n.to_string())).unwrap_or_default();
                vec![a.0, a.1, pr.0, pr.1]
            })
            .collect();
        p.table(&["Assigned to", "Issues", "Priority", "Issues"], &[200.0, 60.0, 180.0, 60.0], &rows);
    }

    // model checker
    p.heading("Model Checker");
    let line = match s.check {
        Some((r, c, f)) => format!("{r} rules, {c} items checked, {f} failed ({:.1} % passed)", if c == 0 { 100.0 } else { 100.0 * (c - f.min(c)) as f64 / c as f64 }),
        None => "Not run in this session - Analyze › Checker.".into(),
    };
    txt(&mut p.c, M, p.y, 9.5, false, [50, 50, 60], &line);
    p.y -= 22.0;

    // modele
    p.heading("Models");
    let rows: Vec<Vec<String>> = s.models.iter().map(|m| vec![m.name.clone(), m.format.clone(), thousands(m.elements), thousands(m.triangles), if m.points > 0 { thousands(m.points) } else { "-".into() }]).collect();
    p.table(&["Model", "Format", "Elements", "Triangles", "Points"], &[220.0, 60.0, 70.0, 80.0, 80.0], &rows);
    if !s.categories.is_empty() {
        let rows: Vec<Vec<String>> = s.categories.iter().map(|(c, n)| vec![c.clone(), thousands(*n)]).collect();
        p.table(&["Category (most items)", "Items"], &[400.0, 110.0], &rows);
    }
    if !s.levels.is_empty() {
        p.heading("Levels");
        let rows: Vec<Vec<String>> = s.levels.iter().map(|(n, z)| vec![n.clone(), format!("{z:+.2} m")]).collect();
        p.table(&["Level", "Elevation"], &[400.0, 110.0], &rows);
    }
    p.finish()
}

// ---------------------------------------------------------------- Word (.docx)

fn w_par(text: &str, style: &str) -> String {
    format!("<w:p><w:pPr><w:pStyle w:val=\"{style}\"/></w:pPr><w:r><w:t xml:space=\"preserve\">{}</w:t></w:r></w:p>", esc(text))
}

fn w_table(head: &[&str], rows: &[Vec<String>]) -> String {
    let cell = |t: &str, header: bool| {
        let shade = if header { "<w:shd w:val=\"clear\" w:color=\"auto\" w:fill=\"DCE6F8\"/>" } else { "" };
        let bold = if header { "<w:rPr><w:b/></w:rPr>" } else { "" };
        format!("<w:tc><w:tcPr>{shade}</w:tcPr><w:p><w:pPr><w:spacing w:before=\"40\" w:after=\"40\"/></w:pPr><w:r>{bold}<w:t xml:space=\"preserve\">{}</w:t></w:r></w:p></w:tc>", esc(t))
    };
    let mut s = String::from("<w:tbl><w:tblPr><w:tblStyle w:val=\"MBTable\"/><w:tblW w:w=\"5000\" w:type=\"pct\"/></w:tblPr>");
    // grila coloanelor (ceruta de schema Word): latime egala pe pagina A4 cu margini de 2 cm
    let colw = 9638 / head.len().max(1);
    s.push_str("<w:tblGrid>");
    for _ in head {
        s.push_str(&format!("<w:gridCol w:w=\"{colw}\"/>"));
    }
    s.push_str("</w:tblGrid>");
    s.push_str("<w:tr><w:trPr><w:tblHeader/></w:trPr>");
    for h in head {
        s.push_str(&cell(h, true));
    }
    s.push_str("</w:tr>");
    for r in rows {
        s.push_str("<w:tr>");
        for v in r {
            s.push_str(&cell(v, false));
        }
        s.push_str("</w:tr>");
    }
    s.push_str("</w:tbl>");
    s.push_str(&w_par("", "Normal"));
    s
}

pub fn docx(s: &Summary) -> Vec<u8> {
    let mut b = String::new();
    b.push_str(&w_par("Project dashboard", "Title"));
    b.push_str(&w_par(&format!("{}  ·  {}  ·  {}", s.project, s.date, if s.author.is_empty() { "ManageBIM" } else { &s.author }), "Subtitle"));
    b.push_str(&w_par("Summary", "Heading1"));
    let k: Vec<Vec<String>> = s.kpis().into_iter().map(|(l, v, d)| vec![l.to_string(), v, d]).collect();
    b.push_str(&w_table(&["Indicator", "Value", "Detail"], &k));

    b.push_str(&w_par("Clash Detective", "Heading1"));
    if s.clash.is_empty() {
        b.push_str(&w_par("No clash tests.", "Normal"));
    } else {
        let rows: Vec<Vec<String>> = s
            .clash
            .iter()
            .map(|c| {
                let mut r = vec![c.name.clone(), c.kind.clone(), c.total.to_string()];
                r.extend(c.by_status.iter().map(|n| n.to_string()));
                r.push(format!("{} / {} / {}", c.by_severity[0], c.by_severity[1], c.by_severity[2]));
                r.push(c.last_run.clone());
                r
            })
            .collect();
        b.push_str(&w_table(&["Test", "Type", "Total", "New", "Active", "Reviewed", "Approved", "Resolved", "Critical / Major / Minor", "Last run"], &rows));
    }

    b.push_str(&w_par("Viewpoints & issues", "Heading1"));
    b.push_str(&w_par(&format!("{} viewpoints, {} open issues.", s.viewpoints, s.issues_open()), "Normal"));
    for (title, list) in [("By status", &s.issue_status), ("By priority", &s.issue_priority), ("By assignee", &s.issue_assignee)] {
        if !list.is_empty() {
            b.push_str(&w_par(title, "Heading2"));
            let rows: Vec<Vec<String>> = list.iter().map(|(a, n)| vec![if a.is_empty() { "(unassigned)".into() } else { a.clone() }, n.to_string()]).collect();
            b.push_str(&w_table(&[title.trim_start_matches("By ").to_string().as_str(), "Issues"], &rows));
        }
    }

    b.push_str(&w_par("Model Checker", "Heading1"));
    b.push_str(&w_par(
        &match s.check {
            Some((r, c, f)) => format!("{r} rules, {c} items checked, {f} failed."),
            None => "Not run in this session.".into(),
        },
        "Normal",
    ));

    b.push_str(&w_par("Models", "Heading1"));
    let rows: Vec<Vec<String>> = s.models.iter().map(|m| vec![m.name.clone(), m.format.clone(), thousands(m.elements), thousands(m.triangles), if m.points > 0 { thousands(m.points) } else { "-".into() }]).collect();
    b.push_str(&w_table(&["Model", "Format", "Elements", "Triangles", "Points"], &rows));
    if !s.categories.is_empty() {
        b.push_str(&w_par("Categories with the most items", "Heading2"));
        let rows: Vec<Vec<String>> = s.categories.iter().map(|(c, n)| vec![c.clone(), thousands(*n)]).collect();
        b.push_str(&w_table(&["Category", "Items"], &rows));
    }
    if !s.levels.is_empty() {
        b.push_str(&w_par("Levels", "Heading2"));
        let rows: Vec<Vec<String>> = s.levels.iter().map(|(n, z)| vec![n.clone(), format!("{z:+.2} m")]).collect();
        b.push_str(&w_table(&["Level", "Elevation"], &rows));
    }
    b.push_str(&w_par(&format!("Other: {} selection / search sets, {} coordination objects, {} material rules, {} 4D tasks, {} hidden items.", s.sets, s.coord_objects, s.materials, s.tasks, s.hidden), "Normal"));

    let document = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<w:document xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\"><w:body>{b}<w:sectPr><w:pgSz w:w=\"11906\" w:h=\"16838\"/><w:pgMar w:top=\"1134\" w:right=\"1134\" w:bottom=\"1134\" w:left=\"1134\" w:header=\"567\" w:footer=\"567\" w:gutter=\"0\"/></w:sectPr></w:body></w:document>"
    );
    let styles = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:styles xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
<w:docDefaults><w:rPrDefault><w:rPr><w:rFonts w:ascii="Calibri" w:hAnsi="Calibri" w:cs="Calibri"/><w:sz w:val="20"/></w:rPr></w:rPrDefault><w:pPrDefault><w:pPr><w:spacing w:after="80"/></w:pPr></w:pPrDefault></w:docDefaults>
<w:style w:type="paragraph" w:default="1" w:styleId="Normal"><w:name w:val="Normal"/></w:style>
<w:style w:type="paragraph" w:styleId="Title"><w:name w:val="Title"/><w:basedOn w:val="Normal"/><w:pPr><w:spacing w:after="60"/></w:pPr><w:rPr><w:b/><w:color w:val="1A2233"/><w:sz w:val="48"/></w:rPr></w:style>
<w:style w:type="paragraph" w:styleId="Subtitle"><w:name w:val="Subtitle"/><w:basedOn w:val="Normal"/><w:pPr><w:pBdr><w:bottom w:val="single" w:sz="12" w:space="6" w:color="2563DE"/></w:pBdr><w:spacing w:after="240"/></w:pPr><w:rPr><w:color w:val="5A6070"/><w:sz w:val="22"/></w:rPr></w:style>
<w:style w:type="paragraph" w:styleId="Heading1"><w:name w:val="heading 1"/><w:basedOn w:val="Normal"/><w:pPr><w:keepNext/><w:spacing w:before="280" w:after="100"/><w:outlineLvl w:val="0"/></w:pPr><w:rPr><w:b/><w:color w:val="2563DE"/><w:sz w:val="30"/></w:rPr></w:style>
<w:style w:type="paragraph" w:styleId="Heading2"><w:name w:val="heading 2"/><w:basedOn w:val="Normal"/><w:pPr><w:keepNext/><w:spacing w:before="160" w:after="60"/><w:outlineLvl w:val="1"/></w:pPr><w:rPr><w:b/><w:color w:val="1A2233"/><w:sz w:val="24"/></w:rPr></w:style>
<w:style w:type="table" w:styleId="MBTable"><w:name w:val="ManageBIM Table"/><w:tblPr><w:tblBorders><w:top w:val="single" w:sz="4" w:color="C8D0E0"/><w:bottom w:val="single" w:sz="4" w:color="C8D0E0"/><w:insideH w:val="single" w:sz="4" w:color="E2E6EF"/></w:tblBorders><w:tblCellMar><w:left w:w="80" w:type="dxa"/><w:right w:w="80" w:type="dxa"/></w:tblCellMar></w:tblPr></w:style>
</w:styles>"#;
    let content_types = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/><Override PartName="/word/styles.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml"/></Types>"#;
    let rels = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>"#;
    let doc_rels = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.xml"/></Relationships>"#;
    let mut z = Zip::default();
    z.add("[Content_Types].xml", content_types.as_bytes());
    z.add("_rels/.rels", rels.as_bytes());
    z.add("word/document.xml", document.as_bytes());
    z.add("word/_rels/document.xml.rels", doc_rels.as_bytes());
    z.add("word/styles.xml", styles.as_bytes());
    z.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Summary {
        Summary {
            project: "Block A".into(),
            date: "2026-11-26".into(),
            models: vec![ModelRow { name: "Arch.ifc".into(), format: "IFC".into(), elements: 120, triangles: 1872, points: 0 }],
            levels: vec![("Ground Floor".into(), 0.0), ("Level 1".into(), 3.3)],
            categories: vec![("IfcBeam".into(), 36)],
            clash: vec![ClashRow { name: "Structure vs HVAC".into(), kind: "Hard".into(), total: 10, by_status: [4, 2, 1, 2, 1], by_severity: [3, 3, 1], last_run: "today".into() }],
            viewpoints: 5,
            issue_status: vec![("Open".into(), 3), ("Closed".into(), 2)],
            issue_priority: vec![("High".into(), 2)],
            issue_assignee: vec![("Ion".into(), 3)],
            check: Some((4, 100, 7)),
            ..Default::default()
        }
    }

    #[test]
    fn kpis_and_counts() {
        let s = sample();
        assert_eq!(s.clashes_open(), 7);
        assert_eq!(s.issues_open(), 3);
        assert_eq!(thousands(1234567), "1 234 567");
    }

    #[test]
    fn pdf_and_docx_are_written() {
        let s = sample();
        let p = pdf(&s);
        let t = String::from_utf8_lossy(&p);
        assert!(t.starts_with("%PDF") && t.contains("Project dashboard") && t.contains("Structure vs HVAC"));
        let d = docx(&s);
        assert_eq!(&d[..2], b"PK");
        let z = crate::fcstd::Zip::parse(d.clone()).expect("zip");
        let doc = String::from_utf8(z.read("word/document.xml").expect("document")).unwrap();
        assert!(doc.contains("Structure vs HVAC") && doc.contains("Heading1"));
    }
}

#[cfg(test)]
mod preview {
    /// manual: MB_DASH_OUT=<folder> cargo test --lib dashboard_preview -- --ignored
    #[test]
    #[ignore]
    fn dashboard_preview() {
        let Some(dir) = std::env::var_os("MB_DASH_OUT") else { return };
        let dir = std::path::PathBuf::from(dir);
        let mut s = super::tests_sample();
        s.clash.push(super::ClashRow { name: "Headroom 2.10 m".into(), kind: "Headroom 2.10 m".into(), total: 5, by_status: [5, 0, 0, 0, 0], by_severity: [1, 0, 4], last_run: "2026-11-26".into() });
        std::fs::write(dir.join("dashboard.pdf"), super::pdf(&s)).unwrap();
        std::fs::write(dir.join("dashboard.docx"), super::docx(&s)).unwrap();
    }
}

#[cfg(test)]
pub(crate) fn tests_sample() -> Summary {
    Summary {
        project: "Block A - Residential".into(),
        author: "Florin".into(),
        date: "2026-11-26".into(),
        models: vec![
            ModelRow { name: "Block_A_Architecture_Structure.ifc".into(), format: "IFC".into(), elements: 120, triangles: 1872, points: 0 },
            ModelRow { name: "Block_A_MEP.ifc".into(), format: "IFC".into(), elements: 33, triangles: 2410, points: 0 },
        ],
        levels: vec![("Ground Floor".into(), 0.0), ("Level 1".into(), 3.3), ("Level 2".into(), 6.6)],
        categories: vec![("IfcBeam".into(), 36), ("IfcColumn".into(), 45), ("IfcWall".into(), 21), ("IfcWindow".into(), 12)],
        clash: vec![ClashRow { name: "Structure vs HVAC".into(), kind: "Hard".into(), total: 200, by_status: [120, 40, 20, 15, 5], by_severity: [60, 80, 40], last_run: "2026-11-26".into() }],
        viewpoints: 12,
        issue_status: vec![("Open".into(), 7), ("In Progress".into(), 3), ("Closed".into(), 2)],
        issue_priority: vec![("High".into(), 4), ("Normal".into(), 8)],
        issue_assignee: vec![("MEP - HVAC".into(), 6), ("Structure".into(), 4), (String::new(), 2)],
        check: Some((6, 450, 23)),
        sets: 5,
        coord_objects: 3,
        materials: 8,
        tasks: 14,
        hidden: 0,
    }
}
