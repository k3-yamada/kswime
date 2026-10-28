//! Presentation: command-line commands.  Only this layer reads argv and
//! stdin/stdout.

use crate::application::{InputSession, Strength};
use crate::domain::{Knowledge, Lang, Piece};
use crate::infrastructure::CorrectionFile;
use std::io::{self, BufRead};
use std::time::Instant;

pub struct Opts {
    pub before: Option<String>,
    pub after: Option<String>,
    pub strength: Strength,
    pub sets: Vec<(String, f64)>,
    pub learn: Option<String>,
    /// weights file to load before / save after (learning)
    pub weights: Option<String>,
    /// update weights while evaluating / replaying
    pub online: bool,
    /// append `before<TAB>typed<TAB>committed` lines here (TUI)
    pub record: Option<String>,
    pub verbose: bool,
    /// `bench --budget`: exit 1 when a resource budget is exceeded
    pub budget: bool,
    pub positional: Vec<String>,
}

pub fn parse_opts(args: &[String]) -> Opts {
    let mut o = Opts { before: None, after: None, strength: Strength::Balanced, sets: Vec::new(), learn: None, weights: None, online: false, record: None, verbose: false, budget: false, positional: Vec::new() };
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--before" => o.before = it.next().cloned(),
            "--after" => o.after = it.next().cloned(),
            "--weights" => o.weights = it.next().cloned(),
            "--online" => o.online = true,
            "--record" => o.record = it.next().cloned(),
            "--strength" => {
                o.strength = match it.next().map(|s| s.as_str()) {
                    Some("aggressive") => Strength::Aggressive,
                    Some("conservative") => Strength::Conservative,
                    _ => Strength::Balanced,
                }
            }
            "--set" => {
                if let Some((k, v)) = it.next().and_then(|s| s.split_once('=')) {
                    o.sets.push((k.to_string(), v.parse().expect("number")));
                }
            }
            "--learn" => o.learn = it.next().cloned(),
            "-v" | "--verbose" => o.verbose = true,
            "--budget" => o.budget = true,
            _ => o.positional.push(a.clone()),
        }
    }
    o
}

pub fn session<'k>(kb: &'k dyn Knowledge, o: &Opts) -> InputSession<'k> {
    let mut s = InputSession::new(kb);
    s.set_strength(o.strength);
    if let Some(p) = &o.weights {
        if let Ok(t) = std::fs::read_to_string(p) {
            s.weights_mut().load_tsv(&t);
        }
    }
    for (k, v) in &o.sets {
        if !s.set(k, *v) {
            eprintln!("unknown param {k}");
        }
    }
    if let Some(b) = &o.before {
        s.set_context_from_text(b);
    }
    if let Some(a) = &o.after {
        s.set_context_after(a);
    }
    if let Some(p) = &o.learn {
        s.load_corrections(&CorrectionFile::new(p));
    }
    s
}

pub const HELP: &str = "kswime — 半角/全角キーのいらないローマ字入力エンジン（依存ゼロ）

  kswime                   対話入力（端末）。Enter 確定、Tab 候補、Ctrl+L 日英切替、Ctrl+U 取消、Ctrl+C 終了
  kswime convert TEXT      一回変換
  kswime judge TEXT        区切り、候補、打鍵ごとの P(英語)
  kswime stream TEXT       1打鍵ずつの推移
  kswime eval FILE.tsv     訂正打鍵数と正解率（カテゴリ<TAB>入力<TAB>期待[<TAB>前文脈]）
  kswime replay LOG.tsv    記録の再生（前文脈<TAB>入力<TAB>確定文）。--online で学習しながら
  kswime weights           現在の重み一覧（名前、値、説明）
  kswime bench [FILE.tsv]  資源計測: 起動時間、常駐メモリ、1 打鍵の判定遅延 p50/p99（--budget で上限超過を終了コード 1 に）
  echo TEXT | kswime       行ごとに変換

  --before TEXT  --after TEXT  --strength aggressive|balanced|conservative
  --set key=value  --weights FILE（読込。--online 時と対話入力の終了時に保存）  --online
  --learn FILE（語ごとの訂正の読込・保存）  --record FILE（対話入力の記録。replay 用）  -v
";

pub fn convert(kb: &dyn Knowledge, o: &Opts) {
    let mut s = session(kb, o);
    s.push_str(&o.positional.join(" "));
    let pieces = s.compose(true);
    println!("{}", s.render(&pieces));
}

