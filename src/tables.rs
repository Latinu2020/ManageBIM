//! CSV / Excel (.xlsx) tables for the material library and the colour profile (Appearance Profiler):
//! export, editing in Excel, import back. Reading .xlsx: the first sheet, text / number values.
use crate::materials::{Material, Pattern};
use crate::profiler::{Profile, ProfileMode, ProfileRule, Unmatched};
use crate::qto::{Cell, Sheet};

// ------------------------------------------------------------------ generic reading / writing

/// The rows of a CSV (separator `;`, `,` or tab, detected from the first line; quotes as in Excel).
pub fn csv_rows(text: &str) -> Vec<Vec<String>> {
    let text = text.trim_start_matches('\u{feff}');
    let first = text.lines().next().unwrap_or("");
    let sep = [';', '\t', ','].into_iter().max_by_key(|c| first.matches(*c).count()).unwrap_or(';');
    let mut rows = Vec::new();
    let mut row = Vec::new();
    let mut cur = String::new();
    let mut q = false;
    let mut it = text.chars().peekable();
    while let Some(c) = it.next() {
        if q {
            if c == '"' {
                if it.peek() == Some(&'"') {
                    cur.push('"');
                    it.next();
                } else {
                    q = false;
                }
            } else {
                cur.push(c);
            }
        } else if c == '"' {
            q = true;
        } else if c == sep {
            row.push(std::mem::take(&mut cur));
        } else if c == '\n' || c == '\r' {
            if c == '\r' && it.peek() == Some(&'\n') {
                it.next();
            }
            row.push(std::mem::take(&mut cur));
            if row.iter().any(|s| !s.trim().is_empty()) {
                rows.push(std::mem::take(&mut row));
            } else {
                row.clear();
            }
        } else {
            cur.push(c);
        }
    }
    row.push(cur);
    if row.iter().any(|s| !s.trim().is_empty()) {
        rows.push(row);
    }
    rows
}

fn csv_cell(s: &str) -> String {
    if s.contains([';', '"', '\n', '\r']) { format!("\"{}\"", s.replace('"', "\"\"")) } else { s.to_string() }
}

/// CSV for Excel in Romanian / Europe: `;` between columns, UTF-8 BOM (diacritics appear correctly).
pub fn to_csv(headers: &[&str], rows: &[Vec<String>]) -> String {
    let mut s = String::from("\u{feff}");
    s.push_str(&headers.iter().map(|h| csv_cell(h)).collect::<Vec<_>>().join(";"));
    s.push('\n');
    for r in rows {
        s.push_str(&r.iter().map(|c| csv_cell(c)).collect::<Vec<_>>().join(";"));
        s.push('\n');
    }
    s
}

fn xml_unescape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find('&') {
        out.push_str(&rest[..i]);
        let tail = &rest[i..];
        let Some(j) = tail.find(';') else {
            out.push_str(tail);
            return out;
        };
        let ent = &tail[1..j];
        match ent {
            "amp" => out.push('&'),
            "lt" => out.push('<'),
            "gt" => out.push('>'),
            "quot" => out.push('"'),
            "apos" => out.push('\''),
            e if e.starts_with("#x") => out.extend(u32::from_str_radix(&e[2..], 16).ok().and_then(char::from_u32)),
            e if e.starts_with('#') => out.extend(e[1..].parse::<u32>().ok().and_then(char::from_u32)),
            _ => out.push_str(&tail[..=j]),
        }
        rest = &tail[j + 1..];
    }
    out.push_str(rest);
    out
}

/// The text of all `<t>...</t>` in a fragment (inlineStr / sharedStrings cells with formatting).
fn t_text(frag: &str) -> String {
    let mut s = String::new();
    let mut rest = frag;
    while let Some(i) = rest.find("<t") {
        let after = &rest[i + 2..];
        // <t> or <t xml:space=...>, not <tc...>
        if !(after.starts_with('>') || after.starts_with(' ')) {
            rest = after;
            continue;
        }
        let Some(gt) = after.find('>') else { break };
        if after[..gt].ends_with('/') {
            rest = &after[gt + 1..];
            continue;
        }
        let body = &after[gt + 1..];
        let Some(end) = body.find("</t>") else { break };
        s.push_str(&xml_unescape(&body[..end]));
        rest = &body[end + 4..];
    }
    s
}

