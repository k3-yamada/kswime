//! Build `data/model.bin` from public raw data.  No crates, only std.
//!
//!   kswime-build-model data/raw data/model.bin [--max-ja-cost 8000] [--max-surfaces 6]
//!
//! Expects in `data/raw/`:
//!   mozc_dict00.txt … mozc_dict09.txt   (Mozc dictionary_oss, BSD-3)
//!   id.def, connection_single_column.txt, romanji-hiragana.tsv (Mozc)
//!   boundary.def, segmenter.def   (Mozc bunsetsu rules)
//!   en_gbooks.tsv         (word<TAB>count, Google Books Ngram v3 via
//!                          hackerb9/gwordlist, CC BY 3.0)
//!   en_scowl70.txt        (SCOWL/ESDB words up to size 70; see SCOWL_COPYRIGHT.txt)
//!   en_extra.txt          (supplementary English words; optional)
//!   cmudict.dict          (CMU Pronouncing Dictionary, BSD; optional —
//!                          enables the katakana ↔ English bridge)

use kswime::infrastructure::model_builder::BuildEntry as JaEntry;
use kswime::infrastructure::model_builder::{boundary_matrix, boundary_penalties, Builder};
use std::collections::{HashMap, HashSet};
use std::path::Path;

fn read(dir: &Path, name: &str) -> String {
    std::fs::read_to_string(dir.join(name)).unwrap_or_else(|e| panic!("{name}: {e}"))
}

fn is_hiragana_reading(s: &str) -> bool {
    !s.is_empty() && s.chars().all(|c| matches!(c, 'ぁ'..='ゖ' | 'ー'))
}

