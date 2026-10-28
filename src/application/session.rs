//! Use case orchestration: an input session that a front-end drives with
//! keystrokes and reads back as composed pieces.  Depends only on the domain
//! ports; no file, OS or UI knowledge.

use crate::application::convert;
use crate::application::judge::Lattice;
use crate::application::learn::Learner;
use crate::application::weights::{feat, Config, Strength, Weights, FEATURE_DOCS, FEATURE_NAMES};
use crate::domain::{Analysis, Context, CorrectionStore, Knowledge, Lang, Piece};
use std::collections::HashMap;

pub struct InputSession<'k> {
    kb: &'k dyn Knowledge,
    weights: Weights,
    config: Config,
    learner: Learner,
    buffer: Vec<char>,
    ctx: Context,
    /// language of the text right after the caret (host-provided)
    after_lang: Option<Lang>,
    /// per-token bias the user taught (negative favours English)
    corrections: HashMap<String, i32>,
    /// candidate choice per Japanese run, reset on buffer change
    choices: HashMap<usize, usize>,
    /// keystroke languages the user forced in this composition
    forced: Vec<Option<Lang>>,
    /// keystrokes moved by boundary nudges (Shift+arrow): learned once, when
    /// the composition settles, not once per key press
    nudged: Vec<bool>,
    /// a boundary adjustment is waiting to be learned
    nudge_pending: bool,
    /// learning from accepted commits (perceptron) on/off
    pub learn_on_commit: bool,
    /// secret mode: corrections still apply to the current composition but
    /// nothing is remembered (no token memory, no weight update)
    pub learning_paused: bool,
}

impl<'k> InputSession<'k> {
    pub fn new(kb: &'k dyn Knowledge) -> Self {
        InputSession {
            kb,
            weights: Weights::default(),
            config: Config::default(),
            learner: Learner::default(),
            buffer: Vec::new(),
            ctx: Context { rid: kb.pos().bos, lang: None, after_space: false, en_run: 0 },
            after_lang: None,
            corrections: HashMap::new(),
            choices: HashMap::new(),
            forced: Vec::new(),
            nudged: Vec::new(),
            nudge_pending: false,
            learn_on_commit: true,
            learning_paused: false,
        }
    }

    // ---- configuration --------------------------------------------------