fn attr_of(tag: &str, name: &str) -> Option<String> {
    let k = format!(" {name}=\"");
    let i = tag.find(&k)? + k.len();
    let j = tag[i..].find('"')? + i;
    Some(tag[i..j].to_string())
}

/// The column (0..) from the cell reference ("C12" -> 2).
fn col_index(r: &str) -> usize {
    let mut n = 0usize;
    for c in r.chars().take_while(|c| c.is_ascii_alphabetic()) {
        n = n * 26 + (c.to_ascii_uppercase() as usize - 'A' as usize + 1);
    }
    n.saturating_sub(1)
}

/// The first sheet of an .xlsx, as rows of text (numbers with a decimal point).
pub fn xlsx_rows(data: Vec<u8>) -> Result<Vec<Vec<String>>, String> {
    let zip = crate::fcstd::Zip::parse(data).map_err(|_| "Not a valid Excel workbook (.xlsx).".to_string())?;
    let shared: Vec<String> = zip
        .text("xl/sharedStrings.xml")
        .map(|x| x.split("<si").skip(1).map(|si| t_text(si.split("</si>").next().unwrap_or(""))).collect())
        .unwrap_or_default();
    let mut sheets: Vec<&String> = zip.names().filter(|n| n.starts_with("xl/worksheets/sheet") && n.ends_with(".xml")).collect();
    // sheet1.xml before sheet10.xml
    sheets.sort_by_key(|n| n.trim_start_matches("xl/worksheets/sheet").trim_end_matches(".xml").parse::<u32>().unwrap_or(u32::MAX));
    let name = sheets.first().ok_or("The workbook has no sheet.")?.to_string();
    let x = zip.text(&name).ok_or("Cannot read the first sheet.")?;
    let mut rows = Vec::new();
    for rpart in x.split("<row").skip(1) {
        let rbody = rpart.split("</row>").next().unwrap_or("");
        let mut row: Vec<String> = Vec::new();
        for cpart in rbody.split("<c ").skip(1) {
            let tag_end = cpart.find('>').unwrap_or(cpart.len());
            let tag = format!(" {}", &cpart[..tag_end]);
            let body = if cpart[..tag_end].ends_with('/') { "" } else { cpart.get(tag_end + 1..).unwrap_or("").split("</c>").next().unwrap_or("") };
            let col = attr_of(&tag, "r").map(|r| col_index(&r)).unwrap_or(row.len());
            let ty = attr_of(&tag, "t").unwrap_or_default();
            let v = body.find("<v>").and_then(|i| body[i + 3..].find("</v>").map(|j| xml_unescape(&body[i + 3..i + 3 + j])));
            let val = match ty.as_str() {
                "s" => v.and_then(|i| i.trim().parse::<usize>().ok()).and_then(|i| shared.get(i).cloned()).unwrap_or_default(),
                "inlineStr" => t_text(body),
                _ => v.unwrap_or_default(),
            };
            if col > 10_000 {
                continue;
            }
            while row.len() < col {
                row.push(String::new());
            }
            if row.len() == col {
                row.push(val);
            } else {
                row[col] = val;
            }
        }
        if row.iter().any(|s| !s.trim().is_empty()) {
            rows.push(row);
        }
    }
    Ok(rows)
}

/// The rows of a .csv or .xlsx file (by extension / content).
pub fn read_rows(path: &str) -> Result<Vec<Vec<String>>, String> {
    let data = std::fs::read(path).map_err(|e| format!("Cannot read the file: {e}"))?;
    if data.starts_with(b"PK") {
        xlsx_rows(data)
    } else {
        Ok(csv_rows(&String::from_utf8_lossy(&data)))
    }
}

