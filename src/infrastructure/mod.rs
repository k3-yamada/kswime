//! Infrastructure layer: concrete implementations of the domain ports.
//!
//! * `model`           — embedded binary model (lexicons, grammar, romaji)
//! * `model_builder`   — writes that format from public raw data
//! * `correction_file` — learned corrections on disk

pub mod correction_file;
pub mod model;
pub mod model_builder;

pub use correction_file::{CorrectionFile, MemoryStore};
pub use model::{load_embedded_model, Model, MODEL_BYTES};
