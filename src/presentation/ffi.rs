//! Presentation: C ABI for native front-ends (the macOS Input Method Kit
//! shell in `macos/`).  See `include/kswime.h`.
//!
//! Conventions: all strings are UTF-8; strings returned by the engine stay
//! valid until the next call on the same engine; `lang` is 0 = Japanese,
//! 1 = English, 2 = neutral; `run` indexes Japanese runs (negative counts
//! from the end).  One model is loaded per process and shared.

use crate::application::{InputSession, Strength};
use crate::domain::{Lang, Piece};
use crate::infrastructure::{correction_file, load_embedded_model, Model};
use std::ffi::{c_char, c_int, CStr, CString};
use std::sync::OnceLock;

static MODEL: OnceLock<Model<'static>> = OnceLock::new();

fn model() -> &'static Model<'static> {
    MODEL.get_or_init(load_embedded_model)
}

pub struct KswEngine {
    session: InputSession<'static>,
    pieces: Vec<Piece>,
    p_en: Vec<f64>,
    strings: Vec<CString>,
    rendered: CString,
}

impl KswEngine {
    fn keep(&mut self, s: String) -> *const c_char {
        let c = CString::new(s.replace('\0', "")).unwrap();
        let p = c.as_ptr();
        self.strings.push(c);
        p
    }
}

fn lang_code(l: Lang) -> c_int {
    match l {
        Lang::Ja => 0,
        Lang::En => 1,
        Lang::Neutral => 2,
    }
}

unsafe fn cstr<'a>(p: *const c_char) -> &'a str {
    if p.is_null() {
        return "";
    }
    unsafe { CStr::from_ptr(p) }.to_str().unwrap_or("")
}

/// Load the embedded model (first call) and create an engine.
#[unsafe(no_mangle)]
pub extern "C" fn ksw_new() -> *mut KswEngine {
    let e = KswEngine {
        session: InputSession::new(model()),
        pieces: Vec::new(),
        p_en: Vec::new(),
        strings: Vec::new(),
        rendered: CString::default(),
    };
    Box::into_raw(Box::new(e))
}

/// # Safety
/// `e` must come from `ksw_new` and not be used afterwards.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ksw_free(e: *mut KswEngine) {
    if !e.is_null() {
        drop(unsafe { Box::from_raw(e) });
    }
}

/// Pre-load the model (call once at start-up to hide the ~0.3 s).
#[unsafe(no_mangle)]
pub extern "C" fn ksw_preload() {
    let _ = model();
}

/// 0 = aggressive, 1 = balanced, 2 = conservative.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ksw_set_strength(e: *mut KswEngine, s: c_int) {
    let e = unsafe { &mut *e };
    e.session.set_strength(match s {
        0 => Strength::Aggressive,
        2 => Strength::Conservative,
        _ => Strength::Balanced,
    });
}

/// Set any feature weight or config value by name.  Returns 1 if known.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ksw_set_param(e: *mut KswEngine, key: *const c_char, value: f64) -> c_int {
    let e = unsafe { &mut *e };
    e.session.set(unsafe { cstr(key) }, value) as c_int
}

/// Text before the caret (from the host's surrounding-text API).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ksw_set_context(e: *mut KswEngine, before: *const c_char) {
    let e = unsafe { &mut *e };
    e.session.set_context_from_text(unsafe { cstr(before) });
}

/// Text right after the caret.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ksw_set_context_after(e: *mut KswEngine, after: *const c_char) {
    let e = unsafe { &mut *e };
    e.session.set_context_after(unsafe { cstr(after) });
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ksw_reset_context(e: *mut KswEngine) {
    unsafe { &mut *e }.session.reset_context();
}

/// Append keystrokes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ksw_push(e: *mut KswEngine, text: *const c_char) {
    let e = unsafe { &mut *e };
    e.session.push_str(unsafe { cstr(text) });
}

/// Remove the last keystroke.  Returns 0 when the buffer was empty.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ksw_backspace(e: *mut KswEngine) -> c_int {
    unsafe { &mut *e }.session.backspace().is_some() as c_int
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ksw_clear(e: *mut KswEngine) {
    unsafe { &mut *e }.session.clear();
}

