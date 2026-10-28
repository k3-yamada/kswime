//! Infrastructure: writer side of the binary model format, plus the parser
//! for Mozc's bunsetsu rule files (`boundary.def`, `segmenter.def`).

use crate::infrastructure::model::{MAGIC, VERSION};


/// Owned dictionary entry while building (the runtime `JaEntry` borrows).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuildEntry {
    pub reading: String,
    pub surface: String,
    pub lid: u16,
    pub rid: u16,
    pub cost: i32,
    pub kata_only: bool,
}

pub struct Builder {
    pub bos: u16,
    pub noun_general: u16,
    pub noun_proper: u16,
    pub noun_sahen: u16,
    pub noun_number: u16,
    pub function_ranges: Vec<(u16, u16)>,
    pub romaji: Vec<(String, String, String)>,
    pub conn_n: usize,
    pub conn: Vec<i32>,
    pub prefix_pen: Vec<i32>,
    pub suffix_pen: Vec<i32>,
    /// boundary[rid * n + lid]
    pub boundary: Vec<bool>,
    pub ja: Vec<BuildEntry>,
    pub en: Vec<(String, i32, bool)>,
    /// (loose hiragana key, english, katakana, rank), cheapest English first;
    /// rank 0 = katakana form attested in the Japanese lexicon, 1 = first
    /// pronunciation variant, 2 = other variant
    pub loan: Vec<(String, String, String, u8)>,
}

impl Builder {
    pub fn write(&self) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(MAGIC);
        out.extend_from_slice(&VERSION.to_le_bytes());
        for v in [self.bos, self.noun_general, self.noun_proper, self.noun_sahen, self.noun_number] {
            out.extend_from_slice(&v.to_le_bytes());
        }
        out.extend_from_slice(&(self.function_ranges.len() as u16).to_le_bytes());
        for (lo, hi) in &self.function_ranges {
            out.extend_from_slice(&lo.to_le_bytes());
            out.extend_from_slice(&hi.to_le_bytes());
        }
        out.extend_from_slice(&(self.romaji.len() as u32).to_le_bytes());
        for (k, o, p) in &self.romaji {
            put_str8(&mut out, k);
            put_str8(&mut out, o);
            put_str8(&mut out, p);
        }
        out.extend_from_slice(&(self.conn_n as u16).to_le_bytes());
        for &c in &self.conn {
            out.extend_from_slice(&(c.clamp(0, 65535) as u16).to_le_bytes());
        }
        for &p in &self.prefix_pen {
            out.extend_from_slice(&(p.clamp(0, 65535) as u16).to_le_bytes());
        }
        for &p in &self.suffix_pen {
            out.extend_from_slice(&(p.clamp(0, 65535) as u16).to_le_bytes());
        }
        let mut bits = vec![0u8; (self.conn_n * self.conn_n).div_ceil(8)];
        for (i, &b) in self.boundary.iter().enumerate() {
            if b {
                bits[i / 8] |= 1 << (i % 8);
            }
        }
        out.extend_from_slice(&bits);
        let mut ja: Vec<&BuildEntry> = self.ja.iter().collect();
        ja.sort_by(|a, b| a.reading.chars().cmp(b.reading.chars()).then(a.cost.cmp(&b.cost)));
        out.extend_from_slice(&(ja.len() as u32).to_le_bytes());
        for e in ja {
            put_str8(&mut out, &e.reading);
            put_str8(&mut out, &e.surface);
            out.extend_from_slice(&e.lid.to_le_bytes());
            out.extend_from_slice(&e.rid.to_le_bytes());
            out.extend_from_slice(&(e.cost.clamp(0, 65535) as u16).to_le_bytes());
            out.push(if e.kata_only { 1 } else { 0 });
        }
        out.extend_from_slice(&(self.en.len() as u32).to_le_bytes());
        for (w, c, extra) in &self.en {
            put_str8(&mut out, w);
            out.extend_from_slice(&((*c).clamp(0, 65535) as u16).to_le_bytes());
            out.push(if *extra { 1 } else { 0 });
        }
        out.extend_from_slice(&(self.loan.len() as u32).to_le_bytes());
        for (k, w, kata, rank) in &self.loan {
            put_str8(&mut out, k);
            put_str8(&mut out, w);
            put_str8(&mut out, kata);
            out.push(*rank);
        }
        out
    }
}

fn put_str8(out: &mut Vec<u8>, s: &str) {
    let b = s.as_bytes();
    assert!(b.len() < 256, "string too long: {s}");
    out.push(b.len() as u8);
    out.extend_from_slice(b);
}

