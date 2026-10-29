//! Use case: convert a Japanese kana run to kanji.
//!
//! A plain word lattice over the kana string with Mozc's class-conditional
//! word costs and POS bigram, decoded with an N-best Viterbi so the user can
//! cycle through alternatives.  Unknown kana are passed through as single
//! characters at a high cost.

use crate::domain::transliterate::loose_key;
use crate::domain::{Candidate, Knowledge, SegEnd};


/// A hypothesis stored on a lattice node: total cost and back-pointer
/// (previous node index, previous hypothesis rank, entry index or None for
/// a pass-through char).
#[derive(Clone, Copy)]
struct Hyp<'s> {
    cost: i64,
    rid: u16,
    prev_node: usize,
    prev_rank: usize,
    surface: Option<&'s str>,
    /// kana span length
    span: usize,
    /// this word opens a bunsetsu (sentence start, a grammar boundary, or a
    /// forced cut)
    starts_seg: bool,
}

const UNKNOWN_CHAR_COST: i64 = 9000;
/// Extra cost of rendering a katakana reading as its English spelling
/// (so カタカナ stays the default and the English form is a candidate).
const LOAN_ENGLISH_COST: i64 = 2500;
/// Longest kana span looked up in the loanword bridge.
const LOAN_MAX_KANA: usize = 12;

/// Convert `kana` to its N best surface strings.  `left_rid` is the POS of
/// the word before the run (BOS if none); `eos` adds the sentence-end
/// transition when the run ends the input.
pub fn convert(kb: &dyn Knowledge, kana: &[char], left_rid: u16, eos: bool, n_best: usize, use_boundary: bool) -> Vec<Candidate> {
    convert_with_cuts(kb, kana, left_rid, eos, n_best, use_boundary, &[])
}

/// Same as `convert`, with bunsetsu boundaries the user forced: each entry of
/// `cuts` is a kana position, and a boundary is forced right before it, so no
/// word may cross it and the word starting there pays the boundary penalties
/// whatever the grammar says.  Positions outside `1..kana.len()` are ignored.
///
/// A cut can only separate; it cannot join what the grammar splits.
pub fn convert_with_cuts(kb: &dyn Knowledge, kana: &[char], left_rid: u16, eos: bool, n_best: usize, use_boundary: bool, cuts: &[usize]) -> Vec<Candidate> {
    let cuts: Vec<usize> = cuts.iter().copied().filter(|&c| c > 0 && c < kana.len()).collect();
    let mut out = decode(kb, kana, left_rid, eos, n_best, use_boundary, LOAN_ENGLISH_COST, &cuts);
    if kana.is_empty() {
        return out;
    }
    // plain hiragana and katakana are always offered, then the English
    // rendering: the best path when loanword arcs are favoured (こみっとする →
    // commitする), plus the conventional katakana of a whole-run loanword
    let hira: String = kana.iter().collect();
    let kata: String = kana.iter().map(|c| to_katakana(*c)).collect();
    let mut extras = vec![hira.clone(), kata];
    if n_best > 1 {
        for c in decode(kb, kana, left_rid, eos, 2, use_boundary, -2500, &cuts).into_iter().take(2) {
            if c.surface.bytes().any(|b| b.is_ascii_alphabetic()) {
                extras.push(c.surface);
            }
        }
        // a whole-run loanword is one word: it would cross any forced cut
        if kana.len() <= LOAN_MAX_KANA && cuts.is_empty() {
            for w in kb.english_for_kana(&loose_key(&hira)).iter().take(1) {
                if let Some(k) = kb.katakana_for_english(w).first() {
                    extras.push(k.to_string());
                }
            }
        }
    }
    for extra in extras {
        if !out.iter().any(|c| c.surface == extra) {
            // plain kana and loanword spellings are one bunsetsu
            let segs = vec![SegEnd { kana: kana.len(), chars: extra.chars().count() }];
            out.push(Candidate { surface: extra, cost: i64::MAX / 2, segs });
        }
    }
    out
}

