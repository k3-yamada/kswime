//! Infrastructure: the compact binary model implementing the domain ports
//! (Japanese lexicon, POS grammar with bunsetsu boundaries, English lexicon,
//! loanword bridge, romaji table).  Built once from public data by
//! `kswime-build-model`, embedded into the binary with `include_bytes!`.
//!
//! The model is read in place: every string the ports hand out is a slice
//! of the embedded bytes, the connection matrix and boundary bits are read
//! where they lie, and the indexes are flat sorted arrays searched by
//! bisection.  Loading therefore allocates only the indexes (a few dozen MB
//! for a million entries) and takes well under 100 ms; nothing is copied
//! out of the model (開発計画 9 節、縮小策 2〜3).
//!
//! All costs are on the Mozc scale: `cost = -500 · ln p`.
//!
//! Layout (little-endian):
//!   "KSWM" u32 version
//!   u16 bos, u16 noun_general, u16 noun_proper, u16 noun_sahen, u16 noun_number
//!   u16 n_function_ranges, (u16 lo, u16 hi)*
//!   romaji: u32 n, (u8 klen, key, u8 olen, out, u8 plen, pending)*
//!   conn:   u16 n, n*n u16 (cost, saturated at 65535)
//!   bunsetsu: n u16 prefix penalties, n u16 suffix penalties,
//!             ceil(n*n/8) bytes boundary bits (bit = boundary between rid,lid)
//!   ja:     u32 n, (u8 rlen, reading, u8 slen, surface, u16 lid, u16 rid,
//!                   u16 cost, u8 flags)*     sorted by reading
//!   en:     u32 n, (u8 wlen, word, u16 cost, u8 flags)*
//!   loan:   u32 n, (u8 klen, loose_hiragana, u8 wlen, english, u8 flen, katakana, u8 rank)*  sorted by loose key

use crate::domain::{EnWord, EnglishLexicon, JaEntry, JapaneseLexicon, Knowledge, LoanwordBridge, PosGrammar, PosIds, RomajiTable, COST_SCALE};

pub(crate) const MAGIC: &[u8; 4] = b"KSWM";
pub(crate) const VERSION: u32 = 3;

/// 26 letters, apostrophe, other.
const NL: usize = 28;

/// A sorted (key, start, end) index into a flat array.
type Index<'a> = Vec<(&'a str, u32, u32)>;

pub struct Model<'a> {
    pub romaji: RomajiTable,
    pub bos: u16,
    pub noun_general: u16,
    pub noun_proper: u16,
    pub noun_sahen: u16,
    pub noun_number: u16,
    function_pos: Vec<bool>,
    conn_n: usize,
    /// n*n little-endian u16, in the model bytes
    conn: &'a [u8],
    prefix_pen: Vec<u16>,
    suffix_pen: Vec<u16>,
    /// boundary bits, in the model bytes
    boundary: &'a [u8],
    /// all entries, sorted by reading (strings borrow the model bytes)
    pub ja: Vec<JaEntry<'a>>,
    /// reading → range in `ja`
    ja_index: Index<'a>,
    /// sorted by word
    en_words: Vec<(&'a str, u16, bool)>,
    /// letter trigram / bigram weights for the spelling model
    en_tri: Vec<[f32; NL]>,
    en_bi: Vec<[f32; NL]>,
    /// loose hiragana key → range in `loan_words`
    loan_by_kana: Index<'a>,
    loan_words: Vec<&'a str>,
    /// English word → range in `loan_kata`
    loan_by_en: Index<'a>,
    loan_kata: Vec<&'a str>,
}

#[inline]
fn letter_idx(b: u8) -> usize {
    match b {
        b'a'..=b'z' => (b - b'a') as usize,
        b'\'' => 26,
        _ => 27,
    }
}

struct Reader<'a> {
    b: &'a [u8],
    p: usize,
}

