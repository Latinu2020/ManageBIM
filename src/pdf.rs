//! Plansele PDF din viewpoints: o pagina A3 landscape pe viewpoint - imaginea mare, descrierea si
//! comentariile, cartusul (proiect, viewpoint, stare, responsabil, data, numarul plansei).
//! Scriitor PDF minimal, fara dependinte: imagini JPEG (DCTDecode), fonturile standard Helvetica.

use crate::markup::ReportItem;

const W: f32 = 1190.55; // A3 landscape, puncte
const H: f32 = 841.89;
const M: f32 = 28.0;
const STRIP: f32 = 150.0;

/// textul in WinAnsi (fonturile standard nu au diacriticele romanesti s, t cu virgula)
fn ansi(s: &str) -> Vec<u8> {
    let mut out = Vec::new();
    for c in s.chars() {
        let c = match c {
            'ă' | 'â' => 'a',
            'Ă' | 'Â' => 'A',
            'î' => 'i',
            'Î' => 'I',
            'ș' | 'ş' => 's',
            'Ș' | 'Ş' => 'S',
            'ț' | 'ţ' => 't',
            'Ț' | 'Ţ' => 'T',
            '–' | '—' => '-',
            '“' | '”' | '„' => '"',
            '‘' | '’' => '\'',
            '·' => '-',
            '×' => 'x',
            c => c,
        };
        let b = c as u32;
        let b = if b < 256 { b as u8 } else { b'?' };
        if matches!(b, b'(' | b')' | b'\\') {
            out.push(b'\\');
        }
        if b == b'\n' || b == b'\r' {
            out.push(b' ');
            continue;
        }
        out.push(b);
    }
    out
}

/// latimea aproximativa a textului Helvetica (puncte)
fn text_w(s: &str, size: f32) -> f32 {
    s.chars().map(|c| if c.is_ascii_uppercase() || c == 'm' || c == 'w' { 0.68 } else if c == ' ' || c == 'i' || c == 'l' || c == '.' || c == ',' { 0.28 } else { 0.52 }).sum::<f32>() * size
}

/// imparte textul in randuri de cel mult `max_w` puncte
fn wrap(s: &str, size: f32, max_w: f32) -> Vec<String> {
    let mut out = Vec::new();
    for para in s.split('\n') {
        let mut line = String::new();
        for word in para.split_whitespace() {
            let cand = if line.is_empty() { word.to_string() } else { format!("{line} {word}") };
            if text_w(&cand, size) > max_w && !line.is_empty() {
                out.push(std::mem::take(&mut line));
                line = word.to_string();
            } else {
                line = cand;
            }
        }
        out.push(line);
    }
    out
}

struct Page {
    content: Vec<u8>,
    image: Option<(Vec<u8>, u32, u32)>,
}

fn text(c: &mut Vec<u8>, x: f32, y: f32, size: f32, bold: bool, s: &str) {
    c.extend_from_slice(format!("BT /{} {size:.1} Tf {x:.1} {y:.1} Td (", if bold { "F2" } else { "F1" }).as_bytes());
    c.extend_from_slice(&ansi(s));
    c.extend_from_slice(b") Tj ET\n");
}

