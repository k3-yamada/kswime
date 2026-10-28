//! Application layer: use cases.  Depends on the domain only.
//!
//! * `judge`    — JudgeBuffer: log-linear Japanese/English/identifier lattice
//! * `convert`  — ConvertKana: kana → kanji N-best with bunsetsu penalties
//! * `learn`    — LearnWeights: perceptron step from accepted/corrected input
//! * `session`  — InputSession: the interactor front-ends drive
//! * `weights`  — feature weights, structural config, strength presets

pub mod convert;
pub mod judge;
pub mod learn;
pub mod session;
pub mod weights;

pub use learn::Learner;
pub use session::InputSession;
pub use weights::{Config, Strength, Weights};
