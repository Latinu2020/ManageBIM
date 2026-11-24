//! buildingSMART Data Dictionary (bSDD) - cautare online de clase (IFC, Uniclass, ETIM, NL-SfB...).
//! API public, fara cont: https://api.bsdd.buildingsmart.org (TextSearch v1, Dictionary v1).
//! Cererea HTTPS se face cu `curl` (inclus in Windows 10 / 11, Linux, macOS) pe un fir separat,
//! ca interfata sa nu se blocheze; fara dependinte noi (TLS) in aplicatie.
use serde::Deserialize;
use std::sync::mpsc::{channel, Receiver};

pub const API: &str = "https://api.bsdd.buildingsmart.org";

/// O clasa gasita in bSDD.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct BsddClass {
    pub dictionary_uri: String,
    pub dictionary_name: String,
    pub name: String,
    pub reference_code: String,
    pub uri: String,
    pub description: String,
    pub parent_class_name: String,
}

impl BsddClass {
    /// codul afisat / scris in proprietate: referenceCode, altfel ultimul segment din URI
    pub fn code(&self) -> String {
        if !self.reference_code.trim().is_empty() {
            return self.reference_code.trim().to_string();
        }
        self.uri.rsplit('/').next().unwrap_or("").to_string()
    }
    pub fn to_item(&self) -> crate::classification::ClassItem {
        crate::classification::ClassItem {
            system: self.dictionary_name.clone(),
            code: self.code(),
            name: self.name.clone(),
            description: if self.uri.is_empty() { self.description.clone() } else { self.uri.clone() },
        }
    }
}

/// Un dictionar publicat in bSDD (pentru filtrul cautarii).
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct BsddDictionary {
    pub name: String,
    pub uri: String,
    pub version: String,
}

impl BsddDictionary {
    pub fn label(&self) -> String {
        if self.version.is_empty() { self.name.clone() } else { format!("{} {}", self.name, self.version) }
    }
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct SearchResp {
    classes: Vec<BsddClass>,
    #[serde(rename = "totalCount")]
    total_count: usize,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct DictResp {
    dictionaries: Vec<BsddDictionary>,
}

/// Codificare procent pentru parametrii din URL (RFC 3986, caractere nerezervate pastrate).
pub fn url_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len() * 3);
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(b as char),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

pub fn search_url(text: &str, dictionary_uri: Option<&str>, limit: usize) -> String {
    let mut u = format!("{API}/api/TextSearch/v1?SearchText={}&TypeFilter=Classes&Offset=0&Limit={}", url_encode(text.trim()), limit.clamp(1, 1000));
    if let Some(d) = dictionary_uri.filter(|d| !d.is_empty()) {
        u.push_str(&format!("&DictionaryUris={}", url_encode(d)));
    }
    u
}

/// (clase, numarul total de rezultate din bSDD)
pub fn parse_search(json: &str) -> Result<(Vec<BsddClass>, usize), String> {
    let r: SearchResp = serde_json::from_str(json).map_err(|e| format!("bSDD answer not understood: {e}"))?;
    let total = r.total_count.max(r.classes.len());
    Ok((r.classes, total))
}

pub fn parse_dictionaries(json: &str) -> Result<Vec<BsddDictionary>, String> {
    let r: DictResp = serde_json::from_str(json).map_err(|e| format!("bSDD answer not understood: {e}"))?;
    let mut d: Vec<_> = r.dictionaries.into_iter().filter(|d| !d.uri.is_empty()).collect();
    d.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()).then(b.version.cmp(&a.version)));
    Ok(d)
}

/// GET prin curl; intoarce corpul raspunsului.
fn http_get(url: &str) -> Result<String, String> {
    let mut cmd = std::process::Command::new("curl");
    cmd.args(["-s", "-S", "-f", "-L", "-m", "25", "-H", "Accept: application/json", url]);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // fara fereastra de consola care clipeste
        cmd.creation_flags(0x0800_0000);
    }
    let out = cmd.output().map_err(|e| format!("curl is not available ({e}). bSDD search needs curl (included in Windows 10 / 11)."))?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr).trim().to_string();
        return Err(if err.is_empty() { "bSDD did not answer - check the internet connection.".into() } else { format!("bSDD: {err}") });
    }
    String::from_utf8(out.stdout).map_err(|_| "bSDD answer is not valid UTF-8".to_string())
}

pub type SearchResult = Result<(Vec<BsddClass>, usize), String>;

/// Porneste cautarea pe un fir separat; rezultatul vine pe canal.
pub fn spawn_search(text: String, dictionary_uri: Option<String>, limit: usize) -> Receiver<SearchResult> {
    let (tx, rx) = channel();
    std::thread::spawn(move || {
        let r = http_get(&search_url(&text, dictionary_uri.as_deref(), limit)).and_then(|j| parse_search(&j));
        let _ = tx.send(r);
    });
    rx
}

pub fn spawn_dictionaries() -> Receiver<Result<Vec<BsddDictionary>, String>> {
    let (tx, rx) = channel();
    std::thread::spawn(move || {
        let r = http_get(&format!("{API}/api/Dictionary/v1?Limit=1000")).and_then(|j| parse_dictionaries(&j));
        let _ = tx.send(r);
    });
    rx
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_query() {
        assert_eq!(url_encode("wall type ă/&"), "wall%20type%20%C4%83%2F%26");
        let u = search_url(" door ", Some("https://x.org/a"), 50);
        assert!(u.contains("SearchText=door&") && u.contains("DictionaryUris=https%3A%2F%2Fx.org%2Fa") && u.contains("Limit=50"), "{u}");
    }

    #[test]
    fn parses_search_answer() {
        let j = r#"{"classes":[{"dictionaryUri":"https://identifier.buildingsmart.org/uri/buildingsmart/ifc/4.3","dictionaryName":"IFC","name":"Wall","referenceCode":"IfcWall","uri":"https://identifier.buildingsmart.org/uri/buildingsmart/ifc/4.3/class/IfcWall","classType":"Class","description":"d","parentClassName":"Built Element","relatedIfcEntityNames":[]},{"dictionaryName":"X","name":"No code","uri":"https://a/b/class/K-1"}],"totalCount":7620,"offset":0,"count":2}"#;
        let (c, total) = parse_search(j).unwrap();
        assert_eq!(total, 7620);
        assert_eq!(c[0].code(), "IfcWall");
        assert_eq!(c[1].code(), "K-1");
        assert_eq!(c[0].to_item().system, "IFC");
    }

    #[test]
    fn parses_dictionaries() {
        let j = r#"{"dictionaries":[{"name":"ETIM","uri":"u2","version":"10.0"},{"name":"IFC","uri":"u1","version":"4.3"},{"name":"ETIM","uri":"u3","version":"10.1"},{"name":"Bad"}]}"#;
        let d = parse_dictionaries(j).unwrap();
        assert_eq!(d.len(), 3);
        assert_eq!(d[0].label(), "ETIM 10.1");
    }
}
