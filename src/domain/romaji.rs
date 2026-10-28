//! Romaji → kana parsing driven by a Mozc-style romaji table.
//!
//! The parser is deliberately *generous*: it produces every kana string that a
//! prefix of the keystroke run could legitimately mean, so that the lattice
//! decoder (not this module) decides where a Japanese span ends.

use std::collections::HashMap;

#[derive(Debug, Clone)]
struct Entry {
    output: Vec<char>,
    pending: String,
}

#[derive(Debug, Default)]
pub struct RomajiTable {
    exact: HashMap<String, Entry>,
    /// every proper prefix of every key (used to know whether a pending run
    /// can still grow into a longer key)
    prefixes: std::collections::HashSet<String>,
    max_key: usize,
}

/// One point at which the keystroke run `raw[start..end]` forms a complete
/// kana sequence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KanaPoint {
    /// exclusive end offset in bytes relative to the start of the run
    pub end: usize,
    pub kana: Vec<char>,
}

impl RomajiTable {
    pub fn from_tsv(text: &str) -> Self {
        let mut t = RomajiTable::default();
        for line in text.lines() {
            if line.trim().is_empty() || line.starts_with('#') {
                continue;
            }
            let mut cols = line.split('\t');
            let key = match cols.next() {
                Some(k) if !k.is_empty() => k.to_string(),
                _ => continue,
            };
            let out: Vec<char> = cols.next().unwrap_or("").chars().collect();
            let pending = cols.next().unwrap_or("").to_string();
            t.max_key = t.max_key.max(key.len());
            for i in 1..key.len() {
                t.prefixes.insert(key[..i].to_string());
            }
            t.exact.insert(key, Entry { output: out, pending });
        }
        t
    }

    pub fn has_prefix(&self, s: &str) -> bool {
        self.prefixes.contains(s)
    }

    /// Enumerate all complete-kana end points of the run `raw` (ASCII bytes,
    /// already lower-cased, letters and `-`/`'` only).  Returns points in
    /// increasing `end` order.  Stops at the first byte that cannot be part of
    /// any romaji sequence.
    pub fn parse_points(&self, raw: &[u8], max_len: usize) -> Vec<KanaPoint> {
        let mut points = Vec::new();
        let mut kana: Vec<char> = Vec::new();
        let mut pending = String::new();
        let limit = raw.len().min(max_len);
        let mut i = 0;
        while i < limit {
            let c = raw[i] as char;
            let mut cand = pending.clone();
            cand.push(c);
            let next = raw.get(i + 1).map(|b| *b as char);

            // MS-IME style "nn": ん, and if a vowel / y follows, keep an "n"
            // pending so that "konnichiha" → こんにちは.  "nn" + consonant → ん.
            if cand == "nn" {
                kana.push('ん');
                pending.clear();
                if matches!(next, Some('a' | 'i' | 'u' | 'e' | 'o' | 'y')) {
                    pending.push('n');
                }
                i += 1;
                self.push_point(&mut points, i, &kana, &pending);
                continue;
            }

            if let Some(e) = self.exact.get(&cand) {
                if self.has_prefix(&cand) {
                    // could still become a longer key (e.g. "ky" → "kya"):
                    // Mozc keeps it pending; a *complete* reading here is still
                    // reported when the entry has no pending residue.
                    pending = cand;
                    // "n" is both exact (ん) and a prefix; report "…ん" as a
                    // possible end point.
                    if pending == "n" {
                        let mut k = kana.clone();
                        k.push('ん');
                        points.push(KanaPoint { end: i + 1, kana: k });
                    }
                } else {
                    kana.extend(e.output.iter());
                    pending = e.pending.clone();
                    self.push_point(&mut points, i + 1, &kana, &pending);
                }
                i += 1;
                continue;
            }

            if self.has_prefix(&cand) {
                pending = cand;
                i += 1;
                continue;
            }

            // No key starts with `cand`.  If the pending run is itself a key
            // (typically "n"), flush it and retry this byte with that residue.
            if !pending.is_empty() {
                if let Some(e) = self.exact.get(&pending) {
                    kana.extend(e.output.iter());
                    pending = e.pending.clone();
                    // retry same byte with new pending
                    continue;
                }
            }
            break;
        }
        // trailing pending "n" → ん was already reported when it was pushed
        points.dedup();
        points
    }

    fn push_point(&self, points: &mut Vec<KanaPoint>, end: usize, kana: &[char], pending: &str) {
        if pending.is_empty() {
            points.push(KanaPoint { end, kana: kana.to_vec() });
        } else if pending == "n" {
            let mut k = kana.to_vec();
            k.push('ん');
            points.push(KanaPoint { end, kana: k });
        }
    }

    /// Convert a whole run, succeeding only when the run is a complete kana
    /// sequence (a trailing `n` counts as ん).
    pub fn convert_exact(&self, raw: &str) -> Option<String> {
        let b = raw.as_bytes();
        let pts = self.parse_points(b, b.len());
        pts.into_iter().rev().find(|p| p.end == b.len()).map(|p| p.kana.into_iter().collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn table() -> RomajiTable {
        RomajiTable::from_tsv(include_str!("../../data/raw/romanji-hiragana.tsv"))
    }

    #[test]
    fn basic() {
        let t = table();
        assert_eq!(t.convert_exact("kyouha").as_deref(), Some("きょうは"));
        assert_eq!(t.convert_exact("kensaku").as_deref(), Some("けんさく"));
        assert_eq!(t.convert_exact("konnichiha").as_deref(), Some("こんにちは"));
        assert_eq!(t.convert_exact("konnnichiha").as_deref(), Some("こんにちは"));
        assert_eq!(t.convert_exact("kanji").as_deref(), Some("かんじ"));
        assert_eq!(t.convert_exact("gakkou").as_deref(), Some("がっこう"));
        assert_eq!(t.convert_exact("matcha").as_deref(), Some("まっちゃ"));
        assert_eq!(t.convert_exact("hon").as_deref(), Some("ほん"));
        assert_eq!(t.convert_exact("ky"), None);
        assert_eq!(t.convert_exact("strong"), None);
        assert_eq!(t.convert_exact("github"), None);
        assert_eq!(t.convert_exact("sushi").as_deref(), Some("すし"));
    }

    #[test]
    fn points() {
        let t = table();
        let pts = t.parse_points(b"kanji", 16);
        let ends: Vec<usize> = pts.iter().map(|p| p.end).collect();
        assert!(ends.contains(&2)); // か
        assert!(ends.contains(&3)); // かん
        assert!(ends.contains(&5)); // かんじ
    }
}
