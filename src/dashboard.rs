//! The project Dashboard: the summary of the models, clashes, issues, checks and the rest
//! of the data, shown in a window (ui/dashboard.rs) and exported as a PDF (A4) or Word (.docx) report.

use crate::markup::{esc, Zip};

/// JPEG image: the bytes, the width, the height
pub type Jpeg = (Vec<u8>, u32, u32);

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
    /// Critical, Major, Minor (the open results)
    pub by_severity: [usize; 3],
    pub last_run: String,
    pub tolerance: String,
    /// the test groups (name, count, open, assignees)
    pub groups: Vec<GroupRow>,
    /// all the results (for the detailed table and the image sheets)
    pub results: Vec<ResultRow>,
}

#[derive(Clone, Debug, Default)]
pub struct GroupRow {
    pub name: String,
    pub count: usize,
    pub open: usize,
    pub status: String,
    pub assigned: String,
}

#[derive(Clone, Debug, Default)]
pub struct ResultRow {
    pub name: String,
    pub group: String,
    pub status: String,
    pub severity: String,
    pub distance_mm: f32,
    pub item_a: String,
    pub item_b: String,
    pub location: String,
    pub assigned: String,
    pub comment: String,
    pub found: String,
    /// the view capture on the clash (JPEG, width, height) - only for the results chosen for sheets
    pub image: Option<Jpeg>,
}

#[derive(Clone, Debug, Default)]
pub struct Summary {
    pub project: String,
    pub author: String,
    pub date: String,
    pub models: Vec<ModelRow>,
    pub levels: Vec<(String, f64)>,
    /// the most frequent categories (name, count)
    pub categories: Vec<(String, usize)>,
    pub clash: Vec<ClashRow>,
    pub viewpoints: usize,
    /// status, priority, assignee -> count
    pub issue_status: Vec<(String, usize)>,
    pub issue_priority: Vec<(String, usize)>,
    pub issue_assignee: Vec<(String, usize)>,
    /// each viewpoint (issue) with the details and the image with markups
    pub views: Vec<VpRow>,
    /// (rules, checked elements, errors) - None if Model Checker has not run
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
    /// the main indicators: (label, value, detail)
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
    /// the image of the current page (the PDF writer has one image per page: the clash sheets)
    img: Option<Jpeg>,
}