impl<'a> Reader<'a> {
    fn u8(&mut self) -> u8 {
        let v = self.b[self.p];
        self.p += 1;
        v
    }
    fn u16(&mut self) -> u16 {
        let v = u16::from_le_bytes([self.b[self.p], self.b[self.p + 1]]);
        self.p += 2;
        v
    }
    fn u32(&mut self) -> u32 {
        let v = u32::from_le_bytes(self.b[self.p..self.p + 4].try_into().unwrap());
        self.p += 4;
        v
    }
    fn str8(&mut self) -> &'a str {
        let n = self.u8() as usize;
        let s = std::str::from_utf8(&self.b[self.p..self.p + n]).unwrap();
        self.p += n;
        s
    }
}

/// Bisection in a sorted index.  UTF-8 byte order equals code-point order,
/// so comparing `str`s agrees with the builder's `chars()` sort.
#[inline]
fn find<'i, 'a>(index: &'i [(&'a str, u32, u32)], key: &str) -> Option<&'i (&'a str, u32, u32)> {
    index.binary_search_by(|r| r.0.cmp(key)).ok().map(|i| &index[i])
}

/// Build (key, start, end) from a run of equal keys.
fn push_range<'a>(index: &mut Index<'a>, key: &'a str, i: u32) {
    match index.last_mut() {
        Some(last) if last.0 == key => last.2 = i + 1,
        _ => index.push((key, i, i + 1)),
    }
}

