# kswime

半角/全角キーを押さずに日本語と英語を混ぜて打てる、ローマ字入力エンジンです。
何にも依存しません。外部クレートなし（`cargo tree` は自分自身だけ）、外部 IME なし、実行時に読むファイルなし。
辞書と統計モデルは実行ファイルに埋め込み、かな漢字変換も自前で行います。

```
mitsumoriwoexceldetsukutta  →  見積もりをexcelで作った
gitlabnimergeshita          →  gitlabにmergeした
kaigihazoomdeyarimasu       →  会議はzoomでやります
Please review it            →  Please review it
はやく + kitekudasai          →  来てください
```

## 構成

Clean Architecture の 4 層です。詳細は [ARCHITECTURE.md](ARCHITECTURE.md)。

| 層 | 内容 | 場所 |
|---|---|---|
| domain | ローマ字規則、値オブジェクト、ポート（trait） | `src/domain/` |
| application | 判定（結合ラティス）、変換（N-best Viterbi＋文節境界）、入力セッション | `src/application/` |
| infrastructure | 埋め込みモデル、モデル生成、学習の永続化 | `src/infrastructure/` |
| presentation | CLI、端末 UI、C ABI | `src/presentation/`、`include/kswime.h` |
| macOS | Input Method Kit シェル（Swift Package、Xcode 不要） | `macos/` |

判定ロジックの設計根拠（Meltype・Mozc・azooKey の調査と新方式）は [docs/DESIGN.md](docs/DESIGN.md) にあります。

## ビルド

```sh
tools/fetch_data.sh                                            # 公開データを data/raw/ へ
cargo run --release --bin kswime-build-model -- data/raw data/model.bin   # 約 10 秒、63 MB
cargo build --release                                          # モデルを埋め込んだ単体バイナリ
cargo test --release
```

配布物には生成済みの `data/model.bin` を同梱しています。生データを取り直さなくても `cargo build --release` だけで動きます。

macOS の IME として使う場合は `macos/build.sh --install` です（[macos/README.md](macos/README.md)）。

## 使い方

```sh
kswime                          # 対話入力（Unix 端末）
kswime convert "mitsumoriwoexceldetsukutta"
kswime judge "cursordecommitsuru"       # 区切り、候補、打鍵ごとの P(英語)
kswime stream "kaigihazoomdeyarimasu"   # 1 打鍵ずつの推移
echo "mitsumorishowookutte" | kswime    # 行モード
kswime eval tests/data/cases.tsv -v     # 訂正打鍵数と正解率
kswime eval tests/data/cases.tsv --online --weights w.tsv   # 採点しながら学習し、重みを保存
kswime replay typing-log.tsv --online   # 記録の再生（学習ありで利用者の体験を再現）
kswime weights                           # 重みの一覧と説明
kswime bench tests/data/cases.tsv        # 起動時間、常駐メモリ、1 打鍵の判定遅延 p50/p99。--budget で上限超過を終了コード 1 に
```

対話入力のキー: Enter 確定、Tab / Shift+Tab 候補の巡回、Ctrl+L 直前の語の日英を反転して学習、Ctrl+E 判定の理由を表示、Ctrl+U 取消、Ctrl+C 終了。表示は確信度で変わります（黄: 暫定、淡色: 未決）。
`--weights FILE` と `--learn FILE` で重みと語ごとの訂正を保存・復元し、`--record FILE` で打鍵を記録します。`--strength` と `--set 名前=値` は重みを直接変えます。

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

`Piece` は `lang`（Ja / En / Neutral）、`text`、日本語なら `kana` と `candidates` を持ちます。他言語からは `include/kswime.h` の C ABI（`cargo build --release` で `libkswime.a` / `.so` / `.dylib`）を使います。

## 判定器の重みと学習

判定器は対数線形モデルです。ラティスの各弧と遷移は特徴ベクトルを持ち、コストは重みとの内積です（`kswime weights` で一覧）。重みの初期値は次の規則で置いています。

- 辞書コストと品詞バイグラム: 倍率 1.0
- モデルが成立するために必要な構造コスト（切り替え、未知語、辞書外英語、未決の末尾）: 事前に置いた定数
- それ以外の経験則（大文字、識別子風、カタカナ語、英語間の空白、文節境界 など）: 0

0 の重みは利用者の操作で学習されます。F6/F10 やトグルで語の言語を直すと、直した経路を正解としてパーセプトロン更新が走り、同じ傾向の入力が次から直ります。`--weights FILE` で保存・復元し、macOS 版は `~/Library/Application Support/Kswime/weights.tsv` に自動保存します。学習は「学習をすべて消去」で初期値に戻せます。

識別子（`@ / : _` を含む語、`example.com` 型のドメイン、`i18n` 型）は重みではなく、独立した生成源としてラティスに入ります。

句読点は直前の語ではなく文に従います。日本語の文の中の英単語の後でも `cpu,memori` → `cpu、メモリ` です。英語だけの文（`Please review it, thanks.`）は半角のまま、`10.5` の小数点と `1,000` の桁区切りも変えません。

シークレットモード（macOS のメニュー、エンジンでは `learning_paused`）では、訂正はその場の入力にだけ効き、語の記憶、重み、打鍵記録のどれにも残りません。画面共有や他人の文章を打つときに使います。

## カタカナ語と英語の橋渡し

CMU 発音辞書から英単語のカタカナ読みを生成し、双方向の候補にしています。

