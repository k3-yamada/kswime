<h1 align="center">kswime</h1>

<p align="center">
半角/全角キーを押さずに、日本語と英語を混ぜて打てるローマ字入力エンジン。<br>
打鍵列からどこが日本語でどこが英語かを判定し、日本語だけをかな漢字変換します。
</p>

<p align="center">
<a href="https://github.com/k3-yamada/kswime/actions/workflows/ci.yml"><img alt="ci" src="https://github.com/k3-yamada/kswime/actions/workflows/ci.yml/badge.svg"></a>
<img alt="dependencies: none" src="https://img.shields.io/badge/dependencies-none-success">
<img alt="rust" src="https://img.shields.io/badge/rust-2024_edition-orange">
<img alt="platform" src="https://img.shields.io/badge/platform-macOS%20IME%20%7C%20CLI%20%7C%20C%20ABI-lightgrey">
<a href="LICENSE"><img alt="license: MIT" src="https://img.shields.io/badge/license-MIT-blue"></a>
</p>

```
mitsumoriwoexceldetsukutta   →  見積もりをexcelで作った
gitlabnimergeshita           →  gitlabにmergeした
kaigihazoomdeyarimasu        →  会議はzoomでやります
cpu,memorinosiyouryou        →  cpu、メモリの使用量
komittosuru                  →  コミットする   （候補: commitする）
coffee                       →  coffee        （候補: コーヒー）
Please review it             →  Please review it
```

名前は、最初の試作 `kanaswitch`（コマンド名 `ksw`）に `ime` を縫い合わせたもので、「kana switch をなくす IME」の略です。読みは「ケースウィム」です。