/// Raport PDF pentru Clash Detective: antet, tabel cu rezultatele testului curent si cartus.
/// Foloseste acelasi scriitor PDF ca plansele de viewpoint (fonturi standard, fara dependinte).
pub fn clash_report_pdf(project: &str, test: &str, rows: &[(String, String, String, String, String)], date: &str, tb: &TitleBlock) -> Vec<u8> {
    const PW: f32 = 841.89; // A4 landscape
    const PH: f32 = 595.28;
    const MG: f32 = 28.0;
    const ROW: f32 = 13.0;
    const TOP: f32 = PH - 100.0; // primul rand de date
    const BOTTOM: f32 = 70.0; // deasupra cartusului
    let per_page = (((TOP - BOTTOM) / ROW) as usize).max(1);
    let n_pages = rows.len().div_ceil(per_page).max(1);
    let cols = [MG, MG + 35.0, MG + 115.0, MG + 195.0, MG + 455.0, MG + 545.0];
    let heads = ["No", "Status", "Severity", "Elements", "Distance", "Group"];
    // textul care ar intra peste coloana urmatoare se scurteaza cu "..."
    let fit = |s: &str, w: f32| -> String {
        if text_w(s, 7.5) <= w {
            return s.to_string();
        }
        let mut t: String = s.to_string();
        while !t.is_empty() && text_w(&format!("{t}..."), 7.5) > w {
            t.pop();
        }
        format!("{t}...")
    };
    let mut pages = Vec::new();
    for p in 0..n_pages {
        let mut c = Vec::new();
        // antet (pe fiecare pagina)
        text(&mut c, MG, PH - 34.0, 15.0, true, "CLASH DETECTIVE - REPORT");
        text(&mut c, MG, PH - 50.0, 9.0, false, &format!("Project: {}", tb.project_or(project)));
        text(&mut c, MG, PH - 62.0, 9.0, false, &format!("Test: {test}    Date: {date}    Clashes: {}", rows.len()));
        c.extend_from_slice(format!("0.5 w {MG} {:.1} m {:.1} {:.1} l S\n", PH - 70.0, PW - MG, PH - 70.0).as_bytes());
        for (x, h) in cols.iter().zip(heads) {
            text(&mut c, *x, PH - 86.0, 8.5, true, h);
        }
        let mut y = TOP;
        for (i, r) in rows.iter().enumerate().skip(p * per_page).take(per_page) {
            let vals = [format!("{}", i + 1), r.0.clone(), r.1.clone(), r.2.clone(), r.3.clone(), r.4.clone()];
            for (k, (x, v)) in cols.iter().zip(vals).enumerate() {
                let w = cols.get(k + 1).map_or(PW - MG - x, |nx| nx - x - 6.0);
                text(&mut c, *x, y, 7.5, false, &fit(&v, w));
            }
            y -= ROW;
        }
        // cartus
        let ty = 24.0;
        c.extend_from_slice(format!("0.5 w {MG} {ty} m {:.1} {ty} l {:.1} {:.1} l {MG} {:.1} l h S\n", PW - MG, PW - MG, ty + 30.0, ty + 30.0).as_bytes());
        text(&mut c, MG + 6.0, ty + 18.0, 10.0, true, tb.project_or(project));
        text(&mut c, MG + 6.0, ty + 7.0, 7.5, false, &format!("Clash report - {test}"));
        text(&mut c, PW - MG - 150.0, ty + 18.0, 8.0, false, &format!("Page {} / {n_pages}  |  {}", p + 1, tb.paper));
        text(&mut c, PW - MG - 150.0, ty + 7.0, 8.0, false, &format!("Rev {}  |  {}", tb.revision, date));
        pages.push(Page { content: c, image: None });
    }
    write_pdf(&pages, tb.project_or(project), PW, PH)
}