impl Pdf {
    fn new(title: &str) -> Pdf {
        let mut p = Pdf { pages: Vec::new(), c: Vec::new(), y: 0.0, title: title.to_string(), img: None };
        p.page();
        p
    }
    fn page(&mut self) {
        if !self.c.is_empty() {
            self.pages.push(crate::pdf::Page { content: std::mem::take(&mut self.c), image: self.img.take() });
        }
        // the top strip
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
                // cut to the column width
                let mut s = v.clone();
                if crate::pdf::text_w(&s, 8.5) > w - 6.0 {
                    // "..." (the standard PDF font does not have the ellipsis character)
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
    /// horizontal bar split into colored segments, with legend
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
    /// the image (JPEG) on the current page, in the given rectangle (points), with a thin border
    #[allow(clippy::too_many_arguments)]
    fn image(&mut self, jpg: &[u8], w: u32, h: u32, x: f32, y: f32, dw: f32, dh: f32) {
        self.img = Some((jpg.to_vec(), w, h));
        self.c.extend_from_slice(format!("q {dw:.2} 0 0 {dh:.2} {x:.2} {y:.2} cm /Im1 Do Q\n").as_bytes());
        rgb(&mut self.c, [200, 205, 215], false);
        self.c.extend_from_slice(format!("0.6 w {x:.2} {y:.2} {dw:.2} {dh:.2} re S\n").as_bytes());
    }
    /// label rows: value (with the long text split over several rows)
    fn fields(&mut self, rows: &[(&str, String)]) {
        let lw = 92.0;
        let vw = PW - 2.0 * M - lw;
        for (k, v) in rows {
            let lines = crate::pdf::wrap(if v.is_empty() { "-" } else { v }, 9.0, vw - 8.0);
            let h = 13.0 * lines.len() as f32 + 4.0;
            self.need(h);
            rect(&mut self.c, M, self.y - h + 9.0, lw, h, [244, 247, 253]);
            txt(&mut self.c, M + 6.0, self.y, 8.5, true, [70, 80, 100], k);
            for (i, l) in lines.iter().enumerate() {
                txt(&mut self.c, M + lw + 8.0, self.y - 13.0 * i as f32, 9.0, false, [30, 30, 35], l);
            }
            self.y -= h;
        }
    }
    fn finish(mut self) -> Vec<u8> {
        let n = {
            self.pages.push(crate::pdf::Page { content: std::mem::take(&mut self.c), image: self.img.take() });
            self.pages.len()
        };
        // page numbering
        for (i, p) in self.pages.iter_mut().enumerate() {
            let s = format!("Page {} / {n}", i + 1);
            txt(&mut p.content, PW - M - crate::pdf::text_w(&s, 8.0), 28.0, 8.0, false, [140, 140, 140], &s);
        }
        crate::pdf::write_pdf(&self.pages, &self.title, PW, PH)
    }
}

pub fn pdf(s: &Summary) -> Vec<u8> {
    let mut p = Pdf::new(&format!("Project dashboard - {}", s.project));
    // title
    txt(&mut p.c, M, p.y - 14.0, 22.0, true, [20, 25, 35], "Project dashboard");
    p.y -= 34.0;
    txt(&mut p.c, M, p.y, 11.0, false, [90, 95, 105], &format!("{}  -  {}  -  {}", s.project, s.date, if s.author.is_empty() { "ManageBIM" } else { &s.author }));
    p.y -= 26.0;
    // indicators (2 rows x 3)
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

    // clashes
    p.heading("Clash Detective");
    if s.clash.is_empty() {
        txt(&mut p.c, M, p.y, 9.0, false, [110, 110, 120], "No clash tests.");
        p.y -= 16.0;
    } else {
        let mut all = [0usize; 5];
        for c in &s.clash {
            for (a, b) in all.iter_mut().zip(c.by_status.iter()) {
                *a += *b;
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

    // models
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

    // ---- the details of each test: groups and all results
    for c in s.clash.iter().filter(|c| !c.results.is_empty()) {
        p.page();
        p.heading(&format!("Clash test - {}", c.name));
        txt(&mut p.c, M, p.y, 9.0, false, [90, 95, 105], &format!("{} · tolerance {} · {} results · last run {}", c.kind, c.tolerance, c.total, if c.last_run.is_empty() { "-" } else { &c.last_run }));
        p.y -= 18.0;
        let parts: Vec<(String, usize, [u8; 3])> = (0..5).map(|k| (STATUS[k].to_string(), c.by_status[k], STATUS_RGB[k])).collect();
        p.stacked("By status", &parts);
        let sev: Vec<(String, usize, [u8; 3])> = (0..3).map(|k| (SEVERITY[k].to_string(), c.by_severity[k], SEVERITY_RGB[k])).collect();
        p.stacked("Open results by severity", &sev);
        if !c.groups.is_empty() {
            txt(&mut p.c, M, p.y, 10.5, true, [30, 35, 45], "Groups");
            p.y -= 14.0;
            let rows: Vec<Vec<String>> = c.groups.iter().map(|g| vec![g.name.clone(), g.count.to_string(), g.open.to_string(), g.status.clone(), g.assigned.clone()]).collect();
            p.table(&["Group", "Results", "Open", "Status", "Assigned to"], &[200.0, 50.0, 45.0, 75.0, 145.0], &rows);
        }
        txt(&mut p.c, M, p.y, 10.5, true, [30, 35, 45], "Results");
        p.y -= 14.0;
        let rows: Vec<Vec<String>> = c
            .results
            .iter()
            .map(|r| vec![r.name.clone(), r.status.clone(), r.severity.clone(), format!("{:.0}", r.distance_mm), r.item_a.clone(), r.item_b.clone(), r.location.clone(), r.assigned.clone()])
            .collect();
        p.table(&["Clash", "Status", "Severity", "mm", "Item A", "Item B", "Location", "Assigned"], &[48.0, 48.0, 48.0, 34.0, 112.0, 112.0, 60.0, 53.0], &rows);
    }

    // ---- viewpoints / issues: the table, then one sheet per viewpoint with the image and the markups
    if !s.views.is_empty() {
        p.page();
        p.heading("Viewpoints & markups");
        let rows: Vec<Vec<String>> = s
            .views
            .iter()
            .map(|v| vec![v.name.clone(), v.status.clone(), v.priority.clone(), if v.assigned.is_empty() { "-".into() } else { v.assigned.clone() }, v.markups.to_string(), v.comments.len().to_string(), v.folder.clone()])
            .collect();
        p.table(&["Viewpoint", "Status", "Priority", "Assigned to", "Markups", "Comments", "Folder"], &[150.0, 60.0, 55.0, 90.0, 48.0, 55.0, 57.0], &rows);
        for v in &s.views {
            p.page();
            txt(&mut p.c, M, p.y - 4.0, 15.0, true, [20, 25, 35], &v.name);
            let lbl = format!("{} · {}", v.status, v.priority);
            let lw = crate::pdf::text_w(&lbl, 9.0) + 16.0;
            let col = match v.status.to_ascii_lowercase().as_str() {
                "closed" | "resolved" => [60, 175, 90],
                "in progress" => [240, 150, 40],
                _ => [230, 70, 60],
            };
            rect(&mut p.c, PW - M - lw, p.y - 8.0, lw, 16.0, col);
            txt(&mut p.c, PW - M - lw + 8.0, p.y - 3.0, 9.0, true, [255, 255, 255], &lbl);
            p.y -= 24.0;
            if let Some((jpg, w, h)) = &v.image {
                let dw = PW - 2.0 * M;
                let dh = (dw * *h as f32 / (*w).max(1) as f32).min(360.0);
                let dw2 = dh * *w as f32 / (*h).max(1) as f32;
                p.image(jpg, *w, *h, M + (dw - dw2) / 2.0, p.y - dh, dw2, dh);
                p.y -= dh + 18.0;
            }
            let mut f = vec![
                ("Assigned to", v.assigned.clone()),
                ("Author", v.author.clone()),
                ("Created", v.created.clone()),
                ("Folder", v.folder.clone()),
                ("Markups", v.markups.to_string()),
                ("Description", v.description.clone()),
            ];
            for (i, c) in v.comments.iter().enumerate() {
                f.push((if i == 0 { "Comments" } else { "" }, c.clone()));
            }
            p.fields(&f);
        }
    }

    // ---- the clash sheets: one page per clash, with the view image
    for c in &s.clash {
        for r in c.results.iter().filter(|r| r.image.is_some()) {
            let Some((jpg, w, h)) = &r.image else { continue };
            p.page();
            txt(&mut p.c, M, p.y - 4.0, 15.0, true, [20, 25, 35], &format!("{}  ·  {}", r.name, c.name));
            // the severity label on the right
            let k = SEVERITY.iter().position(|x| *x == r.severity).unwrap_or(2);
            let lbl = format!("{} · {}", r.severity, r.status);
            let lw = crate::pdf::text_w(&lbl, 9.0) + 16.0;
            rect(&mut p.c, PW - M - lw, p.y - 8.0, lw, 16.0, SEVERITY_RGB[k]);
            txt(&mut p.c, PW - M - lw + 8.0, p.y - 3.0, 9.0, true, [255, 255, 255], &lbl);
            p.y -= 24.0;
            let dw = PW - 2.0 * M;
            let dh = (dw * *h as f32 / (*w).max(1) as f32).min(360.0);
            let dw2 = dh * *w as f32 / (*h).max(1) as f32;
            p.image(jpg, *w, *h, M + (dw - dw2) / 2.0, p.y - dh, dw2, dh);
            p.y -= dh + 18.0;
            p.fields(&[
                ("Distance", format!("{:.0} mm", r.distance_mm)),
                ("Group", r.group.clone()),
                ("Item A", r.item_a.clone()),
                ("Item B", r.item_b.clone()),
                ("Location", r.location.clone()),
                ("Assigned to", r.assigned.clone()),
                ("Found", r.found.clone()),
                ("Comment", r.comment.clone()),
            ]);
        }
    }
    p.finish()
}

// ---------------------------------------------------------------- Word (.docx)

fn w_par(text: &str, style: &str) -> String {
    format!("<w:p><w:pPr><w:pStyle w:val=\"{style}\"/></w:pPr><w:r><w:t xml:space=\"preserve\">{}</w:t></w:r></w:p>", esc(text))
}

const W_PAGE_BREAK: &str = "<w:p><w:r><w:br w:type=\"page\"/></w:r></w:p>";

/// image `n` (word/media/clash<n>.jpeg) over the full page width (16 cm), centered
fn w_image(n: usize, w: u32, h: u32) -> String {
    let cx: u64 = 16 * 360_000;
    let cy: u64 = cx * h.max(1) as u64 / w.max(1) as u64;
    format!(
        "<w:p><w:pPr><w:jc w:val=\"center\"/></w:pPr><w:r><w:drawing><wp:inline distT=\"0\" distB=\"0\" distL=\"0\" distR=\"0\"><wp:extent cx=\"{cx}\" cy=\"{cy}\"/><wp:docPr id=\"{n}\" name=\"Clash {n}\"/><a:graphic><a:graphicData uri=\"http://schemas.openxmlformats.org/drawingml/2006/picture\"><pic:pic><pic:nvPicPr><pic:cNvPr id=\"{n}\" name=\"clash{n}.jpeg\"/><pic:cNvPicPr/></pic:nvPicPr><pic:blipFill><a:blip r:embed=\"rIdImg{n}\"/><a:stretch><a:fillRect/></a:stretch></pic:blipFill><pic:spPr><a:xfrm><a:off x=\"0\" y=\"0\"/><a:ext cx=\"{cx}\" cy=\"{cy}\"/></a:xfrm><a:prstGeom prst=\"rect\"><a:avLst/></a:prstGeom></pic:spPr></pic:pic></a:graphicData></a:graphic></wp:inline></w:drawing></w:r></w:p>"
    )
}

fn w_table(head: &[&str], rows: &[Vec<String>]) -> String {
    let cell = |t: &str, header: bool| {
        let shade = if header { "<w:shd w:val=\"clear\" w:color=\"auto\" w:fill=\"DCE6F8\"/>" } else { "" };
        let bold = if header { "<w:rPr><w:b/></w:rPr>" } else { "" };
        format!("<w:tc><w:tcPr>{shade}</w:tcPr><w:p><w:pPr><w:spacing w:before=\"40\" w:after=\"40\"/></w:pPr><w:r>{bold}<w:t xml:space=\"preserve\">{}</w:t></w:r></w:p></w:tc>", esc(t))
    };
    let mut s = String::from("<w:tbl><w:tblPr><w:tblStyle w:val=\"MBTable\"/><w:tblW w:w=\"5000\" w:type=\"pct\"/></w:tblPr>");
    // the column grid (required by the Word schema): equal width on the A4 page with 2 cm margins
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

    // ---- viewpoints / issues with the markups
    let mut media: Vec<Vec<u8>> = Vec::new();
    if !s.views.is_empty() {
        b.push_str(W_PAGE_BREAK);
        b.push_str(&w_par("Viewpoints & markups", "Heading1"));
        let rows: Vec<Vec<String>> = s.views.iter().map(|v| vec![v.name.clone(), v.status.clone(), v.priority.clone(), v.assigned.clone(), v.markups.to_string(), v.comments.len().to_string(), v.folder.clone()]).collect();
        b.push_str(&w_table(&["Viewpoint", "Status", "Priority", "Assigned to", "Markups", "Comments", "Folder"], &rows));
        for v in &s.views {
            b.push_str(&w_par(&format!("{} - {} / {}", v.name, v.status, v.priority), "Heading2"));
            if let Some((jpg, w, h)) = &v.image {
                media.push(jpg.clone());
                b.push_str(&w_image(media.len(), *w, *h));
            }
            let mut rows = vec![
                vec!["Assigned to".to_string(), v.assigned.clone()],
                vec!["Author".to_string(), v.author.clone()],
                vec!["Created".to_string(), v.created.clone()],
                vec!["Folder".to_string(), v.folder.clone()],
                vec!["Markups".to_string(), v.markups.to_string()],
                vec!["Description".to_string(), v.description.clone()],
            ];
            for c in &v.comments {
                rows.push(vec!["Comment".to_string(), c.clone()]);
            }
            b.push_str(&w_table(&["Field", "Value"], &rows));
        }
    }

    // ---- the details of the clash tests: groups, all results, the image sheets
    for c in s.clash.iter().filter(|c| !c.results.is_empty()) {
        b.push_str(W_PAGE_BREAK);
        b.push_str(&w_par(&format!("Clash test - {}", c.name), "Heading1"));
        b.push_str(&w_par(&format!("{} · tolerance {} · {} results · last run {}", c.kind, c.tolerance, c.total, if c.last_run.is_empty() { "-" } else { &c.last_run }), "Normal"));
        let st: Vec<Vec<String>> = vec![c.by_status.iter().map(|n| n.to_string()).chain(c.by_severity.iter().map(|n| n.to_string())).collect()];
        b.push_str(&w_table(&["New", "Active", "Reviewed", "Approved", "Resolved", "Critical", "Major", "Minor"], &st));
        if !c.groups.is_empty() {
            b.push_str(&w_par("Groups", "Heading2"));
            let rows: Vec<Vec<String>> = c.groups.iter().map(|g| vec![g.name.clone(), g.count.to_string(), g.open.to_string(), g.status.clone(), g.assigned.clone()]).collect();
            b.push_str(&w_table(&["Group", "Results", "Open", "Status", "Assigned to"], &rows));
        }
        b.push_str(&w_par("Results", "Heading2"));
        let rows: Vec<Vec<String>> = c
            .results
            .iter()
            .map(|r| vec![r.name.clone(), r.group.clone(), r.status.clone(), r.severity.clone(), format!("{:.0}", r.distance_mm), r.item_a.clone(), r.item_b.clone(), r.location.clone(), r.assigned.clone(), r.comment.clone()])
            .collect();
        b.push_str(&w_table(&["Clash", "Group", "Status", "Severity", "Distance (mm)", "Item A", "Item B", "Location", "Assigned to", "Comment"], &rows));
        // the sheets: the image and the details of each chosen clash
        for r in c.results.iter().filter(|r| r.image.is_some()) {
            let Some((jpg, w, h)) = &r.image else { continue };
            media.push(jpg.clone());
            let n = media.len();
            b.push_str(&w_par(&format!("{} · {} - {} / {}", r.name, c.name, r.severity, r.status), "Heading2"));
            b.push_str(&w_image(n, *w, *h));
            let rows = vec![
                vec!["Distance".to_string(), format!("{:.0} mm", r.distance_mm)],
                vec!["Group".to_string(), r.group.clone()],
                vec!["Item A".to_string(), r.item_a.clone()],
                vec!["Item B".to_string(), r.item_b.clone()],
                vec!["Location".to_string(), r.location.clone()],
                vec!["Assigned to".to_string(), r.assigned.clone()],
                vec!["Found".to_string(), r.found.clone()],
                vec!["Comment".to_string(), r.comment.clone()],
            ];
            b.push_str(&w_table(&["Field", "Value"], &rows));
        }
    }

    let document = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<w:document xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\" xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\" xmlns:wp=\"http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing\" xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\" xmlns:pic=\"http://schemas.openxmlformats.org/drawingml/2006/picture\"><w:body>{b}<w:sectPr><w:pgSz w:w=\"11906\" w:h=\"16838\"/><w:pgMar w:top=\"1134\" w:right=\"1134\" w:bottom=\"1134\" w:left=\"1134\" w:header=\"567\" w:footer=\"567\" w:gutter=\"0\"/></w:sectPr></w:body></w:document>"
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
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="jpeg" ContentType="image/jpeg"/><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/><Override PartName="/word/styles.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml"/></Types>"#;
    let rels = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>"#;
    let mut doc_rels = String::from(r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.xml"/>"#);
    for i in 1..=media.len() {
        doc_rels.push_str(&format!("<Relationship Id=\"rIdImg{i}\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/image\" Target=\"media/clash{i}.jpeg\"/>"));
    }
    doc_rels.push_str("</Relationships>");
    let mut z = Zip::default();
    z.add("[Content_Types].xml", content_types.as_bytes());
    z.add("_rels/.rels", rels.as_bytes());
    z.add("word/document.xml", document.as_bytes());
    z.add("word/_rels/document.xml.rels", doc_rels.as_bytes());
    z.add("word/styles.xml", styles.as_bytes());
    for (i, m) in media.iter().enumerate() {
        z.add(&format!("word/media/clash{}.jpeg", i + 1), m);
    }
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
            clash: vec![ClashRow { name: "Structure vs HVAC".into(), kind: "Hard".into(), total: 10, by_status: [4, 2, 1, 2, 1], by_severity: [3, 3, 1], last_run: "today".into(), ..Default::default() }],
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
        s.clash.push(super::ClashRow { name: "Headroom 2.10 m".into(), kind: "Headroom 2.10 m".into(), total: 5, by_status: [5, 0, 0, 0, 0], by_severity: [1, 0, 4], last_run: "2026-11-26".into(), ..Default::default() });
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
        clash: vec![ClashRow { name: "Structure vs HVAC".into(), kind: "Hard".into(), total: 200, by_status: [120, 40, 20, 15, 5], by_severity: [60, 80, 40], last_run: "2026-11-26".into(), tolerance: "10 mm".into(), groups: sample_groups(), results: sample_results() }],
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
        views: sample_views(),
    }
}

#[cfg(test)]
fn sample_views() -> Vec<VpRow> {
    let img = sample_results().into_iter().find_map(|r| r.image);
    vec![
        VpRow { name: "Duct through beam B12".into(), folder: "Level 1".into(), status: "Open".into(), priority: "High".into(), assigned: "MEP - HVAC".into(), author: "Florin".into(), created: "26 Nov 2026".into(), description: "Reroute the supply duct under the beam or add a sleeve.".into(), comments: vec!["Ana (26 Nov 2026): Sleeve not allowed here, reroute.".into()], markups: 3, image: img.clone() },
        VpRow { name: "Missing fire damper".into(), folder: String::new(), status: "In Progress".into(), priority: "Normal".into(), assigned: "MEP - HVAC".into(), author: "Florin".into(), created: "25 Nov 2026".into(), description: String::new(), comments: Vec::new(), markups: 0, image: img },
    ]
}

#[cfg(test)]
fn sample_groups() -> Vec<GroupRow> {
    vec![
        GroupRow { name: "Level 1 - Ducts vs Beams".into(), count: 12, open: 10, status: "Active".into(), assigned: "MEP - HVAC".into() },
        GroupRow { name: "Level 2 - Pipes vs Slab".into(), count: 4, open: 4, status: "New".into(), assigned: "Structure".into() },
    ]
}

#[cfg(test)]
fn sample_results() -> Vec<ResultRow> {
    // a small test image (gradient) as JPEG
    let (w, h) = (320u32, 180u32);
    let mut rgb = Vec::with_capacity((w * h * 3) as usize);
    for y in 0..h {
        for x in 0..w {
            rgb.extend_from_slice(&[(x * 255 / w) as u8, (y * 255 / h) as u8, 160]);
        }
    }
    let mut jpg = Vec::new();
    jpeg_encoder::Encoder::new(&mut jpg, 85).encode(&rgb, w as u16, h as u16, jpeg_encoder::ColorType::Rgb).unwrap();
    (1..=30)
        .map(|i| ResultRow {
            name: format!("Clash{i}"),
            group: if i <= 12 { "Level 1 - Ducts vs Beams".into() } else { String::new() },
            status: if i % 3 == 0 { "Active".into() } else { "New".into() },
            severity: ["Critical", "Major", "Minor"][i % 3].into(),
            distance_mm: -(i as f32) * 7.5,
            item_a: format!("Beam B{i}-2 L1 [Block_A_Architecture_Structure.ifc]"),
            item_b: format!("Duct Branch SA-0{i} 300x250 [Block_A_MEP.ifc]"),
            location: "C-4 / Level 1".into(),
            assigned: "MEP - HVAC".into(),
            comment: if i == 1 { "Lower the duct by 150 mm under the beam; check the ceiling void.".into() } else { String::new() },
            found: "2026-11-26 08:44 UTC".into(),
            image: if i <= 2 { Some((jpg.clone(), w, h)) } else { None },
        })
        .collect()
}

/// The export with images: the clashes to photograph (test, result), one by one, one capture per frame.
pub struct CaptureJob {
    pub pdf: bool,
    pub path: String,
    pub queue: Vec<(usize, usize)>,
    pub idx: usize,
    /// 0 = the next clash to show, >0 = frames to wait, then the capture
    pub wait: u32,
    pub waiting: bool,
    pub images: Vec<((usize, usize), Jpeg)>,
    pub saved_cam: crate::camera::Camera,
    pub saved_bottom: bool,
    pub saved_test: usize,
    /// the viewpoints photographed after the clashes (with the markups drawn over the image)
    pub views: Vec<usize>,
    pub view_images: Vec<(usize, Jpeg)>,
    pub saved_section: crate::app::Section,
    pub saved_hidden: std::collections::HashSet<crate::model::ElemId>,
    pub saved_selected: std::collections::BTreeSet<crate::model::ElemId>,
    pub saved_vp: Option<usize>,
}

/// Which clashes get an image sheet: the open ones (New / Active / Reviewed); when the test has groups,
/// the first of each group plus the ungrouped ones. At most `per_test` per test and `total` in the whole report.
pub fn pick_images(tests: &[(Vec<String>, Vec<bool>)], per_test: usize, total: usize) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    for (ti, (groups, open)) in tests.iter().enumerate() {
        let mut seen: std::collections::HashSet<&str> = Default::default();
        let mut n = 0;
        for (ri, g) in groups.iter().enumerate() {
            if !open[ri] || n >= per_test || out.len() >= total {
                continue;
            }
            if !g.is_empty() && !seen.insert(g.as_str()) {
                continue;
            }
            out.push((ti, ri));
            n += 1;
        }
    }
    out
}

#[cfg(test)]
mod pick_tests {
    #[test]
    fn one_per_group_plus_ungrouped_open_only() {
        let groups = vec!["G1".to_string(), "G1".into(), "".into(), "G2".into(), "".into()];
        let open = vec![true, true, true, true, false];
        let p = super::pick_images(&[(groups, open)], 25, 60);
        assert_eq!(p, vec![(0, 0), (0, 2), (0, 3)]);
    }
}

/// A viewpoint / issue in the report, with the markups drawn on the image.
#[derive(Clone, Debug, Default)]
pub struct VpRow {
    pub name: String,
    pub folder: String,
    pub status: String,
    pub priority: String,
    pub assigned: String,
    pub author: String,
    pub created: String,
    pub description: String,
    /// "author (date): text"
    pub comments: Vec<String>,
    pub markups: usize,
    pub image: Option<Jpeg>,
}