pub fn judge(kb: &dyn Knowledge, o: &Opts, load_ms: u128) {
    let mut s = session(kb, o);
    let text = o.positional.join(" ");
    s.push_str(&text);
    let t = Instant::now();
    let a = s.analyze_final();
    let pieces = s.compose_from(&a);
    let dt = t.elapsed();
    println!("{}", s.render(&pieces));
    for p in &pieces {
        let cands = if p.candidates.len() > 1 {
            format!("  [{}]", p.candidates[1..].iter().take(12).cloned().collect::<Vec<_>>().join(" "))
        } else {
            String::new()
        };
        println!("  {:<8} {:<12} {}{}", format!("{:?}", p.lang), p.text, p.kana, cands);
    }
    for line in s.explain(&a) {
        println!("    {line}");
    }
    if o.verbose {
        for (name, f, c) in s.feature_report(&a) {
            println!("    {name:<18} f={f:<10} cost={c:.0}");
        }
    }
    let probs: Vec<String> = text.chars().zip(a.p_en.iter()).map(|(c, p)| format!("{c}:{p:.2}")).collect();
    println!("  P(En) {}", probs.join(" "));
    println!("  stable@0.9={}  cost={}  {:.2} ms (model {load_ms} ms)", a.stable_prefix(0.9), a.best_cost, dt.as_secs_f64() * 1e3);
}

pub fn stream(kb: &dyn Knowledge, o: &Opts) {
    let mut s = session(kb, o);
    for c in o.positional.join(" ").chars() {
        s.push(c);
        let a = s.analyze();
        let pieces = s.compose_from(&a);
        let labels: String = a.p_en.iter().map(|p| if *p > 0.5 { 'E' } else { 'J' }).collect();
        println!("{:<24} {:<24} {} stable={}", s.buffer_string(), s.render(&pieces), labels, a.stable_prefix(0.9));
    }
}

fn normalize(s: &str) -> String {
    s.chars()
        .filter(|c| !c.is_whitespace())
        .map(|c| match c {
            'ァ'..='ヶ' => char::from_u32(c as u32 - 0x60).unwrap(),
            _ => c.to_ascii_lowercase(),
        })
        .collect()
}

/// Score a TSV case file.  Main metric: correction keystrokes per 1,000
/// typed characters — language flips needed to reach the expected
/// segmentation plus candidate steps needed to reach the expected kanji.
/// Secondary metric: exact-match accuracy (kana or kanji rendering).
/// With `--online` the weights learn from each case after scoring it.
pub fn eval(kb: &dyn Knowledge, o: &Opts) {
    let path = o.positional.first().expect("case file");
    let text = std::fs::read_to_string(path).expect("read");
    let mut by_cat: std::collections::BTreeMap<String, (usize, usize, usize, usize)> = Default::default(); // ok, n, corrections, chars
    let mut fails = Vec::new();
    let t = Instant::now();
    let mut n = 0;
    let mut shared = session(kb, o);
    for line in text.lines() {
        let f: Vec<&str> = line.split('\t').collect();
        if f.len() < 3 || line.starts_with('#') {
            continue;
        }
        n += 1;
        let (cat, typed, expected) = (f[0], f[1], f[2]);
        shared.clear();
        shared.reset_context();
        if let Some(b) = f.get(3) {
            if !b.is_empty() {
                shared.set_context_from_text(b);
            }
        }
        shared.push_str(typed);
        let a = shared.analyze_final();
        let pieces = shared.compose_from(&a);
        let converted = shared.render(&pieces);
        let kana: String = pieces.iter().map(|p| if p.lang == Lang::Ja { p.kana.as_str() } else { p.text.as_str() }).collect();
        let ok = normalize(&converted) == normalize(expected) || normalize(&kana) == normalize(expected);
        let corr = correction_keystrokes(&shared, &a, &pieces, expected);
        let c = by_cat.entry(cat.to_string()).or_insert((0, 0, 0, 0));
        c.1 += 1;
        c.2 += corr;
        c.3 += typed.chars().count();
        if ok {
            c.0 += 1;
        } else {
            fails.push(format!("[{cat}] {typed:<26} want {expected:<26} got {converted:<26} ({kana}) corr={corr}"));
        }
        if o.online {
            shared.learn_from_expected(expected);
        }
    }
    let dt = t.elapsed();
    let (mut tp, mut tn, mut tc, mut tch) = (0, 0, 0, 0);
    println!("{:<10} {:>9} {:>8} {:>10}", "category", "exact", "corr", "corr/1000");
    for (cat, (p, q, c, ch)) in &by_cat {
        println!("{cat:<10} {p:>4}/{q:<4} {c:>8} {:>10.1}", 1000.0 * *c as f64 / (*ch).max(1) as f64);
        tp += p;
        tn += q;
        tc += c;
        tch += ch;
    }
    println!(
        "TOTAL      {tp:>4}/{tn:<4} {tc:>8} {:>10.1}   exact {:.1}%  ({:.2} ms/case)",
        1000.0 * tc as f64 / tch.max(1) as f64,
        100.0 * tp as f64 / tn.max(1) as f64,
        dt.as_secs_f64() * 1e3 / n.max(1) as f64
    );
    if o.verbose {
        for f in fails {
            println!("{f}");
        }
    }
    if o.online {
        println!("learning updates: {}", shared.learner().updates);
        if let Some(p) = &o.weights {
            let _ = std::fs::write(p, shared.weights().to_tsv());
        }
    }
}

