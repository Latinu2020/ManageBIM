//! Appearance Profiler (ca in Navisworks): coloreaza elementele dupa valoarea unei proprietati.
//! Doua moduri: o culoare pe fiecare valoare distincta (Level, System, Fire Rating...) sau
//! intervale numerice cu gradient (Volume, Area, Elevation...).
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub enum ProfileMode {
    /// o regula pe fiecare valoare distincta
    #[default]
    Values,
    /// intervale numerice egale intre minim si maxim
    Ranges,
}

/// Ce se intampla cu elementele care nu au proprietatea / nu intra in nicio regula.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub enum Unmatched {
    #[default]
    Keep,
    Ghost,
    Hide,
}

impl Unmatched {
    pub const ALL: [Unmatched; 3] = [Unmatched::Keep, Unmatched::Ghost, Unmatched::Hide];
    pub fn label(self) -> &'static str {
        match self {
            Unmatched::Keep => "Keep their colours",
            Unmatched::Ghost => "Ghost (transparent)",
            Unmatched::Hide => "Hide",
        }
    }
}

/// culoarea "fantoma" pentru elementele fara regula
pub const GHOST: [u8; 4] = [150, 150, 155, 40];

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ProfileRule {
    pub label: String,
    /// doar in modul Ranges: [lo, hi) (ultimul interval include maximul)
    #[serde(default)]
    pub lo: Option<f64>,
    #[serde(default)]
    pub hi: Option<f64>,
    pub color: [u8; 4],
    pub visible: bool,
    #[serde(default)]
    pub count: u32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Profile {
    /// categoria proprietatii ("Item" = Name / IFC Type / Level / Model / GlobalId)
    pub cat: String,
    pub prop: String,
    pub mode: ProfileMode,
    pub bins: u32,
    pub rules: Vec<ProfileRule>,
    pub unmatched: Unmatched,
    pub active: bool,
}

impl Default for Profile {
    fn default() -> Self {
        Profile { cat: crate::app::ITEM_CAT.to_string(), prop: "Level".into(), mode: ProfileMode::Values, bins: 6, rules: Vec::new(), unmatched: Unmatched::Keep, active: false }
    }
}

/// Mai multe valori distincte decat atat nu au sens pe ecran (si nici in legenda).
pub const MAX_RULES: usize = 250;

/// Culori distincte: nuanta pe unghiul de aur, saturatie / luminozitate alternante.
pub fn palette(i: usize) -> [u8; 4] {
    let h = (i as f64 * 0.618_033_988_75).fract();
    let s = [0.70, 0.55, 0.85][i % 3];
    let v = [0.92, 0.78, 0.65][(i / 3) % 3];
    hsv(h, s, v)
}

/// Gradient albastru -> verde -> galben -> rosu, t in [0, 1].
pub fn ramp(t: f64) -> [u8; 4] {
    let t = t.clamp(0.0, 1.0);
    hsv((1.0 - t) * 0.66, 0.80, 0.92)
}

fn hsv(h: f64, s: f64, v: f64) -> [u8; 4] {
    let i = (h * 6.0).floor();
    let f = h * 6.0 - i;
    let (p, q, t) = (v * (1.0 - s), v * (1.0 - f * s), v * (1.0 - (1.0 - f) * s));
    let (r, g, b) = match i as i32 % 6 {
        0 => (v, t, p),
        1 => (q, v, p),
        2 => (p, v, t),
        3 => (p, q, v),
        4 => (t, p, v),
        _ => (v, p, q),
    };
    [(r * 255.0).round() as u8, (g * 255.0).round() as u8, (b * 255.0).round() as u8, 255]
}

/// sortare "naturala": "Level 2" < "Level 10", numerele dupa valoare
pub fn natural_cmp(a: &str, b: &str) -> std::cmp::Ordering {
    // aceeasi ordine totala ca in app.rs (o sortare cu ordine inconsecventa opreste programul)
    crate::app::natural_cmp(a, b)
}

impl Profile {
    /// Reguli pentru valori distincte. `counts`: valoare -> numar de elemente.
    /// Intoarce cate valori nu au incaput (peste MAX_RULES).
    pub fn build_values(&mut self, counts: Vec<(String, u32)>) -> usize {
        let mut v: Vec<(String, u32)> = counts.into_iter().filter(|(s, _)| !s.trim().is_empty()).collect();
        v.sort_by(|a, b| natural_cmp(&a.0, &b.0));
        let dropped = v.len().saturating_sub(MAX_RULES);
        if dropped > 0 {
            // pastram valorile cele mai frecvente, apoi din nou in ordine naturala
            v.sort_by(|a, b| b.1.cmp(&a.1));
            v.truncate(MAX_RULES);
            v.sort_by(|a, b| natural_cmp(&a.0, &b.0));
        }
        self.rules = v.into_iter().enumerate().map(|(i, (label, count))| ProfileRule { label, lo: None, hi: None, color: palette(i), visible: true, count }).collect();
        dropped
    }

    /// Intervale egale intre minimul si maximul valorilor numerice.
    pub fn build_ranges(&mut self, nums: &[f64], unit: &str) {
        let nums: Vec<f64> = nums.iter().copied().filter(|x| x.is_finite()).collect();
        self.rules.clear();
        let (Some(lo), Some(hi)) = (nums.iter().copied().reduce(f64::min), nums.iter().copied().reduce(f64::max)) else { return };
        let n = if hi > lo { self.bins.clamp(2, 20) as usize } else { 1 };
        let w = if n > 1 { (hi - lo) / n as f64 } else { 0.0 };
        let fmt = |x: f64| {
            let s = if (hi - lo).abs() >= 100.0 { format!("{x:.0}") } else if (hi - lo).abs() >= 1.0 { format!("{x:.2}") } else { format!("{x:.4}") };
            if unit.is_empty() { s } else { format!("{s} {unit}") }
        };
        for k in 0..n {
            let a = lo + w * k as f64;
            let b = if k + 1 == n { hi } else { lo + w * (k + 1) as f64 };
            let t = if n > 1 { k as f64 / (n - 1) as f64 } else { 0.5 };
            self.rules.push(ProfileRule { label: format!("{} – {}", fmt(a), fmt(b)), lo: Some(a), hi: Some(b), color: ramp(t), visible: true, count: 0 });
        }
        for &x in &nums {
            if let Some(r) = self.range_rule(x) {
                self.rules[r].count += 1;
            }
        }
    }

    fn range_rule(&self, x: f64) -> Option<usize> {
        let last = self.rules.len().checked_sub(1)?;
        self.rules.iter().enumerate().position(|(i, r)| match (r.lo, r.hi) {
            // [lo, hi), iar ultimul interval include si maximul
            (Some(a), Some(b)) => x >= a && (x < b || (i == last && x <= b)),
            _ => false,
        })
    }

    /// Regula care se aplica unui element, dupa valorile proprietatii lui (prima care se potriveste).
    pub fn rule_for(&self, vals: &[&str]) -> Option<usize> {
        match self.mode {
            ProfileMode::Values => vals.iter().find_map(|v| {
                let v = v.trim();
                self.rules.iter().position(|r| r.label.eq_ignore_ascii_case(v))
            }),
            ProfileMode::Ranges => vals.iter().find_map(|v| crate::app::leading_number(v)).and_then(|x| self.range_rule(x)),
        }
    }
}

impl Profile {
    /// Adauga reguli (culori noi) pentru valorile care nu au inca regula. Pastreaza regulile existente
    /// si culorile lor (ex. dupa importul unui profil pe un model nou). Intoarce cate s-au adaugat.
    pub fn add_missing(&mut self, counts: Vec<(String, u32)>) -> usize {
        let mut new: Vec<(String, u32)> = counts.into_iter().filter(|(v, _)| !v.trim().is_empty() && !self.rules.iter().any(|r| r.label.eq_ignore_ascii_case(v.trim()))).collect();
        new.sort_by(|a, b| natural_cmp(&a.0, &b.0));
        let room = MAX_RULES.saturating_sub(self.rules.len());
        new.truncate(room);
        let n = new.len();
        let start = self.rules.len();
        for (k, (label, count)) in new.into_iter().enumerate() {
            self.rules.push(ProfileRule { label, lo: None, hi: None, color: palette(start + k), visible: true, count });
        }
        n
    }

    /// Regula noua, manuala: in modul Values o valoare scrisa de utilizator, in Ranges dupa ultimul interval.
    pub fn add_rule(&mut self) {
        let i = self.rules.len();
        let rule = match self.mode {
            ProfileMode::Values => ProfileRule { label: format!("Value {}", i + 1), lo: None, hi: None, color: palette(i), visible: true, count: 0 },
            ProfileMode::Ranges => {
                let lo = self.rules.last().and_then(|r| r.hi).unwrap_or(0.0);
                let w = self.rules.last().and_then(|r| Some(r.hi? - r.lo?)).filter(|w| *w > 0.0).unwrap_or(1.0);
                let mut r = ProfileRule { label: String::new(), lo: Some(lo), hi: Some(lo + w), color: ramp(1.0), visible: true, count: 0 };
                r.label = range_label(lo, lo + w, "");
                r
            }
        };
        self.rules.push(rule);
    }

    /// Dupa editarea manuala a limitelor unui interval: refacem eticheta (pastram unitatea).
    pub fn relabel_range(&mut self, i: usize) {
        let Some(r) = self.rules.get_mut(i) else { return };
        if let (Some(a), Some(b)) = (r.lo, r.hi) {
            let unit = r.label.rsplit(' ').next().filter(|u| leading_number_free(u)).unwrap_or("").to_string();
            r.label = range_label(a, b, &unit);
        }
    }
}

fn leading_number_free(s: &str) -> bool {
    !s.is_empty() && crate::app::leading_number(s).is_none() && s != "–"
}

fn range_label(a: f64, b: f64, unit: &str) -> String {
    let f = |x: f64| if x.fract() == 0.0 { format!("{x:.0}") } else { format!("{x:.2}") };
    if unit.is_empty() { format!("{} – {}", f(a), f(b)) } else { format!("{} – {} {unit}", f(a), f(b)) }
}

/// Fisier .bimprofile: un profil refolosibil (proprietate, mod, reguli cu culori).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ProfileFile {
    pub app: String,
    pub kind: String,
    pub version: u32,
    pub profile: Profile,
}

