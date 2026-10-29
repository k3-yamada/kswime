//! Domain layer: the rules of romaji input, independent of any model
//! format, file, OS or UI.  Nothing in here performs I/O.

pub mod composition;
pub mod ports;
pub mod romaji;
pub mod transliterate;

pub use composition::{Analysis, Candidate, Context, Lang, Piece, PieceSeg, SegEnd, Segment};
pub use ports::{CorrectionStore, EnWord, EnglishLexicon, JaEntry, JapaneseLexicon, Knowledge, LoanwordBridge, PosGrammar, PosIds};
pub use romaji::{KanaPoint, RomajiTable};

/// Mozc cost scale: `cost = COST_SCALE · (−ln p)`.
pub const COST_SCALE: f64 = 500.0;