impl<'a> Model<'a> {
    pub fn from_bytes(bytes: &'a [u8]) -> Result<Model<'a>, String> {
        if bytes.len() < 8 || &bytes[..4] != MAGIC {
            return Err("not a kswime model".into());
        }
        let mut r = Reader { b: bytes, p: 4 };
        let ver = r.u32();
        if ver != VERSION {
            return Err(format!("model version {ver}, expected {VERSION}"));
        }
        let bos = r.u16();
        let noun_general = r.u16();
        let noun_proper = r.u16();
        let noun_sahen = r.u16();
        let noun_number = r.u16();
        let nf = r.u16() as usize;
        let mut ranges = Vec::with_capacity(nf);
        for _ in 0..nf {
            ranges.push((r.u16(), r.u16()));
        }
        // romaji
        let n = r.u32() as usize;
        let mut tsv = String::new();
        for _ in 0..n {
            let k = r.str8();
            let o = r.str8();
            let p = r.str8();
            tsv.push_str(k);
            tsv.push('\t');
            tsv.push_str(o);
            tsv.push('\t');
            tsv.push_str(p);
            tsv.push('\n');
        }
        let romaji = RomajiTable::from_tsv(&tsv);
        // connection matrix: left in place
        let conn_n = r.u16() as usize;
        let conn = &bytes[r.p..r.p + conn_n * conn_n * 2];
        r.p += conn_n * conn_n * 2;
        let mut prefix_pen = Vec::with_capacity(conn_n);
        let mut suffix_pen = Vec::with_capacity(conn_n);
        for _ in 0..conn_n {
            prefix_pen.push(r.u16());
        }
        for _ in 0..conn_n {
            suffix_pen.push(r.u16());
        }
        let nbytes = (conn_n * conn_n).div_ceil(8);
        let boundary = &bytes[r.p..r.p + nbytes];
        r.p += nbytes;
        let mut function_pos = vec![false; conn_n];
        for (lo, hi) in ranges {
            for i in lo as usize..=(hi as usize).min(conn_n - 1) {
                function_pos[i] = true;
            }
        }
        // Japanese lexicon
        let n = r.u32() as usize;
        let mut ja = Vec::with_capacity(n);
        let mut ja_index: Index<'a> = Vec::new();
        for i in 0..n {
            let reading = r.str8();
            let surface = r.str8();
            let lid = r.u16();
            let rid = r.u16();
            let cost = r.u16();
            let flags = r.u8();
            push_range(&mut ja_index, reading, i as u32);
            ja.push(JaEntry { surface, lid, rid, cost, kata_only: flags & 1 != 0 });
        }
        // English lexicon and spelling model
        let n = r.u32() as usize;
        let mut en_words: Vec<(&'a str, u16, bool)> = Vec::with_capacity(n);
        let mut en_tri = vec![[0.0f32; NL]; NL * NL];
        let mut en_bi = vec![[0.0f32; NL]; NL];
        for _ in 0..n {
            let w = r.str8();
            let cost = r.u16();
            let flags = r.u8();
            en_words.push((w, cost, flags & 1 != 0));
            let p = (-(cost as f64) / COST_SCALE).exp();
            let weight = (p * 1e8).max(1.0).ln() as f32;
            let (mut a, mut b) = (b' ', b' ');
            for c in w.bytes().chain(std::iter::once(b' ')) {
                let x = letter_idx(c);
                en_tri[letter_idx(a) * NL + letter_idx(b)][x] += weight;
                en_bi[letter_idx(b)][x] += weight;
                a = b;
                b = c;
            }
        }
        en_words.sort_unstable_by(|x, y| x.0.cmp(y.0));
        // loanword bridge: records are sorted by loose key in the file; the
        // English-side index is built by sorting references (word, rank)
        let mut loan_by_kana: Index<'a> = Vec::new();
        let mut loan_words: Vec<&'a str> = Vec::new();
        let mut by_en: Vec<(&'a str, u8, &'a str)> = Vec::new();
        if r.p < bytes.len() {
            let n = r.u32() as usize;
            loan_words.reserve(n);
            by_en.reserve(n);
            for _ in 0..n {
                let key = r.str8();
                let word = r.str8();
                let kata = r.str8();
                let rank = r.u8();
                // words for a key: keep file order (cheapest first), dedupe
                let dup = loan_by_kana.last().is_some_and(|l| l.0 == key) && loan_words[loan_by_kana.last().unwrap().1 as usize..].contains(&word);
                if !dup {
                    push_range(&mut loan_by_kana, key, loan_words.len() as u32);
                    loan_words.push(word);
                }
                by_en.push((word, rank, kata));
            }
        }
        by_en.sort_by(|x, y| (x.0, x.1).cmp(&(y.0, y.1))); // stable: file order within a rank
        let mut loan_by_en: Index<'a> = Vec::new();
        let mut loan_kata: Vec<&'a str> = Vec::with_capacity(by_en.len());
        for (word, _, kata) in by_en {
            let dup = loan_by_en.last().is_some_and(|l| l.0 == word) && loan_kata[loan_by_en.last().unwrap().1 as usize..].contains(&kata);
            if !dup {
                push_range(&mut loan_by_en, word, loan_kata.len() as u32);
                loan_kata.push(kata);
            }
        }
        Ok(Model {
            romaji,
            bos,
            noun_general,
            noun_proper,
            noun_sahen,
            noun_number,
            function_pos,
            conn_n,
            conn,
            prefix_pen,
            suffix_pen,
            boundary,
            ja,
            ja_index,
            en_words,
            en_tri,
            en_bi,
            loan_by_kana,
            loan_words,
            loan_by_en,
            loan_kata,
        })
    }
}

impl JapaneseLexicon for Model<'_> {
    fn lookup(&self, reading: &[char]) -> &[JaEntry<'_>] {
        let key: String = reading.iter().collect();
        match find(&self.ja_index, &key) {
            Some(r) => &self.ja[r.1 as usize..r.2 as usize],
            None => &[],
        }
    }

    fn has_prefix(&self, prefix: &[char]) -> bool {
        let key: String = prefix.iter().collect();
        let i = self.ja_index.partition_point(|r| r.0 < key.as_str());
        self.ja_index.get(i).is_some_and(|r| r.0.starts_with(key.as_str()))
    }

    fn prefixes(&self, kana: &[char], max_len: usize) -> Vec<(usize, &JaEntry<'_>)> {
        let mut out = Vec::new();
        let mut key = String::new();
        for (len, c) in kana.iter().take(max_len).enumerate() {
            key.push(*c);
            let i = self.ja_index.partition_point(|r| r.0 < key.as_str());
            match self.ja_index.get(i) {
                Some(r) if r.0.starts_with(key.as_str()) => {
                    if r.0 == key {
                        for idx in r.1..r.2 {
                            out.push((len + 1, &self.ja[idx as usize]));
                        }
                    }
                }
                _ => break,
            }
        }
        out
    }

    fn max_reading_len(&self) -> usize {
        16
    }
}

impl PosGrammar for Model<'_> {
    fn pos(&self) -> PosIds {
        PosIds {
            bos: self.bos,
            noun_general: self.noun_general,
            noun_proper: self.noun_proper,
            noun_sahen: self.noun_sahen,
            noun_number: self.noun_number,
        }
    }

