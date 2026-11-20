//! Selectie sincronizata ManageBIM <-> Revit / Navisworks, prin fisiere mici in
//! %LOCALAPPDATA%\ManageBIM\Sync (fara retea, fara drepturi de admin):
//! - `managebim.sel` - scris de ManageBIM cand se schimba selectia
//! - `revit.sel`, `navis.sel` - scrise de add-in-uri
//!
//! Format: prima linie `MBSEL1`, apoi cate o linie pe element cu identificatorii lui separati prin `|`
//! (UniqueId Revit, IfcGUID / GlobalId comprimat, GUID standard). Cititorul potriveste oricare dintre ei.

use crate::model::Element;
use std::path::PathBuf;

pub const OURS: &str = "managebim.sel";
pub const PEERS: [(&str, &str); 2] = [("revit.sel", "Revit"), ("navis.sel", "Navisworks")];

pub fn sync_dir() -> Option<PathBuf> {
    crate::formats::data_root().map(|d| d.join("ManageBIM").join("Sync"))
}

const B64: &[u8; 64] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz_$";

/// GlobalId IFC (22 caractere) -> GUID standard "xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx" (litere mici)
pub fn ifc_to_guid(s: &str) -> Option<String> {
    let s = s.trim().as_bytes();
    if s.len() != 22 {
        return None;
    }
    let mut v: u128 = 0;
    for (i, c) in s.iter().enumerate() {
        let d = B64.iter().position(|b| b == c)? as u128;
        if i == 0 && d > 3 {
            return None;
        }
        v = (v << 6) | d;
    }
    let h = format!("{v:032x}");
    Some(format!("{}-{}-{}-{}-{}", &h[0..8], &h[8..12], &h[12..16], &h[16..20], &h[20..32]))
}

/// GUID standard -> GlobalId IFC comprimat (22 caractere)
pub fn guid_to_ifc(g: &str) -> Option<String> {
    let hex: String = g.trim().trim_matches(['{', '}']).chars().filter(|c| *c != '-').collect();
    if hex.len() != 32 {
        return None;
    }
    let mut v = u128::from_str_radix(&hex, 16).ok()?;
    let mut out = [0u8; 22];
    for i in (0..22).rev() {
        out[i] = B64[(v & 63) as usize];
        v >>= 6;
    }
    Some(String::from_utf8_lossy(&out).to_string())
}

/// Toti identificatorii unui element, in forma folosita la potrivire (GUID-urile standard cu litere mici).
pub fn ids_of(e: &Element) -> Vec<String> {
    let mut v: Vec<String> = Vec::new();
    let mut push = |s: &str| {
        let s = s.trim();
        if s.is_empty() {
            return;
        }
        let norm = if s.len() == 36 && s.matches('-').count() == 4 { s.to_ascii_lowercase() } else { s.to_string() };
        if !v.contains(&norm) {
            v.push(norm.clone());
        }
        // GlobalId IFC <-> GUID standard (Navisworks si Revit ExportUtils folosesc forma standard)
        if let Some(g) = ifc_to_guid(&norm) {
            if !v.contains(&g) {
                v.push(g);
            }
        } else if let Some(c) = guid_to_ifc(&norm) {
            if !v.contains(&c) {
                v.push(c);
            }
        }
    };
    push(&e.guid);
    // GUID-ul de dupa ':' (modele cu prefix "model|guid" sau Revit "UniqueId:...")
    if let Some(tail) = e.guid.rsplit(':').next() {
        if tail != e.guid {
            push(tail);
        }
    }
    for p in &e.props {
        if matches!(p.name.as_str(), "UniqueId" | "Unique Id" | "IfcGUID" | "IfcGuid" | "Ifc GUID" | "IFC GUID" | "GlobalId" | "IfcGlobalId" | "Instance GUID" | "Instance Guid" | "GUID") {
            push(&p.value);
        }
    }
    v
}

/// Scrie selectia (atomic: .tmp + redenumire), cate o linie pe element.
pub fn write_selection(name: &str, elems: &[Vec<String>]) -> Result<(), String> {
    let dir = sync_dir().ok_or("LOCALAPPDATA not set")?;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let mut s = String::from("MBSEL1\n");
    for ids in elems {
        s.push_str(&ids.join("|"));
        s.push('\n');
    }
    let tmp = dir.join(format!("{name}.tmp"));
    std::fs::write(&tmp, s).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, dir.join(name)).map_err(|e| e.to_string())
}

/// Identificatorii din fisierul unui add-in (toti, de pe toate liniile).
pub fn parse_selection(text: &str) -> Option<Vec<String>> {
    let mut lines = text.lines();
    if lines.next()?.trim() != "MBSEL1" {
        return None;
    }
    let mut out = Vec::new();
    for l in lines {
        for t in l.split('|') {
            let t = t.trim();
            if !t.is_empty() {
                out.push(if t.len() == 36 && t.matches('-').count() == 4 { t.to_ascii_lowercase() } else { t.to_string() });
            }
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ifc_guid_round_trip() {
        let g = "0f1e2d3c-4b5a-6978-8796-a5b4c3d2e1f0";
        let c = guid_to_ifc(g).unwrap();
        assert_eq!(c.len(), 22);
        assert_eq!(ifc_to_guid(&c).unwrap(), g);
        assert!(ifc_to_guid("too short").is_none());
    }

    #[test]
    fn parse_reads_all_ids() {
        let v = parse_selection("MBSEL1\nabc|0F1E2D3C-4B5A-6978-8796-A5B4C3D2E1F0\n\nxyz\n").unwrap();
        assert_eq!(v, vec!["abc", "0f1e2d3c-4b5a-6978-8796-a5b4c3d2e1f0", "xyz"]);
        assert!(parse_selection("garbage").is_none());
    }
}

/// Starea legaturii in aplicatie.
pub struct SyncState {
    pub enabled_at: std::time::SystemTime,
    pub last_poll: std::time::Instant,
    /// hash-ul ultimei selectii trimise / primite
    pub sent: u64,
    /// data fisierelor Revit / Navisworks citite ultima data
    pub seen: [Option<std::time::SystemTime>; 2],
    /// cand a scris ultima data fiecare add-in (pentru starea din bara)
    pub peer_seen: [Option<std::time::Instant>; 2],
    pub index: std::collections::HashMap<String, crate::model::ElemId>,
    pub index_key: usize,
    pub last_msg: String,
}

impl Default for SyncState {
    fn default() -> Self {
        SyncState { enabled_at: std::time::SystemTime::now(), last_poll: std::time::Instant::now(), sent: u64::MAX, seen: [None; 2], peer_seen: [None; 2], index: Default::default(), index_key: 0, last_msg: String::new() }
    }
}
