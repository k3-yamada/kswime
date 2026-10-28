//! English pronunciation (ARPAbet, as in CMUdict) → katakana.
//!
//! Loanword katakana follows the pronunciation far more regularly than the
//! spelling, so the phoneme string is the right input.  The rules here are
//! the conventional ones (short vowel + final voiceless stop → sokuon,
//! "er" → アー, final consonant → +ウ段), producing the most common form;
//! a second, looser key with long-vowel marks removed is used for matching
//! so that コミット and コミト both find "commit".

const VOWELS: &[(&str, &str)] = &[
    ("AA", "a"), ("AE", "a"), ("AH", "a"), ("AO", "o:"), ("AW", "au"), ("AY", "ai"),
    ("EH", "e"), ("ER", "a:"), ("EY", "ei"), ("IH", "i"), ("IY", "i:"), ("OW", "o:"),
    ("OY", "oi"), ("UH", "u"), ("UW", "u:"),
    // spelling-driven alternatives (see arpabet_to_katakana_variants)
    ("AH_O", "o"), ("ER_O", "o:"),
];

fn vowel(p: &str) -> Option<&'static str> {
    VOWELS.iter().find(|(k, _)| *k == p).map(|(_, v)| *v)
}

/// Kana for consonant `c` (romanised key) + vowel letter.
fn cv(c: &str, v: char) -> &'static str {
    macro_rules! row {
        ($a:expr, $i:expr, $u:expr, $e:expr, $o:expr) => {
            match v { 'a' => $a, 'i' => $i, 'u' => $u, 'e' => $e, _ => $o }
        };
    }
    match c {
        "" => row!("ア", "イ", "ウ", "エ", "オ"),
        "k" => row!("カ", "キ", "ク", "ケ", "コ"),
        "g" => row!("ガ", "ギ", "グ", "ゲ", "ゴ"),
        "s" => row!("サ", "シ", "ス", "セ", "ソ"),
        "z" => row!("ザ", "ジ", "ズ", "ゼ", "ゾ"),
        "t" => row!("タ", "ティ", "トゥ", "テ", "ト"),
        "d" => row!("ダ", "ディ", "ドゥ", "デ", "ド"),
        "n" => row!("ナ", "ニ", "ヌ", "ネ", "ノ"),
        "h" => row!("ハ", "ヒ", "フ", "ヘ", "ホ"),
        "f" => row!("ファ", "フィ", "フ", "フェ", "フォ"),
        "v" => row!("ヴァ", "ヴィ", "ヴ", "ヴェ", "ヴォ"),
        "b" => row!("バ", "ビ", "ブ", "ベ", "ボ"),
        "p" => row!("パ", "ピ", "プ", "ペ", "ポ"),
        "m" => row!("マ", "ミ", "ム", "メ", "モ"),
        "y" => row!("ヤ", "イ", "ユ", "イェ", "ヨ"),
        "r" => row!("ラ", "リ", "ル", "レ", "ロ"),
        "l" => row!("ラ", "リ", "ル", "レ", "ロ"),
        "w" => row!("ワ", "ウィ", "ウ", "ウェ", "ウォ"),
        "sh" => row!("シャ", "シ", "シュ", "シェ", "ショ"),
        "zh" => row!("ジャ", "ジ", "ジュ", "ジェ", "ジョ"),
        "ch" => row!("チャ", "チ", "チュ", "チェ", "チョ"),
        "j" => row!("ジャ", "ジ", "ジュ", "ジェ", "ジョ"),
        "th" => row!("サ", "シ", "ス", "セ", "ソ"),
        "dh" => row!("ザ", "ジ", "ズ", "ゼ", "ゾ"),
        "ky" => row!("キャ", "キ", "キュ", "キェ", "キョ"),
        "ny" => row!("ニャ", "ニ", "ニュ", "ニェ", "ニョ"),
        "hy" => row!("ヒャ", "ヒ", "ヒュ", "ヒェ", "ヒョ"),
        "my" => row!("ミャ", "ミ", "ミュ", "ミェ", "ミョ"),
        "ry" => row!("リャ", "リ", "リュ", "リェ", "リョ"),
        "py" => row!("ピャ", "ピ", "ピュ", "ピェ", "ピョ"),
        "by" => row!("ビャ", "ビ", "ビュ", "ビェ", "ビョ"),
        "gy" => row!("ギャ", "ギ", "ギュ", "ギェ", "ギョ"),
        "ty" => row!("テャ", "ティ", "テュ", "テェ", "テョ"),
        "dy" => row!("デャ", "ディ", "デュ", "デェ", "デョ"),
        "kw" => row!("クァ", "クィ", "ク", "クェ", "クォ"),
        "gw" => row!("グァ", "グィ", "グ", "グェ", "グォ"),
        _ => "",
    }
}

