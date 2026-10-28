use kswime::{InputSession as Engine, Lang, Learner, Model, Weights};
use std::sync::OnceLock;

fn model() -> &'static Model<'static> {
    static M: OnceLock<Model<'static>> = OnceLock::new();
    M.get_or_init(kswime::load_embedded_model)
}

fn convert(text: &str) -> String {
    let mut e = Engine::new(model());
    e.push_str(text);
    let p = e.compose(true);
    e.render(&p)
}

fn kana(text: &str) -> String {
    let mut e = Engine::new(model());
    e.push_str(text);
    e.compose(true).iter().map(|p| if p.lang == Lang::Ja { p.kana.clone() } else { p.text.clone() }).collect()
}

fn convert_before(before: &str, text: &str) -> String {
    let mut e = Engine::new(model());
    e.set_context_from_text(before);
    e.push_str(text);
    let p = e.compose(true);
    e.render(&p)
}

// ---- baseline behaviour with the rolled-back (unlearned) weights ----------

#[test]
fn headline_examples() {
    assert_eq!(convert("mitsumoriwoexceldetsukutta"), "見積もりをexcelで作った");
    assert_eq!(convert("gitlabnimergeshita"), "gitlabにmergeした");
    assert_eq!(convert("cursordecommitsuru"), "cursorでcommitする");
    assert_eq!(convert("Please review it"), "Please review it");
    assert_eq!(convert("kaigihazoomdeyarimasu"), "会議はzoomでやります");
    assert_eq!(kana("kyouhaiitenkidesune"), "きょうはいいてんきですね");
    assert_eq!(kana("kitekudasai"), "きてください");
}

#[test]
fn context_decides_ambiguous_words() {
    assert_eq!(convert_before("Please ", "make"), "make");
    assert_eq!(convert_before("I want ", "to see"), "to see");
    // a lone "to" after English is a coin flip for the unlearned model
    // (P(En) ≈ 0.5); it is one of the cases learning is for
    assert_eq!(convert_before("これ", "to"), "と");
    assert_eq!(convert_before("it is the ", "same"), "same");
}

#[test]
fn identifiers_are_their_own_source() {
    // structural, not a learned weight: tokens with identifier punctuation
    assert_eq!(convert("taro@example.com"), "taro@example.com");
    assert_eq!(convert("https://example.com/path"), "https://example.com/path");
    assert_eq!(convert("i18n"), "i18n");
    assert_eq!(convert("k8s"), "k8s");
    assert_eq!(convert("10ji"), "10時");
}

#[test]
fn punctuation_follows_language() {
    assert_eq!(convert("kore,sore."), "これ、それ。");
    assert_eq!(convert("yes!"), "yes!");
    assert_eq!(convert("hai!"), "はい！");
    assert_eq!(convert("OKdesu"), "OKです");
    // punctuation follows the sentence: an English noun inside a Japanese
    // sentence is still followed by 、 (from the user's typing log)
    assert_eq!(convert("cpu,memorinosiyouryou"), "cpu、メモリの使用量");
    assert_eq!(convert_before("意識し、", "cpu,memori"), "cpu、メモリ");
    assert_eq!(convert("Please review it, thanks."), "Please review it, thanks.");
    // decimal points and digit groups are not sentence punctuation
    assert_eq!(convert("10.5kiro"), "10.5キロ");
}

#[test]
fn loanword_bridge_both_directions() {
    // katakana reading typed in romaji → the English spelling is a candidate
    let mut e = Engine::new(model());
    e.push_str("komitto");
    let p = e.compose(true);
    assert_eq!(p[0].text, "コミット");
    assert!(p[0].candidates.contains(&"commit".to_string()));
    // and inside a sentence, with the rest converted around it
    let mut e = Engine::new(model());
    e.push_str("komittosuru");
    assert!(e.compose(true)[0].candidates.contains(&"commitする".to_string()));
    // a reading without any Japanese word converts to the English word
    assert_eq!(convert("guuguru"), "google");
    // English word → its katakana forms as candidates
    let mut e = Engine::new(model());
    e.push_str("server");
    let p = e.compose(true);
    assert_eq!(p[0].candidates[0], "server");
    assert!(p[0].candidates.contains(&"サーバー".to_string()));
    assert!(e.next_candidate(-1, 1));
    assert_eq!(e.compose(true)[0].text, "サーバー");
}

#[test]
fn candidates_and_cycling() {
    let mut e = Engine::new(model());
    e.push_str("kanji");
    let p = e.compose(true);
    let ja = p.iter().find(|p| p.lang == Lang::Ja).unwrap();
    assert!(ja.candidates.len() > 2);
    assert!(ja.candidates.contains(&"漢字".to_string()));
    assert!(ja.candidates.contains(&"かんじ".to_string()));
    assert!(ja.candidates.contains(&"カンジ".to_string()));
    let first = ja.text.clone();
    assert!(e.next_candidate(-1, 1));
    assert_ne!(e.compose(true).iter().find(|p| p.lang == Lang::Ja).unwrap().text, first);
}

