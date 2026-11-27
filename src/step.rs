//! Minimal parser for STEP files (ISO 10303-21), used by the IFC loader.
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq)]
pub enum V {
    Null,
    Star,
    Ref(u32),
    Int(i64),
    Num(f64),
    Str(String),
    Enum(String),
    List(Vec<V>),
    Typed(String, Box<V>),
}

impl V {
    pub fn as_ref(&self) -> Option<u32> {
        match self {
            V::Ref(r) => Some(*r),
            _ => None,
        }
    }
    pub fn as_f64(&self) -> Option<f64> {
        match self {
            V::Num(n) => Some(*n),
            V::Int(i) => Some(*i as f64),
            V::Typed(_, v) => v.as_f64(),
            _ => None,
        }
    }
    pub fn as_i64(&self) -> Option<i64> {
        match self {
            V::Int(i) => Some(*i),
            V::Num(n) => Some(*n as i64),
            V::Typed(_, v) => v.as_i64(),
            _ => None,
        }
    }
    pub fn as_str(&self) -> Option<&str> {
        match self {
            V::Str(s) => Some(s),
            V::Typed(_, v) => v.as_str(),
            _ => None,
        }
    }
    pub fn as_enum(&self) -> Option<&str> {
        match self {
            V::Enum(s) => Some(s),
            _ => None,
        }
    }
    pub fn as_list(&self) -> &[V] {
        match self {
            V::List(l) => l,
            _ => &[],
        }
    }
    pub fn is_null(&self) -> bool {
        matches!(self, V::Null | V::Star)
    }
    /// Displayable text for the properties panel.
    pub fn display(&self) -> String {
        match self {
            V::Null | V::Star => String::new(),
            V::Ref(r) => format!("#{r}"),
            V::Int(i) => i.to_string(),
            V::Num(n) => {
                let s = format!("{n:.4}");
                let s = s.trim_end_matches('0').trim_end_matches('.');
                s.to_string()
            }
            V::Str(s) => s.clone(),
            V::Enum(e) => match e.as_str() {
                "T" => "Yes".into(),
                "F" => "No".into(),
                o => o.to_string(),
            },
            V::List(l) => l.iter().map(|v| v.display()).collect::<Vec<_>>().join(", "),
            V::Typed(_, v) => v.display(),
        }
    }
}

pub struct Entity {
    pub ty: u32,
    start: u32,
    end: u32,
}

pub struct StepFile {
    data: Vec<u8>,
    pub ents: HashMap<u32, Entity>,
    pub type_names: Vec<String>,
    type_ids: HashMap<String, u32>,
    pub by_type: Vec<Vec<u32>>,
    pub schema: String,
}

impl StepFile {
    pub fn parse(data: Vec<u8>) -> Result<StepFile, String> {
        let mut f = StepFile {
            data,
            ents: HashMap::new(),
            type_names: Vec::new(),
            type_ids: HashMap::new(),
            by_type: Vec::new(),
            schema: String::new(),
        };
        let d = &f.data;
        let n = d.len();
        if n < 20 || !d[..n.min(200)].windows(13).any(|w| w == b"ISO-10303-21;") {
            return Err("This is not a valid STEP/IFC file (missing the ISO-10303-21 header).".into());
        }
        if let Some(p) = find(d, b"FILE_SCHEMA") {
            let e = (p + 200).min(n);
            if let Some(a) = d[p..e].iter().position(|&c| c == b'\'') {
                if let Some(b) = d[p + a + 1..e].iter().position(|&c| c == b'\'') {
                    f.schema = String::from_utf8_lossy(&d[p + a + 1..p + a + 1 + b]).to_string();
                }
            }
        }
        let mut i = find(d, b"DATA;").map(|p| p + 5).unwrap_or(0);
        let mut ents: HashMap<u32, Entity> = HashMap::new();
        let mut type_names: Vec<String> = Vec::new();
        let mut type_ids: HashMap<String, u32> = HashMap::new();
        while i < n {
            // skip whitespace / comments
            let c = d[i];
            if c.is_ascii_whitespace() {
                i += 1;
                continue;
            }
            if c == b'/' && i + 1 < n && d[i + 1] == b'*' {
                i += 2;
                while i + 1 < n && !(d[i] == b'*' && d[i + 1] == b'/') {
                    i += 1;
                }
                i += 2;
                continue;
            }
            if c != b'#' {
                // ENDSEC; etc. -> up to ';'
                while i < n && d[i] != b';' {
                    i += 1;
                }
                i += 1;
                continue;
            }
            // #id
            i += 1;
            let mut id: u32 = 0;
            while i < n && d[i].is_ascii_digit() {
                id = id.wrapping_mul(10).wrapping_add((d[i] - b'0') as u32);
                i += 1;
            }
            while i < n && (d[i].is_ascii_whitespace() || d[i] == b'=') {
                i += 1;
            }
            let ts = i;
            while i < n && (d[i].is_ascii_alphanumeric() || d[i] == b'_') {
                i += 1;
            }
            let te = i;
            while i < n && d[i].is_ascii_whitespace() {
                i += 1;
            }
            let args_start = i;
            // finds ';' outside strings
            let mut in_str = false;
            while i < n {
                let c = d[i];
                if c == b'\'' {
                    in_str = !in_str;
                } else if c == b';' && !in_str {
                    break;
                }
                i += 1;
            }
            let args_end = i;
            i += 1;
            if te > ts {
                let name = std::str::from_utf8(&d[ts..te]).unwrap_or("").to_ascii_uppercase();
                let tid = match type_ids.get(&name) {
                    Some(t) => *t,
                    None => {
                        let t = type_names.len() as u32;
                        type_ids.insert(name.clone(), t);
                        type_names.push(name);
                        t
                    }
                };
                ents.insert(id, Entity { ty: tid, start: args_start as u32, end: args_end as u32 });
            }
        }
        let mut by_type = vec![Vec::new(); type_names.len()];
        for (id, e) in &ents {
            by_type[e.ty as usize].push(*id);
        }
        for v in &mut by_type {
            v.sort_unstable();
        }
        f.ents = ents;
        f.type_names = type_names;
        f.type_ids = type_ids;
        f.by_type = by_type;
        if f.data.len() > u32::MAX as usize {
            return Err("File too large (over 4 GB).".into());
        }
        Ok(f)
    }