/// Consonant without a following vowel.
fn c_alone(c: &str) -> &'static str {
    match c {
        "k" => "ク", "g" => "グ", "s" => "ス", "z" => "ズ", "t" => "ト", "d" => "ド",
        "n" => "ン", "h" => "ホ", "f" => "フ", "v" => "ヴ", "b" => "ブ", "p" => "プ",
        "m" => "ム", "r" => "ー", "l" => "ル", "sh" => "シュ", "zh" => "ジュ", "ch" => "チ",
        "j" => "ジ", "th" => "ス", "dh" => "ズ", "ng" => "ング", "w" => "ウ", "y" => "イ",
        _ => "",
    }
}

fn consonant(p: &str) -> Option<&'static str> {
    Some(match p {
        "B" => "b", "CH" => "ch", "D" => "d", "DH" => "dh", "F" => "f", "G" => "g", "HH" => "h",
        "JH" => "j", "K" => "k", "L" => "l", "M" => "m", "N" => "n", "NG" => "ng", "P" => "p",
        "R" => "r", "S" => "s", "SH" => "sh", "T" => "t", "TH" => "th", "V" => "v", "W" => "w",
        "Y" => "y", "Z" => "z", "ZH" => "zh",
        _ => return None,
    })
}

/// All plausible katakana forms of an ARPAbet phoneme sequence.  Japanese
/// loanwords follow the pronunciation but pick the vowel of /ʌ/, /ə/, /ɑ/
/// and /ɜr/ from the spelling (commit → コミット, cut → カット, docker →
/// ドッカー), so those vowels are expanded to both ア and オ (up to 3 such
/// positions, 8 variants).
pub fn arpabet_to_katakana_variants(phones: &[&str]) -> Vec<String> {
    let ambiguous: Vec<usize> = phones
        .iter()
        .enumerate()
        .filter(|(_, p)| matches!(p.trim_end_matches(|c: char| c.is_ascii_digit()), "AH" | "AA" | "ER"))
        .map(|(i, _)| i)
        .take(3)
        .collect();
    let mut out = Vec::new();
    for mask in 0..(1u32 << ambiguous.len()) {
        let mut ps: Vec<String> = phones.iter().map(|p| p.to_string()).collect();
        for (bit, &i) in ambiguous.iter().enumerate() {
            if mask & (1 << bit) != 0 {
                let stress: String = ps[i].chars().filter(|c| c.is_ascii_digit()).collect();
                let base = ps[i].trim_end_matches(|c: char| c.is_ascii_digit());
                ps[i] = format!("{}{stress}", if base == "ER" { "ER_O" } else { "AH_O" });
            }
        }
        let refs: Vec<&str> = ps.iter().map(|s| s.as_str()).collect();
        if let Some(k) = arpabet_to_katakana(&refs) {
            if !out.contains(&k) {
                out.push(k);
            }
        }
    }
    out
}

