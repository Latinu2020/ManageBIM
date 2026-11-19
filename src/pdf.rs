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

/// PDF-ul cu cate o plansa pentru fiecare viewpoint.
pub fn sheets(project: &str, items: &[ReportItem], date: &str) -> Vec<u8> {
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
        // ---- cartusul (dreapta jos)
        let tb_w = 380.0;
        let tx = W - M - tb_w;
        c.extend_from_slice(format!("0.6 w {tx:.1} {M} m {tx:.1} {:.1} l S\n", M + STRIP).as_bytes());
        let rows = [(M + STRIP - 34.0, true), (M + 76.0, false), (M + 40.0, false)];
        for (y, _) in rows {
            c.extend_from_slice(format!("0.4 w {tx:.1} {y:.1} m {:.1} {y:.1} l S\n", W - M).as_bytes());
        }
        text(&mut c, tx + 10.0, M + STRIP - 22.0, 15.0, true, project);
        text(&mut c, tx + 10.0, M + STRIP - 52.0, 8.0, false, "VIEWPOINT");
        let title = format!("{}. {}", it.index, it.name);
        text(&mut c, tx + 10.0, M + STRIP - 68.0, 12.5, true, &wrap(&title, 12.5, tb_w - 20.0).into_iter().next().unwrap_or_default());
        let meta = format!("Status: {}   Priority: {}   Assigned: {}", it.issue.status, it.issue.priority, if it.issue.assigned.is_empty() { "-" } else { &it.issue.assigned });
        text(&mut c, tx + 10.0, M + 58.0, 8.5, false, &meta);
        text(&mut c, tx + 10.0, M + 46.0 - 0.0, 8.5, false, &format!("Author: {}   Created: {}", it.issue.author, it.issue.created.get(..10).unwrap_or(&it.issue.created)));
        text(&mut c, tx + 10.0, M + 16.0, 8.0, false, &format!("ManageBIM  -  {date}"));
        let sheet = format!("S-{:02}", k + 1);
        text(&mut c, W - M - 10.0 - text_w(&sheet, 18.0), M + 12.0, 18.0, true, &sheet);
        text(&mut c, W - M - 60.0, M + 32.0, 7.5, false, &format!("{} / {}", k + 1, n));
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
        pages.push(Page { content: c, image: img });
    }
    write_pdf(&pages, project)
}

fn write_pdf(pages: &[Page], title: &str) -> Vec<u8> {
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
        obj(&mut out, &mut offs, pid, format!("<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {W} {H}] /Resources << /Font << /F1 3 0 R /F2 4 0 R >> {res}>> /Contents {} 0 R >>", pid + 1).as_bytes());
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
}