/// N-best Viterbi over the kana string.  `loan_bias` is added to the cost of
/// loanword arcs (English spelling for a katakana reading).  `cuts` are the
/// validated forced boundary positions (see `convert_with_cuts`).
#[allow(clippy::too_many_arguments)]
fn decode(kb: &dyn Knowledge, kana: &[char], left_rid: u16, eos: bool, n_best: usize, use_boundary: bool, loan_bias: i64, cuts: &[usize]) -> Vec<Candidate> {
    let n = kana.len();
    if n == 0 {
        return vec![Candidate { surface: String::new(), cost: 0, segs: Vec::new() }];
    }
    let k = n_best.max(1);
    let pos_ids = kb.pos();
    // nodes[pos] = hypotheses ending at pos (sorted by cost, ≤ k per distinct rid… we keep k overall)
    let mut nodes: Vec<Vec<Hyp<'_>>> = vec![Vec::new(); n + 1];
    nodes[0].push(Hyp { cost: 0, rid: left_rid, prev_node: 0, prev_rank: 0, surface: None, span: 0, starts_seg: false });
    for pos in 0..n {
        if nodes[pos].is_empty() {
            continue;
        }
        let mut arcs: Vec<(usize, Option<&str>, u16, u16, i64)> = Vec::new(); // (len, surface, lid, rid, cost)
        for (len, e) in kb.prefixes(&kana[pos..], kb.max_reading_len()) {
            arcs.push((len, Some(e.surface), e.lid, e.rid, e.cost as i64));
        }
        // katakana reading of an English word → the English spelling itself
        // (こみっと → commit), as a noun; the kana→kanji N-best then offers it
        for len in 2..=kana.len().saturating_sub(pos).min(LOAN_MAX_KANA) {
            // only where the reading has no native (non-katakana) word: する,
            // した stay Japanese; さーばー, こみっと may become English
            if kb.lookup(&kana[pos..pos + len]).iter().any(|e| !e.kata_only) {
                continue;
            }
            let span: String = kana[pos..pos + len].iter().collect();
            let key = loose_key(&span);
            for (rank, w) in kb.english_for_kana(&key).iter().take(3).enumerate() {
                let ecost = kb.word(w).map(|e| e.cost as i64).unwrap_or(8000);
                arcs.push((len, Some(*w), pos_ids.noun_sahen, pos_ids.noun_sahen, ecost + loan_bias + rank as i64 * 500));
            }
        }
        // pass-through single kana (unknown)
        arcs.push((1, None, pos_ids.noun_general, pos_ids.noun_general, UNKNOWN_CHAR_COST));
        let prevs: Vec<(usize, Hyp)> = nodes[pos].iter().copied().enumerate().collect();
        let forced_here = cuts.contains(&pos);
        for (len, surface, lid, rid, wcost) in arcs {
            let end = pos + len;
            // a forced boundary: no word may cross it
            if cuts.iter().any(|&c| pos < c && c < end) {
                continue;
            }
            for &(rank, h) in &prevs {
                let mut total = h.cost + wcost + kb.connection(h.rid, lid) as i64;
                if use_boundary {
                    // sentence start is always a bunsetsu start; after a
                    // non-Japanese token (English, number) only when the
                    // grammar says so (a particle continues the bunsetsu)
                    if pos == 0 {
                        if h.rid == pos_ids.bos || kb.is_boundary(h.rid, lid) {
                            total += kb.prefix_penalty(lid) as i64;
                        }
                    } else if forced_here || kb.is_boundary(h.rid, lid) {
                        total += (kb.suffix_penalty(h.rid) + kb.prefix_penalty(lid)) as i64;
                    }
                }
                // without boundary modelling the whole run is one bunsetsu
                let starts_seg = pos == 0 || (use_boundary && (forced_here || kb.is_boundary(h.rid, lid)));
                let hyp = Hyp { cost: total, rid, prev_node: pos, prev_rank: rank, surface, span: len, starts_seg };
                insert_hyp(&mut nodes[end], hyp, k);
            }
        }
    }
    // final ranking with optional EOS transition
    let mut finals: Vec<(i64, usize)> = nodes[n]
        .iter()
        .enumerate()
        .map(|(r, h)| {
            let mut c = h.cost;
            if eos {
                c += kb.connection(h.rid, pos_ids.bos) as i64;
                if use_boundary {
                    c += kb.suffix_penalty(h.rid) as i64;
                }
            }
            (c, r)
        })
        .collect();
    finals.sort();
    let mut out: Vec<Candidate> = Vec::new();
    for (cost, rank) in finals {
        // (surface, kana length, opens a bunsetsu), last word first
        let mut words: Vec<(String, usize, bool)> = Vec::new();
        let mut node = n;
        let mut r = rank;
        while node > 0 {
            let h = nodes[node][r];
            let sf = match h.surface {
                Some(sf) => sf.to_string(),
                None => kana[node - h.span..node].iter().collect(),
            };
            words.push((sf, h.span, h.starts_seg));
            node = h.prev_node;
            r = h.prev_rank;
        }
        words.reverse();
        let surface: String = words.iter().map(|w| w.0.as_str()).collect();
        // a bunsetsu ends where the next word opens one, and at the run's end
        let mut segs: Vec<SegEnd> = Vec::new();
        let (mut kana_end, mut chars_end) = (0, 0);
        for (i, (sf, span, _)) in words.iter().enumerate() {
            kana_end += span;
            chars_end += sf.chars().count();
            if words.get(i + 1).is_none_or(|next| next.2) {
                segs.push(SegEnd { kana: kana_end, chars: chars_end });
            }
        }
        if !out.iter().any(|c| c.surface == surface) {
            out.push(Candidate { surface, cost, segs });
        }
        if out.len() >= k {
            break;
        }
    }
    out
}

fn insert_hyp<'s>(list: &mut Vec<Hyp<'s>>, h: Hyp<'s>, k: usize) {
    // keep list sorted by cost, at most k entries
    let pos = list.partition_point(|x| x.cost <= h.cost);
    if pos >= k {
        return;
    }
    list.insert(pos, h);
    if list.len() > k {
        list.truncate(k);
    }
}

pub fn to_katakana(c: char) -> char {
    match c {
        'ぁ'..='ゖ' => char::from_u32(c as u32 + 0x60).unwrap_or(c),
        _ => c,
    }
}