/// PDF-ul cu cate o plansa pentru fiecare viewpoint.
pub fn sheets(project: &str, items: &[ReportItem], date: &str, tb: &TitleBlock) -> Vec<u8> {
    let mut pages: Vec<Page> = Vec::new();
    let n = items.len();
    for (k, it) in items.iter().enumerate() {
        let mut c: Vec<u8> = Vec::new();
        // chenarul plansei
        c.extend_from_slice(format!("0.15 0.17 0.2 RG 1.2 w {M} {M} {:.1} {:.1} re S\n", W - 2.0 * M, H - 2.0 * M).as_bytes());
        // linia de deasupra benzii de jos
        c.extend_from_slice(format!("0.6 w {M} {:.1} m {:.1} {:.1} l S\n", M + STRIP, W - M, M + STRIP).as_bytes());
        // imaginea: incadrata in zona de sus, cu proportiile pastrate
        let (aw, ah) = (W - 2.0 * M - 16.0, H - 2.0 * M - STRIP - 16.0);
        let img = crate::app::png_rgb(&it.image).and_then(|(rgb, w, h)| {
            let mut jpg = Vec::new();
            let enc = jpeg_encoder::Encoder::new(&mut jpg, 88);
            enc.encode(&rgb, w as u16, h as u16, jpeg_encoder::ColorType::Rgb).ok()?;
            Some((jpg, w, h))
        });
        if let Some((_, w, h)) = &img {
            let s = (aw / *w as f32).min(ah / *h as f32);
            let (iw, ih) = (*w as f32 * s, *h as f32 * s);
            let x = M + 8.0 + (aw - iw) / 2.0;
            let y = M + STRIP + 8.0 + (ah - ih) / 2.0;
            c.extend_from_slice(format!("q {iw:.2} 0 0 {ih:.2} {x:.2} {y:.2} cm /Im1 Do Q\n").as_bytes());
        } else {
            text(&mut c, M + 20.0, H / 2.0, 14.0, false, "(no image)");
        }
        // ---- cartusul (dreapta jos): firma / proiect, viewpoint, stare, desenat / verificat, numar si revizie
        let tb_w = 380.0;
        let tx = W - M - tb_w;
        c.extend_from_slice(format!("0.6 w {tx:.1} {M} m {tx:.1} {:.1} l S\n", M + STRIP).as_bytes());
        for y in [M + STRIP - 38.0, M + 78.0, M + 40.0] {
            c.extend_from_slice(format!("0.4 w {tx:.1} {y:.1} m {:.1} {y:.1} l S\n", W - M).as_bytes());
        }
        let sx = W - M - 90.0;
        c.extend_from_slice(format!("0.4 w {sx:.1} {M} m {sx:.1} {:.1} l S\n", M + 40.0).as_bytes());
        if !tb.company.trim().is_empty() {
            text(&mut c, tx + 10.0, M + STRIP - 13.0, 7.5, false, &tb.company.to_uppercase());
        }
        text(&mut c, tx + 10.0, M + STRIP - 30.0, 14.0, true, tb.project_or(project));
        if !tb.client.trim().is_empty() {
            text(&mut c, tx + 230.0, M + STRIP - 13.0, 7.5, false, &format!("Client: {}", tb.client));
        }
        text(&mut c, tx + 10.0, M + STRIP - 52.0, 7.5, false, "VIEWPOINT");
        let title = format!("{}. {}", it.index, it.name);
        text(&mut c, tx + 10.0, M + STRIP - 67.0, 12.0, true, &wrap(&title, 12.0, tb_w - 20.0).into_iter().next().unwrap_or_default());
        let meta = format!("Status: {}   Priority: {}   Assigned: {}", it.issue.status, it.issue.priority, if it.issue.assigned.is_empty() { "-" } else { &it.issue.assigned });
        text(&mut c, tx + 10.0, M + 64.0, 8.0, false, &meta);
        let who = format!("Drawn: {}   Checked: {}", if tb.drawn.is_empty() { &it.issue.author } else { &tb.drawn }, if tb.checked.is_empty() { "-" } else { &tb.checked });
        text(&mut c, tx + 10.0, M + 51.0, 8.0, false, &who);
        text(&mut c, tx + 10.0, M + 25.0, 8.0, false, &format!("Date: {}   Rev: {}", date.get(..10).unwrap_or(date), if tb.revision.is_empty() { "-" } else { &tb.revision }));
        if !tb.notes.trim().is_empty() {
            text(&mut c, tx + 10.0, M + 12.0, 7.0, false, &wrap(&tb.notes, 7.0, 260.0).into_iter().next().unwrap_or_default());
        } else {
            text(&mut c, tx + 10.0, M + 12.0, 7.0, false, "ManageBIM");
        }
        let sheet = format!("{}{:02}", tb.number_prefix, k + 1);
        text(&mut c, W - M - 45.0 - text_w(&sheet, 16.0) / 2.0, M + 18.0, 16.0, true, &sheet);
        text(&mut c, W - M - 60.0, M + 6.0, 6.5, false, &format!("sheet {} of {}", k + 1, n));
        // ---- descriere si comentarii (stanga jos)
        let lw = W - 2.0 * M - tb_w - 24.0;
        let mut y = M + STRIP - 20.0;
        text(&mut c, M + 12.0, y, 8.0, true, "DESCRIPTION");
        y -= 13.0;
        let mut lines: Vec<(String, bool)> = wrap(if it.description.is_empty() { "-" } else { &it.description }, 9.0, lw).into_iter().map(|l| (l, false)).collect();
        if !it.comments.is_empty() {
            lines.push((String::new(), false));
            lines.push(("COMMENTS".into(), true));
            for cm in &it.comments {
                for (i, l) in wrap(&format!("{} ({}): {}", cm.author, cm.date.get(..10).unwrap_or(&cm.date), cm.text), 9.0, lw).into_iter().enumerate() {
                    lines.push((if i == 0 { l } else { format!("   {l}") }, false));
                }
            }
        }
        for (l, bold) in lines {
            if y < M + 10.0 {
                break;
            }
            text(&mut c, M + 12.0, y, if bold { 8.0 } else { 9.0 }, bold, &l);
            y -= if bold { 13.0 } else { 11.5 };
        }
        // formatul ales (A2 / A1 / A0 au aceleasi proportii ca A3): plansa se scaleaza intreaga
        let s = (paper_mm(&tb.paper).0 / 420.0) as f32;
        let mut full = format!("q {s:.4} 0 0 {s:.4} 0 0 cm\n").into_bytes();
        full.extend_from_slice(&c);
        full.extend_from_slice(b"Q\n");
        pages.push(Page { content: full, image: img });
    }
    let s = (paper_mm(&tb.paper).0 / 420.0) as f32;
    write_pdf(&pages, tb.project_or(project), W * s, H * s)
}