fn is_katakana_only(s: &str) -> bool {
    !s.is_empty() && s.chars().all(|c| matches!(c, 'ァ'..='ヶ' | 'ー' | '・'))
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() < 2 {
        eprintln!("usage: kswime-build-model <raw dir> <out.bin> [--max-ja-cost N] [--max-surfaces N]");
        std::process::exit(2);
    }
    let raw = Path::new(&args[0]);
    let out_path = Path::new(&args[1]);
    let mut max_ja_cost = 8500i32;
    let mut max_surfaces = 6usize;
    let mut max_en_words = 80000usize;
    let mut gen_bench_dir: Option<String> = None;
    let mut it = args[2..].iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--max-ja-cost" => max_ja_cost = it.next().unwrap().parse().unwrap(),
            "--max-surfaces" => max_surfaces = it.next().unwrap().parse().unwrap(),
            "--max-en-words" => max_en_words = it.next().unwrap().parse().unwrap(),
            "--gen-bench" => gen_bench_dir = it.next().cloned(),
            _ => panic!("unknown option {a}"),
        }
    }

    // ---- POS ids ---------------------------------------------------------
    let mut ids: HashMap<String, u16> = HashMap::new();
    let mut features: Vec<String> = Vec::new();
    for line in read(raw, "id.def").lines() {
        if let Some((n, name)) = line.split_once(' ') {
            let n: u16 = n.parse().unwrap();
            ids.insert(name.to_string(), n);
            if features.len() <= n as usize {
                features.resize(n as usize + 1, String::new());
            }
            features[n as usize] = name.to_string();
        }
    }
    let id = |name: &str| *ids.get(name).unwrap_or_else(|| panic!("id.def lacks {name}"));
    let bos = id("BOS/EOS,*,*,*,*,*,*");
    let noun_general = id("名詞,一般,*,*,*,*,*");
    let noun_proper = id("名詞,固有名詞,一般,*,*,*,*");
    let noun_sahen = id("名詞,サ変接続,*,*,*,*,*");
    let noun_number = id("名詞,数,*,*,*,*,*");
    let func_prefixes = ["助詞", "助動詞", "名詞,接尾", "動詞,接尾", "形容詞,接尾", "接尾", "名詞,非自立", "動詞,非自立", "形容詞,非自立", "記号"];
    let mut func_ids: Vec<u16> = ids
        .iter()
        .filter(|(name, _)| func_prefixes.iter().any(|p| name.starts_with(p)))
        .map(|(_, i)| *i)
        .collect();
    func_ids.sort_unstable();
    let mut function_ranges: Vec<(u16, u16)> = Vec::new();
    for i in func_ids {
        match function_ranges.last_mut() {
            Some(r) if r.1 + 1 == i => r.1 = i,
            _ => function_ranges.push((i, i)),
        }
    }

    // ---- romaji table ----------------------------------------------------
    let mut romaji = Vec::new();
    for line in read(raw, "romanji-hiragana.tsv").lines() {
        let mut c = line.split('\t');
        let (Some(k), Some(o)) = (c.next(), c.next()) else { continue };
        if k.is_empty() {
            continue;
        }
        romaji.push((k.to_string(), o.to_string(), c.next().unwrap_or("").to_string()));
    }

    // ---- connection matrix ---------------------------------------------
    let conn_txt = read(raw, "connection_single_column.txt");
    let mut lines = conn_txt.split_ascii_whitespace();
    let conn_n: usize = lines.next().unwrap().parse().unwrap();
    let conn: Vec<i32> = lines.map(|v| v.parse().unwrap()).collect();
    assert_eq!(conn.len(), conn_n * conn_n);
    assert_eq!(features.len(), conn_n, "id.def and connection matrix disagree");

    // ---- bunsetsu boundary model --------------------------------------
    let (prefix_pen, suffix_pen) = boundary_penalties(&features, &read(raw, "boundary.def"));
    let boundary = boundary_matrix(&features, &read(raw, "segmenter.def"));
    eprintln!(
        "bunsetsu: {} prefix / {} suffix penalised ids, {:.1}% boundaries",
        prefix_pen.iter().filter(|p| **p > 0).count(),
        suffix_pen.iter().filter(|p| **p > 0).count(),
        100.0 * boundary.iter().filter(|b| **b).count() as f64 / boundary.len() as f64
    );

    // ---- Japanese lexicon ----------------------------------------------
    // key (reading) → entries; dedupe on (surface, lid, rid) keeping min cost
    let mut by_reading: HashMap<String, HashMap<(String, u16, u16), i32>> = HashMap::new();
    let mut n_in = 0usize;
    for i in 0..10 {
        let text = read(raw, &format!("mozc_dict0{i}.txt"));
        for line in text.lines() {
            let f: Vec<&str> = line.split('\t').collect();
            if f.len() < 5 {
                continue;
            }
            n_in += 1;
            let (reading, lid, rid, cost, surface) = (f[0], f[1], f[2], f[3], f[4]);
            if !is_hiragana_reading(reading) || reading.len() > 60 || surface.len() > 120 {
                continue;
            }
            let cost: i32 = cost.parse().unwrap();
            if cost > max_ja_cost {
                continue;
            }
            let e = by_reading.entry(reading.to_string()).or_default();
            let key = (surface.to_string(), lid.parse().unwrap(), rid.parse().unwrap());
            let c = e.entry(key).or_insert(i32::MAX);
            if cost < *c {
                *c = cost;
            }
        }
    }
    let mut ja: Vec<JaEntry> = Vec::new();
    for (reading, entries) in by_reading {
        let mut v: Vec<((String, u16, u16), i32)> = entries.into_iter().collect();
        v.sort_by(|a, b| a.1.cmp(&b.1));
        // Keep: the cheapest `max_surfaces` distinct surfaces, plus the
        // cheapest entry of every distinct left POS id (so conjugated verb
        // forms such as 書い/住ん survive among the many noun homophones), and
        // per surface its POS variants within 2000 of its best (function words
        // such as で legitimately carry several POS ids at the same cost).
        let best_cost = v.first().map(|e| e.1).unwrap_or(0);
        let mut best_per_surface: HashMap<String, (i32, usize)> = HashMap::new();
        let mut seen_lid: HashSet<u16> = HashSet::new();
        for ((surface, lid, rid), cost) in v {
            let new_lid = seen_lid.insert(lid) && cost <= best_cost + 6000;
            let n_surf = best_per_surface.len();
            let e = match best_per_surface.get_mut(&surface) {
                Some(e) => e,
                None => {
                    if n_surf >= max_surfaces && !new_lid {
                        continue;
                    }
                    best_per_surface.entry(surface.clone()).or_insert((cost, 0))
                }
            };
            if (cost > e.0 + 2000 && !new_lid) || e.1 >= 12 {
                continue;
            }
            e.1 += 1;
            let kata_only = is_katakana_only(&surface);
            ja.push(JaEntry { reading: reading.clone(), surface, lid, rid, cost, kata_only });
        }
    }
    eprintln!("ja: {n_in} rows in → {} entries", ja.len());

    // ---- English lexicon -----------------------------------------------
    // Source: Google Books Ngram corpus v3 (CC BY 3.0) as aggregated by
    // hackerb9/gwordlist (frequency-all.txt.gz), case-folded alphabetic
    // 1-grams with total counts (tools/fetch_data.sh produces en_gbooks.tsv).
    let block: HashSet<&str> = "je la le les el que de du des et est un une mon ma mes si ya da ne ni ga wo \
        se te ta ra ri sa ki ka ko ku ke ru mu mi mo lt mm hmm mmm shh aah ooh uh um von der das \
        ja ji ju jo ze zo zu za ge gu gi bi bu bo ba pa pi pu pe po wa su ii iii iv vi vii viii ix xi xii"
        .split_whitespace()
        .collect();
    let keep: HashSet<&str> = "i a to me do no be so go you he we it in on at an or as if is us up"
        .split_whitespace()
        .collect();
    let is_mora = |w: &str| -> bool {
        let b = w.as_bytes();
        let vowel = |c: u8| matches!(c, b'a' | b'i' | b'u' | b'e' | b'o');
        match b.len() {
            1 => vowel(b[0]),
            2 => b"kstnhmyrwgzdbpjfv".contains(&b[0]) && vowel(b[1]),
            3 => matches!(w, "shi" | "chi" | "tsu"),
            _ => false,
        }
    };
    // SCOWL / ESDB words up to size 70 (Kevin Atkinson, MIT-like licence;
    // data/raw/SCOWL_COPYRIGHT.txt).  Short tokens (≤ 4 letters) must be a
    // SCOWL word or a simple inflection of one, which removes OCR and
    // foreign-language noise such as "yas", "tbe" from the book corpus.
    let verified: HashSet<String> = std::fs::read_to_string(raw.join("en_scowl70.txt"))
        .map(|t| t.lines().map(|l| l.trim().to_string()).collect())
        .unwrap_or_default();
    let stems = |w: &str| -> Vec<String> {
        let mut out = vec![w.to_string()];
        for suf in ["s", "es", "ed", "d", "ing", "er"] {
            if w.ends_with(suf) && w.len() > suf.len() + 1 {
                let base = &w[..w.len() - suf.len()];
                out.push(base.to_string());
                out.push(format!("{base}e"));
            }
        }
        if w.ends_with("ies") && w.len() > 4 {
            out.push(format!("{}y", &w[..w.len() - 3]));
        }
        out
    };
    let mut rows: Vec<(String, u64)> = Vec::new();
    let mut total: u64 = 0;
    for line in read(raw, "en_gbooks.tsv").lines() {
        let Some((w, c)) = line.split_once('\t') else { continue };
        let Ok(count) = c.trim().parse::<u64>() else { continue };
        total += count;
        if !w.bytes().all(|b| b.is_ascii_lowercase()) {
            continue;
        }
        rows.push((w.to_string(), count));
    }
    let max_words: usize = max_en_words;
    let mut en: Vec<(String, i32, bool)> = Vec::new();
    let mut have: HashSet<String> = HashSet::new();
    let mut dropped = 0usize;
    for (w, c) in rows {
        if en.len() >= max_words {
            break;
        }
        let short_ok = w.len() > 4 || verified.is_empty() || stems(&w).iter().any(|x| verified.contains(x));
        let ok = keep.contains(w.as_str())
            || (!block.contains(w.as_str())
                && !is_mora(&w)
                && short_ok
                && w.bytes().any(|b| matches!(b, b'a' | b'e' | b'i' | b'o' | b'u' | b'y')));
        if !ok {
            dropped += 1;
            continue;
        }
        let cost = (-500.0 * ((c as f64) / (total as f64)).ln()).round() as i32;
        have.insert(w.clone());
        en.push((w, cost, false));
    }
    // supplementary vocabulary (technical terms, colloquial words,
    // contractions) that a book corpus lacks
    let extra_cost = 6500;
    let mut n_extra = 0;
    if let Ok(text) = std::fs::read_to_string(raw.join("en_extra.txt")) {
        for line in text.lines() {
            let line = line.split('#').next().unwrap();
            for w in line.split_whitespace() {
                let w = w.to_ascii_lowercase();
                if have.insert(w.clone()) {
                    en.push((w, extra_cost, true));
                    n_extra += 1;
                } else if let Some(e) = en.iter_mut().find(|e| e.0 == w) {
                    e.2 = true;
                }
            }
        }
    }
    eprintln!("en: {} words ({dropped} dropped, {n_extra} extra), total count {total}", en.len());

    // ---- loanword bridge (katakana reading ↔ English spelling) -----------
    // Forms come from the pronunciation (CMUdict) and from the spelling;
    // both are matched by loose key against the katakana words the Japanese
    // lexicon attests, and the attested spelling (コーヒー, サーバー) is what
    // the user sees.  Generated forms without an attested match are kept as
    // lower-ranked fallbacks so that unknown readings still find the word.
    let mut loan: Vec<(String, String, String, u8)> = Vec::new();
    {
        use kswime::domain::transliterate::{arpabet_to_katakana_variants, katakana_to_hiragana, loose_key, spelling_to_katakana_variants};
        let en_cost: HashMap<&str, i32> = en.iter().map(|(w, c, _)| (w.as_str(), *c)).collect();
        let mut attested_by_loose: HashMap<String, Vec<(i32, String)>> = HashMap::new();
        for e in ja.iter().filter(|e| e.kata_only) {
            attested_by_loose
                .entry(loose_key(&katakana_to_hiragana(&e.surface)))
                .or_default()
                .push((e.cost, e.surface.clone()));
        }
        for v in attested_by_loose.values_mut() {
            v.sort();
            v.dedup_by(|a, b| a.1 == b.1);
        }
        let mut pron: HashMap<String, Vec<Vec<String>>> = HashMap::new();
        if let Ok(text) = std::fs::read_to_string(raw.join("cmudict.dict")) {
            for line in text.lines() {
                let mut it = line.split_whitespace();
                let Some(word) = it.next() else { continue };
                let word = word.split('(').next().unwrap();
                if !word.bytes().all(|b| b.is_ascii_lowercase()) || !en_cost.contains_key(word) {
                    continue;
                }
                pron.entry(word.to_string()).or_default().push(it.map(String::from).collect());
            }
        }
        let mut seen: HashSet<(String, String, String)> = HashSet::new();
        let mut n_attested = 0usize;
        for (word, _, _) in &en {
            // (katakana, primary?, generated rank): primary = straight from
            // the pronunciation; derived = ヴ→バ / ファ→ハ respellings; spelling
            // forms are the last resort
            let mut forms: Vec<(String, bool, u8)> = Vec::new();
            let add = |f: String, primary: bool, rank: u8, forms: &mut Vec<(String, bool, u8)>| {
                if !forms.iter().any(|(x, _, _)| *x == f) {
                    forms.push((f, primary, rank));
                }
            };
            if let Some(ps) = pron.get(word) {
                for phones in ps {
                    let refs: Vec<&str> = phones.iter().map(|s| s.as_str()).collect();
                    for k in arpabet_to_katakana_variants(&refs) {
                        let vb = k.replace("ヴァ", "バ").replace("ヴィ", "ビ").replace("ヴェ", "ベ").replace("ヴォ", "ボ").replace('ヴ', "ブ");
                        let fh = vb.replace("ファ", "ハ").replace("フィ", "ヒ").replace("フェ", "ヘ").replace("フォ", "ホ");
                        add(k, true, 20, &mut forms);
                        add(vb, false, 21, &mut forms);
                        add(fh, false, 22, &mut forms);
                    }
                }
            }
            // spelling forms: the only source for words without a pronunciation
            // (gitlab, figma); otherwise just the first one, to catch
            // spelling-driven conventions the pronunciation misses
            let n_spell = if pron.contains_key(word) { 1 } else { 4 };
            for f in spelling_to_katakana_variants(word).into_iter().take(n_spell) {
                add(f, false, 30, &mut forms);
            }
            let mut emitted_attested: HashSet<String> = HashSet::new();
            let mut n_generated = 0usize;
            for (kata, primary, gen_rank) in forms {
                let key = loose_key(&katakana_to_hiragana(&kata));
                if key.chars().count() < 2 {
                    continue;
                }
                if let Some(att) = attested_by_loose.get(&key) {
                    for (cost, surface) in att.iter().take(2) {
                        // rank by Mozc cost (1000 per band); a form reached
                        // through a respelling (ヴ→バ, ファ→ハ) pays two bands so
                        // that フィグマ beats ヒグマ while コーヒー (common) still
                        // beats コフィー (rare)
                        let band = (*cost / 1000).clamp(0, 9) as u8;
                        let r = if primary { band } else { (band + 2).min(19) };
                        if emitted_attested.insert(surface.clone()) && seen.insert((key.clone(), word.clone(), surface.clone())) {
                            loan.push((key.clone(), word.clone(), surface.clone(), r));
                            n_attested += 1;
                        }
                    }
                }
                // keep at most three generated (non-attested) forms per word
                if n_generated < 3 && seen.insert((key.clone(), word.clone(), kata.clone())) {
                    loan.push((key, word.clone(), kata, gen_rank));
                    n_generated += 1;
                }
            }
        }
        // per loose key: attested forms first, then cheapest English
        loan.sort_by(|a, b| a.0.cmp(&b.0).then(a.3.cmp(&b.3)).then_with(|| en_cost[a.1.as_str()].cmp(&en_cost[b.1.as_str()])));
        eprintln!("loan: {} katakana↔english pairs ({n_attested} attested)", loan.len());
    }

    if let Some(dir) = gen_bench_dir.as_deref() {
        gen_bench(dir, &en, &ja);
    }

    let b = Builder { bos, noun_general, noun_proper, noun_sahen, noun_number, function_ranges, romaji, conn_n, conn, prefix_pen, suffix_pen, boundary, ja, en, loan };
    let bytes = b.write();
    std::fs::write(out_path, &bytes).unwrap();
    eprintln!("wrote {} ({:.1} MB)", out_path.display(), bytes.len() as f64 / 1e6);
}