/// The table with a header: finds the header row (the first that contains `key`) and returns (columns, data rows).
fn table<'a>(rows: &'a [Vec<String>], key: &str) -> Option<(Vec<String>, &'a [Vec<String>])> {
    let h = rows.iter().position(|r| r.iter().any(|c| c.trim().eq_ignore_ascii_case(key)))?;
    Some((rows[h].iter().map(|c| c.trim().to_lowercase()).collect(), &rows[h + 1..]))
}

fn get<'a>(cols: &[String], row: &'a [String], names: &[&str]) -> Option<&'a str> {
    let i = cols.iter().position(|c| names.iter().any(|n| c == n))?;
    row.get(i).map(|s| s.trim()).filter(|s| !s.is_empty())
}

/// A number from text (also with a decimal comma).
fn num(s: Option<&str>) -> Option<f64> {
    let s = s?.trim().trim_end_matches('%').trim().replace(',', ".");
    s.parse::<f64>().ok().filter(|x| x.is_finite())
}

pub fn hex(c: [u8; 3]) -> String {
    format!("#{:02X}{:02X}{:02X}", c[0], c[1], c[2])
}

/// "#RRGGBB", "RRGGBB" or "r,g,b" / "r g b".
pub fn parse_color(s: &str) -> Option<[u8; 3]> {
    let t = s.trim().trim_start_matches('#');
    if t.len() == 6 && t.chars().all(|c| c.is_ascii_hexdigit()) {
        let v = u32::from_str_radix(t, 16).ok()?;
        return Some([(v >> 16) as u8, (v >> 8) as u8, v as u8]);
    }
    let p: Vec<u8> = t.split([',', ' ', ';', '/']).filter(|x| !x.is_empty()).filter_map(|x| x.trim().parse::<f64>().ok()).map(|x| x.clamp(0.0, 255.0).round() as u8).collect();
    (p.len() == 3).then(|| [p[0], p[1], p[2]])
}

fn sheet(name: &str, title: &str, headers: &[&str], widths: &[f64], rows: Vec<Vec<Cell>>) -> Vec<u8> {
    crate::qto::xlsx(&[Sheet { name: name.into(), title: title.into(), headers: headers.iter().map(|h| h.to_string()).collect(), widths: widths.to_vec(), rows, total_from: None }])
}

fn as_text(rows: &[Vec<Cell>]) -> Vec<Vec<String>> {
    rows.iter()
        .map(|r| {
            r.iter()
                .map(|c| match c {
                    Cell::S(s) => s.clone(),
                    Cell::N(v) => {
                        let s = format!("{v}");
                        s.replace('.', ",")
                    }
                    Cell::Empty => String::new(),
                })
                .collect()
        })
        .collect()
}

// ------------------------------------------------------------------ materials

const MAT_HEADERS: [&str; 8] = ["Name", "Colour", "Transparency %", "Gloss", "Metallic", "Texture", "Texture size m", "Image"];

fn mat_rows(mats: &[Material]) -> Vec<Vec<Cell>> {
    mats.iter()
        .map(|m| {
            vec![
                Cell::S(m.name.clone()),
                Cell::S(hex(m.color)),
                Cell::N(m.transparency as f64),
                Cell::N(m.gloss as f64),
                Cell::N(m.metal as f64),
                Cell::S(m.pattern.label().to_string()),
                Cell::N(m.scale as f64),
                Cell::S(m.texture.clone()),
            ]
        })
        .collect()
}

pub fn materials_csv(mats: &[Material]) -> String {
    to_csv(&MAT_HEADERS, &as_text(&mat_rows(mats)))
}

pub fn materials_xlsx(mats: &[Material]) -> Vec<u8> {
    sheet("Materials", "ManageBIM material library", &MAT_HEADERS, &[26.0, 11.0, 15.0, 9.0, 10.0, 18.0, 15.0, 40.0], mat_rows(mats))
}

fn pattern_of(s: &str) -> Pattern {
    let s = s.trim().to_lowercase();
    Pattern::ALL.into_iter().find(|p| p.label().to_lowercase() == s || format!("{p:?}").to_lowercase() == s).unwrap_or(Pattern::None)
}