/// Plansa PDF cu taietura 2D a sectiunii (linii vectoriale + etichete), cu cartus.
/// Foloseste aceleasi segmente si texte ca `cut_dxf_annot`, deci pozitia anotatiilor
/// e exact cea din DWG.
pub fn section_sheet(segs: &[(String, [f64; 4])], texts: &[crate::app::CadText], tb: &TitleBlock, project: &str, title: &str, date: &str) -> Vec<u8> {
    let mut c: Vec<u8> = Vec::new();
    // chenarul plansei si linia de deasupra benzii de jos
    c.extend_from_slice(format!("0.15 0.17 0.2 RG 1.2 w {M} {M} {:.1} {:.1} re S\n", W - 2.0 * M, H - 2.0 * M).as_bytes());
    c.extend_from_slice(format!("0.6 w {M} {:.1} m {:.1} {:.1} l S\n", M + STRIP, W - M, M + STRIP).as_bytes());

    let (mut x0, mut y0, mut x1, mut y1) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
    for (_, p) in segs {
        x0 = x0.min(p[0]).min(p[2]);
        y0 = y0.min(p[1]).min(p[3]);
        x1 = x1.max(p[0]).max(p[2]);
        y1 = y1.max(p[1]).max(p[3]);
    }
    if !segs.is_empty() {
        let aw = (W - 2.0 * M - 32.0) as f64;
        let ah = (H - 2.0 * M - STRIP - 32.0) as f64;
        let sc = (aw / (x1 - x0).max(1e-6)).min(ah / (y1 - y0).max(1e-6));
        let ox = M as f64 + 16.0 + (aw - (x1 - x0) * sc) / 2.0 - x0 * sc;
        let oy = (M + STRIP) as f64 + 16.0 + (ah - (y1 - y0) * sc) / 2.0 - y0 * sc;
        let map = |x: f64, y: f64| -> (f32, f32) { ((x * sc + ox) as f32, (y * sc + oy) as f32) };
        // grosimi ca in desenul tehnic (ISO 128, stil Inventor): conturul taiat gros, muchiile vizibile
        // medii, cele ascunse subtiri si intrerupte, hasura / cotele / axele fine.
        // Ordinea: intai liniile fine, la sfarsit contururile groase (raman deasupra).
        let rank = |l: &str| match l.to_ascii_uppercase().as_str() {
            "HATCH" | "GRIDS" | "LEVELS" | "DIMS" => 0,
            "HIDDEN" | "BEHIND" => 1,
            "VISIBLE" => 2,
            _ => 3,
        };
        let mut order: Vec<usize> = (0..segs.len()).collect();
        order.sort_by_key(|&i| rank(&segs[i].0));
        // capete si colturi rotunjite: liniile groase se imbina curat
        c.extend_from_slice(b"1 J 1 j
");
        let mut last = "";
        for i in order {
            let (layer, p) = &segs[i];
            let (ax, ay) = map(p[0], p[1]);
            let (bx, by) = map(p[2], p[3]);
            let style = match rank(layer) {
                0 if layer.eq_ignore_ascii_case("HATCH") => "[] 0 d 0.62 0.64 0.68 RG 0.25 w ",
                0 => "[] 0 d 0.35 0.40 0.50 RG 0.3 w ",
                1 => "[2.4 1.6] 0 d 0.40 0.42 0.46 RG 0.4 w ",
                2 => "[] 0 d 0.08 0.09 0.11 RG 0.55 w ",
                _ => "[] 0 d 0.0 0.0 0.0 RG 1.1 w ",
            };
            if style != last {
                c.extend_from_slice(style.as_bytes());
                last = style;
            }
            c.extend_from_slice(format!("{ax:.2} {ay:.2} m {bx:.2} {by:.2} l S\n").as_bytes());
        }
        c.extend_from_slice(b"[] 0 d\n");
        // etichetele: CadText.mm e inaltimea pe hartie (1 mm = 72/25.4 pt)
        for t in texts {
            let (x, y) = map(t.x, t.y);
            let size = (t.mm * 72.0 / 25.4) as f32;
            let r = (t.rot as f32).to_radians();
            let (co, si) = (r.cos(), r.sin());
            c.extend_from_slice(format!("BT /F1 {size:.1} Tf {co:.4} {si:.4} {:.4} {co:.4} {x:.1} {y:.1} Tm (", -si).as_bytes());
            c.extend_from_slice(&ansi(&t.s));
            c.extend_from_slice(b") Tj ET\n");
        }
    }
    // ---- cartusul (acelasi continut ca la plansele din viewpoints)
    let tb_w = 380.0;
    let tx = W - M - tb_w;
    c.extend_from_slice(format!("0.6 w {tx:.1} {M} m {tx:.1} {:.1} l S\n", M + STRIP).as_bytes());
    for y in [M + STRIP - 38.0, M + 78.0, M + 40.0] {
        c.extend_from_slice(format!("0.4 w {tx:.1} {y:.1} m {:.1} {y:.1} l S\n", W - M).as_bytes());
    }
    c.extend_from_slice(format!("0.4 w {:.1} {M} m {:.1} {:.1} l S\n", W - M - 90.0, W - M - 90.0, M + 40.0).as_bytes());
    if !tb.company.trim().is_empty() {
        text(&mut c, tx + 10.0, M + STRIP - 13.0, 7.5, false, &tb.company.to_uppercase());
    }
    text(&mut c, tx + 10.0, M + STRIP - 30.0, 14.0, true, tb.project_or(project));
    if !tb.client.trim().is_empty() {
        text(&mut c, tx + 230.0, M + STRIP - 13.0, 7.5, false, &format!("Client: {}", tb.client));
    }
    text(&mut c, tx + 10.0, M + STRIP - 52.0, 7.5, false, "SECTION");
    text(&mut c, tx + 10.0, M + STRIP - 67.0, 12.0, true, &wrap(title, 12.0, tb_w - 20.0).into_iter().next().unwrap_or_default());
    text(&mut c, tx + 10.0, M + 64.0, 8.0, false, &format!("Drawn: {}   Checked: {}", if tb.drawn.is_empty() { "-" } else { &tb.drawn }, if tb.checked.is_empty() { "-" } else { &tb.checked }));
    text(&mut c, tx + 10.0, M + 46.0, 8.0, false, &format!("Date: {}   Rev: {}", date.get(..10).unwrap_or(date), if tb.revision.is_empty() { "-" } else { &tb.revision }));
    if !tb.notes.trim().is_empty() {
        text(&mut c, tx + 10.0, M + 25.0, 7.0, false, &wrap(&tb.notes, 7.0, 260.0).into_iter().next().unwrap_or_default());
    } else {
        text(&mut c, tx + 10.0, M + 25.0, 7.0, false, "ManageBIM");
    }
    text(&mut c, W - M - 45.0, M + 18.0, 16.0, true, &format!("{}{:02}", tb.number_prefix, 1));

    let s = (paper_mm(&tb.paper).0 / 420.0) as f32;
    write_pdf(&[Page { content: c, image: None }], tb.project_or(project), W * s, H * s)
}

fn write_pdf(pages: &[Page], title: &str, pw: f32, ph: f32) -> Vec<u8> {
    let mut out: Vec<u8> = b"%PDF-1.4\n%\xE2\xE3\xCF\xD3\n".to_vec();
    let mut offs: Vec<usize> = Vec::new();
    // obiecte: 1 catalog, 2 pagini, 3 F1, 4 F2, 5 info, apoi pe pagina: pagina, continut, imagine
    let page_ids: Vec<usize> = (0..pages.len()).map(|i| 6 + i * 3).collect();
    let obj = |out: &mut Vec<u8>, offs: &mut Vec<usize>, id: usize, body: &[u8]| {
        while offs.len() < id {
            offs.push(0);
        }
        offs[id - 1] = out.len();
        out.extend_from_slice(format!("{id} 0 obj\n").as_bytes());
        out.extend_from_slice(body);
        out.extend_from_slice(b"\nendobj\n");
    };
    obj(&mut out, &mut offs, 1, b"<< /Type /Catalog /Pages 2 0 R >>");
    let kids: Vec<String> = page_ids.iter().map(|i| format!("{i} 0 R")).collect();
    obj(&mut out, &mut offs, 2, format!("<< /Type /Pages /Kids [{}] /Count {} >>", kids.join(" "), pages.len()).as_bytes());
    obj(&mut out, &mut offs, 3, b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>");
    obj(&mut out, &mut offs, 4, b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica-Bold /Encoding /WinAnsiEncoding >>");
    let mut info = b"<< /Producer (ManageBIM) /Title (".to_vec();
    info.extend_from_slice(&ansi(title));
    info.extend_from_slice(b") >>");
    obj(&mut out, &mut offs, 5, &info);
    for (i, p) in pages.iter().enumerate() {
        let pid = page_ids[i];
        let res = if p.image.is_some() { format!("/XObject << /Im1 {} 0 R >> ", pid + 2) } else { String::new() };
        obj(&mut out, &mut offs, pid, format!("<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {pw:.2} {ph:.2}] /Resources << /Font << /F1 3 0 R /F2 4 0 R >> {res}>> /Contents {} 0 R >>", pid + 1).as_bytes());
        let mut body = format!("<< /Length {} >>\nstream\n", p.content.len()).into_bytes();
        body.extend_from_slice(&p.content);
        body.extend_from_slice(b"\nendstream");
        obj(&mut out, &mut offs, pid + 1, &body);
        let mut ib: Vec<u8> = Vec::new();
        if let Some((jpg, w, h)) = &p.image {
            ib.extend_from_slice(format!("<< /Type /XObject /Subtype /Image /Width {w} /Height {h} /ColorSpace /DeviceRGB /BitsPerComponent 8 /Filter /DCTDecode /Length {} >>\nstream\n", jpg.len()).as_bytes());
            ib.extend_from_slice(jpg);
            ib.extend_from_slice(b"\nendstream");
        } else {
            ib.extend_from_slice(b"null");
        }
        obj(&mut out, &mut offs, pid + 2, &ib);
    }
    let xref = out.len();
    out.extend_from_slice(format!("xref\n0 {}\n0000000000 65535 f \n", offs.len() + 1).as_bytes());
    for o in &offs {
        out.extend_from_slice(format!("{o:010} 00000 n \n").as_bytes());
    }
    out.extend_from_slice(format!("trailer\n<< /Size {} /Root 1 0 R /Info 5 0 R >>\nstartxref\n{xref}\n%%EOF\n", offs.len() + 1).as_bytes());
    out
}

#[cfg(test)]
mod tests {
    #[test]
    fn wraps_and_escapes() {
        assert_eq!(super::ansi("a(b)"), b"a\\(b\\)".to_vec());
        assert_eq!(super::ansi("ș"), b"s".to_vec());
        assert!(super::wrap("one two three four five six", 9.0, 40.0).len() > 1);
    }

    /// raportul de clash continua pe pagini noi (inainte se oprea dupa prima pagina)
    #[test]
    fn clash_report_has_all_pages() {
        let rows: Vec<(String, String, String, String, String)> = (0..120).map(|i| ("New".into(), "Major".into(), format!("Very long element name number {i} that would not fit in the column <-> another long name"), "-12 mm".into(), String::new())).collect();
        let pdf = super::clash_report_pdf("P", "T", &rows, "2026-11-23", &super::TitleBlock::default());
        let s = String::from_utf8_lossy(&pdf);
        let pages = s.matches("/Type /Page ").count() + s.matches("/Type /Page/").count() + s.matches("/Type /Page>").count();
        assert!(s.contains("/Count 4") || pages >= 4, "expected 4 pages");
        assert!(s.contains("Page 4 / 4"));
        assert!(s.contains("(120)"), "last row number present");
    }
}

/// Cartusul planselor (PDF si DWG), editabil si salvat in proiect.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct TitleBlock {
    pub company: String,
    /// gol = numele proiectului (.bimr)
    pub project: String,
    pub client: String,
    pub drawn: String,
    pub checked: String,
    /// prefixul numarului de plansa: "S-" -> S-01, S-02...
    pub number_prefix: String,
    pub revision: String,
    pub notes: String,
    /// formatul planselor: "A3", "A2", "A1", "A0"
    pub paper: String,
    /// scara DWG-ului pe plansa: 0 = automata (cea mai mare care incape), altfel 1:N
    pub scale: u32,
}

impl Default for TitleBlock {
    fn default() -> Self {
        TitleBlock { company: String::new(), project: String::new(), client: String::new(), drawn: String::new(), checked: String::new(), number_prefix: "S-".into(), revision: "A".into(), notes: String::new(), paper: "A3".into(), scale: 0 }
    }
}

impl TitleBlock {
    /// numele proiectului afisat (cel din cartus sau cel dat)
    pub fn project_or<'a>(&'a self, fallback: &'a str) -> &'a str {
        if self.project.trim().is_empty() { fallback } else { self.project.trim() }
    }
}

pub const PAPERS: [&str; 4] = ["A3", "A2", "A1", "A0"];
pub const SCALES: [u32; 12] = [20, 25, 50, 75, 100, 150, 200, 250, 500, 1000, 2000, 5000];

/// marimea foii (landscape) in milimetri
pub fn paper_mm(p: &str) -> (f64, f64) {
    match p {
        "A2" => (594.0, 420.0),
        "A1" => (841.0, 594.0),
        "A0" => (1189.0, 841.0),
        _ => (420.0, 297.0),
    }
}
