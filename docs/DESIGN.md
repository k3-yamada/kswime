> この設計書は判定ロジック kanaswitch（2026-10-25 納品）向けに書いたものです。kswime は同じ判定器を application/judge.rs に移植し、変換器・C ABI・macOS シェルを加えています。評価値は kanaswitch 時点のものです。

# 和英自動判定ロジック 設計書

作成日: 2026-10-25（JST）
対象: ローマ字入力 IME における、打鍵列の日本語／英語の自動判定

## 1. 調査結果の要約

### 1.1 Meltype（yksr-melt/Meltype、v1.2 系、GPL-3.0）

- 判定単位は「区間」で、区切り位置はかな 1 音の境目に限ります。`CompositionDetector.FindSpans()` が左から貪欲に、最長の区間から `IsEnglishSpan()` を試します。
- 英語と判断する根拠は、(a) ローマ字として読めない綴り（`RomajiDetector.Analyze().IsValid`、l/x/v 系は除外）、(b) 手作りの英単語表（english.txt 約 3,000 語、propernouns、english-readable）、(c) Windows では ISpellChecker、Mac/Linux では SCOWL 由来の 39,095 語、(d) カーソル前後の文脈点数（英語 +1、日本語 −1、英文中 +2）です。
- 曖昧語（sushi、make、i など）は文脈点数が閾値 `needed` を超えたときだけ英語になります。「判定の強さ」4 段階はこの閾値と、英語接頭辞を認める条件を変えます。
- 確定後の遡及修正（`CorrectPreviousCommit`、直近 4 語）、F10/F6 による学習（`LanguageMemory`、3,000 語 LRU）、`/ $ @` のシジル語、コード行の判定（`LineContext`）を持ちます。
- Issue #838（2026-10-17）で作者側が構造的限界を認めています。67 件の誤判定報告のうち 20 件を綴り別の規則追加で直しており、「入力全体を日本語として読んだ場合と重みで比べていない」と記しています。寄稿者が試作した `feat/reading-comparison-beta` は、日本語語 1.0、付属語 0.8、英語既知 1.0、未知 1.5/文字、切替 1 という手作りの重みの DP です。
- 典型的な誤判定: pedia→ぺぢあ、tmux→tむx、dlsite→dlして、issue→いっすえ、gameover→がめover、VRChat→Vアールチャット、人名。

### 1.2 Mozc（google/mozc）

- 言語判定の確率モデルはありません。指定コミット c8706c5 は `AggregateEnglishUsingRawInput`（現 `EnglishDecoder::DecodeUsingRawInput`）を QWERTY ローマ字モードに限定するものです。この関数は `composer.GetRawString()` の生打鍵をキーとして同じシステム辞書を前方一致検索し、「ごおgぇ」から「Google」を出します。有効化には `Request.language_aware_input == LANGUAGE_AWARE_SUGGESTION` かつ QWERTY_MOBILE テーブルが必要で、デスクトップ版では呼ばれません。
- `rewriter/language_aware_rewriter.cc` の `IsRawQuery()` が実質の判定規則です。生打鍵が 4 文字以上、かな化後の中間に英字が残る（`ContainsScriptType(key, ALPHABET)`）なら生文字列を候補の先頭に、辞書の読みに無く辞書の表記に生文字列がある（house）なら 3 番目に挿入します。末尾の未確定子音は日本語の入力途中として扱います。
- `composer/mode_switching_handler.cc` は google/chrome/android/http/www. など固定語で英数モードへ切り替えます。Shift 付き英字は一時的に半角英数へ入ります。
- コスト体系は `cost = −500·ln p`。語コストは品詞条件付き、品詞バイグラムは `connection_single_column.txt`（2,672 × 2,672）。未知語はアルファベット連続 1 ノードで 16,383、1 文字 32,767、数字 3,000。候補フィルタは最良から 6,907 超を落とします。
- 英語語彙と英語言語モデルは `SupplementalModelInterface` の裏にあり OSS には含まれません。

### 1.3 azooKey（AzooKeyKanaKanjiConverter）

- 言語は入力です。`KeyboardLanguage { en_US, ja_JP, … }` をホストが `setKeyboardLanguage` で渡し、変換器は判定しません。
- 生打鍵を保持する `ComposingText.input` と、そこから導出する `convertTarget` を分離しています。未確定の末尾英字（"かんs"）はそのまま残り、`TypoCorrectionGenerator` と予測が不安定接尾辞として扱います。
- 英語は同じ LOUDS 辞書に ASCII 読みの項目として入り、`getTopLevelAdditionalCandidate` がローマ字入力中に固定ペナルティ −10 で英語候補を足します。これは判定ではなく候補追加です。
- Zenzai（zenz-v3、llama.cpp）は「カタカナ入力＋候補」を与え、各トークンで argmax が一致するかを確認する草案・審査方式です。入力をカタカナ化するため生のローマ字は見ません。日英判定に使うには、生 ASCII を与える新しいプロンプト形式と対数尤度の比較規則、追加学習が必要です。
- 2025-08-22 のコミット ebaa3172 で、英単語（attack の tt が っt に化ける問題）に対処するグラフ方式の入力管理を削除しています。

