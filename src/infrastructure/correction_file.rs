//! Infrastructure: learned corrections persisted as a tab-separated file.

use crate::domain::CorrectionStore;
use std::path::PathBuf;

pub struct CorrectionFile {
    pub path: PathBuf,
}

impl CorrectionFile {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        CorrectionFile { path: path.into() }
    }
}

impl CorrectionStore for CorrectionFile {
    fn load(&self) -> Vec<(String, i32)> {
        let Ok(text) = std::fs::read_to_string(&self.path) else { return Vec::new() };
        parse(&text)
    }

    fn save(&self, entries: &[(String, i32)]) -> Result<(), String> {
        if let Some(dir) = self.path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        std::fs::write(&self.path, format(entries)).map_err(|e| e.to_string())
    }
}

pub fn parse(text: &str) -> Vec<(String, i32)> {
    text.lines()
        .filter_map(|l| {
            let (k, v) = l.split_once('\t')?;
            Some((k.to_string(), v.trim().parse().ok()?))
        })
        .collect()
}

pub fn format(entries: &[(String, i32)]) -> String {
    entries.iter().map(|(k, b)| format!("{k}\t{b}\n")).collect()
}

/// In-memory store for tests and the FFI (the host persists the text).
pub struct MemoryStore(pub std::cell::RefCell<Vec<(String, i32)>>);

impl CorrectionStore for MemoryStore {
    fn load(&self) -> Vec<(String, i32)> {
        self.0.borrow().clone()
    }
    fn save(&self, entries: &[(String, i32)]) -> Result<(), String> {
        *self.0.borrow_mut() = entries.to_vec();
        Ok(())
    }
}