    #[inline]
    fn connection(&self, left_rid: u16, right_lid: u16) -> i32 {
        let i = (left_rid as usize * self.conn_n + right_lid as usize) * 2;
        u16::from_le_bytes([self.conn[i], self.conn[i + 1]]) as i32
    }

    #[inline]
    fn is_function(&self, lid: u16) -> bool {
        self.function_pos.get(lid as usize).copied().unwrap_or(false)
    }

    #[inline]
    fn is_boundary(&self, left_rid: u16, right_lid: u16) -> bool {
        let bit = left_rid as usize * self.conn_n + right_lid as usize;
        self.boundary[bit / 8] & (1 << (bit % 8)) != 0
    }

    #[inline]
    fn prefix_penalty(&self, lid: u16) -> i32 {
        self.prefix_pen.get(lid as usize).copied().unwrap_or(0) as i32
    }

    #[inline]
    fn suffix_penalty(&self, rid: u16) -> i32 {
        self.suffix_pen.get(rid as usize).copied().unwrap_or(0) as i32
    }
}

impl EnglishLexicon for Model<'_> {
    fn word(&self, w: &str) -> Option<EnWord> {
        self.en_words.binary_search_by(|e| e.0.cmp(w)).ok().map(|i| {
            let e = self.en_words[i];
            EnWord { cost: e.1 as i32, supplementary: e.2 }
        })
    }

    fn spelling_cost(&self, w: &str) -> i32 {
        let mut nll = 0.0f64;
        let (mut a, mut b) = (b' ', b' ');
        for c in w.bytes().map(|c| c.to_ascii_lowercase()).chain(std::iter::once(b' ')) {
            let x = letter_idx(c);
            let t = &self.en_tri[letter_idx(a) * NL + letter_idx(b)];
            let n: f32 = t.iter().sum();
            let pt = if n > 0.0 { (t[x] / n) as f64 } else { 0.0 };
            let bi = &self.en_bi[letter_idx(b)];
            let s: f32 = bi.iter().sum();
            let pb = if s > 0.0 { (bi[x] / s) as f64 } else { 0.0 };
            let n = n as f64;
            let lt = n / (n + 20.0);
            let p = lt * pt + (1.0 - lt) * (0.85 * pb + 0.15 / NL as f64);
            nll += -(p.max(1e-9)).ln();
            a = b;
            b = c;
        }
        (nll * COST_SCALE) as i32
    }
}

impl LoanwordBridge for Model<'_> {
    fn english_for_kana(&self, kana_loose: &str) -> &[&str] {
        match find(&self.loan_by_kana, kana_loose) {
            Some(r) => &self.loan_words[r.1 as usize..r.2 as usize],
            None => &[],
        }
    }
    fn katakana_for_english(&self, word: &str) -> &[&str] {
        match find(&self.loan_by_en, word) {
            Some(r) => &self.loan_kata[r.1 as usize..r.2 as usize],
            None => &[],
        }
    }
}

impl Knowledge for Model<'_> {
    fn romaji(&self) -> &RomajiTable {
        &self.romaji
    }
}

/// The embedded model.  Build it with `kswime-build-model`.
pub static MODEL_BYTES: &[u8] = include_bytes!("../../data/model.bin");

/// Index the embedded model (no copy of the data; see the module notes).
pub fn load_embedded_model() -> Model<'static> {
    Model::from_bytes(MODEL_BYTES).expect("embedded model")
}