/// Convert an ARPAbet phoneme sequence (stress digits allowed) to katakana.
/// Returns None for unknown phonemes.
pub fn arpabet_to_katakana(phones: &[&str]) -> Option<String> {
    let ps: Vec<String> = phones.iter().map(|p| p.trim_end_matches(|c: char| c.is_ascii_digit()).to_string()).collect();
    let mut out = String::new();
    let mut i = 0;
    let voiceless_stop = |c: &str| matches!(c, "k" | "t" | "p" | "ch" | "sh" | "s");
    while i < ps.len() {
        let p = ps[i].as_str();
        if let Some(v) = vowel(p) {
            // bare vowel
            out.push_str(&vowel_kana("", v));
            i += 1;
            continue;
        }
        let Some(mut c) = consonant(p) else { return None };
        // consonant clusters that fuse: C + Y + vowel → palatalised row;
        // K/G + W + vowel → クァ row
        let mut j = i + 1;
        if j < ps.len() && ps[j] == "Y" && j + 1 < ps.len() && vowel(&ps[j + 1]).is_some()
            && matches!(c, "k" | "n" | "h" | "m" | "r" | "p" | "b" | "g" | "t" | "d")
        {
            c = match c { "k" => "ky", "n" => "ny", "h" => "hy", "m" => "my", "r" => "ry", "p" => "py", "b" => "by", "g" => "gy", "t" => "ty", _ => "dy" };
            j += 1;
        } else if j < ps.len() && ps[j] == "W" && j + 1 < ps.len() && vowel(&ps[j + 1]).is_some() && matches!(c, "k" | "g") {
            c = if c == "k" { "kw" } else { "gw" };
            j += 1;
        }
        // unstressed schwa before a final L or N drops: google → グーグル,
        // button → ボタン keeps its vowel (N), so only L
        if j + 2 == ps.len() && ps[j] == "AH" && phones[j].ends_with('0') && ps[j + 1] == "L" && !c.is_empty() {
            out.push_str(c_alone(c));
            i = j + 1;
            continue;
        }
        if j < ps.len() {
            if let Some(v) = vowel(&ps[j]) {
                // sokuon: short vowel followed by a final / pre-consonant
                // voiceless stop (commit → コミット, cut → カット)
                let short = !v.contains(':') && v.len() == 1;
                let next_c = ps.get(j + 1).map(|s| s.as_str());
                let stop_next = next_c.is_some_and(|n| consonant(n).is_some_and(voiceless_stop));
                let after_stop_final = j + 2 >= ps.len() || vowel(&ps[j + 2]).is_none();
                // commit → コミット, docker → ドッカー (stressed short vowel +
                // stop + ER), but not "ticket" → チケット's second syllable
                let stressed = phones[j].ends_with('1');
                let gem = short && stop_next && (after_stop_final || (stressed && ps.get(j + 2).is_some_and(|v| v == "ER" || v == "ER_O")));
                out.push_str(&vowel_kana(c, v));
                if gem && !matches!(c, "ng") {
                    out.push('ッ');
                }
                i = j + 1;
                continue;
            }
        }
        // consonant without vowel
        out.push_str(c_alone(c));
        i = j;
    }
    // trailing "ー" after a consonant alone (e.g. "r") is fine; collapse ーー
    while out.contains("ーー") {
        out = out.replace("ーー", "ー");
    }
    if out.is_empty() { None } else { Some(out) }
}

fn vowel_kana(c: &str, v: &str) -> String {
    let mut chars = v.chars();
    let first = chars.next().unwrap();
    let mut s = String::from(cv(c, first));
    for extra in chars {
        match extra {
            ':' => s.push('ー'),
            'i' => s.push('イ'),
            'u' => s.push('ウ'),
            _ => {}
        }
    }
    s
}

/// Katakana → hiragana, used as the lexicon key.
pub fn katakana_to_hiragana(s: &str) -> String {
    s.chars().map(|c| match c { 'ァ'..='ヶ' => char::from_u32(c as u32 - 0x60).unwrap(), _ => c }).collect()
}

