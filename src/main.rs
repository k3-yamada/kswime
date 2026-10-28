//! kswime — dependency-free Japanese/English auto-switching romaji input.
//! Thin entry point; everything lives in `presentation`.

use kswime::presentation::{cli, tui};
use std::time::Instant;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let cmd = args.first().map(|s| s.as_str()).unwrap_or("");
    if matches!(cmd, "-h" | "--help" | "help") {
        print!("{}", cli::HELP);
        return;
    }
    let t0 = Instant::now();
    let model = kswime::load_embedded_model();
    let load_ms = t0.elapsed().as_millis();
    match cmd {
        "convert" => cli::convert(&model, &cli::parse_opts(&args[1..])),
        "judge" => cli::judge(&model, &cli::parse_opts(&args[1..]), load_ms),
        "stream" => cli::stream(&model, &cli::parse_opts(&args[1..])),
        "eval" => cli::eval(&model, &cli::parse_opts(&args[1..])),
        "replay" => cli::replay(&model, &cli::parse_opts(&args[1..])),
        "weights" => cli::weights(&model, &cli::parse_opts(&args[1..])),
        "bench" => cli::bench(&model, &cli::parse_opts(&args[1..]), load_ms),
        "" | "tui" => {
            let o = cli::parse_opts(if cmd.is_empty() { &args } else { &args[1..] });
            if tui::is_tty() { tui::run(&model, &o) } else { cli::line_mode(&model, &o) }
        }
        _ => cli::convert(&model, &cli::parse_opts(&args)),
    }
}
