//! Feature weights of the log-linear judge.
//!
//! Every arc and transition in the lattice carries a sparse feature vector;
//! its cost is the dot product with these weights (Mozc units, 500 · nats).
//! Two kinds of feature exist:
//!
//! * cost features — the value is already a cost from the lexicon or the
//!   grammar (word cost, spelling cost, POS bigram).  Their weights are
//!   dimensionless scale factors, initially 1.0.
//! * indicator features — the value is a count (uppercase keystrokes,
//!   switches, …).  Their weights are costs per count.
//!
//! Initial values follow the rollback rule: lexicon and grammar at 1.0,
//! the a-priori structural costs the model cannot work without (switch,
//! unknown words, OOV), and 0 for every heuristic.  Heuristics are learned
//! from the user's own commits and corrections (`learn.rs`).

use std::fmt::Write as _;

macro_rules! features {
    ($( $name:ident = $idx:expr, $kind:ident, $init:expr, $doc:expr; )*) => {
        /// Feature indices.
        pub mod feat {
            $( #[doc = $doc] pub const $name: usize = $idx; )*
        }
        pub const N_FEATURES: usize = 0 $( + { let _ = $idx; 1 } )*;
        pub const FEATURE_NAMES: [&str; N_FEATURES] = [ $( stringify!($name), )* ];
        pub const FEATURE_KINDS: [Kind; N_FEATURES] = [ $( Kind::$kind, )* ];
        pub const FEATURE_INIT: [f32; N_FEATURES] = [ $( $init as f32, )* ];
        pub const FEATURE_DOCS: [&str; N_FEATURES] = [ $( $doc, )* ];
    };
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Cost,
    Indicator,
}

features! {
    // ---- cost features (scale factors, initially 1) ----------------------
    JA_WORD          = 0,  Cost, 1.0, "lexicon cost of a Japanese word";
    JA_UNKNOWN       = 1,  Cost, 1.0, "cost of an unknown kana run (base + per kana)";
    EN_WORD          = 2,  Cost, 1.0, "lexicon cost of an English word";
    EN_SPELL         = 3,  Cost, 1.0, "letter-trigram cost of an out-of-vocabulary English token";
    CONNECTION       = 4,  Cost, 1.0, "POS bigram cost between adjacent words";
    USER_BIAS        = 5,  Cost, 1.0, "per-token bias the user taught (negative favours English)";
    BOUNDARY         = 6,  Cost, 0.0, "bunsetsu boundary penalty inside Japanese";
    // ---- structural indicators the model needs a-priori ------------------
    EN_OOV           = 7,  Indicator, 5000.0, "an English token not in the lexicon";
    EN_OOV_LETTER    = 8,  Indicator, 400.0,  "per letter of an OOV English token";
    JA_UNKNOWN_FLAG  = 9,  Indicator, 7000.0, "a kana run not in the lexicon";
    JA_UNKNOWN_KANA  = 10, Indicator, 2000.0, "per kana of an unknown run";
    SWITCH_JA_EN     = 11, Indicator, 1800.0, "Japanese followed by English";
    SWITCH_EN_JA     = 12, Indicator, 1800.0, "English followed by Japanese";
    EN_CONTINUE      = 13, Indicator, 2000.0, "English followed by English";
    EN_BIAS          = 14, Indicator, 0.0,    "global prior per English token (strength setting)";
    EN_SINGLE_LETTER = 15, Indicator, 3000.0, "lower-case one-letter English word";
    PENDING_AFTER_JA = 16, Indicator, 3200.0, "undecided tail after Japanese while typing";
    PENDING_AFTER_EN = 17, Indicator, 1000.0, "undecided tail after English while typing";
    // ---- heuristics: start at 0, learned from the user -------------------
    EN_GLUE          = 18, Indicator, 0.0, "English word glued to the previous English word (no space)";
    EN_SPACE_RUN1    = 19, Indicator, 0.0, "English after a space when one English word precedes";
    EN_SPACE_RUN2    = 20, Indicator, 0.0, "English after a space inside an English sentence";
    JA_AFTER_SPACE   = 21, Indicator, 0.0, "Japanese token after a space";
    UPPER_IN_JA      = 22, Indicator, 0.0, "per upper-case keystroke inside a Japanese run";
    CAPITAL_EN       = 23, Indicator, 0.0, "English word starting with a capital (negative = reward)";
    UPPER_INSIDE_EN  = 24, Indicator, 0.0, "per capital after the first inside a mixed-case English word";
    RARE_LETTER      = 25, Indicator, 0.0, "per l/v/x/q keystroke inside a Japanese run";
    RARE_DIGRAPH     = 26, Indicator, 0.0, "th/dh inside a Japanese run";
    RARE_KANA        = 27, Indicator, 0.0, "per ぢ/づ/ゔ in a Japanese run";
    JA_BAD_START     = 28, Indicator, 0.0, "Japanese run starting with ん/っ/small kana where no Japanese precedes";
    LOANWORD         = 29, Indicator, 0.0, "katakana-only Japanese entry whose keystrokes also spell an English word";
    FUNC_AT_BOS      = 30, Indicator, 0.0, "Japanese function word with nothing before it";
    CAP_RUN_CONTENT  = 31, Indicator, 0.0, "Japanese content word glued to a Capitalised English token";
    AFTER_EN         = 32, Indicator, 0.0, "last token is Japanese while the text after the caret starts with English";
    AFTER_JA         = 33, Indicator, 0.0, "last token is English while the text after the caret starts with Japanese";
    MORA_VIOLATION   = 34, Indicator, 0.0, "per consonant cluster an English token has that Japanese phonotactics forbid (negative favours English)";
}

#[derive(Debug, Clone, PartialEq)]
pub struct Weights {
    pub w: [f32; N_FEATURES],
}

impl Default for Weights {
    fn default() -> Self {
        Weights { w: FEATURE_INIT }
    }
}

impl Weights {
    pub fn get(&self, name: &str) -> Option<f32> {
        FEATURE_NAMES.iter().position(|n| n.eq_ignore_ascii_case(name)).map(|i| self.w[i])
    }

    /// Set a weight by name (case-insensitive).  Returns false for an
    /// unknown name.
    pub fn set(&mut self, name: &str, v: f32) -> bool {
        match FEATURE_NAMES.iter().position(|n| n.eq_ignore_ascii_case(name)) {
            Some(i) => {
                self.w[i] = v;
                true
            }
            None => false,
        }
    }

    #[inline]
    pub fn dot(&self, sparse: &[(u8, f32)]) -> f32 {
        sparse.iter().map(|(i, v)| self.w[*i as usize] * v).sum()
    }

    /// `name<TAB>value` lines, one per feature.
    pub fn to_tsv(&self) -> String {
        let mut s = String::new();
        for (i, n) in FEATURE_NAMES.iter().enumerate() {
            let _ = writeln!(s, "{n}\t{}", self.w[i]);
        }
        s
    }

    pub fn load_tsv(&mut self, text: &str) {
        for line in text.lines() {
            if let Some((k, v)) = line.split_once('\t') {
                if let Ok(x) = v.trim().parse::<f32>() {
                    self.set(k.trim(), x);
                }
            }
        }
    }
}

/// Structural settings that are not learned.
#[derive(Debug, Clone)]
pub struct Config {
    pub en_max_len: usize,
    pub ja_max_kana: usize,
    pub en_oov_min_len: usize,
    pub ja_unknown_min_kana: usize,
    pub pending_max_len: usize,
    /// Temperature for posteriors (1.0 = raw model probabilities).
    pub temperature: f64,
    /// Apply bunsetsu boundary penalties in the kana→kanji converter.
    pub use_boundary: bool,
    /// Number of conversion candidates to produce.
    pub n_best: usize,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            en_max_len: 24,
            ja_max_kana: 16,
            en_oov_min_len: 2,
            ja_unknown_min_kana: 2,
            pending_max_len: 4,
            temperature: 1.0,
            use_boundary: true,
            n_best: 8,
        }
    }
}

/// Meltype-style sensitivity presets: one English prior each (EN_BIAS).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Strength {
    Aggressive,
    Balanced,
    Conservative,
}

impl Strength {
    pub fn en_bias(self) -> f32 {
        match self {
            Strength::Aggressive => -1500.0,
            Strength::Balanced => 0.0,
            Strength::Conservative => 2000.0,
        }
    }
}