/// Keystrokes the user would need to turn the prediction into `expected`:
/// one per maximal keystroke run whose language differs from the gold path
/// (F6/F10 flips) plus candidate steps for Japanese runs whose expected
/// surface is kanji and differs (Tab presses, capped at the candidate count).
/// Returns 0 when the rendering already matches.  When no gold path exists
/// (the expected text cannot be typed with this romaji), counts the whole
/// input as one correction per differing run.
pub fn correction_keystrokes(s: &InputSession, a: &crate::domain::Analysis, pieces: &[Piece], expected: &str) -> usize {
    let n = s.buffer().len();
    let converted = s.render(pieces);
    let kana: String = pieces.iter().map(|p| if p.lang == Lang::Ja { p.kana.as_str() } else { p.text.as_str() }).collect();
    if normalize(&converted) == normalize(expected) {
        return 0;
    }
    let no_bias = std::collections::HashMap::new();
    let lat = crate::application::judge::Lattice::build(s.kb(), s.weights(), s.config(), s.buffer(), s.context(), None, &no_bias, false);
    let Some(gold) = lat.analyze_matching(expected) else {
        return a.segments.iter().filter(|x| x.lang != Lang::Neutral).count().max(1);
    };
    let label = |an: &crate::domain::Analysis| -> Vec<Lang> {
        let mut v = vec![Lang::Neutral; n];
        for sg in &an.segments {
            for k in sg.start..sg.end {
                v[k] = sg.lang;
            }
        }
        v
    };
    let (lp, lg) = (label(a), label(&gold));
    let mut flips = 0;
    let mut in_run = false;
    for k in 0..n {
        let differs = lp[k] != lg[k] && lg[k] != Lang::Neutral && lp[k] != Lang::Neutral;
        if differs && !in_run {
            flips += 1;
        }
        in_run = differs;
    }
    // candidate steps: only when the languages already agree
    let mut steps = 0;
    if flips == 0 && normalize(&kana) != normalize(expected) {
        let exp_norm = normalize(expected);
        for p in pieces.iter().filter(|p| p.lang == Lang::Ja) {
            // find a candidate contained in expected at the right place: use
            // the first candidate whose normalized form appears in expected
            let pos = p.candidates.iter().position(|c| exp_norm.contains(&normalize(c)));
            steps += match pos {
                Some(i) if i == p.selected => 0,
                Some(i) => i.max(1),
                None => p.candidates.len().max(1),
            };
        }
    }
    flips + steps
}

/// Replay a typing log: `before<TAB>typed<TAB>gold_text` per line.  Reports
/// correction keystrokes per 1,000 characters; with `--online` the weights
/// learn from every line after it is scored (so the number measures what the
/// user would have experienced).
pub fn replay(kb: &dyn Knowledge, o: &Opts) {
    let path = o.positional.first().expect("log file");
    let text = std::fs::read_to_string(path).expect("read");
    let mut s = session(kb, o);
    let (mut lines, mut corr, mut chars, mut exact) = (0usize, 0usize, 0usize, 0usize);
    let mut per_hundred: Vec<(usize, usize)> = Vec::new();
    for line in text.lines() {
        let f: Vec<&str> = line.split('\t').collect();
        if f.len() < 3 || line.starts_with('#') {
            continue;
        }
        s.clear();
        s.reset_context();
        if !f[0].is_empty() {
            s.set_context_from_text(f[0]);
        }
        s.push_str(f[1]);
        let a = s.analyze_final();
        let pieces = s.compose_from(&a);
        let c = correction_keystrokes(&s, &a, &pieces, f[2]);
        if c == 0 {
            exact += 1;
        }
        lines += 1;
        corr += c;
        chars += f[1].chars().count();
        if per_hundred.len() < lines / 100 + 1 {
            per_hundred.push((0, 0));
        }
        let last = per_hundred.last_mut().unwrap();
        last.0 += c;
        last.1 += f[1].chars().count();
        if o.online {
            s.learn_from_expected(f[2]);
        }
    }
    println!("lines {lines}  chars {chars}  corrections {corr}  corr/1000 {:.1}  exact {:.1}%", 1000.0 * corr as f64 / chars.max(1) as f64, 100.0 * exact as f64 / lines.max(1) as f64);
    if per_hundred.len() > 1 {
        let trend: Vec<String> = per_hundred.iter().map(|(c, ch)| format!("{:.0}", 1000.0 * *c as f64 / (*ch).max(1) as f64)).collect();
        println!("corr/1000 per 100 lines: {}", trend.join(" "));
    }
    if o.online {
        println!("learning updates: {}", s.learner().updates);
        if let Some(p) = &o.weights {
            let _ = std::fs::write(p, s.weights().to_tsv());
        }
    }
}