    pub fn ty(&self, id: u32) -> &str {
        match self.ents.get(&id) {
            Some(e) => &self.type_names[e.ty as usize],
            None => "",
        }
    }

    pub fn of_type(&self, name: &str) -> &[u32] {
        match self.type_ids.get(name) {
            Some(t) => &self.by_type[*t as usize],
            None => &[],
        }
    }

    /// The entity arguments, parsed.
    pub fn args(&self, id: u32) -> Vec<V> {
        let Some(e) = self.ents.get(&id) else { return Vec::new() };
        let s = &self.data[e.start as usize..e.end as usize];
        let mut p = Parser { s, i: 0 };
        p.ws();
        if p.peek() == Some(b'(') {
            match p.value() {
                V::List(l) => l,
                _ => Vec::new(),
            }
        } else {
            Vec::new()
        }
    }
}

fn find(d: &[u8], pat: &[u8]) -> Option<usize> {
    d.windows(pat.len()).position(|w| w == pat)
}

struct Parser<'a> {
    s: &'a [u8],
    i: usize,
}

impl<'a> Parser<'a> {
    fn peek(&self) -> Option<u8> {
        self.s.get(self.i).copied()
    }
    fn ws(&mut self) {
        while let Some(c) = self.peek() {
            if c.is_ascii_whitespace() {
                self.i += 1;
            } else {
                break;
            }
        }
    }
    fn value(&mut self) -> V {
        self.ws();
        let Some(c) = self.peek() else { return V::Null };
        match c {
            b'$' => {
                self.i += 1;
                V::Null
            }
            b'*' => {
                self.i += 1;
                V::Star
            }
            b'#' => {
                self.i += 1;
                let mut id = 0u32;
                while let Some(c) = self.peek() {
                    if c.is_ascii_digit() {
                        id = id.wrapping_mul(10).wrapping_add((c - b'0') as u32);
                        self.i += 1;
                    } else {
                        break;
                    }
                }
                V::Ref(id)
            }
            b'\'' => {
                self.i += 1;
                let st = self.i;
                loop {
                    match self.peek() {
                        None => break,
                        Some(b'\'') => {
                            if self.s.get(self.i + 1) == Some(&b'\'') {
                                self.i += 2;
                            } else {
                                break;
                            }
                        }
                        _ => self.i += 1,
                    }
                }
                let raw = &self.s[st..self.i.min(self.s.len())];
                self.i += 1;
                V::Str(decode_string(raw))
            }
            b'.' if self.s.get(self.i + 1).is_some_and(|c| c.is_ascii_alphabetic()) => {
                self.i += 1;
                let st = self.i;
                while let Some(c) = self.peek() {
                    if c == b'.' {
                        break;
                    }
                    self.i += 1;
                }
                let e = String::from_utf8_lossy(&self.s[st..self.i]).to_string();
                self.i += 1;
                V::Enum(e)
            }
            b'(' => {
                self.i += 1;
                let mut l = Vec::new();
                loop {
                    self.ws();
                    match self.peek() {
                        None => break,
                        Some(b')') => {
                            self.i += 1;
                            break;
                        }
                        Some(b',') => {
                            self.i += 1;
                        }
                        _ => {
                            let before = self.i;
                            l.push(self.value());
                            if self.i == before {
                                self.i += 1; // avoids an infinite loop on unknown characters
                            }
                        }
                    }
                }
                V::List(l)
            }
            c if c.is_ascii_digit() || c == b'-' || c == b'+' || c == b'.' => {
                let st = self.i;
                let mut is_float = false;
                while let Some(c) = self.peek() {
                    if c.is_ascii_digit() || c == b'-' || c == b'+' {
                        self.i += 1;
                    } else if c == b'.' || c == b'E' || c == b'e' {
                        is_float = true;
                        self.i += 1;
                    } else {
                        break;
                    }
                }
                let t = std::str::from_utf8(&self.s[st..self.i]).unwrap_or("0");
                if is_float {
                    V::Num(t.parse::<f64>().unwrap_or_else(|_| t.trim_end_matches('.').parse().unwrap_or(0.0)))
                } else {
                    t.parse::<i64>().map(V::Int).unwrap_or(V::Num(t.parse().unwrap_or(0.0)))
                }
            }
            c if c.is_ascii_alphabetic() => {
                let st = self.i;
                while let Some(c) = self.peek() {
                    if c.is_ascii_alphanumeric() || c == b'_' {
                        self.i += 1;
                    } else {
                        break;
                    }
                }
                let name = String::from_utf8_lossy(&self.s[st..self.i]).to_ascii_uppercase();
                self.ws();
                if self.peek() == Some(b'(') {
                    let inner = self.value();
                    let inner = match inner {
                        V::List(mut l) if l.len() == 1 => l.pop().unwrap(),
                        o => o,
                    };
                    V::Typed(name, Box::new(inner))
                } else {
                    V::Enum(name)
                }
            }
            _ => V::Null,
        }
    }
}