- `komitto` → コミット（第 1 候補）、commit（候補）。`komittosuru` → コミットする、候補に commitする。
- `guuguru` のように日本語辞書に無い読みは英語綴り（google）に変換し、候補にグーグルを添えます。
- `server` と英語で打てば、候補にサーバーが出ます。Tab で切り替えられます。

カタカナ形は 3 系統から集めます。(1) CMU 発音辞書の発音から生成した形（ヴ→バ、ファ→ハ の旧来表記も派生させる）、(2) 綴りから規則で生成した形（発音辞書に無い gitlab、figma、terraform などの固有名詞のため）、(3) それらと読みが一致する Mozc 辞書の実在カタカナ語。利用者に見せるのは (3) の実在形を優先し（coffee → コーヒー、server → サーバー、version → バージョン）、Mozc の頻度順に並べます。(1)(2) だけの形は辞書に無い読みの受け皿として残します。

照合は長音とヴ／バ、ティ／チ の差を吸収しますが、ファ／ハ は吸収しません（フィグマ と ヒグマ を区別するため。コーヒー は派生形で届きます）。

## 資源の上限

`kswime bench` が起動時間、常駐メモリ、1 打鍵の判定遅延（judge＋compose）を測り、上限（起動 300 ms、常駐 120 MB、p99 5 ms）を超えると `--budget` 付きで終了コード 1 を返します（CI で使います）。現状（Linux x86-64、独自 117 件）は起動 0.16 秒、常駐 107 MB、p99 3.4 ms で、上限内です。モデルは埋め込みバイトをその場で参照し（文字列の複製なし、索引だけを構築）、辞書項目は 24 バイトの値です。ゼロコピー化の前は起動 0.42 秒、常駐 327 MB でした。

## 評価

主指標は「訂正打鍵数／1,000 文字」です。予測を期待どおりにするのに必要な言語反転の回数と候補送りの回数を数えます。正解率は副指標です。

| セット（学習前の初期重み） | 件数 | 訂正／1,000 字 | 完全一致 |
|---|---|---|---|
| tests/data/cases.tsv（独自） | 117 | 9.2 | 88.0% |
| tests/data/mixed_bench.tsv（合成混在文） | 600 | 1.0 | 98.2% |
| tests/data/ja_bench.tsv（日本語＋助詞、誤英語化の検査） | 1,000 | 1.9 | 99.5% |
| tests/data/convert.tsv（かな漢字変換） | 50 | 32.9 | 84.0% |

学習の効果は再生で測ります。独自テスト 117 件を 5 周ぶん打った記録（585 行、9,745 字）を `kswime replay` にかけると、学習なしで 9.2、`--online` で 3.9（100 行ごとの推移 7 → 5 → 4 → 4 → 2 → 2）でした。これは同じ文を繰り返す理想条件の値で、実際の文章ではこれより緩やかです。

### 評価の原則と開発経緯

他製品に同梱されたテスト集合は、採点にも重みの調整にも使っていません。初期重みは上の規則だけで決め、経験則の重みは 0 から利用者の操作で学習されます。設計の着手段階では Meltype の公開リポジトリと同梱テストを調査し、どの入力が難しいかを把握しました。その知見は特徴量の「種類」（大文字、識別子、カタカナ語、英語間の空白など）に反映されていますが、重みの「値」には反映されていません。

独自の評価には `tests/data/` の形式（カテゴリ、入力、期待、前文脈のタブ区切り）か、対話入力の `--record` で得た記録を `kswime replay` にかけてください。

## 設計上の判断

- モデルは Mozc の読み辞書（コスト 8,500 以下、読みごとに表記 6 件＋品詞ごとの最安 1 件）、接続行列（16 bit、量子化なし）、文節境界規則（boundary.def、segmenter.def）から生成します。
- 英語辞書は Google Books Ngram（CC BY 3.0、hackerb9/gwordlist 経由）の上位 8 万語です。4 文字以下の語は SCOWL/ESDB（size 70 まで、MIT 類似）の語かその活用形に限り、OCR 由来の雑音を除いています。書籍コーパスに乏しい技術語・口語・短縮形は `data/raw/en_extra.txt`（自作）で補います。ShareAlike 条件や商用利用に制限のある出典は使っていません。
- 変換器は Mozc と同じ文節境界ペナルティを使います。それでも残る同音異義語の誤り（きのう→機能／昨日、かんじ→感じ／漢字）は、Mozc OSS 辞書のコストそのものに由来し、文脈なしでは Mozc も同じ順位です。候補巡回と学習で補います。
- バイナリは約 63 MB です。縮小したい場合は `kswime-build-model --max-ja-cost 7000 --max-surfaces 3` で約 32 MB まで落とせます。

## ライセンスと出典

コードは MIT License です。`data/model.bin` は次の公開データから生成しています。NOTICE を参照してください。

- Mozc 辞書・品詞表・接続行列・ローマ字表・文節規則（Google、BSD-3-Clause）。辞書は IPAdic（NAIST）、ICOT Free Software、沖縄辞書（パブリックドメイン）に由来し、それぞれの表示を NOTICE に含めています
- Google Books Ngram v3 由来の英語頻度表（Google、CC BY 3.0。集計は hackerb9/gwordlist）
- SCOWL / ESDB の単語表（Kevin Atkinson、MIT 類似）
- CMU Pronouncing Dictionary（カーネギーメロン大学、BSD）
