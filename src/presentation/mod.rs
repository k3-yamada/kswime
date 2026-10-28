//! Presentation layer: how the outside world drives the use cases.
//!
//! * `cli` — command line (convert / judge / stream / eval / line mode)
//! * `tui` — interactive terminal input
//! * `ffi` — C ABI for native front-ends (the macOS IMK shell)

pub mod cli;
pub mod ffi;
pub mod tui;