/// Number of keystrokes in the buffer.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ksw_buffer_len(e: *mut KswEngine) -> c_int {
    unsafe { &*e }.session.buffer().len() as c_int
}

/// The raw buffer.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ksw_buffer(e: *mut KswEngine) -> *const c_char {
    let e = unsafe { &mut *e };
    let s = e.session.buffer_string();
    e.keep(s)
}

/// Judge and convert.  `final_input` != 0 decides every keystroke; 0 lets a
/// short tail stay undecided while typing.  Returns the number of pieces.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ksw_compose(e: *mut KswEngine, final_input: c_int) -> c_int {
    let e = unsafe { &mut *e };
    let a = if final_input != 0 { e.session.analyze_final() } else { e.session.analyze() };
    e.pieces = e.session.compose_from(&a);
    e.p_en = a.p_en;
    e.strings.clear();
    e.rendered = CString::new(e.session.render(&e.pieces)).unwrap();
    e.pieces.len() as c_int
}

/// Whole composed line of the last `ksw_compose`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ksw_rendered(e: *mut KswEngine) -> *const c_char {
    unsafe { &*e }.rendered.as_ptr()
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ksw_piece_lang(e: *mut KswEngine, i: c_int) -> c_int {
    unsafe { &*e }.pieces.get(i as usize).map(|p| lang_code(p.lang)).unwrap_or(-1)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ksw_piece_start(e: *mut KswEngine, i: c_int) -> c_int {
    unsafe { &*e }.pieces.get(i as usize).map(|p| p.start as c_int).unwrap_or(-1)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ksw_piece_end(e: *mut KswEngine, i: c_int) -> c_int {
    unsafe { &*e }.pieces.get(i as usize).map(|p| p.end as c_int).unwrap_or(-1)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ksw_piece_pending(e: *mut KswEngine, i: c_int) -> c_int {
    unsafe { &*e }.pieces.get(i as usize).map(|p| p.pending as c_int).unwrap_or(0)
}

/// 0 = confident, 1 = tentative, 2 = undecided.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ksw_piece_confidence(e: *mut KswEngine, i: c_int) -> c_int {
    unsafe { &*e }.pieces.get(i as usize).map(|p| p.confidence as c_int).unwrap_or(0)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ksw_piece_text(e: *mut KswEngine, i: c_int) -> *const c_char {
    let e = unsafe { &mut *e };
    let s = e.pieces.get(i as usize).map(|p| p.text.clone()).unwrap_or_default();
    e.keep(s)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ksw_piece_kana(e: *mut KswEngine, i: c_int) -> *const c_char {
    let e = unsafe { &mut *e };
    let s = e.pieces.get(i as usize).map(|p| p.kana.clone()).unwrap_or_default();
    e.keep(s)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ksw_piece_candidate_count(e: *mut KswEngine, i: c_int) -> c_int {
    unsafe { &*e }.pieces.get(i as usize).map(|p| p.candidates.len() as c_int).unwrap_or(0)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ksw_piece_selected(e: *mut KswEngine, i: c_int) -> c_int {
    unsafe { &*e }.pieces.get(i as usize).map(|p| p.selected as c_int).unwrap_or(0)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ksw_piece_candidate(e: *mut KswEngine, i: c_int, c: c_int) -> *const c_char {
    let e = unsafe { &mut *e };
    let s = e.pieces.get(i as usize).and_then(|p| p.candidates.get(c as usize)).cloned().unwrap_or_default();
    e.keep(s)
}

/// P(English) of keystroke `i` from the last `ksw_compose`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ksw_p_en(e: *mut KswEngine, i: c_int) -> f64 {
    unsafe { &*e }.p_en.get(i as usize).copied().unwrap_or(0.0)
}

/// Keystrokes from the start that are decided with ≥ `confidence`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ksw_stable_prefix(e: *mut KswEngine, confidence: f64) -> c_int {
    unsafe { &*e }.p_en.iter().take_while(|p| **p >= confidence || **p <= 1.0 - confidence).count() as c_int
}

/// Cycle the candidate of the `run`-th Japanese run by `step`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ksw_next_candidate(e: *mut KswEngine, run: c_int, step: c_int) -> c_int {
    unsafe { &mut *e }.session.next_candidate(run as isize, step as isize) as c_int
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ksw_select_candidate(e: *mut KswEngine, run: c_int, index: c_int) -> c_int {
    unsafe { &mut *e }.session.select_candidate(run as isize, index as usize) as c_int
}

/// Flip the language of the last token and learn it.  Returns the new
/// language code or -1 when the buffer is empty.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ksw_toggle_last_language(e: *mut KswEngine) -> c_int {
    match unsafe { &mut *e }.session.toggle_last_language() {
        Some((_, l)) => lang_code(l),
        None => -1,
    }
}

/// Force keystrokes [start, end) to Japanese (0) or English (1) for this
/// composition and learn from it.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ksw_force_language(e: *mut KswEngine, start: c_int, end: c_int, lang: c_int) {
    let e = unsafe { &mut *e };
    let n = e.session.buffer().len() as c_int;
    if start < 0 || end > n || start >= end {
        return;
    }
    let l = if lang == 1 { Lang::En } else { Lang::Ja };
    e.session.force_language(start as usize, end as usize, l);
}

/// One explanation line per decided token of the last `ksw_compose`,
/// joined with newlines.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ksw_explain(e: *mut KswEngine) -> *const c_char {
    let e = unsafe { &mut *e };
    let a = e.session.analyze_final();
    let s = e.session.explain(&a).join("\n");
    e.keep(s)
}

/// Learned feature weights as `name<TAB>value` lines (the host persists them).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ksw_export_weights(e: *mut KswEngine) -> *const c_char {
    let e = unsafe { &mut *e };
    let s = e.session.weights().to_tsv();
    e.keep(s)
}

/// Secret mode: 1 pauses all learning (token memory and weights stay as
/// they are; corrections still apply to the current composition).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ksw_set_learning_paused(e: *mut KswEngine, paused: c_int) {
    let e = unsafe { &mut *e };
    e.session.learning_paused = paused != 0;
}