    pub fn kb(&self) -> &'k dyn Knowledge {
        self.kb
    }

    pub fn weights(&self) -> &Weights {
        &self.weights
    }

    pub fn weights_mut(&mut self) -> &mut Weights {
        &mut self.weights
    }

    pub fn config(&self) -> &Config {
        &self.config
    }

    pub fn config_mut(&mut self) -> &mut Config {
        &mut self.config
    }

    pub fn learner(&self) -> &Learner {
        &self.learner
    }

    pub fn learner_mut(&mut self) -> &mut Learner {
        &mut self.learner
    }

    pub fn set_strength(&mut self, s: Strength) {
        self.weights.w[feat::EN_BIAS] = s.en_bias();
    }

    /// Set a weight or a config value by name.  Returns false for unknown.
    pub fn set(&mut self, key: &str, v: f64) -> bool {
        if self.weights.set(key, v as f32) {
            return true;
        }
        match key {
            "temperature" => self.config.temperature = v,
            "use_boundary" => self.config.use_boundary = v != 0.0,
            "pending_max_len" => self.config.pending_max_len = v as usize,
            "n_best" => self.config.n_best = v as usize,
            "lr_indicator" => self.learner.lr_indicator = v as f32,
            "lr_cost" => self.learner.lr_cost = v as f32,
            _ => return false,
        }
        true
    }

    // ---- buffer ---------------------------------------------------------

    pub fn buffer(&self) -> &[char] {
        &self.buffer
    }

    pub fn buffer_string(&self) -> String {
        self.buffer.iter().collect()
    }

    pub fn is_empty(&self) -> bool {
        self.buffer.is_empty()
    }

    pub fn push(&mut self, c: char) {
        // Typing on ends a boundary adjustment: learn it now, once.
        self.settle_nudges();
        self.buffer.push(c);
        self.forced.push(None);
        self.nudged.push(false);
        self.choices.clear();
    }

    pub fn push_str(&mut self, s: &str) {
        for c in s.chars() {
            self.push(c);
        }
    }

    pub fn backspace(&mut self) -> Option<char> {
        self.choices.clear();
        // Backspace backs out of a boundary adjustment: the boundary returns
        // to the engine's own and nothing is learned.
        self.discard_nudges();
        self.forced.pop();
        self.nudged.pop();
        self.buffer.pop()
    }

    pub fn clear(&mut self) {
        self.buffer.clear();
        self.forced.clear();
        self.nudged.clear();
        self.nudge_pending = false;
        self.choices.clear();
    }

    // ---- context --------------------------------------------------------

    /// Seed grammar context from the text before the caret.
    pub fn set_context_from_text(&mut self, before: &str) {
        let trimmed = before.trim_end();
        let after_space = before.len() != trimmed.len();
        let lang = match trimmed.chars().last() {
            None => None,
            Some(c) if c.is_ascii_alphabetic() => Some(Lang::En),
            Some(c) if is_japanese_char(c) => Some(Lang::Ja),
            Some(_) => None,
        };
        let pos = self.kb.pos();
        let rid = if lang.is_some() { pos.noun_general } else { pos.bos };
        let mut en_run: u8 = 0;
        for w in trimmed.split_whitespace().rev() {
            if w.chars().all(|c| c.is_ascii_alphabetic() || c == '\'') {
                en_run = (en_run + 1).min(2);
            } else {
                break;
            }
        }
        self.ctx = Context { rid, lang, after_space, en_run };
    }

    /// Text right after the caret (the first non-space character decides).
    pub fn set_context_after(&mut self, after: &str) {
        self.after_lang = match after.trim_start().chars().next() {
            Some(c) if c.is_ascii_alphabetic() => Some(Lang::En),
            Some(c) if is_japanese_char(c) => Some(Lang::Ja),
            _ => None,
        };
    }

    pub fn reset_context(&mut self) {
        self.ctx = Context { rid: self.kb.pos().bos, lang: None, after_space: false, en_run: 0 };
        self.after_lang = None;
    }

    pub fn context(&self) -> Context {
        self.ctx
    }

    // ---- judging --------------------------------------------------------

    fn lattice(&self, allow_pending: bool) -> Lattice<'_> {
        Lattice::build(self.kb, &self.weights, &self.config, &self.buffer, self.ctx, self.after_lang, &self.corrections, allow_pending)
    }

    fn judge(&self, allow_pending: bool) -> Analysis {
        let lat = self.lattice(allow_pending);
        if self.forced.iter().any(|f| f.is_some()) {
            if let Some(a) = lat.analyze_with_labels(&self.forced) {
                let mut a = a;
                // posteriors for a forced path: the free analysis' values
                a.p_en = lat.analyze().p_en;
                return a;
            }
        }
        lat.analyze()
    }

    /// Judge the buffer as it is being typed (a short tail may stay undecided).
    pub fn analyze(&self) -> Analysis {
        self.judge(true)
    }

    /// Judge the buffer as finished input.
    pub fn analyze_final(&self) -> Analysis {
        self.judge(false)
    }

    // ---- composing ------------------------------------------------------

    pub fn compose(&self, final_input: bool) -> Vec<Piece> {
        let a = if final_input { self.analyze_final() } else { self.analyze() };
        self.compose_from(&a)
    }

    pub fn compose_from(&self, a: &Analysis) -> Vec<Piece> {
        let pos = self.kb.pos();
        let mut pieces: Vec<Piece> = Vec::new();
        let mut prev_lang = self.ctx.lang;
        let mut prev_rid = self.ctx.rid;
        let mut run_index = 0usize;
        let segs = &a.segments;
        let confidence = |start: usize, end: usize| -> u8 {
            if a.p_en.is_empty() {
                return 0;
            }
            let worst = a.p_en[start..end].iter().map(|p| (p - 0.5).abs()).fold(0.5, f64::min);
            if worst >= 0.4 { 0 } else if worst >= 0.2 { 1 } else { 2 }
        };
        // Punctuation follows the sentence, not the word before it: in a
        // Japanese sentence an English noun is still followed by 、。
        // (cpu、メモリ).  A sentence is Japanese when the text before the
        // caret or any decided span in the buffer is Japanese.
        let sentence_ja = self.ctx.lang == Some(Lang::Ja) || segs.iter().any(|s| s.lang == Lang::Ja && !s.pending);
        let mut i = 0;
        while i < segs.len() {
            let s = &segs[i];
            match s.lang {
                Lang::Ja => {
                    let start = s.start;
                    let mut end = s.end;
                    let mut kana = s.text.clone();
                    let mut j = i + 1;
                    while j < segs.len() && segs[j].lang == Lang::Ja {
                        kana.push_str(&segs[j].text);
                        end = segs[j].end;
                        j += 1;
                    }
                    let kana_chars: Vec<char> = kana.chars().collect();
                    let eos = j >= segs.len();
                    let cands = convert::convert(self.kb, &kana_chars, prev_rid, eos, self.config.n_best, self.config.use_boundary);
                    let candidates: Vec<String> = cands.into_iter().map(|c| c.surface).collect();
                    let selected = self.choices.get(&run_index).copied().unwrap_or(0).min(candidates.len() - 1);
                    pieces.push(Piece {
                        lang: Lang::Ja,
                        start,
                        end,
                        text: candidates[selected].clone(),
                        kana,
                        candidates,
                        selected,
                        pending: false,
                        confidence: confidence(start, end),
                    });
                    prev_lang = Some(Lang::Ja);
                    prev_rid = pos.noun_general;
                    run_index += 1;
                    i = j;
                    continue;
                }
                Lang::En => {
                    let mut p = Piece::simple(Lang::En, s.start, s.end, s.text.clone(), false);
                    p.confidence = if s.identifier { 0 } else { confidence(s.start, s.end) };
                    // an English word's katakana forms are its candidates
                    // (commit → コミット), so the user can pick the loanword
                    // spelling without retyping
                    if !s.identifier {
                        let lower = s.text.to_ascii_lowercase();
                        let mut cands = vec![s.text.clone()];
                        for k in self.kb.katakana_for_english(&lower).iter().take(2) {
                            cands.push(k.to_string());
                        }
                        if cands.len() > 1 {
                            let selected = self.choices.get(&run_index).copied().unwrap_or(0).min(cands.len() - 1);
                            p.text = cands[selected].clone();
                            p.candidates = cands;
                            p.selected = selected;
                        }
                    }
                    if !p.candidates.is_empty() {
                        run_index += 1;
                    }
                    pieces.push(p);
                    prev_lang = Some(Lang::En);
                    prev_rid = s.rid;
                }
                Lang::Neutral => {
                    let text = if s.pending {
                        if prev_lang != Some(Lang::En) { self.kana_of_tail(&s.text) } else { s.text.clone() }
                    } else {
                        // decimal points and digit groups (10.5, 1,000) stay
                        let between_digits = matches!(s.text.as_str(), "." | ",")
                            && i > 0
                            && segs[i - 1].text.chars().last().is_some_and(|c| c.is_ascii_digit())
                            && segs.get(i + 1).is_some_and(|n| n.text.starts_with(|c: char| c.is_ascii_digit()));
                        if !between_digits && (sentence_ja || prev_lang != Some(Lang::En)) {
                            japanese_symbol(&s.text).unwrap_or_else(|| s.text.clone())
                        } else {
                            s.text.clone()
                        }
                    };
                    if s.text.chars().all(|c| c.is_ascii_digit()) {
                        prev_rid = pos.noun_number;
                    }
                    let mut p = Piece::simple(Lang::Neutral, s.start, s.end, text, s.pending);
                    p.confidence = if s.pending { 2 } else { 0 };
                    pieces.push(p);
                }
            }
            i += 1;
        }
        pieces
    }

    fn kana_of_tail(&self, raw: &str) -> String {
        let lower = raw.to_ascii_lowercase();
        let pts = self.kb.romaji().parse_points(lower.as_bytes(), lower.len());
        match pts.last() {
            Some(pt) => {
                let mut t: String = pt.kana.iter().collect();
                t.push_str(&raw[pt.end..]);
                t
            }
            None => raw.to_string(),
        }
    }

    pub fn render(&self, pieces: &[Piece]) -> String {
        pieces.iter().map(|p| p.text.as_str()).collect()
    }

    /// One line per non-neutral segment saying which features decided it.
    pub fn explain(&self, a: &Analysis) -> Vec<String> {
        let mut out = Vec::new();
        for s in &a.segments {
            if s.lang == Lang::Neutral {
                continue;
            }
            let raw: String = self.buffer[s.start..s.end].iter().collect();
            let what = match (s.lang, s.identifier, s.oov) {
                (_, true, _) => "識別子（@ / : _ や数字を含む語）".to_string(),
                (Lang::En, _, false) => "英語辞書にある語".to_string(),
                (Lang::En, _, true) => "辞書になく、英語の綴りとして読める".to_string(),
                (Lang::Ja, _, false) => format!("日本語辞書の読み「{}」", s.text),
                (Lang::Ja, _, true) => format!("辞書にないかな列「{}」", s.text),
                _ => String::new(),
            };
            out.push(format!("{raw} → {what}（語 {} ＋ 接続 {}）", s.base_cost, s.trans_cost));
        }
        out
    }

    /// Features of the best path with non-zero contribution, largest first.
    pub fn feature_report(&self, a: &Analysis) -> Vec<(String, f32, f32)> {
        let mut v: Vec<(String, f32, f32)> = a
            .features
            .iter()
            .enumerate()
            .filter(|(_, f)| **f != 0.0)
            .map(|(k, f)| (FEATURE_NAMES[k].to_string(), *f, self.weights.w[k] * f))
            .collect();
        v.sort_by(|a, b| b.2.abs().total_cmp(&a.2.abs()));
        v
    }

    pub fn feature_doc(name: &str) -> Option<&'static str> {
        FEATURE_NAMES.iter().position(|n| n.eq_ignore_ascii_case(name)).map(|i| FEATURE_DOCS[i])
    }

    /// Cycle the candidate of the `run`-th piece that has candidates
    /// (Japanese runs and English words; negative = from the end).
    pub fn next_candidate(&mut self, run: isize, step: isize) -> bool {
        let pieces = self.compose(true);
        let runs: Vec<&Piece> = pieces.iter().filter(|p| !p.candidates.is_empty()).collect();
        if runs.is_empty() {
            return false;
        }
        let idx = if run < 0 { runs.len() as isize + run } else { run } as usize;
        let Some(p) = runs.get(idx) else { return false };
        let n = p.candidates.len() as isize;
        let cur = self.choices.get(&idx).copied().unwrap_or(0) as isize;
        self.choices.insert(idx, (((cur + step) % n + n) % n) as usize);
        true
    }

    pub fn select_candidate(&mut self, run: isize, index: usize) -> bool {
        let pieces = self.compose(true);
        let runs: Vec<&Piece> = pieces.iter().filter(|p| !p.candidates.is_empty()).collect();
        let idx = if run < 0 { runs.len() as isize + run } else { run } as usize;
        match runs.get(idx) {
            Some(p) if index < p.candidates.len() => {
                self.choices.insert(idx, index);
                true
            }
            _ => false,
        }
    }

    // ---- learning -------------------------------------------------------

    /// The user corrected a token's language: remember the token and move
    /// the weights towards the corrected path.
    pub fn feedback(&mut self, token: &str, lang: Lang) {
        self.choices.clear();
        if self.learning_paused {
            return;
        }
        let e = self.corrections.entry(token.to_ascii_lowercase()).or_insert(0);
        match lang {
            Lang::En => *e = (*e - 2500).max(-7500),
            Lang::Ja => *e = (*e + 2500).min(7500),
            Lang::Neutral => {}
        }
        self.choices.clear();
    }

    /// Force the language of keystrokes [start, end) for this composition
    /// (what F6/F10 do), learn from it, and return the forced token.
    pub fn force_language(&mut self, start: usize, end: usize, lang: Lang) -> String {
        let raw: String = self.buffer[start..end].iter().collect();
        let predicted = self.lattice(false).analyze();
        for k in start..end.min(self.forced.len()) {
            self.forced[k] = Some(lang);
            // This word is learned right now; it is not a pending nudge too.
            self.nudged[k] = false;
        }
        self.nudge_pending = self.nudged.iter().any(|&n| n);
        self.feedback(&raw, lang);
        if self.learning_paused {
            return raw;
        }
        if let Some(gold) = self.lattice(false).analyze_with_labels(&self.forced) {
            self.learner.update(&mut self.weights, &predicted.features, &gold.features);
        }
        raw
    }

    /// Move a language boundary for this composition without learning (what
    /// Shift+Left/Right do, one keystroke per press).  Learning from a
    /// half-finished adjustment teaches fragments ("e", "ex", "exc", …) and
    /// pushes the weights once per key press, so the learning is deferred to
    /// `settle_nudges`: once, on the final range.
    pub fn nudge_language(&mut self, start: usize, end: usize, lang: Lang) {
        debug_assert_eq!(self.forced.len(), self.nudged.len());
        // A panic here would abort the host process (extern "C"), so the bound
        // does not rely on the three vectors staying the same length.
        for k in start..end.min(self.forced.len()).min(self.nudged.len()) {
            self.forced[k] = Some(lang);
            // In secret mode the boundary still moves, but nothing is marked
            // for learning, so it can never leak into a later settle.
            self.nudged[k] = !self.learning_paused;
        }
        self.nudge_pending = self.nudged.iter().any(|&n| n);
        self.choices.clear();
    }

    /// Learn the boundary adjustments made since the last settle, once.
    ///
    /// * Memory: the words the adjustment ended on (a whole piece whose
    ///   language is the one the user asked for), never the fragments passed
    ///   on the way, and nothing for a keystroke that could not change language.
    /// * Weights: one perceptron step from the free prediction to the path
    ///   that carries only the nudged keystrokes' labels, so another forced
    ///   word (F6/F10, already learned) or a private one is not counted again.
    ///
    /// In secret mode nothing is learned and the pending adjustment is dropped.
    fn settle_nudges(&mut self) {
        if !self.nudge_pending {
            return;
        }
        self.nudge_pending = false;
        if self.learning_paused {
            self.discard_nudges();
            return;
        }
        let predicted = self.lattice(false).analyze();
        let labels: Vec<Option<Lang>> =
            self.forced.iter().zip(&self.nudged).map(|(lang, &nudged)| if nudged { *lang } else { None }).collect();
        // No path carries these labels (the display did not change): there is
        // nothing to learn, so remember nothing either.
        if self.lattice(false).analyze_with_labels(&labels).is_none() {
            self.nudged.iter_mut().for_each(|n| *n = false);
            return;
        }
        let mut remembered: Vec<(usize, usize)> = Vec::new();
        for piece in self.compose(true) {
            let moved = (piece.start..piece.end).any(|k| self.nudged.get(k) == Some(&true) && labels[k] == Some(piece.lang));
            if moved && piece.lang != Lang::Neutral && !remembered.contains(&(piece.start, piece.end)) {
                remembered.push((piece.start, piece.end));
                let raw: String = self.buffer[piece.start..piece.end].iter().collect();
                self.feedback(&raw, piece.lang);
            }
        }
        self.nudged.iter_mut().for_each(|n| *n = false);
        if let Some(gold) = self.lattice(false).analyze_with_labels(&labels) {
            self.learner.update(&mut self.weights, &predicted.features, &gold.features);
        }
    }

    /// Back out of the pending boundary adjustment: the boundary returns to
    /// the engine's own and nothing is learned.
    fn discard_nudges(&mut self) {
        for k in 0..self.nudged.len().min(self.forced.len()) {
            if self.nudged[k] {
                self.forced[k] = None;
                self.nudged[k] = false;
            }
        }
        self.nudge_pending = false;
    }

    /// Drop everything learned-but-not-yet-applied (a boundary adjustment in
    /// progress).  The host calls this when it erases the learned data, so
    /// that an operation begun before the erase does not repopulate it.
    pub fn discard_pending_learning(&mut self) {
        self.discard_nudges();
    }

    /// Flip the language of the last decided token (a whole Japanese run or
    /// an English word), learning from it.
    pub fn toggle_last_language(&mut self) -> Option<(String, Lang)> {
        let pieces = self.compose(true);
        let last = pieces.iter().rev().find(|p| p.lang != Lang::Neutral)?;
        let want = if last.lang == Lang::Ja { Lang::En } else { Lang::Ja };
        let (start, end) = (last.start, last.end);
        let raw = self.force_language(start, end, want);
        Some((raw, want))
    }

    pub fn corrections(&self) -> Vec<(String, i32)> {
        let mut v: Vec<(String, i32)> = self.corrections.iter().map(|(k, v)| (k.clone(), *v)).collect();
        v.sort();
        v
    }

    pub fn clear_corrections(&mut self) {
        self.corrections.clear();
    }

    pub fn load_corrections(&mut self, store: &dyn CorrectionStore) {
        for (k, v) in store.load() {
            self.corrections.insert(k, v);
        }
    }

    pub fn save_corrections(&self, store: &dyn CorrectionStore) -> Result<(), String> {
        store.save(&self.corrections())
    }

    /// Learn from a known gold rendering of the current buffer (used by the
    /// evaluator and the replay tool).  Returns true when weights changed.
    pub fn learn_from_expected(&mut self, expected: &str) -> bool {
        if self.learning_paused {
            return false;
        }
        let lat = self.lattice(false);
        let Some(gold) = lat.analyze_matching(expected) else { return false };
        let predicted = lat.analyze();
        if gold.segments.iter().map(|s| (s.start, s.end, s.lang)).eq(predicted.segments.iter().map(|s| (s.start, s.end, s.lang))) {
            return false;
        }
        self.learner.update(&mut self.weights, &predicted.features, &gold.features)
    }

    // ---- commit ---------------------------------------------------------

    /// Return the composed pieces, clear the buffer and carry the grammar
    /// state over.  A language the user forced with `force_language` (F6/F10)
    /// has already been learned; a boundary adjusted with `nudge_language`
    /// (Shift+arrow) is learned here, once.  An accepted composition teaches
    /// nothing else.
    pub fn commit(&mut self) -> Vec<Piece> {
        self.settle_nudges();
        let a = self.analyze_final();
        let pieces = self.compose_from(&a);
        if !self.buffer.is_empty() {
            self.ctx = a.end_context;
        }
        self.buffer.clear();
        self.forced.clear();
        self.nudged.clear();
        self.choices.clear();
        self.after_lang = None;
        pieces
    }
}

fn is_japanese_char(c: char) -> bool {
    matches!(c as u32, 0x3040..=0x30FF | 0x4E00..=0x9FFF | 0x3000..=0x303F | 0xFF01..=0xFF60)
}

fn japanese_symbol(s: &str) -> Option<String> {
    Some(
        match s {
            "." => "。",
            "," => "、",
            "!" => "！",
            "?" => "？",
            "~" => "～",
            "[" => "「",
            "]" => "」",
            "(" => "（",
            ")" => "）",
            _ => return None,
        }
        .to_string(),
    )
}