/// Decodes a STEP string: '' , \X2\..\X0\ , \X\hh , \S\c
pub fn decode_string(raw: &[u8]) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut i = 0;
    let n = raw.len();
    let mut buf: Vec<u8> = Vec::new();
    let flush = |buf: &mut Vec<u8>, out: &mut String| {
        if !buf.is_empty() {
            match std::str::from_utf8(buf) {
                Ok(s) => out.push_str(s),
                Err(_) => out.extend(buf.iter().map(|&b| b as char)),
            }
            buf.clear();
        }
    };
    while i < n {
        let c = raw[i];
        if c == b'\'' && i + 1 < n && raw[i + 1] == b'\'' {
            buf.push(b'\'');
            i += 2;
        } else if c == b'\\' && i + 1 < n {
            flush(&mut buf, &mut out);
            if raw[i..].starts_with(b"\\X2\\") || raw[i..].starts_with(b"\\X4\\") {
                let w = if raw[i + 2] == b'2' { 4 } else { 8 };
                i += 4;
                let mut units: Vec<u16> = Vec::new();
                while i + w <= n && raw[i] != b'\\' {
                    let h = std::str::from_utf8(&raw[i..i + w]).unwrap_or("0");
                    let cp = u32::from_str_radix(h, 16).unwrap_or(0x3f);
                    if w == 4 {
                        units.push(cp as u16);
                    } else if let Some(ch) = char::from_u32(cp) {
                        let mut b = [0u16; 2];
                        units.extend_from_slice(ch.encode_utf16(&mut b));
                    }
                    i += w;
                }
                out.push_str(&String::from_utf16_lossy(&units));
                if raw[i..].starts_with(b"\\X0\\") {
                    i += 4;
                }
            } else if raw[i..].starts_with(b"\\X\\") && i + 5 <= n {
                let h = std::str::from_utf8(&raw[i + 3..i + 5]).unwrap_or("3F");
                let cp = u32::from_str_radix(h, 16).unwrap_or(0x3f);
                out.push(char::from_u32(cp).unwrap_or('?'));
                i += 5;
            } else if raw[i..].starts_with(b"\\S\\") && i + 4 <= n {
                out.push(char::from_u32(raw[i + 3] as u32 + 128).unwrap_or('?'));
                i += 4;
            } else if raw[i + 1] == b'\\' {
                out.push('\\');
                i += 2;
            } else {
                out.push('\\');
                i += 1;
            }
        } else {
            buf.push(c);
            i += 1;
        }
    }
    flush(&mut buf, &mut out);
    out
}