/// Loose key: long-vowel marks removed, ゔ→ぶ, ティ→チ, so that casual
/// spellings (コミト, サーバ, バージョン/ヴァージョン) still match.  The older
/// ファ→ハ spelling (コーヒー) is handled by explicit variants, not here, so
/// that フィグマ and ヒグマ stay distinct.
pub fn loose_key(hira: &str) -> String {
    let s = hira
        .replace("ゔぁ", "ば")
        .replace("ゔぃ", "び")
        .replace("ゔぇ", "べ")
        .replace("ゔぉ", "ぼ")
        .replace('ゔ', "ぶ")
        .replace("てぃ", "ち")
        .replace("でぃ", "じ")
        .replace("しぇ", "せ")
        .replace("じぇ", "ぜ")
        .replace("ちぇ", "て");
    // drop long-vowel marks and collapse doubled vowels (ぐうぐる, こうひい)
    let mut out = String::new();
    let mut prev: Option<char> = None;
    for c in s.chars().filter(|c| *c != 'ー') {
        let long = match (prev, c) {
            (Some(p), c) if p == c && "あいうえお".contains(c) => true,
            (Some('お'), 'う') | (Some('え'), 'い') => true,
            (Some(p), 'う') if "くすつぬふむゆるぐずづぶぷょゅ".contains(p) => true,
            (Some(p), 'い') if "きしちにひみりぎじぢびぴ".contains(p) => true,
            (Some(p), 'あ') if "かさたなはまやらわがざだばぱゃ".contains(p) => true,
            (Some(p), 'え') if "けせてねへめれげぜでべぺ".contains(p) => true,
            (Some(p), 'お') if "こそとのほもよろごぞどぼぽょ".contains(p) => true,
            _ => false,
        };
        if long {
            continue;
        }
        out.push(c);
        prev = Some(c);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    fn k(s: &str) -> String {
        arpabet_to_katakana(&s.split_whitespace().collect::<Vec<_>>()).unwrap()
    }
    fn v(s: &str) -> Vec<String> {
        arpabet_to_katakana_variants(&s.split_whitespace().collect::<Vec<_>>())
    }
    #[test]
    fn common_loanwords() {
        assert_eq!(k("S ER1 V ER0"), "サーヴァー");
        assert_eq!(k("P UH1 SH"), "プッシュ");
        assert_eq!(k("Z UW1 M"), "ズーム");
        assert_eq!(k("M IY1 T IH0 NG"), "ミーティング");
        assert!(v("K AH0 M IH1 T").contains(&"コミット".to_string()));
        assert!(v("D AA1 K ER0").contains(&"ドッカー".to_string()));
        assert!(v("S L AE1 K").contains(&"スラック".to_string()));
        assert!(v("M ER1 JH").contains(&"マージ".to_string()));
        assert!(v("G UW1 G AH0 L").contains(&"グーグル".to_string()));
    }
}


// ---------------------------------------------------------------------------
// Spelling → katakana (for words without a pronunciation entry, and as an
// extra source of forms that the loose key can match against attested
// katakana).  Letter-cluster rules with the usual Japanese conventions;
// ambiguous vowels produce variants.
// ---------------------------------------------------------------------------

/// Katakana variants from English spelling alone.  Returns up to 8 forms,
/// most conventional first.  None for words with characters outside a–z.
pub fn spelling_to_katakana_variants(word: &str) -> Vec<String> {
    let w: Vec<u8> = word.bytes().map(|b| b.to_ascii_lowercase()).collect();
    if w.is_empty() || !w.iter().all(|b| b.is_ascii_lowercase()) {
        return Vec::new();
    }
    // tokens: Vec of alternatives per step (each alternative = romanised mora
    // string such as "ko", "N", "k:" (consonant alone), "a:")
    let mut steps: Vec<Vec<String>> = Vec::new();
    let n = w.len();
    let mut i = 0;
    let is_v = |b: u8| matches!(b, b'a' | b'e' | b'i' | b'o' | b'u');
    let at = |k: usize| -> u8 { if k < n { w[k] } else { 0 } };
    while i < n {
        let c = w[i];
        let rest = &w[i..];
        // ---- multi-letter endings / clusters -------------------------------
        if rest.starts_with(b"tion") || (rest.starts_with(b"sion") && i > 0 && is_v(w[i - 1])) {
            steps.push(vec!["Kション".into()]);
            i += 4;
            continue;
        }
        if rest.starts_with(b"ture") && i + 4 == n {
            steps.push(vec!["Kチャー".into()]);
            i += 4;
            continue;
        }
        if rest.starts_with(b"ght") {
            steps.push(vec!["Ct".into()]);
            i += 3;
            continue;
        }
        if rest.starts_with(b"igh") {
            steps.push(vec!["ai".into()]);
            i += 3;
            continue;
        }
        if rest == b"le" && i > 0 && !is_v(w[i - 1]) {
            steps.push(vec!["Kル".into()]);
            i += 2;
            continue;
        }
        if rest == b"ed" && i > 0 {
            if matches!(w[i - 1], b't' | b'd') {
                steps.push(vec!["i".into()]);
            }
            steps.push(vec!["Kド".into()]);
            i += 2;
            continue;
        }
        if rest == b"es" && i > 0 && matches!(w[i - 1], b's' | b'x' | b'z') {
            steps.push(vec!["i".into()]);
            steps.push(vec!["Kズ".into()]);
            i += 2;
            continue;
        }
        if rest == b"e" && i > 0 && !is_v(w[i - 1]) && n > 2 {
            // silent final e
            i += 1;
            continue;
        }
        if rest == b"y" && i > 0 {
            steps.push(vec!["i:".into(), "i".into()]);
            i += 1;
            continue;
        }
        // ---- vowel digraphs -------------------------------------------------
        let two: &[u8] = if i + 1 < n { &w[i..i + 2] } else { b"" };
        let vowel_alts: Option<(Vec<&str>, usize)> = match two {
            b"ee" => Some((vec!["i:"], 2)),
            b"ea" => Some((vec!["i:", "e"], 2)),
            b"oo" => Some((vec!["u:", "u"], 2)),
            b"ou" => Some((vec!["au", "a"], 2)),
            b"ow" => Some((vec!["o:", "au"], 2)),
            b"ai" | b"ay" => Some((vec!["ei"], 2)),
            b"ei" | b"ey" => Some((vec!["ei", "i:"], 2)),
            b"oa" => Some((vec!["o:"], 2)),
            b"oi" | b"oy" => Some((vec!["oi"], 2)),
            b"au" | b"aw" => Some((vec!["o:"], 2)),
            b"ie" => Some((vec!["i:", "ai"], 2)),
            b"ue" => Some((vec!["u:"], 2)),
            b"ui" => Some((vec!["ui", "u:"], 2)),
            // vowel + r is a long vowel only when the r closes the syllable
            // (server, car), not before another vowel or a second r (terra)
            b"ar" if !is_v(at(i + 2)) && at(i + 2) != b'r' => Some((vec!["a:"], 2)),
            b"or" if !is_v(at(i + 2)) && at(i + 2) != b'r' => Some((vec!["o:"], 2)),
            b"er" | b"ir" | b"ur" if !is_v(at(i + 2)) && at(i + 2) != b'r' => Some((vec!["a:"], 2)),
            _ => None,
        };
        if is_v(c) {
            if let Some((alts, len)) = vowel_alts {
                steps.push(alts.into_iter().map(String::from).collect());
                i += len;
                continue;
            }
            // single vowel: short vs "long" (name-like) reading
            let next_cons_then_vowel = i + 2 < n && !is_v(at(i + 1)) && is_v(at(i + 2)) && at(i + 1) != at(i + 2);
            let alts: Vec<&str> = match c {
                b'a' => if next_cons_then_vowel { vec!["ei", "a"] } else { vec!["a"] },
                b'e' => vec!["e"],
                b'i' => if next_cons_then_vowel { vec!["ai", "i"] } else { vec!["i"] },
                b'o' => if next_cons_then_vowel { vec!["o:", "o"] } else { vec!["o"] },
                _ => if next_cons_then_vowel { vec!["yu:", "a"] } else { vec!["a", "u"] },
            };
            steps.push(alts.into_iter().map(String::from).collect());
            i += 1;
            continue;
        }
        // ---- consonants -----------------------------------------------------
        let (cons, len): (&str, usize) = match two {
            b"ch" => ("ch", 2),
            b"sh" => ("sh", 2),
            b"th" => ("th", 2),
            b"ph" => ("f", 2),
            b"wh" => ("w", 2),
            b"ck" => ("Qk", 2), // sokuon + k
            b"qu" => ("kw", 2),
            b"ng" if i + 2 == n => ("ng", 2),
            b"kn" if i == 0 => ("n", 2),
            b"wr" if i == 0 => ("r", 2),
            _ => match c {
                b'c' => (if matches!(at(i + 1), b'e' | b'i' | b'y') { "s" } else { "k" }, 1),
                b'g' => ("g", 1),
                b'x' => ("Qks", 1),
                b'j' => ("j", 1),
                b'q' => ("k", 1),
                b'y' => ("y", 1),
                b'v' => ("v", 1),
                b'z' => ("z", 1),
                b'l' => ("r", 1),
                _ => (std::str::from_utf8(&w[i..i + 1]).unwrap(), 1),
            },
        };
        // doubled consonant (tt, pp, ss …): sokuon before voiceless stops / s
        let doubled = len == 1 && at(i + 1) == c && !is_v(c);
        let mut cons_s = cons.to_string();
        if doubled {
            if matches!(c, b't' | b'p' | b'k' | b's') {
                cons_s = format!("Q{cons}");
            }
            i += 1;
        }
        i += len;
        // attach the following vowel in the next loop iteration: emit the
        // consonant as a pending marker
        steps.push(vec![format!("C{cons_s}")]);
    }
    // ---- assemble: consonant markers combine with the next vowel ----------
    let mut outs: Vec<String> = vec![String::new()];
    let mut pending: Vec<Option<String>> = vec![None];
    for alts in steps {
        let mut next_outs = Vec::new();
        let mut next_pending = Vec::new();
        for (o, p) in outs.iter().zip(pending.iter()) {
            for alt in &alts {
                let mut s = o.clone();
                let mut pend = p.clone();
                if let Some(cons) = alt.strip_prefix('C') {
                    // flush a previous consonant without vowel
                    if let Some(prev) = pend.take() {
                        s.push_str(&consonant_alone(&prev));
                    }
                    pend = Some(cons.to_string());
                } else if let Some(lit) = alt.strip_prefix('K') {
                    if let Some(prev) = pend.take() {
                        s.push_str(&consonant_alone(&prev));
                    }
                    s.push_str(lit);
                } else {
                    let cons = pend.take().unwrap_or_default();
                    s.push_str(&romanised_mora(&cons, alt));
                }
                next_outs.push(s);
                next_pending.push(pend);
                if next_outs.len() >= 16 {
                    break;
                }
            }
        }
        outs = next_outs;
        pending = next_pending;
    }
    let mut result: Vec<String> = Vec::new();
    for (o, p) in outs.into_iter().zip(pending) {
        let mut s = o;
        if let Some(prev) = p {
            s.push_str(&consonant_alone(&prev));
        }
        while s.contains("ーー") {
            s = s.replace("ーー", "ー");
        }
        if !s.is_empty() && !result.contains(&s) {
            result.push(s);
        }
        if result.len() >= 8 {
            break;
        }
    }
    result
}

/// `cons` may start with 'Q' (sokuon) and may be "" ; `v` like "a", "ei", "o:".
fn romanised_mora(cons: &str, v: &str) -> String {
    let (soku, cons) = match cons.strip_prefix('Q') {
        Some(rest) => (true, rest),
        None => (false, cons),
    };
    let mut s = String::new();
    if soku {
        s.push('ッ');
    }
    if cons == "ng" {
        s.push_str("ング");
        s.push_str(&vowel_kana("", v));
        return s;
    }
    if cons == "N" {
        s.push('ン');
        return s;
    }
    s.push_str(&vowel_kana(cons, v));
    s
}

fn consonant_alone(cons: &str) -> String {
    let (soku, cons) = match cons.strip_prefix('Q') {
        Some(rest) => (true, rest),
        None => (false, cons),
    };
    let mut s = String::new();
    if soku {
        s.push('ッ');
    }
    s.push_str(match cons {
        "N" => "ン",
        c => c_alone(c),
    });
    s
}

#[cfg(test)]
mod spelling_tests {
    use super::*;
    #[test]
    fn spelling_forms() {
        let v = |w: &str| spelling_to_katakana_variants(w);
        // the spelling form itself is コフィー; コーヒー is reached through the
        // ファ→ハ respelling the model builder adds
        assert!(v("coffee").iter().any(|k| k == "コフィー"), "{:?}", v("coffee"));
        assert!(v("gitlab").iter().any(|k| k == "ギトラブ" || k == "ギットラブ"), "{:?}", v("gitlab"));
        assert!(v("kubernetes").len() >= 1);
        assert!(v("terraform").iter().any(|k| k.starts_with("テラ")), "{:?}", v("terraform"));
    }
}
