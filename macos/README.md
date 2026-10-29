# Kswime for macOS

Input Method Kit（IMK）用のフロントエンドです。Swift Package だけで構成し、Xcode プロジェクトも外部パッケージも使いません。
Rust エンジン（`../target/universal/release/libkswime.a`）を静的リンクし、C ABI（`../include/kswime.h`）経由で呼びます。

## ビルドとインストール

```sh
xcode-select --install          # 未導入なら
rustup target add aarch64-apple-darwin x86_64-apple-darwin
macos/build.sh --install        # 構築、~/Library/Input Methods へ配置、入力ソース登録
```

`--register` は `TISRegisterInputSource` と `TISEnableInputSource` を呼び、`TextInputMenuAgent` を再起動します。ログアウトは不要です。
初回に限り「システム設定 > キーボード > 入力ソース」で Kswime を追加してください。

配布するときは Developer ID で署名し、notarize してください（`codesign --options runtime --timestamp`、`notarytool submit`、`stapler staple`）。Mac App Store では IME を配布できません。

## 操作

| キー | 動作 |
|---|---|
| 英字 | 入力。日本語と判定された区間は漢字に、英語はそのまま表示 |
| Space | 確定時の判定で直前が日本語なら候補を開く／次候補、英語なら空白 |
| ← / → | 直す語を選ぶ（編集モード。選択中の語は反転表示）。一続きの日本語が複数の文節に分かれているときは、まず文節を順に渡り、端で隣の語に移る。確定はしない |
| Shift+← / Shift+→ | 選択中の「文節または語」の右端を動かす。日本語で最後でない文節なら、その右の文節の境界をかな 1 つ分、動かす。それ以外（英語の語、日本語の最後の文節）は、日英の境界を 1 打鍵ずつ縮める／伸ばす。日英の境界は、押すたびには学習せず、確定するか次の文字を打った時点で、最後の境界だけを 1 回学習する。Backspace と Esc で取り消すと学習しない |
| Option+Shift+← / Option+Shift+→ | 日本語の選択中の文節の右端を、かな 1 つ分、前／後ろへ動かす。最後の文節でも、右端の 1 つ前に新しい文節の境界ができる。文節の境界は学習しない |
| ↑ / ↓、Tab / Shift+Tab | 選択中の語の候補を巡回 |
| 1〜9 | 選択中の語の候補を番号で選ぶ |
| Home / End | 先頭／末尾の語を選ぶ |
| Enter | 全体を確定 |
| Esc | 入力取消 |
| Backspace | 編集モード中は編集を抜ける。入力中は 1 打鍵削除 |
| F6 | 選択中の語（既定は直前の語）をひらがなに。英語と判定された語は日本語に直してからひらがなにする（この直しは学習する） |
| F7 | 選択中の語をカタカナに |
| F10 | 選択中の語を英語に直す。語の記憶と重みの両方が学習する |
| 英数 / かな（JIS） | 英数モード／自動モード |
| ⌘⇧J / ⌘⇧; | 自動／英数（Info.plist の tsInputModeKeyEquivalentKey） |

編集モードで英字を打つと、そのまま続きの入力になります（選んだ候補は保たれます）。日本語の一続きの中の文節の区切りは、変換器が最良の候補について決めます。利用者が動かした境界は「必ず区切る」位置として変換器に渡り、確定するか取り消すまで保たれます。ただし、変換器は文法が区切る位置を「つなぐ」ことはできないため、境界を右へ動かしても、文法の境界が残ることがあります。

入力メニューから、判定の強さ、候補の常時表示、判定理由の表示、シークレットモード（学習・語の記憶・打鍵記録を一時停止。訂正はその場の入力にだけ効く）、打鍵の記録（ローカル、`kswime replay` 用、既定オフ）、学習の消去、現在のアプリを常に英数にする設定（既定: Terminal、iTerm2、VS Code）を変更できます。
マークテキストの装飾は確信度を表します（確信: 細い下線、暫定・未決: 点線、未決はさらに淡色）。
学習データは `~/Library/Application Support/Kswime/`（weights.tsv、corrections.tsv、typing-log.tsv）に平文で置かれ、送信機能はありません。

## ファイル

| ファイル | 役割 |
|---|---|
| `Sources/KswimeIME/InputController.swift` | IMKInputController。キー処理、マークテキスト、周辺テキスト、モード |
| `Sources/KswimeIME/Engine.swift` | C ABI の Swift ラッパ |
| `Sources/KswimeIME/CandidatePanel.swift` | 候補ウィンドウ（NSPanel、borderless、nonactivating） |
| `Sources/KswimeIME/Settings.swift` | `~/Library/Application Support/Kswime/` の設定と学習の永続化 |
| `Sources/KswimeIME/InputSourceRegistration.swift` | 入力ソースの登録と有効化 |
| `Sources/CKswime/` | C ヘッダと module map |
| `Resources/Info.plist` | IMK の宣言（接続名、モード、JIS キー、アイコン） |

## 既知の制約と未検証事項

- このディレクトリは macOS 上でのビルドを前提とし、Linux 上で生成したため実機でのコンパイルと動作は未検証です。IMK の API 名は Apple の文書と azooKey-Desktop、macSKK、Meltype の実装に合わせています。
- Secure Keyboard Entry（Terminal の「セキュリティキーボード入力」）が有効なアプリでは、`~/Library/Input Methods` の IME は無効化されます。`/Library/Input Methods` へ入れると回避できます。
- Chromium 系（Chrome、VS Code、Electron）は 1 回のキーイベントで最後の `setMarkedText` しか反映しません。本実装は 1 キーにつき 1 回しか呼びません。
- 確定後の遡及修正（Meltype の re-commit）は未実装です。`ksw_set_context` で前方文脈は使いますが、確定済み文字の置換は行いません。
- 候補の巡回（Tab、↑↓、数字）は、文節ごとではなく、一続きの日本語の全体を単位にします。文節をまたぐ候補（例: commitする）も候補に出ます。文節ごとに候補を選ぶことは、まだできません。
