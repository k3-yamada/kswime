# Windows 版の設計

macOS 版（`macos/`）と同じく、エンジンの C ABI の上に薄い入力メソッドのシェルを載せる。Windows の入力メソッドは Text Services Framework（TSF）のテキストサービス（COM の DLL）である。

状態: **設計のみ。シェルは未実装**。実機（Windows）で動かせる状況になってから実装する。ここでは、エンジンが Windows でビルドできることの確認と、シェルの設計を記す。

## 1. 確認できたこと

| 項目 | 結果 | 確認方法 |
|---|---|---|
| エンジン（lib と bin 2 本）が `x86_64-pc-windows-msvc` で型検査を通る | 通る（エラー 0） | `cargo check --target x86_64-pc-windows-msvc --lib --bins --release`（macOS 上） |
| Unix 専用のコード | 2 か所だけ。いずれもエンジン本体ではない | `src/presentation/tui.rs`（端末の対話入力）、`src/presentation/cli.rs:369`（`bench` の常駐メモリ計測が `/proc/self/statm`） |
| MSVC でのリンクとテスト | **通る**。`cargo build --release --lib --bins` と `cargo test --release`（統合テスト 37 件）が成功し、`kswime.dll`（約 63 MB）と `kswime.dll.lib` ができる | 公開側の CI `engine (Windows, MSVC)` ジョブ（2026-10-28、`windows-latest`） |
| C ABI のスモークテスト（`tests/c/smoke.c`） | **Windows では未実施**（`cl.exe` の環境変数の準備が要る） | 第 2 段階までに足す |

`kswime.lib`（`staticlib`）と `kswime.dll`（`cdylib`）は、MSVC では `libkswime.a` / `libkswime.dylib` と名前が違う。CI はこの 3 ファイル（`kswime.lib`、`kswime.dll`、`kswime.dll.lib`）ができることまで確かめる。

## 2. 構成

```
kswime/                      エンジン（外部クレートなし。cargo tree は自分自身だけ）
windows/                     Windows シェル（別の crate。windows crate に依存してよい）
  Cargo.toml                 crate-type = ["cdylib"]、kswime = { path = ".." }
  src/                       COM の実装、候補窓、登録
```

- エンジンの「依存ゼロ」を守るため、シェルは別の crate にする。ルートの `Cargo.toml` に `windows` crate は入れない。
- シェルからエンジンを呼ぶ方法は 2 つある。**既定は Rust の `rlib` を path 依存で直接呼ぶ**（`kswime::InputSession`）。ヘッダや `.lib` の名前の違いを吸収できる。C ABI（`kswime.lib`）経由にする理由が出たら切り替える。
- `windows/` は、macOS 版の `Engine.swift`（C ABI の薄いラッパ）に当たる層を持たず、`InputSession` を直接持つ。macOS 版の `InputController.swift` が持つ状態（`candidateMode`、`focus`、`lastPieces`）は、そのままの形で移す。

## 3. 実装するもの（TSF）

出典は Microsoft の公式ドキュメント（末尾）。

| 役割 | インターフェース | 備考 |
|---|---|---|
| COM サーバー | `IClassFactory`、`DllGetClassObject`、`DllCanUnloadNow` | インプロセスの DLL |
| 入力メソッド本体 | `ITfTextInputProcessorEx`（`ActivateEx` / `Deactivate`） | 起動時に各シンクを登録 |
| キー入力 | `ITfKeyEventSink`（`OnTestKeyDown` / `OnKeyDown`） | `OnTestKeyDown` で「このキーを自分が食うか」を先に答える |
| 変換中の文字 | `ITfContext` → `ITfContextComposition::StartComposition`、`ITfRange::SetText` | すべて `ITfEditSession` の中で行う |
| 取り消し通知 | `ITfCompositionSink`（`OnCompositionTerminated`） | フォーカス喪失などで強制終了されたとき |
| 表示属性 | `ITfDisplayAttributeProvider`（と `ITfDisplayAttributeInfo`） | 確信度の下線、選択中の語の強調 |
| 周辺テキスト | `ITfContext::GetSelection`、`ITfRange::GetText` | `ksw_set_context` 相当（前後の文脈）に使う |
| フォーカス追跡 | `ITfThreadMgrEventSink` | 文書の切り替えで未確定を確定する |