pub const FILE_KIND: &str = "appearance-profile";

impl Profile {
    pub fn to_file_json(&self) -> String {
        let mut p = self.clone();
        p.rules.iter_mut().for_each(|r| r.count = 0); // numaratorile tin de model, nu de profil
        p.active = true;
        serde_json::to_string_pretty(&ProfileFile { app: "ManageBIM".into(), kind: FILE_KIND.into(), version: 1, profile: p }).unwrap_or_default()
    }

    /// Accepta un fisier .bimprofile (sau un profil simplu in JSON).
    pub fn from_file_json(text: &str) -> Result<Profile, String> {
        if let Ok(f) = serde_json::from_str::<ProfileFile>(text) {
            if f.kind != FILE_KIND {
                return Err(format!("This is not an Appearance Profiler file (kind \"{}\").", f.kind));
            }
            return Ok(f.profile);
        }
        serde_json::from_str::<Profile>(text).map_err(|e| format!("Invalid profile file: {e}"))
    }
}

/// Unitatea dintr-o valoare de tip "12.5 m2" (textul de dupa numar).
pub fn unit_of(v: &str) -> String {
    let t = v.trim();
    let num_end = t.char_indices().take_while(|(i, c)| c.is_ascii_digit() || *c == '.' || *c == ',' || ((*c == '-' || *c == '+') && *i == 0) || *c == 'e' || *c == 'E').map(|(i, c)| i + c.len_utf8()).last().unwrap_or(0);
    t[num_end..].trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn values_sorted_naturally_and_matched() {
        let mut p = Profile::default();
        p.build_values(vec![("Level 10".into(), 2), ("Level 2".into(), 5), ("Ground".into(), 1), ("".into(), 9)]);
        let labels: Vec<_> = p.rules.iter().map(|r| r.label.as_str()).collect();
        assert_eq!(labels, ["Ground", "Level 2", "Level 10"]);
        assert_eq!(p.rule_for(&["level 2"]), Some(1));
        assert_eq!(p.rule_for(&["Roof"]), None);
    }

    #[test]
    fn ranges_cover_min_and_max() {
        let mut p = Profile { mode: ProfileMode::Ranges, bins: 4, ..Default::default() };
        p.build_ranges(&[0.0, 1.0, 2.5, 4.0], "m3");
        assert_eq!(p.rules.len(), 4);
        assert_eq!(p.rule_for(&["0 m3"]), Some(0));
        assert_eq!(p.rule_for(&["4.0 m3"]), Some(3));
        assert_eq!(p.rule_for(&["2.5"]), Some(2));
        assert_eq!(p.rules.iter().map(|r| r.count).sum::<u32>(), 4);
        assert!(p.rules[0].label.ends_with("m3"));
    }

    #[test]
    fn too_many_values_keeps_most_frequent() {
        let mut p = Profile::default();
        let vals: Vec<(String, u32)> = (0..MAX_RULES + 10).map(|i| (format!("V{i}"), if i < 5 { 1 } else { 100 })).collect();
        assert_eq!(p.build_values(vals), 10);
        assert_eq!(p.rules.len(), MAX_RULES);
        // cele 5 valori rare (V0..V4) sunt primele eliminate
        assert!(p.rules.iter().all(|r| r.count == 100));
    }
}