/// The materials from a table (columns by name, in any order; missing = default values).
pub fn materials_from_rows(rows: &[Vec<String>]) -> Result<Vec<Material>, String> {
    let (cols, data) = table(rows, "name").ok_or("No \"Name\" column found (expected the columns of a ManageBIM material export).")?;
    let mut out = Vec::new();
    for r in data {
        let Some(name) = get(&cols, r, &["name"]) else { continue };
        let color = get(&cols, r, &["colour", "color", "rgb"]).and_then(parse_color).unwrap_or([200, 200, 200]);
        let pct = |names: &[&str], max: f64| num(get(&cols, r, names)).map(|x| x.clamp(0.0, max).round() as u8).unwrap_or(0);
        out.push(Material {
            name: name.to_string(),
            color,
            transparency: pct(&["transparency %", "transparency"], 90.0),
            gloss: pct(&["gloss"], 100.0),
            metal: pct(&["metallic", "metal"], 100.0),
            pattern: get(&cols, r, &["texture", "pattern"]).map(pattern_of).unwrap_or(Pattern::None),
            scale: num(get(&cols, r, &["texture size m", "texture size", "scale"])).map(|x| x.clamp(0.05, 4.0) as f32).unwrap_or(1.0),
            texture: get(&cols, r, &["image"]).unwrap_or("").to_string(),
            normal_map: String::new(), roughness_map: String::new(), metallic_map: String::new(),
            ao_map: String::new(), height_map: String::new(),
            normal_strength: 1.0, rough_mult: 1.0, metal_mult: 1.0,
            coat: 0, coat_roughness: 5, sheen: 0, ior: crate::materials::default_ior(),
        });
    }
    if out.is_empty() {
        return Err("The table has no materials.".into());
    }
    Ok(out)
}

// ------------------------------------------------------------------ colour profile

const PROF_HEADERS: [&str; 6] = ["Value", "From", "To", "Colour", "Opacity %", "Visible"];

fn prof_rows(p: &Profile) -> Vec<Vec<Cell>> {
    let mut rows = vec![
        vec![Cell::S("#Category".into()), Cell::S(p.cat.clone())],
        vec![Cell::S("#Property".into()), Cell::S(p.prop.clone())],
        vec![Cell::S("#Mode".into()), Cell::S(if p.mode == ProfileMode::Ranges { "Ranges".into() } else { "Values".into() })],
        vec![Cell::S("#Others".into()), Cell::S(format!("{:?}", p.unmatched))],
    ];
    for r in &p.rules {
        rows.push(vec![
            Cell::S(r.label.clone()),
            r.lo.map(Cell::N).unwrap_or(Cell::Empty),
            r.hi.map(Cell::N).unwrap_or(Cell::Empty),
            Cell::S(hex([r.color[0], r.color[1], r.color[2]])),
            Cell::N((r.color[3] as f64 / 2.55).round()),
            Cell::S(if r.visible { "yes".into() } else { "no".into() }),
        ]);
    }
    rows
}

pub fn profile_csv(p: &Profile) -> String {
    to_csv(&PROF_HEADERS, &as_text(&prof_rows(p)))
}

pub fn profile_xlsx(p: &Profile) -> Vec<u8> {
    sheet("Colour profile", &format!("ManageBIM colour profile - {} / {}", p.cat, p.prop), &PROF_HEADERS, &[34.0, 12.0, 12.0, 11.0, 11.0, 9.0], prof_rows(p))
}

