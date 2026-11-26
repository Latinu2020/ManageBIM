//! Search set-uri "inteligente": indexul proprietatilor modelului (cu numarul de elemente pe valoare),
//! regulile de cautare construite din valorile bifate, proprietatile comune ale selectiei (sugestii)
//! si interpretarea unei cautari scrise in cuvinte ("ducts level 2"). Interfata: ui/smart_sets.rs.

use crate::app::{SearchOp, SearchRule, ITEM_CAT};
use crate::model::{ElemId, Model};
use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

/// (grupa, proprietate) - grupa "Item" = campurile standard (IFC Type, Level, Model, Name)
pub type Key = (String, String);

/// campurile standard folosite de builder (GlobalId e unic pe element - nu e util pentru seturi)
pub const ITEM_FIELDS: [&str; 4] = ["IFC Type", "Level", "Model", "Name"];

/// valorile unui element pentru o cheie
pub fn values_of<'a>(m: &'a Model, e: &'a crate::model::Element, k: &Key) -> Vec<&'a str> {
    crate::app::elem_values(m, e, &k.0, &k.1)
}

#[derive(Default, Clone)]
pub struct Index {
    /// cheie -> valoare -> numarul de elemente
    pub props: BTreeMap<Key, HashMap<String, usize>>,
    /// numarul de elemente care au cheia
    pub count: HashMap<Key, usize>,
    pub elements: usize,
}

impl Index {
    pub fn build<'a>(models: impl Iterator<Item = &'a Arc<Model>>) -> Index {
        let mut ix = Index::default();
        for m in models {
            for e in &m.elements {
                ix.elements += 1;
                let mut seen: Vec<Key> = Vec::new();
                for f in ITEM_FIELDS {
                    let v = match f {
                        "IFC Type" => e.ifc_type.as_str(),
                        "Level" => e.storey.as_str(),
                        "Model" => m.name.as_str(),
                        _ => e.name.as_str(),
                    };
                    if !v.trim().is_empty() {
                        let k = (ITEM_CAT.to_string(), f.to_string());
                        *ix.props.entry(k.clone()).or_default().entry(v.to_string()).or_default() += 1;
                        seen.push(k);
                    }
                }
                for p in &e.props {
                    if p.value.trim().is_empty() {
                        continue;
                    }
                    let k = (p.group.clone(), p.name.clone());
                    *ix.props.entry(k.clone()).or_default().entry(p.value.clone()).or_default() += 1;
                    seen.push(k);
                }
                seen.sort();
                seen.dedup();
                for k in seen {
                    *ix.count.entry(k).or_default() += 1;
                }
            }
        }
        ix
    }

    /// valorile unei chei, cele mai frecvente primele
    pub fn values(&self, k: &Key) -> Vec<(String, usize)> {
        let mut v: Vec<(String, usize)> = self.props.get(k).map(|m| m.iter().map(|(a, b)| (a.clone(), *b)).collect()).unwrap_or_default();
        v.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        v
    }

    /// o proprietate e "utila" pentru seturi daca nu are cate o valoare diferita pe fiecare element (GUID, handle)
    pub fn useful(&self, k: &Key) -> bool {
        let n = self.count.get(k).copied().unwrap_or(0);
        let distinct = self.props.get(k).map(|m| m.len()).unwrap_or(0);
        n > 0 && (distinct < 2 || distinct * 10 < n * 9 || k.0 == ITEM_CAT)
    }
}

/// Regulile unui set: in cadrul unei proprietati valorile sunt SAU, intre proprietati SI.
/// Regulile ManageBIM sunt blocuri SAU de conditii SI, deci se face produsul cartezian (max. 256 blocuri).
pub fn rules_for(choices: &[(Key, Vec<String>)]) -> Vec<SearchRule> {
    let choices: Vec<&(Key, Vec<String>)> = choices.iter().filter(|c| !c.1.is_empty()).collect();
    if choices.is_empty() {
        return Vec::new();
    }
    let mut blocks: Vec<Vec<(&Key, &String)>> = vec![Vec::new()];
    for (k, vals) in &choices {
        let mut next = Vec::new();
        for b in &blocks {
            for v in vals {
                let mut nb = b.clone();
                nb.push((k, v));
                next.push(nb);
                if next.len() > 256 {
                    break;
                }
            }
        }
        blocks = next;
    }
    let mut out = Vec::new();
    for (bi, b) in blocks.iter().enumerate() {
        for (ri, (k, v)) in b.iter().enumerate() {
            out.push(SearchRule::new(&k.0, &k.1, SearchOp::Equals, v, bi > 0 && ri == 0));
        }
    }
    out
}

