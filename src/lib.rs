//! kswime — a dependency-free Japanese/English auto-switching input engine.
//!
//! Layers (Clean Architecture; dependencies point inwards only):
//!
//! ```text
//!   presentation   cli / tui / ffi              (argv, stdin, C ABI)
//!   infrastructure embedded model, files        (implements domain ports)
//!   application    judge, convert, session      (use cases)
//!   domain         romaji, composition, ports   (rules, no I/O)
//! ```

pub mod application;
pub mod domain;
pub mod infrastructure;
pub mod presentation;

pub use application::{Config, InputSession, Learner, Strength, Weights};
pub use domain::{Analysis, Candidate, Context, Knowledge, Lang, Piece, PieceSeg, SegEnd, Segment};
pub use infrastructure::{load_embedded_model, Model, MODEL_BYTES};