最小構成では、言語バー（`ITfLangBarItemButton`）とモードの Compartment は省く。モードは macOS 版と同じく、英数/かなキーと入力プロファイルの切り替えで足りるかを、実装時に確かめる。

### macOS 版との対応

| macOS（`InputController.swift`） | Windows（TSF） |
|---|---|
| `handle(_:client:)` | `ITfKeyEventSink::OnKeyDown`（`OnTestKeyDown` で食うかを先に判定） |
| `setMarkedText` + `selectionRange` | 編集セッション内で `ITfRange::SetText`。選択中の語は表示属性で強調 |
| 確信度の下線（`mark(forStyle:)`） | `ITfDisplayAttributeInfo`（実線、点線、淡色） |
| `commitIfNeeded` → `insertText` | 編集セッション内でコンポジションを終了し、確定文字列を挿入 |
| `CandidatePanel`（NSPanel） | 独立ウィンドウ（下記 4） |
| `Settings`（`~/Library/Application Support/Kswime/`） | `%APPDATA%\Kswime\`（下記 6） |

## 4. 候補窓

- 候補の描画は、自前の独立ウィンドウにする。`WS_POPUP | WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE` でフォーカスを奪わず、位置は編集セッション内で `ITfContextView::GetTextExt` から得た矩形に合わせる。
- macOS 版と同じく、窓の最上段に「何番目/語数　読み」の見出しを出す。アプリによっては、変換中の文字の強調が描画されないため。
- `ITfUIElementMgr`（`ITfCandidateListUIElement`）は、標準の候補窓を描画してくれるものではない。ホストが候補窓を自前描画する場合（ゲームや一部の UWP）に委ねるための仕組みなので、第 2 段階で併用を検討する。
- Per-Monitor DPI（`GetDpiForWindow`）と、画面端での見切れ回避が要る。

## 5. 登録と配布

- `DllRegisterServer` / `DllUnregisterServer` を export し、`HKCR\CLSID\{…}\InProcServer32` に DLL のパスと `ThreadingModel=Apartment` を書く。MSVC では export 名を確実にするため、`build.rs` から `.def` ファイルを渡す。
- プロファイルは `ITfInputProcessorProfileMgr::RegisterProfile` で、日本語（`LANGID 0x0411`）として登録する。カテゴリは `GUID_TFCAT_TIP_KEYBOARD`、`GUID_TFCAT_DISPLAYATTRIBUTEPROVIDER`、ストアアプリ対応の `GUID_TFCAT_TIPCAP_IMMERSIVESUPPORT` と `GUID_TFCAT_TIPCAP_SYSTRAYSUPPORT`。
- **32 ビットと 64 ビットの両方の DLL が要る**。64 ビットだけだと、32 ビットのアプリで入力メソッドが無効になる。配布物は `x86_64-pc-windows-msvc` と `i686-pc-windows-msvc` の 2 つ。ARM64 は第 2 段階。
- 署名: 個人利用や開発では無署名または自己署名でも動くが、配布時の SmartScreen の警告回避や、保護されたプロセスでの読み込みには Authenticode 署名が要る。

## 6. 学習データとストアアプリ

- モデルは約 60 MB を実行ファイルに埋め込むので、辞書ファイルの配置は要らない（`ALL APPLICATION PACKAGES` の読み取り権限の問題が起きにくい）。
- 重み、訂正、記録（`weights.tsv`、`corrections.tsv`、`typing-log.tsv`）は `%APPDATA%\Kswime\` に置く想定。ただし、ストアアプリ（AppContainer）の中では、ここへの書き込みが拒否される可能性がある。**推測**。実機で確かめ、ストアアプリでは保存しない、または ACL を設定する。
- シークレットモード、学習の消去、`discard_pending_learning` の呼び出しは、macOS 版と同じ契約で実装する（`macos/README.md` の操作表）。

## 7. 段階

1. **第 0 段階（済）**: 型検査が通ることの確認。CI に `engine (Windows, MSVC)` ジョブを追加。
2. **第 1 段階（済）**: CI の Windows ジョブが通ることの確認（リンクとテスト）。公開側 `kswime` で通った（2026-10-28）。
3. **第 2 段階**: `windows/` の骨組み（COM の DLL、登録、キー入力を食って文字を確定するだけ）。実機で、メモ帳に打てるところまで。
4. **第 3 段階**: 変換中の文字の表示、候補窓、編集モード（← / →、Shift+← / →、F6/F7/F10）。
5. **第 4 段階**: 32 ビット版、署名、インストーラー、ストアアプリ対応。

第 2 段階以降は、Windows の実機で動かして確かめる必要がある。この Mac からは、ビルドの確認までしかできない。

## 8. 参考にする実装（要ライセンス確認）

次の公開リポジトリが、TSF の入力メソッドを実装している。**ライセンスは未確認**（GitHub の API では判定できなかった。各リポジトリの `LICENSE` を直接読むこと）。**コードを読む前に確認し、ライセンスが MIT と両立しないものは、構成の参考にとどめて、コードは読まない／写さない**。

| リポジトリ | 言語 | 備考 |
|---|---|---|
| `fkunn1326/azooKey-Windows` | Rust | Rust と `windows` crate による TSF の日本語入力 |
| `akaza-im/akaza-ime` | Rust | Rust のエンジンを TSF で包む |
| `chewing/windows-chewing-tsf` | Rust | 登録と UI の完成度が高い（と調査では言われた） |
| `google/mozc`（`src/win32/tip`） | C++ | 商用品質の TSF 実装。Mozc は本エンジンの辞書の出典でもある |
| `EasyIME/PIME`、`rime/weasel` | C++ | 参考 |

`windows` crate は、crates.io の最新の安定版が 0.62.2（2026-10-28 に確認）。必要な features の目安は `implement`、`Win32_Foundation`、`Win32_System_Com`、`Win32_UI_TextServices`、`Win32_UI_WindowsAndMessaging`、`Win32_UI_Input_KeyboardAndMouse`、`Win32_Graphics_Gdi`、`Win32_System_Registry`、`Win32_System_LibraryLoader`（調査による。実装時に最小化する）。

## 出典

- Text Services Framework: https://learn.microsoft.com/en-us/windows/win32/tsf/text-services-framework
- `ITfTextInputProcessorEx`: https://learn.microsoft.com/en-us/windows/win32/api/msctf/nn-msctf-itftextinputprocessorex
- `ITfKeyEventSink`: https://learn.microsoft.com/en-us/windows/win32/api/msctf/nn-msctf-itfkeyeventsink
- `ITfCompositionSink`: https://learn.microsoft.com/en-us/windows/win32/api/msctf/nn-msctf-itfcompositionsink
- `ITfDisplayAttributeProvider`: https://learn.microsoft.com/en-us/windows/win32/api/msctf/nn-msctf-itfdisplayattributeprovider
- `ITfContextView::GetTextExt`: https://learn.microsoft.com/en-us/windows/win32/api/msctf/nf-msctf-itfcontextview-gettextext
- `ITfUIElementMgr`: https://learn.microsoft.com/en-us/windows/win32/api/msctf/nn-msctf-itfuielementmgr
- IME の要件（UWP / AppContainer）: https://learn.microsoft.com/en-us/windows/apps/develop/input/imes/ime-requirements
- `ITfInputProcessorProfileMgr::RegisterProfile`: https://learn.microsoft.com/en-us/windows/win32/api/msctf/nf-msctf-itfinputprocessorprofilemgr-registerprofile
- `ITfCategoryMgr::RegisterCategory`: https://learn.microsoft.com/en-us/windows/win32/api/msctf/nf-msctf-itfcategorymgr-registercategory
- 定義済みカテゴリ（`GUID_TFCAT_*`）: https://learn.microsoft.com/en-us/windows/win32/tsf/predefined-category-values
- `windows` crate（`Win32::UI::TextServices`）: https://docs.rs/windows/latest/windows/Win32/UI/TextServices/index.html
- Rust のリンケージ（MSVC での `staticlib` / `cdylib`）: https://doc.rust-lang.org/reference/linkage.html

調査（インターフェースの一覧、候補窓、登録、参考実装）は agy（Gemini）に依頼し、出典の URL つきで受け取った。リポジトリの実在、`windows` crate の版、既存 CI の構成は、こちらで確認した。インターフェースの役割や登録の手順は、公式ドキュメントの記述に基づく調査結果であり、実装時に公式の API リファレンスで再確認すること。
