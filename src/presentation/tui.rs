//! Presentation: interactive terminal UI (Unix).  Raw mode via `stty`, so no
//! crate is needed.

use crate::application::InputSession;
use crate::domain::{Knowledge, Lang};
use crate::infrastructure::CorrectionFile;
use crate::presentation::cli::{session, Opts};
use std::io::{self, Read, Write};


pub fn is_tty() -> bool {
    // `test -t 0` through sh keeps us crate-free; falls back to line mode
    std::process::Command::new("sh")
        .args(["-c", "test -t 0 && test -t 1"])
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

fn stty(args: &[&str]) {
    let _ = std::process::Command::new("stty")
        .args(args)
        .stdin(std::process::Stdio::inherit())
        .status();
}

pub fn run(kb: &dyn Knowledge, o: &Opts) {
    let mut e = session(kb, o);
    let mut out = io::stdout();
    let mut document = String::new();
    stty(&["raw", "-echo"]);
    let _ = write!(out, "\x1b[2J\x1b[H");
    let _ = write!(out, "kswime  Enter:確定  Tab/Shift+Tab:候補  Ctrl+L:直前の語の日英反転+学習  Ctrl+E:説明  Ctrl+U:取消  Ctrl+C:終了\r\n\r\n");
    let _ = out.flush();
    let mut stdin = io::stdin();
    let mut buf = [0u8; 8];
    let mut pending_utf8: Vec<u8> = Vec::new();
    let mut status = String::new();
    loop {
        redraw(&mut out, &e, &document, &status);
        let n = match stdin.read(&mut buf) {
            Ok(0) | Err(_) => break,
            Ok(n) => n,
        };
        for &b in &buf[..n] {
            match b {
                3 => {
                    // Ctrl+C
                    stty(&["sane"]);
                    let _ = write!(out, "\r\n");
                    if !document.is_empty() {
                        println!("{document}");
                    }
                    if let Some(p) = &o.learn {
                        let _ = e.save_corrections(&CorrectionFile::new(p));
                    }
                    if let Some(p) = &o.weights {
                        let _ = std::fs::write(p, e.weights().to_tsv());
                    }
                    return;
                }
                13 | 10 => {
                    let typed = e.buffer_string();
                    let pieces = e.commit();
                    let text = e.render(&pieces);
                    if let Some(p) = &o.record {
                        if !typed.is_empty() {
                            let ctx = document.lines().last().unwrap_or("").chars().rev().take(20).collect::<Vec<_>>().into_iter().rev().collect::<String>();
                            if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(p) {
                                use std::io::Write as _;
                                let _ = writeln!(f, "{ctx}\t{typed}\t{text}");
                            }
                        }
                    }
                    document.push_str(&text);
                    document.push('\n');
                    status = format!("学習更新 {} 回", e.learner().updates);
                }
                127 | 8 => {
                    if e.backspace().is_none() && document.ends_with('\n') {
                        // nothing to do
                    }
                    status.clear();
                }
                9 => {
                    e.next_candidate(-1, 1);
                }
                12 => {
                    // Ctrl+L
                    if let Some((tok, lang)) = e.toggle_last_language() {
                        status = format!("学習: {tok} → {}", if lang == Lang::En { "英語" } else { "日本語" });
                    }
                }
                21 => {
                    e.clear();
                    status.clear();
                }
                5 => {
                    // Ctrl+E: explain
                    let a = e.analyze_final();
                    status = e.explain(&a).join(" ｜ ");
                }
                27 => {
                    // Esc: may start a sequence (arrows, Shift+Tab = ESC [ Z)
                    pending_utf8.clear();
                    pending_utf8.push(27);
                }
                _ => {
                    if pending_utf8.first() == Some(&27) {
                        pending_utf8.push(b);
                        if pending_utf8 == b"\x1b[Z" {
                            e.next_candidate(-1, -1);
                            pending_utf8.clear();
                        } else if pending_utf8.len() >= 3 {
                            pending_utf8.clear();
                        }
                        continue;
                    }
                    pending_utf8.push(b);
                    if let Ok(s) = std::str::from_utf8(&pending_utf8) {
                        for c in s.chars() {
                            if !c.is_control() {
                                e.push(c);
                            }
                        }
                        pending_utf8.clear();
                    } else if pending_utf8.len() >= 4 {
                        pending_utf8.clear();
                    }
                }
            }
        }
    }
    stty(&["sane"]);
}

fn redraw(out: &mut io::Stdout, e: &InputSession, document: &str, status: &str) {
    let pieces = e.compose(false);
    let mut line = String::new();
    for p in &pieces {
        let conf = match p.confidence { 0 => "", 1 => "\x1b[33m", _ => "\x1b[2m" }; // tentative = yellow, undecided = dim
        match p.lang {
            Lang::Ja => line.push_str(&format!("{conf}\x1b[4m{}\x1b[0m", p.text)),
            Lang::En => line.push_str(&format!("{conf}\x1b[36m{}\x1b[0m", p.text)),
            Lang::Neutral if p.pending => line.push_str(&format!("\x1b[2m{}\x1b[0m", p.text)),
            Lang::Neutral => line.push_str(&p.text),
        }
    }
    let cands = pieces
        .iter()
        .rev()
        .find(|p| p.candidates.len() > 1)
        .map(|p| {
            p.candidates
                .iter()
                .enumerate()
                .take(8)
                .map(|(i, c)| if i == p.selected { format!("[{c}]") } else { c.clone() })
                .collect::<Vec<_>>()
                .join(" ")
        })
        .unwrap_or_default();
    let _ = write!(out, "\x1b[3;1H\x1b[J");
    for l in document.lines() {
        let _ = write!(out, "{l}\r\n");
    }
    let _ = write!(out, "> {line}  \x1b[2m{}\x1b[0m\r\n", e.buffer_string());
    let _ = write!(out, "  {cands}\r\n  \x1b[2m{status}\x1b[0m");
    let _ = out.flush();
}
