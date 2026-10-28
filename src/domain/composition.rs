//! Value objects describing a composition: what language each keystroke
//! was judged to be, the segments, conversion candidates and the grammar
//! state carried between inputs.

/// Language of a keystroke span.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Lang {
    Ja,
    En,
    /// spaces, digits, punctuation: inherit the language of their neighbours
    Neutral,
}

/// Grammar state carried from the text before the caret into the next
/// composition.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Context {
    /// right POS id of the previous word (BOS id when nothing precedes)
    pub rid: u16,
    pub lang: Option<Lang>,
    /// the keystroke before the buffer was a space
    pub after_space: bool,
    /// consecutive English words before the buffer (capped at 2)
    pub en_run: u8,
}

/// One judged span of the keystroke buffer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Segment {
    pub start: usize,
    pub end: usize,
    pub lang: Lang,
    /// kana for Japanese spans, raw keystrokes otherwise
    pub text: String,
    /// not found in any lexicon
    pub oov: bool,
    /// undecided tail still being typed
    pub pending: bool,
    /// identifier-like token (e-mail, URL, path) rendered verbatim
    pub identifier: bool,
    pub lid: u16,
    pub rid: u16,
    /// costs actually paid on the best path (for diagnostics)
    pub base_cost: i32,
    pub trans_cost: i32,
}

/// Result of judging a buffer.
#[derive(Debug, Clone)]
pub struct Analysis {
    pub segments: Vec<Segment>,
    /// P(English) per keystroke
    pub p_en: Vec<f64>,
    pub best_cost: i32,
    /// grammar state after the last segment
    pub end_context: Context,
    /// summed feature vector of the best path (for learning / explanation)
    pub features: Vec<f32>,
}

impl Analysis {
    /// Length of the prefix whose every keystroke is decided with at least
    /// `confidence` posterior.
    pub fn stable_prefix(&self, confidence: f64) -> usize {
        self.p_en.iter().take_while(|p| **p >= confidence || **p <= 1.0 - confidence).count()
    }
}

/// A conversion candidate for a Japanese run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Candidate {
    pub surface: String,
    pub cost: i64,
}

/// One piece of the composed line as a front-end shows it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Piece {
    pub lang: Lang,
    pub start: usize,
    pub end: usize,
    /// what to display / insert
    pub text: String,
    /// kana (Japanese runs only)
    pub kana: String,
    /// `text == candidates[selected]` for Japanese runs
    pub candidates: Vec<String>,
    pub selected: usize,
    pub pending: bool,
    /// 0 = decided with high confidence, 1 = tentative, 2 = undecided
    pub confidence: u8,
}

impl Piece {
    pub fn simple(lang: Lang, start: usize, end: usize, text: String, pending: bool) -> Piece {
        Piece { lang, start, end, text, kana: String::new(), candidates: Vec::new(), selected: 0, pending, confidence: 0 }
    }
}