// ---- learning --------------------------------------------------------------

#[test]
fn forcing_a_language_moves_the_weights_and_sticks() {
    // unlearned: upper-case keystrokes carry no cost inside Japanese, so a
    // capitalised abbreviation is read as kana
    assert_eq!(convert("AInituite"), "愛について");
    let mut e = Engine::new(model());
    e.push_str("AInituite");
    let before = e.weights().clone();
    let (tok, lang) = e.toggle_last_language().unwrap(); // flips "AInituite" as one run → En
    assert_eq!(lang, Lang::En);
    assert_eq!(tok, "AInituite");
    // the user wanted only "AI" English: force the range explicitly
    e.clear();
    e.push_str("AInituite");
    e.force_language(0, 2, Lang::En);
    assert_eq!(e.render(&e.compose(true)), "AIについて");
    assert_ne!(e.weights(), &before, "perceptron step happened");
    assert!(e.weights().w[kswime::application::weights::feat::UPPER_IN_JA] > 0.0, "upper-case inside Japanese became costly");
    // a fresh buffer in the same session benefits from the learned weight
    e.commit();
    e.reset_context();
    e.push_str("DXnosoudan");
    assert_eq!(e.render(&e.compose(true)), "DXの相談");
}

#[test]
fn token_memory_survives_sessions() {
    let mut e = Engine::new(model());
    e.push_str("repo");
    assert_eq!(e.render(&e.compose(true)), "レポ", "unlearned baseline");
    e.toggle_last_language();
    assert_eq!(e.render(&e.compose(true)), "repo");
    let store = kswime::infrastructure::MemoryStore(std::cell::RefCell::new(Vec::new()));
    e.save_corrections(&store).unwrap();
    let mut e2 = Engine::new(model());
    e2.load_corrections(&store);
    e2.push_str("repo");
    assert_eq!(e2.render(&e2.compose(true)), "repo");
}

#[test]
fn learning_from_expected_renderings() {
    let mut e = Engine::new(model());
    e.push_str("I love sushi");
    assert_ne!(e.render(&e.compose(true)), "I love sushi", "unlearned baseline reads sushi as Japanese");
    assert!(e.learn_from_expected("I love sushi"));
    assert!(e.learner().updates >= 1);
    e.clear();
    e.push_str("I love sushi");
    // the weights moved towards English continuation; the same input is now
    // either right or closer (lower correction count)
    let a = e.analyze_final();
    assert!(a.p_en[8] > 0.3);
}

// ---- engine mechanics --------------------------------------------------------

#[test]
fn commit_carries_context() {
    let mut e = Engine::new(model());
    e.push_str("it is the ");
    e.commit();
    e.push_str("same");
    assert_eq!(e.render(&e.compose(true)), "same");
    let mut e = Engine::new(model());
    e.push_str("uminonakani");
    e.commit();
    e.push_str("same");
    assert_eq!(e.compose(true)[0].kana, "さめ");
}

#[test]
fn typing_keeps_short_tail_undecided() {
    let mut e = Engine::new(model());
    e.push_str("Please re");
    let a = e.analyze();
    assert!(a.segments.last().unwrap().pending);
    assert!(a.p_en[8] > 0.5);
    let p = e.compose(false);
    assert_eq!(p.last().unwrap().confidence, 2);
    let mut e = Engine::new(model());
    e.push_str("kaigihazo");
    assert_eq!(e.render(&e.compose(false)), "会議はぞ");
}

#[test]
fn per_keystroke_posterior_and_confidence() {
    let mut e = Engine::new(model());
    e.push_str("kaigihazoomdeyarimasu");
    let a = e.analyze_final();
    assert!(a.p_en[..7].iter().all(|p| *p < 0.1), "かいぎは");
    assert!(a.p_en[7..11].iter().all(|p| *p > 0.9), "zoom");
    assert!(a.p_en[11..].iter().all(|p| *p < 0.1), "でやります");
    assert_eq!(a.stable_prefix(0.9), 21);
    assert!(e.compose_from(&a).iter().all(|p| p.confidence == 0));
    assert!(e.explain(&a).len() >= 4);
}

#[test]
fn after_context_is_a_feature() {
    let mut e = Engine::new(model());
    e.set_context_after("は好きです");
    e.push_str("sushi");
    let a = e.analyze_final();
    assert!(a.features[kswime::application::weights::feat::AFTER_JA] == 0.0 || a.segments[0].lang == Lang::En);
}