/// Numele propus: valorile alese, separate cu " · "
pub fn suggest_name(choices: &[(Key, Vec<String>)]) -> String {
    let parts: Vec<String> = choices
        .iter()
        .filter(|c| !c.1.is_empty())
        .map(|(k, v)| {
            let vals = if v.len() <= 2 { v.join(" / ") } else { format!("{} +{}", v[0], v.len() - 1) };
            if k.0 == ITEM_CAT && k.1 == "IFC Type" { vals } else { format!("{} {}", k.1, vals) }
        })
        .collect();
    if parts.is_empty() { "New set".into() } else { parts.join(" · ") }
}

/// Proprietatile pe care le au TOATE elementele selectate, cu aceeasi valoare: (cheie, valoare, cate elemente
/// din model o au). Cele mai specifice (mai putine elemente in model) primele; GUID-uri si nume unice sunt sarite.
pub fn common_props(models: &[Option<Arc<Model>>], ids: &[ElemId], ix: &Index) -> Vec<(Key, String, usize)> {
    let elem = |id: &ElemId| models.get(id.m as usize).and_then(|m| m.as_ref()).and_then(|m| m.elements.get(id.e as usize).map(|e| (m, e)));
    let Some(first) = ids.first().and_then(elem) else { return Vec::new() };
    let mut cand: Vec<(Key, String)> = Vec::new();
    let (m0, e0) = first;
    for f in ITEM_FIELDS {
        let k = (ITEM_CAT.to_string(), f.to_string());
        if let Some(v) = values_of(m0, e0, &k).first() {
            cand.push((k, v.to_string()));
        }
    }
    for p in &e0.props {
        cand.push(((p.group.clone(), p.name.clone()), p.value.clone()));
    }
    let mut out = Vec::new();
    for (k, v) in cand {
        if v.trim().is_empty() || !ix.useful(&k) {
            continue;
        }
        if ids.iter().skip(1).all(|id| elem(id).map(|(m, e)| values_of(m, e, &k).iter().any(|x| *x == v)).unwrap_or(false)) {
            let n = ix.props.get(&k).and_then(|m| m.get(&v)).copied().unwrap_or(0);
            // o valoare pe care o au toate elementele modelului nu deosebeste nimic
            if n < ix.elements || ix.elements <= ids.len() {
                out.push((k, v, n));
            }
        }
    }
    out.sort_by(|a, b| a.2.cmp(&b.2).then_with(|| a.0.cmp(&b.0)));
    out.dedup_by(|a, b| a.0 == b.0 && a.1 == b.1);
    out
}