/// Forget learned weights and token memory (back to the shipped defaults).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ksw_reset_learning(e: *mut KswEngine) {
    let e = unsafe { &mut *e };
    *e.session.weights_mut() = Default::default();
    e.session.load_corrections(&correction_file::MemoryStore(std::cell::RefCell::new(Vec::new())));
    e.session.clear_corrections();
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ksw_import_weights(e: *mut KswEngine, text: *const c_char) {
    let e = unsafe { &mut *e };
    e.session.weights_mut().load_tsv(unsafe { cstr(text) });
}

/// Teach the engine that `token` is Japanese (0) or English (1).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ksw_feedback(e: *mut KswEngine, token: *const c_char, lang: c_int) {
    let e = unsafe { &mut *e };
    let l = if lang == 1 { Lang::En } else { Lang::Ja };
    e.session.feedback(unsafe { cstr(token) }, l);
}

/// Commit: returns the composed text, clears the buffer, keeps context.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ksw_commit(e: *mut KswEngine) -> *const c_char {
    let e = unsafe { &mut *e };
    let pieces = e.session.commit();
    let text = e.session.render(&pieces);
    e.pieces.clear();
    e.p_en.clear();
    e.strings.clear();
    e.rendered = CString::new(text.replace('\0', "")).unwrap();
    e.rendered.as_ptr()
}

/// Learned corrections as `token<TAB>bias` lines (the host persists them).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ksw_export_corrections(e: *mut KswEngine) -> *const c_char {
    let e = unsafe { &mut *e };
    let s = correction_file::format(&e.session.corrections());
    e.keep(s)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ksw_import_corrections(e: *mut KswEngine, text: *const c_char) {
    let e = unsafe { &mut *e };
    let store = correction_file::MemoryStore(std::cell::RefCell::new(correction_file::parse(unsafe { cstr(text) })));
    e.session.load_corrections(&store);
}

/// Library version string.
#[unsafe(no_mangle)]
pub extern "C" fn ksw_version() -> *const c_char {
    static V: &[u8] = concat!(env!("CARGO_PKG_VERSION"), "\0").as_bytes();
    V.as_ptr() as *const c_char
}
