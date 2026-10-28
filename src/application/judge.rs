//! Use case: judge a keystroke buffer — which keystrokes are Japanese,
//! which are English, which are identifiers — as a log-linear lattice.
//!
//! Every keystroke run gets competing explanations: Japanese arcs (romaji →
//! kana → reading lexicon), English arcs (word lexicon or letter n-gram) and,
//! for tokens that contain identifier punctuation (`@ / : _`), an identifier
//! arc.  Each arc and each transition carries a sparse feature vector; the
//! cost is its dot product with `Weights`.  An English token enters the
//! Japanese grammar as a general or サ変 noun, so the same particle statistics
//! that score `今日は＋名詞＋で＋検索` score `kyouha|zoom|de|kaigi`.
//!
//! Viterbi gives the best segmentation, forward–backward the per-keystroke
//! posterior P(English), and a constrained Viterbi recovers the *gold* path
//! for a rendering the user accepted or corrected (used for learning and for
//! the correction-keystroke metric).

use crate::application::weights::{feat, Config, Weights, N_FEATURES};
use crate::domain::{Analysis, Context, Knowledge, Lang, Segment, COST_SCALE};
use std::collections::HashMap;

/// Sparse feature vector: (feature index, value).
pub type Features = Vec<(u8, f32)>;

#[derive(Debug, Clone)]
pub struct Arc {
    pub start: usize,
    pub end: usize,
    pub lang: Lang,
    pub lid: u16,
    pub rid: u16,
    pub features: Features,
    /// kana for Japanese arcs, raw text otherwise
    pub text: String,
    pub oov: bool,
    bad_start: bool,
    cap: bool,
    pending: bool,
    /// identifier-like token (e-mail, path, URL): rendered verbatim
    pub identifier: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CharClass {
    Letter { upper: bool },
    Hyphen,
    Apostrophe,
    Space,
    Digit,
    Other,
}

fn classify(c: char) -> CharClass {
    match c {
        'a'..='z' => CharClass::Letter { upper: false },
        'A'..='Z' => CharClass::Letter { upper: true },
        '-' => CharClass::Hyphen,
        '\'' => CharClass::Apostrophe,
        ' ' | '\t' => CharClass::Space,
        '0'..='9' => CharClass::Digit,
        _ => CharClass::Other,
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
struct StateKey {
    rid: u16,
    lang: u8, // 0 Ja, 1 En, 2 none(start)
    after_space: bool,
    en_run: u8,
    cap: bool,
}

fn lang_code(l: Option<Lang>) -> u8 {
    match l {
        Some(Lang::Ja) => 0,
        Some(Lang::En) => 1,
        _ => 2,
    }
}

fn is_bad_start_kana(c: char) -> bool {
    matches!(c, 'ん' | 'っ' | 'ゃ' | 'ゅ' | 'ょ' | 'ぁ' | 'ぃ' | 'ぅ' | 'ぇ' | 'ぉ' | 'ー')
}

/// Consonant clusters a Japanese mora cannot contain (phonotactics): count
/// positions where two consonants meet that no romaji key covers.
fn mora_violations(w: &[u8]) -> i32 {
    let is_v = |c: u8| matches!(c, b'a' | b'i' | b'u' | b'e' | b'o');
    let mut n = 0;
    for pair in w.windows(2) {
        let (a, b) = (pair[0], pair[1]);
        if a.is_ascii_alphabetic() && b.is_ascii_alphabetic() && !is_v(a) && !is_v(b) {
            let legal = a == b                       // sokuon: kk, tt …
                || a == b'n'                          // ん + consonant
                || b == b'y' || b == b'h'             // ky, sh, ch, th
                || (a == b't' && b == b's');          // ts
            if !legal {
                n += 1;
            }
        }
    }
    n
}

pub struct Lattice<'a> {
    kb: &'a dyn Knowledge,
    w: &'a Weights,
    cfg: &'a Config,
    input: Vec<char>,
    classes: Vec<CharClass>,
    arcs: Vec<Arc>,
    by_start: Vec<Vec<usize>>,
    user_bias: &'a HashMap<String, i32>,
    ctx: Context,
    allow_pending: bool,
    /// language of the first character after the caret (host-provided)
    after_lang: Option<Lang>,
}

impl<'a> Lattice<'a> {
    pub fn build(
        kb: &'a dyn Knowledge,
        w: &'a Weights,
        cfg: &'a Config,
        input: &[char],
        ctx: Context,
        after_lang: Option<Lang>,
        user_bias: &'a HashMap<String, i32>,
        allow_pending: bool,
    ) -> Self {
        let n = input.len();
        let classes: Vec<CharClass> = input.iter().map(|&c| classify(c)).collect();
        let mut lat = Lattice {
            kb,
            w,
            cfg,
            input: input.to_vec(),
            classes,
            arcs: Vec::new(),
            by_start: vec![Vec::new(); n + 1],
            user_bias,
            ctx,
            allow_pending,
            after_lang,
        };
        lat.add_identifier_arcs();
        for i in 0..n {
            lat.add_arcs_from(i);
        }
        // every keystroke must be explainable: a lone consonant such as "k"
        // or "x" gets a one-letter English fallback arc
        for i in 0..n {
            if lat.by_start[i].is_empty() {
                let text: String = lat.input[i..i + 1].iter().collect();
                let lower = text.to_ascii_lowercase();
                let f: Features = vec![
                    (feat::EN_BIAS as u8, 1.0),
                    (feat::EN_SPELL as u8, lat.kb.spelling_cost(&lower) as f32),
                    (feat::EN_OOV as u8, 1.0),
                    (feat::EN_OOV_LETTER as u8, 1.0),
                ];
                lat.push(Arc {
                    start: i,
                    end: i + 1,
                    lang: Lang::En,
                    lid: lat.kb.pos().noun_proper,
                    rid: lat.kb.pos().noun_proper,
                    features: f,
                    text,
                    oov: true,
                    bad_start: false,
                    cap: false,
                    pending: false,
                    identifier: false,
                });
            }
        }
        lat
    }

    pub fn arcs(&self) -> &[Arc] {
        &self.arcs
    }

    fn is_run_char(&self, i: usize) -> bool {
        matches!(self.classes[i], CharClass::Letter { .. } | CharClass::Hyphen | CharClass::Apostrophe)
    }

    fn push(&mut self, arc: Arc) {
        let idx = self.arcs.len();
        self.by_start[arc.start].push(idx);
        self.arcs.push(arc);
    }

    /// Identifier tokens are their own generative source: a whitespace
    /// delimited token containing `@ _ / : \ +`, a `.` with letters on both
    /// sides (domain names), or letters glued to digits (`i18n`, `k8s`).
    /// They are rendered verbatim and cost nothing; the alternative
    /// explanations still compete, so a Japanese sentence ending in `.`
    /// is not swallowed.
    fn add_identifier_arcs(&mut self) {
        let n = self.input.len();
        let mut i = 0;
        while i < n {
            if self.classes[i] == CharClass::Space {
                i += 1;
                continue;
            }
            let mut j = i;
            while j < n && self.classes[j] != CharClass::Space {
                j += 1;
            }
            let tok = &self.input[i..j];
            let has_punct = tok.iter().any(|c| matches!(c, '@' | '_' | '/' | ':' | '\\' | '+'));
            let domain = tok.windows(3).any(|w| w[1] == '.' && w[0].is_ascii_alphabetic() && w[2].is_ascii_alphabetic());
            // letters–digits–letters (i18n, k8s, h2o); "10ji" (10時) is not
            let letters = tok.iter().filter(|c| c.is_ascii_alphabetic()).count();
            let digits = tok.iter().filter(|c| c.is_ascii_digit()).count();
            let alnum_mix = letters > 0
                && digits > 0
                && letters + digits == tok.len()
                && letters <= 3
                && tok[0].is_ascii_alphabetic()
                && tok[tok.len() - 1].is_ascii_alphabetic();
            if has_punct || domain || alnum_mix {
                let text: String = tok.iter().collect();
                self.push(Arc {
                    start: i,
                    end: j,
                    lang: Lang::En,
                    lid: self.kb.pos().noun_proper,
                    rid: self.kb.pos().noun_proper,
                    features: Vec::new(),
                    text,
                    oov: false,
                    bad_start: false,
                    cap: false,
                    pending: false,
                    identifier: true,
                });
            }
            i = j;
        }
    }

    fn add_arcs_from(&mut self, i: usize) {
        let n = self.input.len();
        if !self.is_run_char(i) {
            let c = self.input[i];
            self.push(Arc {
                start: i,
                end: i + 1,
                lang: Lang::Neutral,
                lid: 0,
                rid: 0,
                features: Vec::new(),
                text: c.to_string(),
                oov: false,
                bad_start: false,
                cap: false,
                pending: false,
                identifier: false,
            });
            return;
        }
        let mut run_end = i;
        while run_end < n && self.is_run_char(run_end) {
            run_end += 1;
        }
        let run: Vec<u8> = self.input[i..run_end].iter().map(|c| c.to_ascii_lowercase() as u8).collect();
        self.add_ja_arcs(i, &run);
        self.add_en_arcs(i, &run);
        if self.allow_pending && run_end == n && run.len() <= self.cfg.pending_max_len {
            self.push(Arc {
                start: i,
                end: n,
                lang: Lang::Neutral,
                lid: 0,
                rid: 0,
                features: Vec::new(),
                text: self.input[i..n].iter().collect(),
                oov: false,
                bad_start: false,
                cap: false,
                pending: true,
                identifier: false,
            });
        }
    }

    fn uppercase_count(&self, s: usize, e: usize) -> i32 {
        self.classes[s..e].iter().filter(|c| matches!(c, CharClass::Letter { upper: true })).count() as i32
    }

    fn add_ja_arcs(&mut self, i: usize, run: &[u8]) {
        let points = self.kb.romaji().parse_points(run, run.len());
        for pt in points {
            if pt.kana.is_empty() || pt.kana.len() > self.cfg.ja_max_kana {
                continue;
            }
            let end = i + pt.end;
            let raw_span = &run[..pt.end];
            let raw = std::str::from_utf8(raw_span).unwrap().to_string();
            let mut common: Features = Vec::new();
            let upper = self.uppercase_count(i, end);
            if upper > 0 {
                common.push((feat::UPPER_IN_JA as u8, upper as f32));
            }
            if let Some(b) = self.user_bias.get(&raw) {
                common.push((feat::USER_BIAS as u8, -(*b as f32)));
            }
            let rare = raw_span.iter().filter(|b| matches!(b, b'l' | b'v' | b'x' | b'q')).count();
            if rare > 0 {
                common.push((feat::RARE_LETTER as u8, rare as f32));
            }
            let dig = raw_span.windows(2).filter(|w| matches!(w, [b't', b'h'] | [b'd', b'h'])).count();
            if dig > 0 {
                common.push((feat::RARE_DIGRAPH as u8, dig as f32));
            }
            let rk = pt.kana.iter().filter(|c| matches!(c, 'ぢ' | 'づ' | 'ゔ')).count();
            if rk > 0 {
                common.push((feat::RARE_KANA as u8, rk as f32));
            }
            let bad_start = is_bad_start_kana(pt.kana[0]);
            let loan_strength: f32 = match self.kb.word(&raw) {
                Some(w) if w.supplementary => 1.0,
                Some(w) => ((7000 - w.cost) as f32 / 2500.0).clamp(0.0, 1.0),
                None => 0.0,
            };
            let text: String = pt.kana.iter().collect();
            let entries = self.kb.lookup(&pt.kana);
            if !entries.is_empty() {
                let mut seen: Vec<(u16, u16)> = Vec::new();
                for e in entries {
                    if seen.len() >= 4 {
                        break;
                    }
                    if seen.contains(&(e.lid, e.rid)) {
                        continue;
                    }
                    seen.push((e.lid, e.rid));
                    let mut f = common.clone();
                    f.push((feat::JA_WORD as u8, e.cost as f32));
                    if e.kata_only && loan_strength > 0.0 {
                        f.push((feat::LOANWORD as u8, loan_strength));
                    }
                    self.push(Arc {
                        start: i,
                        end,
                        lang: Lang::Ja,
                        lid: e.lid,
                        rid: e.rid,
                        features: f,
                        text: text.clone(),
                        oov: false,
                        bad_start,
                        cap: false,
                        pending: false,
                        identifier: false,
                    });
                }
            } else if pt.kana.len() >= self.cfg.ja_unknown_min_kana {
                let mut f = common;
                f.push((feat::JA_UNKNOWN_FLAG as u8, 1.0));
                f.push((feat::JA_UNKNOWN_KANA as u8, pt.kana.len() as f32));
                self.push(Arc {
                    start: i,
                    end,
                    lang: Lang::Ja,
                    lid: self.kb.pos().noun_general,
                    rid: self.kb.pos().noun_general,
                    features: f,
                    text,
                    oov: true,
                    bad_start,
                    cap: false,
                    pending: false,
                    identifier: false,
                });
            }
        }
    }

    fn add_en_arcs(&mut self, i: usize, run: &[u8]) {
        let max_len = run.len().min(self.cfg.en_max_len);
        for len in 1..=max_len {
            let end = i + len;
            let raw_lower = std::str::from_utf8(&run[..len]).unwrap().to_string();
            if raw_lower.ends_with('-') || raw_lower.starts_with('-') || raw_lower.starts_with('\'') {
                continue;
            }
            let first_upper = matches!(self.classes[i], CharClass::Letter { upper: true });
            let n_upper = self.uppercase_count(i, end);
            let n_letters = self.classes[i..end].iter().filter(|c| matches!(c, CharClass::Letter { .. })).count() as i32;
            let mut common: Features = vec![(feat::EN_BIAS as u8, 1.0)];
            if let Some(b) = self.user_bias.get(&raw_lower) {
                common.push((feat::USER_BIAS as u8, *b as f32));
            }
            if first_upper && len >= 2 {
                common.push((feat::CAPITAL_EN as u8, 1.0));
            }
            let inside = n_upper - if first_upper { 1 } else { 0 };
            if inside > 0 && n_upper != n_letters {
                common.push((feat::UPPER_INSIDE_EN as u8, inside as f32));
            }
            let mv = mora_violations(&run[..len]);
            if mv > 0 {
                common.push((feat::MORA_VIOLATION as u8, mv as f32));
            }
            let text: String = self.input[i..end].iter().collect();
            if let Some(word) = self.kb.word(&raw_lower) {
                let mut f = common.clone();
                f.push((feat::EN_WORD as u8, word.cost as f32));
                if len == 1 && !first_upper {
                    f.push((feat::EN_SINGLE_LETTER as u8, 1.0));
                }
                // an English word may behave as a general noun or as a サ変
                // noun ("commitする"); the grammar picks.  The small tie
                // towards サ変 keeps "pushした" a verb.
                for (pos, tie) in [(self.kb.pos().noun_general, 50.0f32), (self.kb.pos().noun_sahen, 0.0)] {
                    let mut ff = f.clone();
                    if tie > 0.0 {
                        ff.push((feat::EN_WORD as u8, tie));
                    }
                    self.push(Arc {
                        start: i,
                        end,
                        lang: Lang::En,
                        lid: pos,
                        rid: pos,
                        features: ff,
                        text: text.clone(),
                        oov: false,
                        bad_start: false,
                        cap: first_upper,
                        pending: false,
                        identifier: false,
                    });
                }
            } else {
                let has_vowel = raw_lower.bytes().any(|b| matches!(b, b'a' | b'e' | b'i' | b'o' | b'u' | b'y'));
                let allowed = first_upper || (len >= self.cfg.en_oov_min_len && (has_vowel || n_upper > 0));
                if !allowed {
                    continue;
                }
                let mut f = common;
                f.push((feat::EN_SPELL as u8, self.kb.spelling_cost(&raw_lower) as f32));
                f.push((feat::EN_OOV as u8, 1.0));
                f.push((feat::EN_OOV_LETTER as u8, len as f32));
                for (pos, tie) in [(self.kb.pos().noun_proper, 50.0f32), (self.kb.pos().noun_sahen, 0.0)] {
                    let mut ff = f.clone();
                    if tie > 0.0 {
                        ff.push((feat::EN_SPELL as u8, tie));
                    }
                    self.push(Arc {
                        start: i,
                        end,
                        lang: Lang::En,
                        lid: pos,
                        rid: pos,
                        features: ff,
                        text: text.clone(),
                        oov: true,
                        bad_start: false,
                        cap: first_upper,
                        pending: false,
                        identifier: false,
                    });
                }
            }
        }
    }

    /// Transition features from a state into `arc`.
    fn transition(&self, prev: StateKey, arc: &Arc) -> Features {
        let mut f: Features = Vec::new();
        match arc.lang {
            Lang::Neutral if arc.pending => {
                f.push((if prev.lang == 1 { feat::PENDING_AFTER_EN } else { feat::PENDING_AFTER_JA } as u8, 1.0));
            }
            Lang::Neutral => {}
            Lang::Ja => {
                f.push((feat::CONNECTION as u8, self.kb.connection(prev.rid, arc.lid) as f32));
                if prev.lang == 1 {
                    f.push((feat::SWITCH_EN_JA as u8, 1.0));
                }
                if prev.after_space {
                    f.push((feat::JA_AFTER_SPACE as u8, 1.0));
                }
                if arc.bad_start && (prev.lang != 0 || prev.after_space) {
                    f.push((feat::JA_BAD_START as u8, 1.0));
                }
                let func = self.kb.is_function(arc.lid);
                if prev.lang == 2 && func {
                    f.push((feat::FUNC_AT_BOS as u8, 1.0));
                }
                if prev.lang == 1 && prev.cap && !prev.after_space && !func {
                    f.push((feat::CAP_RUN_CONTENT as u8, 1.0));
                }
                if prev.lang == 0 && self.kb.is_boundary(prev.rid, arc.lid) {
                    let pen = self.kb.suffix_penalty(prev.rid) + self.kb.prefix_penalty(arc.lid);
                    if pen > 0 {
                        f.push((feat::BOUNDARY as u8, pen as f32));
                    }
                }
            }
            Lang::En => match prev.lang {
                0 => {
                    f.push((feat::CONNECTION as u8, self.kb.connection(prev.rid, arc.lid) as f32));
                    f.push((feat::SWITCH_JA_EN as u8, 1.0));
                }
                1 => {
                    f.push((feat::EN_CONTINUE as u8, 1.0));
                    if !prev.after_space {
                        f.push((feat::EN_GLUE as u8, 1.0));
                    } else if prev.en_run >= 2 {
                        f.push((feat::EN_SPACE_RUN2 as u8, 1.0));
                    } else {
                        f.push((feat::EN_SPACE_RUN1 as u8, 1.0));
                    }
                }
                _ => f.push((feat::CONNECTION as u8, self.kb.connection(prev.rid, arc.lid) as f32)),
            },
        }
        f
    }

    /// Features of ending the buffer in `state` (text after the caret).
    fn end_features(&self, state: StateKey) -> Features {
        let mut f = Vec::new();
        match (self.after_lang, state.lang) {
            (Some(Lang::En), 0) => f.push((feat::AFTER_EN as u8, 1.0)),
            (Some(Lang::Ja), 1) => f.push((feat::AFTER_JA as u8, 1.0)),
            _ => {}
        }
        f
    }

    fn next_state(&self, prev: StateKey, arc: &Arc) -> StateKey {
        match arc.lang {
            Lang::Neutral => StateKey {
                rid: prev.rid,
                lang: prev.lang,
                after_space: !arc.pending && matches!(self.classes[arc.start], CharClass::Space),
                en_run: prev.en_run,
                cap: prev.cap,
            },
            Lang::Ja => StateKey { rid: arc.rid, lang: 0, after_space: false, en_run: 0, cap: false },
            Lang::En => StateKey {
                rid: arc.rid,
                lang: 1,
                after_space: false,
                en_run: (prev.en_run + 1).min(2),
                cap: arc.cap,
            },
        }
    }

    fn start_state(&self) -> StateKey {
        StateKey {
            rid: self.ctx.rid,
            lang: lang_code(self.ctx.lang),
            after_space: self.ctx.after_space,
            en_run: self.ctx.en_run,
            cap: false,
        }
    }

    #[inline]
    fn arc_cost(&self, arc: &Arc) -> f32 {
        self.w.dot(&arc.features)
    }

    // ------------------------------------------------------------------
    // Decoding
    // ------------------------------------------------------------------

    /// Best path, optionally constrained: `allow(arc)` must hold for every
    /// arc on the path.  Returns (segments, total cost, end state, summed
    /// features) or None when no path satisfies the constraint.
    fn viterbi(&self, allow: &dyn Fn(&Arc) -> bool) -> Option<(Vec<Segment>, f32, StateKey, [f32; N_FEATURES])> {
        let n = self.input.len();
        let mut best: Vec<HashMap<StateKey, (f32, Option<(StateKey, usize)>)>> = vec![HashMap::new(); n + 1];
        best[0].insert(self.start_state(), (0.0, None));
        for pos in 0..n {
            let states: Vec<(StateKey, f32)> = best[pos].iter().map(|(k, v)| (*k, v.0)).collect();
            if states.is_empty() {
                continue;
            }
            for &ai in &self.by_start[pos] {
                let arc = &self.arcs[ai];
                if !allow(arc) {
                    continue;
                }
                let ac = self.arc_cost(arc);
                for &(s, c) in &states {
                    let nc = c + ac + self.w.dot(&self.transition(s, arc));
                    let ns = self.next_state(s, arc);
                    let e = best[arc.end].entry(ns).or_insert((f32::INFINITY, None));
                    if nc < e.0 {
                        *e = (nc, Some((s, ai)));
                    }
                }
            }
        }
        let (end_state, mut cur) = best[n]
            .iter()
            .map(|(k, v)| (*k, (v.0 + self.w.dot(&self.end_features(*k)), v.1)))
            .min_by(|a, b| a.1 .0.total_cmp(&b.1 .0))?;
        let total = cur.0;
        let mut path: Vec<(usize, StateKey)> = Vec::new();
        while let Some((ps, ai)) = cur.1 {
            path.push((ai, ps));
            cur = best[self.arcs[ai].start][&ps];
        }
        path.reverse();
        let mut sum = [0.0f32; N_FEATURES];
        for (i, v) in self.end_features(end_state) {
            sum[i as usize] += v;
        }
        let segments: Vec<Segment> = path
            .iter()
            .map(|&(ai, ps)| {
                let a = &self.arcs[ai];
                let tf = self.transition(ps, a);
                for (i, v) in a.features.iter().chain(tf.iter()) {
                    sum[*i as usize] += v;
                }
                Segment {
                    start: a.start,
                    end: a.end,
                    lang: if a.identifier { Lang::En } else { a.lang },
                    text: a.text.clone(),
                    oov: a.oov,
                    pending: a.pending,
                    identifier: a.identifier,
                    lid: a.lid,
                    rid: a.rid,
                    base_cost: self.arc_cost(a) as i32,
                    trans_cost: self.w.dot(&tf) as i32,
                }
            })
            .collect();
        Some((segments, total, end_state, sum))
    }

    pub fn analyze(&self) -> Analysis {
        let n = self.input.len();
        let (segments, best_cost, end_state, features) = self.viterbi(&|_| true).expect("the lattice always has a path");
        let end_context = Context {
            rid: end_state.rid,
            lang: match end_state.lang {
                0 => Some(Lang::Ja),
                1 => Some(Lang::En),
                _ => None,
            },
            after_space: end_state.after_space,
            en_run: end_state.en_run,
        };

        // ---- forward–backward (log-sum-exp) --------------------------------
        let scale = 1.0 / (COST_SCALE * self.cfg.temperature);
        let mut alpha: Vec<HashMap<StateKey, f64>> = vec![HashMap::new(); n + 1];
        alpha[0].insert(self.start_state(), 0.0);
        for pos in 0..n {
            let states: Vec<(StateKey, f64)> = alpha[pos].iter().map(|(k, v)| (*k, *v)).collect();
            for &ai in &self.by_start[pos] {
                let arc = &self.arcs[ai];
                let ac = self.arc_cost(arc) as f64;
                for &(s, la) in &states {
                    let w = -(ac + self.w.dot(&self.transition(s, arc)) as f64) * scale;
                    let ns = self.next_state(s, arc);
                    let e = alpha[arc.end].entry(ns).or_insert(f64::NEG_INFINITY);
                    *e = logaddexp(*e, la + w);
                }
            }
        }
        let mut beta: Vec<HashMap<StateKey, f64>> = vec![HashMap::new(); n + 1];
        let mut log_z = f64::NEG_INFINITY;
        for (s, la) in &alpha[n] {
            let endw = -(self.w.dot(&self.end_features(*s)) as f64) * scale;
            beta[n].insert(*s, endw);
            log_z = logaddexp(log_z, la + endw);
        }
        let mut arc_post = vec![0.0f64; self.arcs.len()];
        for pos in (0..n).rev() {
            let states: Vec<(StateKey, f64)> = alpha[pos].iter().map(|(k, v)| (*k, *v)).collect();
            for &ai in &self.by_start[pos] {
                let arc = &self.arcs[ai];
                let ac = self.arc_cost(arc) as f64;
                for &(s, la) in &states {
                    let w = -(ac + self.w.dot(&self.transition(s, arc)) as f64) * scale;
                    let ns = self.next_state(s, arc);
                    let Some(&lb) = beta[arc.end].get(&ns) else { continue };
                    let e = beta[pos].entry(s).or_insert(f64::NEG_INFINITY);
                    *e = logaddexp(*e, w + lb);
                    if (arc.lang == Lang::En || arc.pending) && log_z.is_finite() {
                        arc_post[ai] += (la + w + lb - log_z).exp();
                    }
                }
            }
        }
        let mut p_en = vec![0.0f64; n];
        let mut p_pending = vec![0.0f64; n];
        for (ai, arc) in self.arcs.iter().enumerate() {
            if arc.lang == Lang::En {
                for k in arc.start..arc.end {
                    p_en[k] += arc_post[ai];
                }
            } else if arc.pending {
                for k in arc.start..arc.end {
                    p_pending[k] += arc_post[ai];
                }
            }
        }
        for k in 0..n {
            let prev = if k > 0 { p_en[k - 1] } else { 0.0 };
            if !self.is_run_char(k) && !self.arcs.iter().any(|a| a.identifier && a.start <= k && k < a.end) {
                p_en[k] = prev;
            } else if p_pending[k] > 0.0 {
                p_en[k] += p_pending[k] * prev;
            }
            p_en[k] = p_en[k].clamp(0.0, 1.0);
        }
        Analysis { segments, p_en, best_cost: best_cost as i32, end_context, features: features.to_vec() }
    }

    /// Best path whose keystroke languages equal `labels` (one per
    /// keystroke; `None` = unconstrained).  Used to recover the gold path
    /// from a rendering the user accepted.
    pub fn analyze_with_labels(&self, labels: &[Option<Lang>]) -> Option<Analysis> {
        let (segments, best_cost, end_state, features) = self.viterbi(&|arc: &Arc| {
            if arc.pending {
                return false;
            }
            (arc.start..arc.end).all(|k| match labels.get(k).copied().flatten() {
                None => true,
                Some(Lang::Neutral) => arc.lang == Lang::Neutral,
                Some(l) => arc.lang == l || (arc.identifier && l == Lang::En),
            })
        })?;
        Some(Analysis {
            segments,
            p_en: Vec::new(),
            best_cost: best_cost as i32,
            end_context: Context {
                rid: end_state.rid,
                lang: match end_state.lang {
                    0 => Some(Lang::Ja),
                    1 => Some(Lang::En),
                    _ => None,
                },
                after_space: end_state.after_space,
                en_run: end_state.en_run,
            },
            features: features.to_vec(),
        })
    }

    /// Best path whose rendering (kana for Japanese, raw text otherwise,
    /// spaces and case ignored) equals `expected`.  Used by the evaluator to
    /// recover gold labels from a test case's expected string.
    pub fn analyze_matching(&self, expected: &str) -> Option<Analysis> {
        let target: Vec<char> = expected.chars().filter(|c| !c.is_whitespace()).map(|c| fold(c)).collect();
        let n = self.input.len();
        // DP over (pos, offset into target) keeping the best state set
        type Key = (StateKey, usize);
        let mut best: Vec<HashMap<Key, (f32, Option<(Key, usize)>)>> = vec![HashMap::new(); n + 1];
        best[0].insert((self.start_state(), 0), (0.0, None));
        for pos in 0..n {
            let entries: Vec<(Key, f32)> = best[pos].iter().map(|(k, v)| (*k, v.0)).collect();
            for &ai in &self.by_start[pos] {
                let arc = &self.arcs[ai];
                if arc.pending {
                    continue;
                }
                let piece: Vec<char> = arc.text.chars().filter(|c| !c.is_whitespace()).map(fold).collect();
                let ac = self.arc_cost(arc);
                for &((s, off), c) in &entries {
                    if off + piece.len() > target.len() || target[off..off + piece.len()] != piece[..] {
                        continue;
                    }
                    let nc = c + ac + self.w.dot(&self.transition(s, arc));
                    let key = (self.next_state(s, arc), off + piece.len());
                    let e = best[arc.end].entry(key).or_insert((f32::INFINITY, None));
                    if nc < e.0 {
                        *e = (nc, Some(((s, off), ai)));
                    }
                }
            }
        }
        let (end_key, mut cur) = best[n]
            .iter()
            .filter(|((_, off), _)| *off == target.len())
            .map(|(k, v)| (*k, *v))
            .min_by(|a, b| a.1 .0.total_cmp(&b.1 .0))?;
        let mut path: Vec<(usize, StateKey)> = Vec::new();
        while let Some((pk, ai)) = cur.1 {
            path.push((ai, pk.0));
            cur = best[self.arcs[ai].start][&pk];
        }
        path.reverse();
        let mut sum = [0.0f32; N_FEATURES];
        for (i, v) in self.end_features(end_key.0) {
            sum[i as usize] += v;
        }
        let segments: Vec<Segment> = path
            .iter()
            .map(|&(ai, ps)| {
                let a = &self.arcs[ai];
                let tf = self.transition(ps, a);
                for (i, v) in a.features.iter().chain(tf.iter()) {
                    sum[*i as usize] += v;
                }
                Segment {
                    start: a.start,
                    end: a.end,
                    lang: if a.identifier { Lang::En } else { a.lang },
                    text: a.text.clone(),
                    oov: a.oov,
                    pending: false,
                    identifier: a.identifier,
                    lid: a.lid,
                    rid: a.rid,
                    base_cost: self.arc_cost(a) as i32,
                    trans_cost: self.w.dot(&tf) as i32,
                }
            })
            .collect();
        Some(Analysis {
            segments,
            p_en: Vec::new(),
            best_cost: cur.0 as i32,
            end_context: Context { rid: end_key.0.rid, lang: None, after_space: false, en_run: 0 },
            features: sum.to_vec(),
        })
    }
}

fn fold(c: char) -> char {
    match c {
        'ァ'..='ヶ' => char::from_u32(c as u32 - 0x60).unwrap(),
        _ => c.to_ascii_lowercase(),
    }
}

#[inline]
fn logaddexp(a: f64, b: f64) -> f64 {
    if a == f64::NEG_INFINITY {
        b
    } else if b == f64::NEG_INFINITY {
        a
    } else if a > b {
        a + (b - a).exp().ln_1p()
    } else {
        b + (a - b).exp().ln_1p()
    }
}