/// "ducts level 2", "walls, Level 1", "valves cs150": fiecare bucata (despartita de virgula / "and" / "si") e
/// cautata intre valorile modelului; campurile standard au prioritate (tip, nivel), apoi proprietatile.
/// "level 2" / "nivel 2" se potriveste pe Level. Intoarce alegerile gasite si bucatile negasite.
pub fn parse_query(q: &str, ix: &Index) -> (Vec<(Key, Vec<String>)>, Vec<String>) {
    let norm = |s: &str| s.to_lowercase().replace(['_', '-'], " ");
    let mut phrases: Vec<String> = Vec::new();
    for part in q.split([',', ';']) {
        for p in part.split(" and ").flat_map(|x| x.split(" si ")).flat_map(|x| x.split(" și ")) {
            let p = p.trim();
            if !p.is_empty() {
                phrases.push(p.to_string());
            }
        }
    }
    // "ducts level 2" fara virgula: cuvantul "level" incepe o bucata noua
    let mut split: Vec<String> = Vec::new();
    for p in phrases {
        let low = p.to_lowercase();
        let mut cut = None;
        for kw in [" level ", " nivel ", " on level ", " pe nivelul "] {
            if let Some(i) = low.find(kw) {
                cut = Some(i);
                break;
            }
        }
        match cut {
            Some(i) if i > 0 => {
                split.push(p[..i].trim().to_string());
                split.push(p[i..].trim().to_string());
            }
            _ => split.push(p),
        }
    }
    let mut out: Vec<(Key, Vec<String>)> = Vec::new();
    let mut missed = Vec::new();
    for phrase in split {
        let mut p = norm(&phrase);
        let mut only_level = false;
        for pre in ["on level ", "pe nivelul ", "level ", "nivel ", "nivelul "] {
            if let Some(rest) = p.strip_prefix(pre) {
                p = rest.trim().to_string();
                only_level = true;
                break;
            }
        }
        // plural simplu: "ducts" -> "duct", "valves" -> "valve"
        let stem = if p.len() > 3 && p.ends_with('s') { p[..p.len() - 1].to_string() } else { p.clone() };
        let mut best: Option<(i32, Key, Vec<String>)> = None;
        for (k, vals) in &ix.props {
            if only_level && !(k.0 == ITEM_CAT && k.1 == "Level") {
                continue;
            }
            if !ix.useful(k) {
                continue;
            }
            let mut hits: Vec<String> = Vec::new();
            let mut exact = false;
            for v in vals.keys() {
                let nv = norm(v);
                if nv == p || nv == stem {
                    exact = true;
                    hits = vec![v.clone()];
                    break;
                }
                // la nivel "2" cauta cuvantul intreg ("Level 2", nu "Level 12")
                let word = nv.split(|c: char| !c.is_alphanumeric()).any(|w| w == p);
                if (only_level && word) || (!only_level && p.len() >= 3 && (nv.contains(&p) || nv.contains(&stem))) {
                    hits.push(v.clone());
                }
            }
            if hits.is_empty() || hits.len() > 40 {
                continue;
            }
            let prio = match (k.0 == ITEM_CAT, k.1.as_str()) {
                (true, "IFC Type") => 50,
                (true, "Level") => 45,
                (true, "Model") => 10,
                (true, _) => 5,
                _ => 20,
            } + if exact { 30 } else { 0 } - hits.len().min(20) as i32;
            if best.as_ref().is_none_or(|b| prio > b.0) {
                hits.sort();
                best = Some((prio, k.clone(), hits));
            }
        }
        match best {
            Some((_, k, v)) => {
                if let Some(c) = out.iter_mut().find(|c| c.0 == k) {
                    for x in v {
                        if !c.1.contains(&x) {
                            c.1.push(x);
                        }
                    }
                } else {
                    out.push((k, v));
                }
            }
            None => missed.push(phrase),
        }
    }
    (out, missed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Prop, RawElement, RawModel};
    use glam::DVec3;

    fn el(name: &str, ty: &str, level: &str, props: &[(&str, &str, &str)]) -> RawElement {
        let t = [DVec3::ZERO, DVec3::X, DVec3::Y];
        RawElement {
            name: name.into(),
            ifc_type: ty.into(),
            guid: format!("g-{name}"),
            storey: level.into(),
            props: props.iter().map(|(g, n, v)| Prop { group: g.to_string(), name: n.to_string(), value: v.to_string() }).collect(),
            tris: vec![t],
            colors: vec![[200, 200, 200, 255]],
            default_hidden: false,
        }
    }

    fn model() -> Vec<Option<Arc<Model>>> {
        let raw = RawModel {
            elements: vec![
                el("D1", "IfcDuctSegment", "Level 2", &[("Pset", "System", "Supply Air")]),
                el("D2", "IfcDuctSegment", "Level 2", &[("Pset", "System", "Supply Air")]),
                el("D3", "IfcDuctSegment", "Level 1", &[("Pset", "System", "Exhaust")]),
                el("W1", "IfcWall", "Level 2", &[("Pset", "FireRating", "EI60")]),
                el("W2", "IfcWall", "Level 12", &[("Pset", "FireRating", "EI60")]),
            ],
            info: Vec::new(),
            warnings: Vec::new(),
        };
        vec![Some(Arc::new(crate::build_model(raw, "t.ifc", DVec3::ZERO)))]
    }

    fn matching(models: &[Option<Arc<Model>>], rules: &[SearchRule]) -> Vec<String> {
        // aceeasi semantica ca App::matches: blocuri SAU de conditii SI, egalitate fara majuscule
        let m = models[0].as_ref().unwrap();
        let mut blocks: Vec<Vec<&SearchRule>> = vec![Vec::new()];
        for (i, r) in rules.iter().enumerate() {
            if r.or && i > 0 {
                blocks.push(Vec::new());
            }
            blocks.last_mut().unwrap().push(r);
        }
        m.elements
            .iter()
            .filter(|e| blocks.iter().any(|b| b.iter().all(|r| crate::app::elem_values(m, e, &r.group, &r.field).iter().any(|v| v.eq_ignore_ascii_case(&r.value)))))
            .map(|e| e.name.clone())
            .collect()
    }

    #[test]
    fn rules_or_within_and_across() {
        let ms = model();
        let ty = (ITEM_CAT.to_string(), "IFC Type".to_string());
        let lv = (ITEM_CAT.to_string(), "Level".to_string());
        let r = rules_for(&[(ty.clone(), vec!["IfcDuctSegment".into(), "IfcWall".into()]), (lv, vec!["Level 2".into()])]);
        assert_eq!(matching(&ms, &r), vec!["D1", "D2", "W1"]);
        assert_eq!(suggest_name(&[(ty, vec!["IfcWall".into()])]), "IfcWall");
    }

    #[test]
    fn common_properties_of_selection() {
        let ms = model();
        let ix = Index::build(ms.iter().flatten());
        let c = common_props(&ms, &[ElemId { m: 0, e: 0 }, ElemId { m: 0, e: 1 }], &ix);
        let names: Vec<String> = c.iter().map(|(k, v, n)| format!("{}={v} ({n})", k.1)).collect();
        assert!(names.contains(&"System=Supply Air (2)".to_string()), "{names:?}");
        assert!(names.contains(&"Level=Level 2 (3)".to_string()), "{names:?}");
        assert!(!names.iter().any(|s| s.starts_with("Name=")), "names differ: {names:?}");
    }

    #[test]
    fn plain_language_query() {
        let ms = model();
        let ix = Index::build(ms.iter().flatten());
        let (c, missed) = parse_query("ducts level 2", &ix);
        assert!(missed.is_empty(), "{missed:?}");
        let r = rules_for(&c);
        assert_eq!(matching(&ms, &r), vec!["D1", "D2"]);
        let (c, _) = parse_query("walls, supply air", &ix);
        assert_eq!(c.len(), 2);
        let (_, missed) = parse_query("pumps", &ix);
        assert_eq!(missed, vec!["pumps"]);
    }
}