### 1.4 その他

- ATOK、Microsoft IME、Hayate、SKK: 大文字（Shift）や専用キーを英語の明示的な合図にします。自動判定の文書化はありません。
- Google 日本語入力「もしかして」候補（2013）: かな化後に英字が残る語を英字候補として提示します。上記 1.2 と同じ仕組みです。
- Apple 日本語入力（ライブ変換）: 英単語を辞書駆動で候補化しますが、規則は非公開です。
- Xerox 特許 US 6,292,772: 語頭・語中・語末に位置制約を付けた n-gram で語全体を敷き詰められるかで言語を判定します。ローマ字表現は romaji 語の 97.5% に一致し、英語語の 1.7% にしか一致しません。両方に一致する語は隣接語の多数決で決めます。
- 短文言語同定（Vatanen 2010）: 5〜21 文字では文字 4〜5-gram のナイーブベイズと absolute discounting が最良です。
- ソフトキーボード向け言語検出（arXiv 2101.03963）: 言語別文字トライグラム、空白を文字として扱う、直近語の再帰重み、2 文字以下の語は前の語に従う、代名詞は無視、判定のキャッシュ。
- Google の翻字キーボード（Hellsten 2017）: 英語を分類せず、生文字列を候補のひとつとして同じスコア尺度で競わせ、差が小さければ生文字列を出します。

### 1.5 調査から得た設計上の結論

1. 公開実装はいずれも「英語にしてよいか」を規則で決め、日本語として読んだ確からしさと重みで比べていません（Meltype #838 の自認、Mozc の `IsRawQuery`）。
2. 一方で、Mozc の品詞付き語コストと接続行列という、日本語の確からしさを測る十分な統計モデルが BSD で公開されています。
3. 「生文字列を候補として同じ尺度で競わせる」（Google 翻字キーボード）と「隣接語の文脈で曖昧語を決める」（Xerox）を、品詞バイグラムの上で統合すれば、規則の積み上げなしに Meltype の主要な誤判定類型を扱えます。

## 2. 新ロジックの定義

### 2.1 問題設定

入力: 打鍵列 x₁…xₙ（英字、空白、数字、記号）。カーソル前の文脈（任意）。
出力: 各打鍵の言語ラベル（日本語／英語／中立）、日本語区間のかな、各打鍵の P(英語)、確信度の高い接頭辞の長さ。

### 2.2 ラティス

位置 0…n の間に次の弧を張ります。コストはすべて Mozc 単位（500 × 自然対数）です。

- 日本語弧 [i, j): `run[i..j]` を Mozc ローマ字表で完全なかな列に変換できる場合、かな読みを辞書で引き、(lid, rid, 語コスト) を持つ弧を最大 4 品詞分張ります。辞書に無い 2 音以上の読みは未知語弧（7,000 ＋ 2,000/音）です。
- 英語弧 [i, j): 英単語辞書にあれば語コスト、無ければ文字トライグラムの綴りコスト＋未知語ペナルティ（5,000 ＋ 400/文字）です。既知語は一般名詞とサ変名詞の 2 品詞、未知語は固有名詞とサ変名詞の 2 品詞で張ります。
- 中立弧: 空白、数字、記号の 1 文字。言語状態を変えずに通過します。
- 未確定弧: 入力中に限り、バッファ末尾 4 打鍵以下の英字列を「入力途中」として通過させます。直前が日本語なら 3,200、英語なら 1,000 です。

### 2.3 遷移

状態は (直前の rid, 直前の言語, 直前が空白か, 連続英単語数, 直前英単語が大文字始まりか) です。

- 日本語弧へ: 接続行列 conn(rid, lid)。英語からの切替は +1,800。文頭の付属語（助詞・助動詞・接尾・非自立）は +4,000。ん・っ・小書きで始まる読みが日本語の続きでない位置にあれば +6,000。大文字始まりの英単語に空白なしで続く自立語は +6,000。
- 英語弧へ: 日本語からは conn(rid, 名詞) +1,800。英語から空白なしは +4,500（glue）、空白ありは直前 1 語なら +2,500、英文中（2 語以上）なら +300。

### 2.4 弧コストに足す特徴

- 日本語弧: 大文字 1 打鍵につき +4,000、l/v/x/q 1 文字につき +2,500、th/dh +2,500、ぢ/づ/ゔ +2,000、識別子文脈（@ _ / : \ + を含むトークン、英字.英字、数字に接する単独英字）+20,000、カタカナ表記しか無い語で打鍵が英単語でもある場合 +4,500 × 英語側の確からしさ。
- 英語弧: 大文字始まり（2 文字以上）−2,500、2 文字目以降の大文字（全大文字を除く）+2,500/文字、小文字 1 文字の既知語 +3,000、母音も大文字も含まない未知語は不許可、利用者バイアス（±2,500 × 訂正回数、上限 ±7,500）。
- 大域: `en_bias`（判定の強さ。Aggressive −1,500、Balanced 0、Conservative +2,000）。