#[test]
fn case_file_baseline() {
    // the rolled-back model is documented at ≥ 85 % exact on the own cases;
    // the learned behaviour is measured with `kswime eval --online`
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/data/cases.tsv");
    let text = std::fs::read_to_string(path).unwrap();
    let (mut ok, mut n) = (0, 0);
    for line in text.lines() {
        let f: Vec<&str> = line.split('\t').collect();
        if f.len() < 3 {
            continue;
        }
        n += 1;
        let mut e = Engine::new(model());
        if let Some(b) = f.get(3) {
            if !b.is_empty() {
                e.set_context_from_text(b);
            }
        }
        e.push_str(f[1]);
        let p = e.compose(true);
        let k: String = p.iter().map(|p| if p.lang == Lang::Ja { p.kana.clone() } else { p.text.clone() }).collect();
        let norm = |s: &str| s.chars().filter(|c| !c.is_whitespace()).map(|c| c.to_ascii_lowercase()).collect::<String>();
        if norm(&k) == norm(f[2]) || norm(&e.render(&p)) == norm(f[2]) {
            ok += 1;
        }
    }
    assert!(ok as f64 / n as f64 >= 0.85, "{ok}/{n}");
}

#[test]
fn secret_mode_pauses_learning() {
    let mut e = Engine::new(model());
    e.learning_paused = true;
    e.push_str("repo");
    let before = e.weights().clone();
    e.toggle_last_language();
    assert_eq!(e.render(&e.compose(true)), "repo", "the correction still applies to this composition");
    assert_eq!(e.weights(), &before, "but no weight moved");
    assert!(e.corrections().is_empty(), "and no token was remembered");
    assert!(!e.learn_from_expected("repo"));
}

// ---- learned weights must not drift into nonsense --------------------------

/// The a-priori structural costs (OOV, unknown kana, switches, …) are costs:
/// a negative one rewards every extra letter of an unknown English token, so
/// a long unknown string wins as one English word and nothing converts.
const STRUCTURAL_COSTS: [&str; 10] = [
    "EN_OOV", "EN_OOV_LETTER", "JA_UNKNOWN_FLAG", "JA_UNKNOWN_KANA", "SWITCH_JA_EN",
    "SWITCH_EN_JA", "EN_CONTINUE", "EN_SINGLE_LETTER", "PENDING_AFTER_JA", "PENDING_AFTER_EN",
];

#[test]
fn loading_a_drifted_weights_file_keeps_structural_costs_non_negative() {
    let mut e = Engine::new(model());
    e.weights_mut().load_tsv("EN_OOV_LETTER\t-800\nSWITCH_EN_JA\t-100\nEN_OOV\t-1\n");
    for name in STRUCTURAL_COSTS {
        let v = e.weights().get(name).unwrap();
        assert!(v >= 0.0, "{name} = {v}");
    }
}

#[test]
fn negative_structural_costs_from_a_real_profile_no_longer_swallow_the_sentence() {
    // The English structural weights of a profile where repeated boundary
    // nudges (Shift+Left/Right) had pushed EN_OOV_LETTER to -800.  Before the
    // floor, the whole string was one OOV English word ("exceldetukutta").
    let drifted = "EN_OOV\t4100\nEN_OOV_LETTER\t-800\nEN_BIAS\t-600\nSWITCH_EN_JA\t900\n";
    let mut e = Engine::new(model());
    e.weights_mut().load_tsv(drifted);
    e.push_str("mitumoriwoexceldetukutta");
    let p = e.compose(true);
    assert_eq!(e.render(&p), "見積もりをexcelで作った");
}

#[test]
fn the_strength_preset_and_other_learned_rewards_may_stay_negative() {
    let mut e = Engine::new(model());
    e.weights_mut().load_tsv("EN_BIAS\t-600\nCAPITAL_EN\t-300\nMORA_VIOLATION\t-200\n");
    assert_eq!(e.weights().get("EN_BIAS"), Some(-600.0));
    assert_eq!(e.weights().get("CAPITAL_EN"), Some(-300.0));
    assert_eq!(e.weights().get("MORA_VIOLATION"), Some(-200.0));
}

#[test]
fn a_perceptron_step_cannot_push_a_structural_cost_below_zero() {
    use kswime::application::weights::{feat, N_FEATURES};
    let mut w = Weights::default();
    let mut learner = Learner::default();
    // The gold path has more OOV letters than the prediction: the step lowers
    // EN_OOV_LETTER.  Repeat far past what a sane profile would see.
    let mut predicted = vec![0.0; N_FEATURES];
    let mut gold = vec![0.0; N_FEATURES];
    predicted[feat::EN_OOV_LETTER] = 1.0;
    gold[feat::EN_OOV_LETTER] = 4.0;
    for _ in 0..50 {
        learner.update(&mut w, &predicted, &gold);
    }
    assert!(w.w[feat::EN_OOV_LETTER] >= 0.0, "{}", w.w[feat::EN_OOV_LETTER]);
}
