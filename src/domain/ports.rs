//! Ports: what the use cases need from the outside world, as traits.  The
//! infrastructure layer implements them (embedded model, files); tests can
//! implement them with hand-written tables.

use crate::domain::romaji::RomajiTable;

/// One dictionary entry: a surface with its POS and cost, found under a
/// reading (the reading itself is the lookup key and is not repeated here).
/// The surface borrows from the lexicon that owns it (the embedded model's
/// bytes), so an entry is a 24-byte `Copy` value and a lexicon of a million
/// entries costs no per-entry allocation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct JaEntry<'a> {
    pub surface: &'a str,
    pub lid: u16,
    pub rid: u16,
    /// Mozc word cost (0..=65535)
    pub cost: u16,
    /// every surface for this reading/POS is katakana (foreign-word
    /// transcription)
    pub kata_only: bool,
}

/// Loanword bridge: hiragana reading of an English word's katakana form
/// ↔ the English spelling (built from a pronunciation dictionary).
pub trait LoanwordBridge {
    /// English spellings whose katakana reading (loosely normalised, see
    /// `transliterate::loose_key`) equals `kana`; cheapest first.
    fn english_for_kana(&self, kana_loose: &str) -> &[&str];
    /// Katakana forms of an English word (most conventional first).
    fn katakana_for_english(&self, word: &str) -> &[&str];
}

/// Japanese reading lexicon.
pub trait JapaneseLexicon {
    /// Entries whose reading equals `reading`, cheapest first.
    fn lookup(&self, reading: &[char]) -> &[JaEntry<'_>];
    /// Does any reading start with `prefix`?
    fn has_prefix(&self, prefix: &[char]) -> bool;
    /// Entries whose reading is a prefix of `kana`, as (reading length, entry).
    fn prefixes(&self, kana: &[char], max_len: usize) -> Vec<(usize, &JaEntry<'_>)>;
    fn max_reading_len(&self) -> usize;
}

/// POS ids the use cases refer to by role.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PosIds {
    pub bos: u16,
    pub noun_general: u16,
    pub noun_proper: u16,
    pub noun_sahen: u16,
    pub noun_number: u16,
}

/// POS bigram grammar and bunsetsu boundary model.
pub trait PosGrammar {
    fn pos(&self) -> PosIds;
    /// cost of `right_lid` following `left_rid`
    fn connection(&self, left_rid: u16, right_lid: u16) -> i32;
    /// particle / auxiliary / suffix / non-independent class?
    fn is_function(&self, lid: u16) -> bool;
    /// Is there a bunsetsu boundary between these two words?
    fn is_boundary(&self, left_rid: u16, right_lid: u16) -> bool;
    /// penalty for a word with `lid` starting a bunsetsu
    fn prefix_penalty(&self, lid: u16) -> i32;
    /// penalty for a word with `rid` ending a bunsetsu
    fn suffix_penalty(&self, rid: u16) -> i32;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EnWord {
    pub cost: i32,
    /// came from the supplementary (technical) list rather than frequency data
    pub supplementary: bool,
}

/// English word lexicon and spelling model.
pub trait EnglishLexicon {
    fn word(&self, w: &str) -> Option<EnWord>;
    /// letter n-gram cost of an arbitrary token
    fn spelling_cost(&self, w: &str) -> i32;
}

/// Everything the judging and conversion use cases need.
pub trait Knowledge: JapaneseLexicon + PosGrammar + EnglishLexicon + LoanwordBridge {
    fn romaji(&self) -> &RomajiTable;
}

/// Persistence of what the user taught the engine (token → bias).
pub trait CorrectionStore {
    fn load(&self) -> Vec<(String, i32)>;
    fn save(&self, entries: &[(String, i32)]) -> Result<(), String>;
}