// ---------------------------------------------------------------------------
// Mozc rule patterns (a tiny subset of regex: comma-separated fields, `*`
// as "any single field", `(a|b|c)` alternation inside a field, `^` anchor,
// trailing `,` = prefix match).  Enough for boundary.def / segmenter.def.
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct FeaturePattern {
    fields: Vec<Vec<String>>, // alternatives per field; empty vec = wildcard
    prefix: bool,
    any: bool,
}

impl FeaturePattern {
    pub fn parse(p: &str) -> FeaturePattern {
        let p = p.trim();
        if p == "*" {
            return FeaturePattern { fields: Vec::new(), prefix: true, any: true };
        }
        let p = p.strip_prefix('^').unwrap_or(p);
        let p = p.strip_suffix('$').unwrap_or(p);
        let prefix = p.ends_with(',');
        let p = p.trim_end_matches(',');
        let mut fields = Vec::new();
        for f in split_fields(p) {
            let f = f.trim();
            if f == "*" {
                fields.push(Vec::new());
            } else if f.starts_with('(') && f.ends_with(')') {
                let inner = &f[1..f.len() - 1];
                let inner = inner.strip_prefix("?:").unwrap_or(inner);
                fields.push(inner.split('|').map(|s| s.to_string()).collect());
            } else {
                fields.push(vec![f.to_string()]);
            }
        }
        FeaturePattern { fields, prefix, any: false }
    }

    /// `feature` is a comma-separated POS feature string from id.def.
    pub fn matches(&self, feature: &str) -> bool {
        if self.any {
            return true;
        }
        let fs: Vec<&str> = feature.split(',').collect();
        // Mozc semantics: the pattern is anchored and matched field by field;
        // a trailing comma additionally requires at least one more field.
        if fs.len() < self.fields.len() || (self.prefix && fs.len() == self.fields.len()) {
            return false;
        }
        self.fields.iter().zip(fs.iter()).all(|(alts, f)| alts.is_empty() || alts.iter().any(|a| a == f))
    }
}

/// Split on commas that are not inside parentheses.
fn split_fields(p: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut depth = 0;
    let mut cur = String::new();
    for c in p.chars() {
        match c {
            '(' => {
                depth += 1;
                cur.push(c);
            }
            ')' => {
                depth -= 1;
                cur.push(c);
            }
            ',' if depth == 0 => {
                out.push(std::mem::take(&mut cur));
            }
            _ => cur.push(c),
        }
    }
    out.push(cur);
    out
}

/// Penalties per POS id from boundary.def (first matching rule wins).
pub fn boundary_penalties(features: &[String], rules_text: &str) -> (Vec<i32>, Vec<i32>) {
    let mut prefix_rules: Vec<(FeaturePattern, i32)> = Vec::new();
    let mut suffix_rules: Vec<(FeaturePattern, i32)> = Vec::new();
    for line in rules_text.lines() {
        let line = line.split('#').next().unwrap().trim();
        if line.is_empty() {
            continue;
        }
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 3 {
            continue;
        }
        let pat = FeaturePattern::parse(parts[1]);
        let pen: i32 = parts[2].parse().unwrap_or(0);
        match parts[0] {
            "PREFIX" => prefix_rules.push((pat, pen)),
            "SUFFIX" => suffix_rules.push((pat, pen)),
            _ => {}
        }
    }
    let first = |rules: &[(FeaturePattern, i32)], f: &str| rules.iter().find(|(p, _)| p.matches(f)).map(|(_, c)| *c).unwrap_or(0);
    let prefix = features.iter().map(|f| first(&prefix_rules, f)).collect();
    let suffix = features.iter().map(|f| first(&suffix_rules, f)).collect();
    (prefix, suffix)
}

/// Boundary matrix from segmenter.def (first matching rule wins; the file's
/// own last rule `* * true` is the default).
pub fn boundary_matrix(features: &[String], rules_text: &str) -> Vec<bool> {
    let mut rules: Vec<(FeaturePattern, FeaturePattern, bool)> = Vec::new();
    for line in rules_text.lines() {
        let line = line.split('#').next().unwrap().trim();
        if line.is_empty() {
            continue;
        }
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 3 {
            continue;
        }
        rules.push((FeaturePattern::parse(parts[0]), FeaturePattern::parse(parts[1]), parts[2] == "true"));
    }
    let n = features.len();
    // pre-evaluate each rule side per id
    let left: Vec<Vec<bool>> = rules.iter().map(|(l, _, _)| features.iter().map(|f| l.matches(f)).collect()).collect();
    let right: Vec<Vec<bool>> = rules.iter().map(|(_, r, _)| features.iter().map(|f| r.matches(f)).collect()).collect();
    let mut m = vec![true; n * n];
    for rid in 0..n {
        for lid in 0..n {
            for (k, (_, _, b)) in rules.iter().enumerate() {
                if left[k][rid] && right[k][lid] {
                    m[rid * n + lid] = *b;
                    break;
                }
            }
        }
    }
    m
}