// ---------------------------------------------------------------------------
// Synthetic benchmarks (deterministic), written as TSV case files:
//   mixed_bench.tsv  Japanese prefix + English word not readable as romaji
//                    + Japanese suffix   (recall on the easy class, at scale)
//   ja_bench.tsv     frequent Japanese reading + particle, typed in romaji
//                    (false positives: Japanese wrongly judged English)
// ---------------------------------------------------------------------------

fn gen_bench(dir: &str, en: &[(String, i32, bool)], ja: &[JaEntry]) {
    use std::fmt::Write as _;
    let is_vowel = |c: u8| matches!(c, b'a' | b'i' | b'u' | b'e' | b'o');
    // "readable as romaji": a sequence of (consonant cluster)(vowel) morae or n
    let readable = |w: &str| -> bool {
        let b = w.as_bytes();
        let mut i = 0;
        while i < b.len() {
            if b[i] == b'n' && (i + 1 == b.len() || !is_vowel(b[i + 1]) && b[i + 1] != b'y') {
                i += 1;
                continue;
            }
            let start = i;
            while i < b.len() && !is_vowel(b[i]) {
                i += 1;
            }
            let cons = &w[start..i];
            if i >= b.len() {
                return false;
            }
            let ok = matches!(cons, "" | "k" | "g" | "s" | "z" | "t" | "d" | "n" | "h" | "b" | "p" | "m" | "y" | "r" | "w" | "j" | "f"
                | "ky" | "gy" | "sh" | "ch" | "ts" | "ny" | "hy" | "by" | "py" | "my" | "ry" | "th" | "dh")
                || (cons.len() == 2 && cons.as_bytes()[0] == cons.as_bytes()[1]); // sokuon
            if !ok {
                return false;
            }
            i += 1;
        }
        true
    };
    let prefixes = [("sorede", "それで"), ("kinouha", "きのうは"), ("chotto", "ちょっと"), ("atode", "あとで"), ("kyoumo", "きょうも"),
        ("sugoku", "すごく"), ("mottekita", "もってきた"), ("tsukatte", "つかって"), ("watashino", "わたしの"), ("minnani", "みんなに"),
        ("zenbu", "ぜんぶ"), ("korekara", "これから"), ("", "")];
    let suffixes = [("wotsukau", "をつかう"), ("gasuki", "がすき"), ("dekensaku", "でけんさく"), ("nitsuite", "について"), ("toiu", "という"),
        ("mademiru", "までみる"), ("kamo", "かも"), ("nanode", "なので"), ("ha", "は"), ("wo", "を"), ("ga", "が"), ("ni", "に"), ("de", "で"),
        ("shita", "した"), ("", "")];
    let mut words: Vec<&str> = en.iter().filter(|(w, _, extra)| !extra && (4..=10).contains(&w.len()) && !readable(w)).map(|(w, _, _)| w.as_str()).collect();
    // deterministic shuffle (LCG)
    let mut seed: u64 = 20261026;
    for i in (1..words.len()).rev() {
        seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        let j = (seed >> 33) as usize % (i + 1);
        words.swap(i, j);
    }
    let mut out = String::new();
    for (i, w) in words.iter().take(300).enumerate() {
        for frame in 0..2 {
            let p = prefixes[(i * 2 + frame) % prefixes.len()];
            let mut s = suffixes[(i * 3 + frame * 5) % suffixes.len()];
            if p.0.is_empty() && s.0.is_empty() {
                s = suffixes[0];
            }
            let _ = writeln!(out, "mixed\t{}{}{}\t{}{}{}\t", p.0, w, s.0, p.1, w, s.1);
        }
    }
    std::fs::write(format!("{dir}/mixed_bench.tsv"), out).unwrap();

    // Japanese words: frequent readings (cost ≤ 4000, 2–6 kana, not katakana
    // only), converted to romaji, with a particle
    let en_set: HashSet<&str> = en.iter().map(|(w, _, _)| w.as_str()).collect();
    let particles = [("wo", "を"), ("ga", "が"), ("ni", "に"), ("ha", "は"), ("de", "で"), ("no", "の"), ("", "")];
    let mut seen: HashSet<&str> = HashSet::new();
    let mut cands: Vec<&JaEntry> = ja.iter().filter(|e| !e.kata_only && e.cost <= 4000 && (2..=6).contains(&e.reading.chars().count())).collect();
    cands.sort_by(|a, b| a.reading.cmp(&b.reading).then(a.cost.cmp(&b.cost)));
    cands.retain(|e| seen.insert(e.reading.as_str()));
    let mut seed: u64 = 7;
    for i in (1..cands.len()).rev() {
        seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        let j = (seed >> 33) as usize % (i + 1);
        cands.swap(i, j);
    }
    let mut out = String::new();
    let mut n = 0;
    for e in cands {
        let Some(rom) = kana_to_romaji(&e.reading) else { continue };
        if en_set.contains(rom.as_str()) {
            continue;
        }
        let p = particles[n % particles.len()];
        let _ = writeln!(out, "ja_word\t{}{}\t{}{}\t", rom, p.0, e.reading, p.1);
        n += 1;
        if n >= 1000 {
            break;
        }
    }
    std::fs::write(format!("{dir}/ja_bench.tsv"), out).unwrap();
    eprintln!("benchmarks written to {dir}");
}