### 2.5 推論

- Viterbi で最良経路を求め、区間と各区間の実コストを返します。
- 同じラティスで前向き後ろ向きを計算し、英語弧の事後確率の和を各打鍵の P(英語) とします。未確定弧は直前打鍵の値を引き継ぎます。
- `stable_prefix(θ)` は、P(英語) が θ 以上または 1−θ 以下である打鍵が先頭から続く長さです。ホストはここまでを先行確定できます。
- `commit()` は最良経路の終端状態（rid、言語、空白、連続英単語数）を次バッファの初期状態にします。ホストから surrounding text を渡す場合は `set_context_from_text()` が末尾の文字種と英単語数から初期状態を作ります。

### 2.6 学習

`feedback(token, lang)` は小文字化した打鍵列をキーに ±2,500 を加算します。英語弧にはそのまま、日本語弧には符号を反転して足します。同じ語を 3 回訂正すると約 e^{15} の確率比になり、辞書の重みをほぼ覆します。

## 3. 評価

| セット | 件数 | 正解率 | 備考 |
|---|---|---|---|
| 独自テスト（tests/data/cases.json） | 122 | 98.4% | 日本語、英語、混在、曖昧語＋文脈、識別子、記号、大文字 |
| Meltype QualityTests（外部） | 216 | 92.1% | Meltype 同梱の期待値。評価のみに使用し再配布しない |
| 合成 mixed_bench | 600 | 99.5% | 日本語＋読めない英単語＋日本語 |
| 合成 ja_bench | 1,000 | 98.8% | 日本語単語＋助詞。誤英語化率 1.2% |

Meltype 側の残り 17 件の内訳: 文脈なし曖昧語を日本語既定にする期待（same、mine、repo）、英単語間の と 挿入（devtoprod、dottonet など）、pedia 系の接尾辞規則、Japanese 語の空白打ち（kore ha pen desu）です。と 挿入と -pedia は Meltype が個別規則で扱う類型で、本ロジックでは一般原則を優先して対応していません。

主要パラメータ（切替コスト、glue、未知語ペナルティ、ローン語ペナルティ）を ±50% 動かしても 4 セット合計の正解率は 1% 未満しか動かず、既定値は平坦な領域にあります。

処理時間は 21 打鍵で 0.3〜0.9 ms（release、単一スレッド）です。モデルは起動時に約 1 秒で読み込み、メモリは約 210 MB です。

## 4. 既知の限界と次の段階

1. 日本語側の未知語モデルが粗い。Mozc と同じく文字種ベースですが、固有名詞の読みに弱いです。読みの文字 n-gram を Mozc 辞書から学習して置き換える余地があります。
2. 英語の語彙が字幕由来で、技術用語は `tools/en_extra.txt` の手作業補完に頼っています。ホスト側のユーザー辞書や、Meltype のように OS のスペルチェッカーを英語弧の追加ソースにできます。
3. 文脈の持ち越しは直前 1 語の品詞に限ります。確定済みテキストを形態素解析して rid を得れば、文頭の付属語規則をより正確にできます。
4. 英単語同士の と 挿入（dev と prod）は、英語弧の後の と を他の助詞より高くする品詞側の調整で扱えますが、根拠となる統計がないため未実装です。
5. モデル容量は約 45 MB です。読みを上位品詞 1 件に絞り、接続行列を品詞クラスタで圧縮すれば 10 MB 以下にできます。
6. LLM 化の経路として、Zenzai の `ZenzContext.getLogits` に生 ASCII を与えるプロンプトを追加し、本ロジックの N-best を再採点する構成が自然です。本ロジックは候補生成と事後確率を担い、LLM は曖昧語の文脈判断だけを担います。

## 5. 参照

- Meltype: https://github.com/yksr-melt/Meltype （docs/WINDOWS.md、src/Meltype.Core/Composition/CompositionDetector.cs、Issue #838）
- Mozc: https://github.com/google/mozc/commit/c8706c50f348a40c7bf77bdb07dd2f34154f3600 、src/rewriter/language_aware_rewriter.cc 、src/prediction/english_decoder.cc
- azooKey: https://github.com/azooKey/AzooKeyKanaKanjiConverter （Sources/KanaKanjiConverterModule/、Docs/conversion_algorithms.md、Docs/zenzai.md）
- Xerox 特許 US 6,292,772: https://image-ppubs.uspto.gov/dirsearch-public/print/downloadPdf/6292772
- Vatanen et al., LREC 2010: https://preview.aclanthology.org/menus/L10-1193/
- Language Detection Engine for soft keyboards: https://arxiv.org/abs/2101.03963
- Hellsten et al., 2017（翻字キーボード）: https://preview.aclanthology.org/fix_video/W17-4002.pdf
- Google 日本語入力「もしかして」候補: https://forest.watch.impress.co.jp/docs/news/20131111_623062.html