pub fn profile_from_rows(rows: &[Vec<String>]) -> Result<Profile, String> {
    let (cols, data) = table(rows, "value").ok_or("No \"Value\" column found (expected the columns of a ManageBIM colour profile export).")?;
    let mut p = Profile { active: true, ..Default::default() };
    let mut mode_set = false;
    for r in data {
        let Some(label) = get(&cols, r, &["value"]) else { continue };
        let second = r.get(1).map(|s| s.trim().to_string()).unwrap_or_default();
        match label {
            "#Category" => p.cat = second,
            "#Property" => p.prop = second,
            "#Mode" => {
                p.mode = if second.eq_ignore_ascii_case("ranges") { ProfileMode::Ranges } else { ProfileMode::Values };
                mode_set = true;
            }
            "#Others" => p.unmatched = Unmatched::ALL.into_iter().find(|u| format!("{u:?}").eq_ignore_ascii_case(&second)).unwrap_or(Unmatched::Keep),
            _ => {
                let c = get(&cols, r, &["colour", "color"]).and_then(parse_color).unwrap_or_else(|| {
                    let x = crate::profiler::palette(p.rules.len());
                    [x[0], x[1], x[2]]
                });
                let a = num(get(&cols, r, &["opacity %", "opacity"])).map(|o| (o.clamp(0.0, 100.0) * 2.55).round() as u8).unwrap_or(255);
                let vis = get(&cols, r, &["visible"]).map(|v| !matches!(v.to_lowercase().as_str(), "no" | "nu" | "false" | "0")).unwrap_or(true);
                p.rules.push(ProfileRule { label: label.to_string(), lo: num(get(&cols, r, &["from"])), hi: num(get(&cols, r, &["to"])), color: [c[0], c[1], c[2], a], visible: vis, count: 0 });
            }
        }
    }
    if !mode_set && p.rules.iter().all(|r| r.lo.is_some() && r.hi.is_some()) && !p.rules.is_empty() {
        p.mode = ProfileMode::Ranges;
    }
    if p.rules.is_empty() {
        return Err("The table has no colours.".into());
    }
    if p.prop.trim().is_empty() {
        return Err("The table does not say which property it colours (row \"#Property\").".into());
    }
    Ok(p)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn materials_roundtrip_csv_and_xlsx() {
        let lib = crate::materials::library();
        let back = materials_from_rows(&csv_rows(&materials_csv(&lib))).unwrap();
        assert_eq!(back.len(), lib.len());
        for (a, b) in lib.iter().zip(&back) {
            assert_eq!((a.name.as_str(), a.color, a.transparency, a.gloss, a.metal, a.pattern), (b.name.as_str(), b.color, b.transparency, b.gloss, b.metal, b.pattern));
            assert!((a.scale - b.scale).abs() < 1e-4);
        }
        let back = materials_from_rows(&xlsx_rows(materials_xlsx(&lib)).unwrap()).unwrap();
        assert_eq!(back.len(), lib.len());
        assert_eq!(back[2].color, lib[2].color);
    }

    #[test]
    fn profile_roundtrip_csv_and_xlsx() {
        let mut p = Profile { prop: "Fire Rating".into(), ..Default::default() };
        p.build_values(vec![("EI 60".into(), 3), ("EI 90; special".into(), 2)]);
        p.rules[1].visible = false;
        p.rules[0].color[3] = 128;
        for back in [profile_from_rows(&csv_rows(&profile_csv(&p))).unwrap(), profile_from_rows(&xlsx_rows(profile_xlsx(&p)).unwrap()).unwrap()] {
            assert_eq!(back.prop, "Fire Rating");
            assert_eq!(back.rules.len(), 2);
            assert_eq!(back.rules[1].label, "EI 90; special");
            assert!(!back.rules[1].visible);
            assert_eq!(back.rules[0].color[..3], p.rules[0].color[..3]);
            assert!((back.rules[0].color[3] as i32 - 128).abs() <= 2);
        }
        let mut r = Profile { mode: ProfileMode::Ranges, bins: 3, ..Default::default() };
        r.build_ranges(&[0.0, 3.0, 9.0], "m");
        let back = profile_from_rows(&csv_rows(&profile_csv(&r))).unwrap();
        assert_eq!(back.mode, ProfileMode::Ranges);
        assert_eq!(back.rules[2].hi, Some(9.0));
    }

    #[test]
    fn csv_with_comma_separator_and_quotes() {
        let rows = csv_rows("Name,Colour\n\"Brick, red\",\"200,10,10\"\n");
        let m = materials_from_rows(&rows).unwrap();
        assert_eq!(m[0].name, "Brick, red");
        assert_eq!(m[0].color, [200, 10, 10]);
    }
}