/// Starea ferestrei "Smart set" (pastrata intre cadre).
#[derive(Default)]
pub struct Builder {
    pub open: bool,
    pub query: String,
    pub prop_filter: String,
    pub val_filter: String,
    pub cur: Option<Key>,
    pub choices: Vec<(Key, Vec<String>)>,
    pub missed: Vec<String>,
    pub name: String,
    /// numele a fost scris de utilizator (nu se mai propune automat)
    pub name_edited: bool,
    /// indexul + semnatura modelelor pentru care a fost construit
    pub index: Option<(u64, Index)>,
    /// numarul de elemente gasite pentru alegerile curente (+ semnatura alegerilor)
    pub count: Option<(u64, usize)>,
    /// deschis din Clash Detective ("Ignore everything like this"): testul in care se salveaza excluderea
    pub target_test: Option<usize>,
}

impl Builder {
    /// semnatura alegerilor (numarul se recalculeaza doar cand se schimba)
    pub fn sig(&self) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        self.choices.hash(&mut h);
        h.finish()
    }

    pub fn toggle(&mut self, k: &Key, v: &str) {
        match self.choices.iter_mut().find(|c| &c.0 == k) {
            Some(c) => {
                if let Some(i) = c.1.iter().position(|x| x == v) {
                    c.1.remove(i);
                } else {
                    c.1.push(v.to_string());
                }
            }
            None => self.choices.push((k.clone(), vec![v.to_string()])),
        }
        self.choices.retain(|c| !c.1.is_empty());
    }

    pub fn has(&self, k: &Key, v: &str) -> bool {
        self.choices.iter().any(|c| &c.0 == k && c.1.iter().any(|x| x == v))
    }
}