/// Hepburn-style romaji for a hiragana reading (None when a kana has no
/// simple spelling).
fn kana_to_romaji(kana: &str) -> Option<String> {
    const TABLE: &[(&str, &str)] = &[
        ("きゃ", "kya"), ("きゅ", "kyu"), ("きょ", "kyo"), ("ぎゃ", "gya"), ("ぎゅ", "gyu"), ("ぎょ", "gyo"),
        ("しゃ", "sha"), ("しゅ", "shu"), ("しょ", "sho"), ("じゃ", "ja"), ("じゅ", "ju"), ("じょ", "jo"),
        ("ちゃ", "cha"), ("ちゅ", "chu"), ("ちょ", "cho"), ("にゃ", "nya"), ("にゅ", "nyu"), ("にょ", "nyo"),
        ("ひゃ", "hya"), ("ひゅ", "hyu"), ("ひょ", "hyo"), ("びゃ", "bya"), ("びゅ", "byu"), ("びょ", "byo"),
        ("ぴゃ", "pya"), ("ぴゅ", "pyu"), ("ぴょ", "pyo"), ("みゃ", "mya"), ("みゅ", "myu"), ("みょ", "myo"),
        ("りゃ", "rya"), ("りゅ", "ryu"), ("りょ", "ryo"), ("てぃ", "thi"), ("でぃ", "dhi"), ("ふぁ", "fa"), ("ふぃ", "fi"), ("ふぇ", "fe"), ("ふぉ", "fo"),
        ("あ", "a"), ("い", "i"), ("う", "u"), ("え", "e"), ("お", "o"),
        ("か", "ka"), ("き", "ki"), ("く", "ku"), ("け", "ke"), ("こ", "ko"), ("が", "ga"), ("ぎ", "gi"), ("ぐ", "gu"), ("げ", "ge"), ("ご", "go"),
        ("さ", "sa"), ("し", "shi"), ("す", "su"), ("せ", "se"), ("そ", "so"), ("ざ", "za"), ("じ", "ji"), ("ず", "zu"), ("ぜ", "ze"), ("ぞ", "zo"),
        ("た", "ta"), ("ち", "chi"), ("つ", "tsu"), ("て", "te"), ("と", "to"), ("だ", "da"), ("ぢ", "di"), ("づ", "du"), ("で", "de"), ("ど", "do"),
        ("な", "na"), ("に", "ni"), ("ぬ", "nu"), ("ね", "ne"), ("の", "no"),
        ("は", "ha"), ("ひ", "hi"), ("ふ", "fu"), ("へ", "he"), ("ほ", "ho"), ("ば", "ba"), ("び", "bi"), ("ぶ", "bu"), ("べ", "be"), ("ぼ", "bo"),
        ("ぱ", "pa"), ("ぴ", "pi"), ("ぷ", "pu"), ("ぺ", "pe"), ("ぽ", "po"),
        ("ま", "ma"), ("み", "mi"), ("む", "mu"), ("め", "me"), ("も", "mo"), ("や", "ya"), ("ゆ", "yu"), ("よ", "yo"),
        ("ら", "ra"), ("り", "ri"), ("る", "ru"), ("れ", "re"), ("ろ", "ro"), ("わ", "wa"), ("を", "wo"), ("ー", "-"),
    ];
    let chars: Vec<char> = kana.chars().collect();
    let mut out = String::new();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == 'っ' {
            let rest = kana_to_romaji(&chars[i + 1..].iter().collect::<String>())?;
            let first = rest.as_bytes().first().copied()?;
            if !first.is_ascii_alphabetic() || b"aiueon".contains(&first) {
                return None;
            }
            out.push(first as char);
            out.push_str(&rest);
            return Some(out);
        }
        if chars[i] == 'ん' {
            let next = chars.get(i + 1).copied();
            out.push_str(match next {
                Some(c) if "あいうえおやゆよなにぬねの".contains(c) => "nn",
                _ => "n",
            });
            i += 1;
            continue;
        }
        if i + 1 < chars.len() {
            let two: String = chars[i..i + 2].iter().collect();
            if let Some((_, r)) = TABLE.iter().find(|(k, _)| *k == two) {
                out.push_str(r);
                i += 2;
                continue;
            }
        }
        let one = chars[i].to_string();
        let (_, r) = TABLE.iter().find(|(k, _)| *k == one)?;
        out.push_str(r);
        i += 1;
    }
    Some(out)
}