[English summary](#english-summary) is at the end.

## 何が違うのか

日本語 IME で英単語を打つには、英数キーで切り替えるか、カタカナで打つか、変換候補から英字を探すかのどれかが要ります。技術文書や Slack では 1 文に何度も英単語が入り、その切り替えが打鍵の中で目立つ無駄になります。

kswime は切り替えをなくします。打鍵列の上に日本語の弧と英語の弧を同時に張り、辞書・品詞文法・英語の綴りモデルを一つの対数線形ラティスで解いて、最も自然な分け方を選びます。判定は 1 打鍵ごとに更新され、入力中は未確定の末尾を「未決」として表示します。

- **依存ゼロ。** 外部クレートなし（`cargo tree` は自分自身だけ）、外部 IME なし、実行時に読むファイルなし。辞書と統計モデルは実行ファイルに埋め込み、かな漢字変換も自前です。
- **完全オフライン。** 外部通信はありません。学習データは平文 TSV でローカルに置かれ、シークレットモードで学習と記録を止められます。
- **学習する。** 判定を直すと（F6 / F10 やトグル）、その語を覚えるだけでなく、判定器の重みがパーセプトロン更新で動きます。同じ傾向の入力が次から直ります。
- **カタカナ語と英語の橋渡し。** `komitto` と打てばコミットと commit の両方が候補に出ます。`server` と打てばサーバーが候補に出ます。
- **軽い。** 起動 0.16 秒、常駐 107 MB、1 打鍵の判定遅延 p99 3.4 ms（Linux x86-64）。モデルは埋め込みバイト列をその場で参照し、複製しません。

設計の根拠（Meltype・Mozc・azooKey の調査と新方式）は [docs/DESIGN.md](docs/DESIGN.md)、層構成は [ARCHITECTURE.md](ARCHITECTURE.md) にあります。

## 状態

開発初期です。macOS で日常使用の検証を始めた段階で、設計と評価基盤は固まっていますが、利用者の記録による学習の評価はこれからです。

- 動くもの: macOS IME（Input Method Kit）、CLI、端末 UI、C ABI
- 未対応: Windows（TSF）、Linux（IBus / Fcitx）、推測変換、利用者辞書、誤打鍵補正、かな入力
- 採らないもの: クラウド変換、ニューラル変換、計算・郵便番号・絵文字などの特殊変換

「英日混在入力に特化した IME」を目指し、全部入りは目指しません。

## インストール（macOS）

Xcode は不要です。Xcode Command Line Tools と rustup があれば動きます。

```sh
git clone https://github.com/k3-yamada/kswime.git
cd kswime
tools/fetch_data.sh                                                        # 公開データを data/raw/ へ（数分）
cargo run --release --bin kswime-build-model -- data/raw data/model.bin   # 約 10 秒、63 MB
macos/build.sh --install                                                   # ビルドして ~/Library/Input Methods へ
```

システム設定 > キーボード > 入力ソースで Kswime を追加してください。

`data/model.bin` はリポジトリに含まれません（60 MB の生成物で、`.gitignore` の対象です）。最初に上の `fetch_data.sh` と `kswime-build-model` でモデルを生成してください。生成せずに `cargo build --release` だけすると、`build.rs` が空の仮ファイルを置くため、辞書のない実行ファイルになります。CI の成果物からも取得できます。

操作は [macos/README.md](macos/README.md) にまとめています。主なキーは次のとおりです。

| キー | 動作 |
|---|---|
| 英字 | 入力。日本語と判定された区間は漢字に、英語はそのまま |
| Space | 直前が日本語なら候補を開く／次候補、英語なら空白 |
| ← / → | 直す語を選ぶ（確定しない）。Shift+← / → で日英の境界を動かす |
| F6 / F7 / F10 | 選択中の語をひらがな／カタカナ／英語に。英語を日本語に（またはその逆に）直すとエンジンが学習する |
| Enter / Esc | 全体を確定／取消 |

## CLI

```sh
cargo build --release
./target/release/kswime                          # 対話入力（Unix 端末）
./target/release/kswime convert "mitsumoriwoexceldetsukutta"
./target/release/kswime judge "cursordecommitsuru"      # 区切り、候補、打鍵ごとの P(英語)
./target/release/kswime stream "kaigihazoomdeyarimasu"  # 1 打鍵ずつの推移
echo "mitsumorishowookutte" | ./target/release/kswime   # 行モード
```

対話入力のキーは Enter 確定、Tab / Shift+Tab 候補の巡回、Ctrl+L 直前の語の日英を反転して学習、Ctrl+E 判定の理由、Ctrl+U 取消、Ctrl+C 終了です。`--weights FILE` と `--learn FILE` で重みと語ごとの訂正を保存・復元し、`--record FILE` で打鍵を記録します。

## ライブラリとして使う

```rust
let model = kswime::load_embedded_model();          // 約 0.2 秒、約 110 MB。モデルは複製せず参照する
let mut e = kswime::InputSession::new(&model);
e.set_context_from_text("I bought a ");            // カーソル前の文脈（任意）
e.push_str("kite");
let pieces = e.compose(false);                     // 入力中（末尾は未確定になり得る）
let a = e.analyze_final();                         // 打鍵ごとの P(英語) は a.p_en
e.force_language(0, 4, kswime::Lang::Ja);          // 利用者の訂正（語の記憶＋重みの学習）
let pieces = e.commit();                           // 確定。文法状態を次の入力へ持ち越す
let saved = e.weights().to_tsv();                  // 学習した重み
```

`Piece` は `lang`（Ja / En / Neutral）、`text`、日本語なら `kana` と `candidates` を持ちます。他言語からは [`include/kswime.h`](include/kswime.h) の C ABI（`cargo build --release` で `libkswime.a` / `.so` / `.dylib`）を使います。

## 仕組み

### 判定器

判定器は対数線形モデルです。打鍵列の上に日本語弧（ローマ字→かな→辞書）、英語弧（辞書と綴りモデル）、識別子弧（`@ / : _` を含む語、`example.com` 型、`i18n` 型）を張り、各弧と遷移に特徴ベクトルを付け、重みとの内積をコストとして Viterbi で最良経路を、前向き後ろ向きで打鍵ごとの P(英語) を求めます。日本語の中では Mozc の品詞バイグラムと文節境界規則がそのまま効き、英単語は名詞として文法に参加します。

重みの初期値は次の規則で置き、それ以外は利用者の操作で学習されます（`kswime weights` で一覧）。

- 辞書コストと品詞バイグラム: 倍率 1.0
- モデルが成立するために必要な構造コスト（切り替え、未知語、辞書外英語、未決の末尾）: 事前に置いた定数
- それ以外の経験則（大文字、識別子風、カタカナ語、英語間の空白、文節境界 など）: 0

F6 / F10 で語の言語を直すと、直した経路を正解としてパーセプトロン更新が走ります。macOS 版は `~/Library/Application Support/Kswime/` に重みと語の記憶を自動保存し、「学習をすべて消去」で初期値に戻せます。

句読点は直前の語ではなく文に従います。日本語の文の中の英単語の後でも `cpu,memori` → `cpu、メモリ` で、英語だけの文（`Please review it, thanks.`）は半角のまま、`10.5` と `1,000` も変えません。

### カタカナ語と英語の橋渡し

カタカナ形は 3 系統から集めます。(1) CMU 発音辞書の発音から生成した形（ヴ→バ、ファ→ハ の旧来表記も派生）、(2) 綴りから規則で生成した形（発音辞書に無い gitlab、figma、terraform などのため）、(3) それらと読みが一致する Mozc 辞書の実在カタカナ語。利用者に見せるのは (3) の実在形を優先し（coffee → コーヒー、server → サーバー、version → バージョン）、Mozc の頻度順に並べます。逆向きには、日本語辞書に無い読み（`guuguru`）を英語綴り（google）に変換します。

### 変換器

かな漢字変換は N-best Viterbi で、Mozc の読み辞書（コスト 8,500 以下、読みごとに表記 6 件＋品詞ごとの最安 1 件）、接続行列（16 bit、量子化なし）、文節境界ペナルティ（boundary.def、segmenter.def）を使います。残る同音異義語の誤り（きのう→機能／昨日）は Mozc OSS 辞書のコストそのものに由来し、文脈なしでは Mozc も同じ順位です。候補巡回と学習で補います。

英語辞書は Google Books Ngram（CC BY 3.0、hackerb9/gwordlist 経由）の上位 8 万語です。4 文字以下の語は SCOWL/ESDB の語かその活用形に限り、OCR 由来の雑音を除いています。技術語・口語・短縮形は `data/raw/en_extra.txt`（自作）で補います。

## 評価

主指標は「訂正打鍵数／1,000 文字」です。予測を期待どおりにするのに必要な言語反転の回数と候補送りの回数を数えます。正解率は副指標です。

| セット（学習前の初期重み） | 件数 | 訂正／1,000 字 | 完全一致 |
|---|---|---|---|
| tests/data/cases.tsv（独自） | 117 | 8.2 | 88.9% |
| tests/data/mixed_bench.tsv（合成混在文） | 600 | 1.0 | 98.2% |
| tests/data/ja_bench.tsv（日本語＋助詞、誤英語化の検査） | 1,000 | 1.9 | 99.5% |
| tests/data/convert.tsv（かな漢字変換） | 50 | 32.9 | 84.0% |

学習の効果は再生で測ります。独自テスト 117 件を 5 周ぶん打った記録を `kswime replay` にかけると、学習なしで 9.2、`--online` で 3.9 でした。同じ文を繰り返す理想条件の値で、実際の文章ではこれより緩やかです。

```sh
kswime eval tests/data/cases.tsv -v                        # 訂正打鍵数と正解率
kswime eval tests/data/cases.tsv --online --weights w.tsv  # 採点しながら学習し、重みを保存
kswime replay typing-log.tsv --online                      # 記録の再生（学習ありで利用者の体験を再現）
kswime bench tests/data/cases.tsv --budget                 # 起動時間、常駐メモリ、1 打鍵の判定遅延。上限超過で終了コード 1
```

他製品に同梱されたテスト集合は、採点にも重みの調整にも使っていません。設計の着手段階で Meltype の公開リポジトリと同梱テストを調査し、どの入力が難しいかを把握しました。その知見は特徴量の種類に反映されていますが、重みの値には反映されていません。

## 資源の上限

CPU、メモリ、サイズは常設の制約です。`kswime bench --budget` が起動 300 ms、常駐 120 MB、1 打鍵 p99 5 ms の上限を超えると失敗し、CI で強制しています。現状は起動 0.16 秒、常駐 107 MB、p99 3.4 ms です（Linux x86-64、独自 117 件）。バイナリは約 63 MB で、`kswime-build-model --max-ja-cost 7000 --max-surfaces 3` で約 32 MB まで落とせます。

## 開発

```sh
tools/fetch_data.sh                                                        # 公開データ（Mozc はコミット固定）
cargo run --release --bin kswime-build-model -- data/raw data/model.bin
cargo build --release
cargo test --release
```

CI（GitHub Actions）は Linux でモデル生成、テスト、評価 4 セット、依存方向の検査（domain と application が外側を参照しない）、C ABI スモーク、資源計測を行い、macOS で Kswime.app をコンパイルします。変更はブランチと PR で受け付けます。PR には評価 4 セットと `kswime bench` の値を書いてください。

## ライセンスと出典

コードは [MIT License](LICENSE) です。`data/model.bin` は次の公開データから生成しています。各出典の表示は [NOTICE](NOTICE) にあります。

- Mozc 辞書・品詞表・接続行列・ローマ字表・文節規則（Google、BSD-3-Clause）。辞書は IPAdic（NAIST）、ICOT Free Software、沖縄辞書（パブリックドメイン）に由来し、それぞれの表示を NOTICE に含めています
- Google Books Ngram v3 由来の英語頻度表（Google、CC BY 3.0。集計は hackerb9/gwordlist）
- SCOWL / ESDB の単語表（Kevin Atkinson、MIT 類似）
- CMU Pronouncing Dictionary（カーネギーメロン大学、BSD）

ShareAlike 条件や商用利用に制限のある出典は使っていません。

## English summary

kswime is a romaji input engine that lets you type Japanese and English in one stream without switching input modes. A joint log-linear lattice over the keystrokes carries Japanese arcs (romaji → kana → Mozc dictionary with POS bigrams and bunsetsu rules), English arcs (lexicon plus a letter n-gram spelling model) and identifier arcs, decoded with Viterbi and forward–backward for a per-keystroke P(English). Japanese spans are converted to kanji by a built-in N-best converter.

It has no dependencies (no crates, no external IME, no runtime files; the model is embedded and read in place), runs fully offline, learns from the user's corrections with perceptron updates on the feature weights, and bridges katakana loanwords and English spellings in both directions (`komitto` → コミット / commit, `server` → サーバー). A macOS Input Method Kit shell, a CLI, a terminal UI and a C ABI are included. Resource budgets (start-up ≤ 300 ms, resident ≤ 120 MB, per-keystroke p99 ≤ 5 ms) are enforced in CI.

Code is MIT; the generated model carries the notices of Mozc (BSD-3-Clause, with IPAdic and other dictionary notices), Google Books Ngram (CC BY 3.0 via gwordlist), SCOWL and CMUdict. See NOTICE.