pub fn weights(kb: &dyn Knowledge, o: &Opts) {
    let s = session(kb, o);
    for (i, name) in crate::application::weights::FEATURE_NAMES.iter().enumerate() {
        println!("{name:<18} {:>9.2}  {}", s.weights().w[i], crate::application::weights::FEATURE_DOCS[i]);
    }
}

pub fn line_mode(kb: &dyn Knowledge, o: &Opts) {
    let mut s = session(kb, o);
    for line in io::stdin().lock().lines() {
        let Ok(line) = line else { break };
        s.push_str(&line);
        let pieces = s.commit();
        println!("{}", s.render(&pieces));
    }
}

/// Resource budget (第 2 世代計画 9 節).  `kswime bench --budget` fails when
/// any is exceeded so CI can hold the line.
pub const BUDGET_LOAD_MS: u128 = 300;
pub const BUDGET_RSS_MB: f64 = 120.0;
pub const BUDGET_KEY_P99_MS: f64 = 5.0;

/// Resident set size in MB, from the OS (std only: /proc on Linux, `ps` elsewhere).
pub fn resident_mb() -> Option<f64> {
    if let Ok(s) = std::fs::read_to_string("/proc/self/statm") {
        let pages: f64 = s.split_whitespace().nth(1)?.parse().ok()?;
        return Some(pages * 4096.0 / 1_048_576.0);
    }
    let out = std::process::Command::new("ps").args(["-o", "rss=", "-p", &std::process::id().to_string()]).output().ok()?;
    let kb: f64 = String::from_utf8_lossy(&out.stdout).trim().parse().ok()?;
    Some(kb / 1024.0)
}

/// Startup time, resident memory, per-keystroke judge+compose latency.
/// Keystrokes come from FILE.tsv (eval format) or the built-in examples.
pub fn bench(kb: &dyn Knowledge, o: &Opts, load_ms: u128) {
    let rss = resident_mb();
    let mut inputs: Vec<(String, String)> = Vec::new();
    if let Some(path) = o.positional.first() {
        if let Ok(text) = std::fs::read_to_string(path) {
            for line in text.lines() {
                let f: Vec<&str> = line.split('\t').collect();
                if f.len() >= 3 {
                    inputs.push((f.get(3).map(|s| s.to_string()).unwrap_or_default(), f[1].to_string()));
                }
            }
        }
    }
    if inputs.is_empty() {
        for t in ["mitsumoriwoexceldetsukutta", "gitlabnimergeshita", "Please review the pull request today", "kaigihazoomdeyarimasu", "saitekikasaisyoukamotuneniisikisi,cpu,memorinosiyouryou"] {
            inputs.push((String::new(), t.to_string()));
        }
    }
    let mut lat: Vec<f64> = Vec::new();
    let mut keys = 0usize;
    let t_all = Instant::now();
    for (before, typed) in &inputs {
        let mut s = session(kb, o);
        if !before.is_empty() {
            s.set_context_from_text(before);
        }
        for c in typed.chars() {
            let t = Instant::now();
            s.push(c);
            let a = s.analyze();
            let _ = s.compose_from(&a);
            lat.push(t.elapsed().as_secs_f64() * 1000.0);
            keys += 1;
        }
        let _ = s.commit();
    }
    lat.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let pct = |p: f64| lat.get(((lat.len() as f64 - 1.0) * p) as usize).copied().unwrap_or(0.0);
    let (p50, p99, max) = (pct(0.5), pct(0.99), lat.last().copied().unwrap_or(0.0));
    let rss_after = resident_mb();
    println!("load        {load_ms} ms            (budget {BUDGET_LOAD_MS})");
    if let Some(r) = rss {
        println!("rss         {r:.0} MB            (budget {BUDGET_RSS_MB:.0}; after bench {:.0})", rss_after.unwrap_or(r));
    }
    println!("keystrokes  {keys} in {} lines, {:.0} ms total", inputs.len(), t_all.elapsed().as_secs_f64() * 1000.0);
    println!("per key     p50 {p50:.2} ms  p99 {p99:.2} ms  max {max:.2} ms   (budget p99 {BUDGET_KEY_P99_MS})");
    let over = load_ms > BUDGET_LOAD_MS || rss.is_some_and(|r| r > BUDGET_RSS_MB) || p99 > BUDGET_KEY_P99_MS;
    println!("{}", if over { "OVER BUDGET" } else { "within budget" });
    if over && o.budget {
        std::process::exit(1);
    }
}
