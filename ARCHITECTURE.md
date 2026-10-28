# kswime アーキテクチャ

Clean Architecture（Robert C. Martin）の依存性の規則に従います。依存は常に内側へ向かい、内側の層は外側を知りません。

```
presentation    cli / tui / ffi                  argv、stdin、C ABI（Swift IMK シェルが呼ぶ）
infrastructure  model / model_builder / files    domain のポートを実装
application     judge / convert / session        ユースケース
domain          romaji / composition / ports     規則と値オブジェクト。I/O なし
```

`grep -rn "crate::infrastructure\|crate::presentation" src/domain src/application` が空であることを、依存方向の検査として CI に置けます。

## 層ごとの内容

### domain

- `romaji.rs` ローマ字→かなの規則。表は外から与えます（`RomajiTable::from_tsv`）。
- `transliterate.rs` 英語の発音（ARPAbet）→カタカナの規則と、照合用の緩い正規化。
- `composition.rs` 値オブジェクト。`Lang`、`Segment`、`Analysis`、`Piece`、`Candidate`、`Context`。
- `ports.rs` ポート（trait）。`JapaneseLexicon`、`PosGrammar`（接続行列、文節境界、付属語判定）、`EnglishLexicon`、`LoanwordBridge`（カタカナ読み↔英語綴り）、それらを束ねる `Knowledge`、学習の永続化 `CorrectionStore`。

### application

- `judge.rs` ユースケース「打鍵列を判定する」。対数線形ラティス（弧と遷移が疎な特徴ベクトルを持つ）の Viterbi と前向き後ろ向き。制約付き Viterbi（利用者が強制した言語、期待文字列に一致する経路）で正解経路を復元します。識別子は独立した生成源です。`&dyn Knowledge` だけに依存します。
- `weights.rs` 特徴の定義（名前、種類、初期値、説明）と重み。構造設定 `Config`、強さのプリセット。
- `learn.rs` ユースケース「重みを学習する」。予測経路と正解経路の特徴差によるパーセプトロン更新。
- `convert.rs` ユースケース「かな列を漢字に変換する」。N-best Viterbi。Mozc の文節境界ペナルティ（`boundary.def` の PREFIX/SUFFIX と `segmenter.def` の境界規則）を使います。
- `session.rs` 対話子 `InputSession`。バッファ、前後の文脈、候補選択、言語の強制と学習、確信度付きの `Piece`、判定理由の説明。フロントエンドが駆動します。

### infrastructure

- `model.rs` 埋め込みバイナリモデル。`Knowledge` を実装します。バイト列をその場で参照し（`JaEntry<'a>` の表記、英語綴り、カタカナ形はすべてモデルのバイト列への `&str`）、接続行列と境界ビットも複製しません。構築するのは読み・英単語・橋渡しの平坦な整列索引だけで、検索は二分探索です。HashMap は使いません。
- `model_builder.rs` モデルの書き出しと、Mozc 規則ファイルの小さなパターン照合器。
- `correction_file.rs` 学習結果のファイル保存（`CorrectionStore`）とテスト用のメモリ実装。

### presentation

- `cli.rs` `convert` / `judge` / `stream` / `eval` / 行モード。
- `tui.rs` 端末の対話入力。
- `ffi.rs` C ABI。`include/kswime.h` と対応します。Swift 側は `macos/`。

## 境界を越えるデータ

層境界は `Piece` と `Analysis`（値オブジェクト）で越えます。C ABI では不透明ハンドル `KswEngine` と UTF-8 文字列だけを渡し、Rust の型は見せません。panic は FFI を越えません（各関数は値の取得と整数の返却だけで、失敗は負値や空文字で表します）。

## テスト戦略

- ユニット: `domain::romaji` の表駆動テスト。
- ユースケース: `tests/engine.rs` は埋め込みモデルを `Knowledge` として使いますが、同じテストは `Knowledge` を手書き辞書で実装すれば外部データなしで動きます。
- 回帰: `tests/data/*.tsv` を `kswime eval` で採点。
- FFI: `tests/c/smoke.c` を `cc` で静的リンクして実行。

## 文節境界モデル（1 の対応）

Mozc は語コストと接続コストに加えて、文節の先頭・末尾に品詞別のペナルティを足します（`boundary.def`）。境界の有無は `segmenter.def` の規則で決めます。kswime はこの 2 ファイルをビルド時に解釈し、`PosGrammar::is_boundary / prefix_penalty / suffix_penalty` として変換器に供給します。判定器側では特徴 `BOUNDARY`（初期重み 0）として入り、学習で重みが決まります。

接続行列は 8 bit 量子化をやめ、Mozc と同じ精度（16 bit）で保持します。量子化は同音異義語の順位を変えていました。

## 第 2 世代の骨格（対数線形モデル）

第 2 世代開発計画の 0 節を実装した形です。各アイデアは特徴関数として `weights.rs` に名前と初期値を持ち、`judge.rs` が弧・遷移に値を付け、`learn.rs` が利用者の操作から重みを動かします。新しい情報源（打鍵リズム、符号長、アプリ事前分布）は、特徴を 1 つ追加し、`Lattice` の弧生成か遷移で値を付けるだけで組み込めます。評価は `presentation/cli.rs` の `correction_keystrokes`（訂正打鍵数）と `replay` が担います。
