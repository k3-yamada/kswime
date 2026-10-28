// IMKInputController for Kswime.
//
// Key handling is done with the raw-event style (`handle(_:client:)`), which
// every mature macOS IME uses.  The controller owns one Rust engine per text
// client and only renders what the engine composes; no language or
// conversion logic lives here.
import Cocoa
import Carbon
import InputMethodKit

private let kVK_JIS_Eisu_: UInt16 = 102
private let kVK_JIS_Kana_: UInt16 = 104

enum Mode {
    case auto
    case direct
}

@objc(KswimeInputController)
final class KswimeInputController: IMKInputController {
    private let engine = Engine()
    private let panel = CandidatePanel()
    private var mode: Mode = .auto
    /// Space was pressed on a Japanese run: the candidate window is open and
    /// Space / arrows / digits operate on it.
    private var candidateMode = false
    private var lastPieces: [Piece] = []

    override init!(server: IMKServer!, delegate: Any!, client inputClient: Any!) {
        super.init(server: server, delegate: delegate, client: inputClient)
        engine.setStrength(Settings.shared.strength)
        engine.importCorrections(Settings.shared.loadCorrections())
        engine.importWeights(Settings.shared.loadWeights())
        engine.setLearningPaused(Settings.shared.secretMode)
    }

    /// Generation of the learned data this controller's engine was loaded from.
    private var seenLearningGeneration = Settings.shared.learningGeneration

    /// Settings are process-wide but every controller has its own engine, so
    /// a menu action taken in one controller must be re-applied to the others
    /// before they learn or save: drop learning that was erased elsewhere,
    /// and follow secret mode.
    private func syncSharedLearningState() {
        if seenLearningGeneration != Settings.shared.learningGeneration {
            engine.resetLearning()
            seenLearningGeneration = Settings.shared.learningGeneration
        }
        engine.setLearningPaused(Settings.shared.secretMode)
    }

    /// 20 characters before the caret and the committed text, for the
    /// local typing log (off unless the user enables it in the menu).
    private var lastBefore = ""

    // MARK: - IMK lifecycle

    override func recognizedEvents(_ sender: Any!) -> Int {
        Int(NSEvent.EventTypeMask.keyDown.rawValue)
    }

    override func activateServer(_ sender: Any!) {
        // Never query the client's text here (Chromium can deadlock); only
        // the bundle identifier is safe.
        if let bundle = client()?.bundleIdentifier(), Settings.shared.directApps.contains(bundle) {
            mode = .direct
        }
        engine.setStrength(Settings.shared.strength)
        syncSharedLearningState()
    }

    override func deactivateServer(_ sender: Any!) {
        syncSharedLearningState()
        commitIfNeeded()
        panel.hide()
        // Nothing is learned in secret mode, so there is nothing new to save;
        // skipping also keeps a stale engine from overwriting the files.
        guard !Settings.shared.secretMode else { return }
        Settings.shared.saveCorrections(engine.corrections)
        Settings.shared.saveWeights(engine.weightsTSV)
    }

    override func commitComposition(_ sender: Any!) {
        commitIfNeeded()
    }

    /// Mode changed from the input menu or the system shortcut.
    override func setValue(_ value: Any!, forTag tag: Int, client sender: Any!) {
        guard let id = value as? String else { return }
        if id.hasSuffix(".direct") {
            commitIfNeeded()
            mode = .direct
        } else if id.hasSuffix(".auto") {
            mode = .auto
        }
    }

    override func menu() -> NSMenu! {
        let m = NSMenu(title: "Kswime")
        func item(_ title: String, _ sel: Selector, checked: Bool = false, tag: Int = 0) -> NSMenuItem {
            let i = NSMenuItem(title: title, action: sel, keyEquivalent: "")
            i.target = self
            i.state = checked ? .on : .off
            i.tag = tag
            return i
        }
        m.addItem(item("直前の語の日本語／英語を反転して学習 (F6/F10)", #selector(menuToggleLanguage)))
        m.addItem(NSMenuItem.separator())
        let s = Settings.shared.strength
        m.addItem(item("判定: 英語寄り", #selector(menuStrength(_:)), checked: s == 0, tag: 0))
        m.addItem(item("判定: 標準", #selector(menuStrength(_:)), checked: s == 1, tag: 1))
        m.addItem(item("判定: 日本語寄り", #selector(menuStrength(_:)), checked: s == 2, tag: 2))
        m.addItem(NSMenuItem.separator())
        m.addItem(item("候補を入力中も表示", #selector(menuLiveCandidates), checked: Settings.shared.liveCandidates))
        m.addItem(item("なぜこの判定か (Ctrl+E 相当)", #selector(menuExplain)))
        m.addItem(NSMenuItem.separator())
        m.addItem(item("シークレットモード（学習・記録を一時停止）", #selector(menuSecret), checked: Settings.shared.secretMode))
        m.addItem(item("打鍵を記録する（ローカル、replay 用）", #selector(menuRecord), checked: Settings.shared.recordTyping))
        m.addItem(item("学習をすべて消去", #selector(menuErase)))
        if let bundle = client()?.bundleIdentifier() {
            let direct = Settings.shared.directApps.contains(bundle)
            m.addItem(item("このアプリでは常に英数 (\(bundle))", #selector(menuDirectApp), checked: direct))
        }
        m.addItem(NSMenuItem.separator())
        m.addItem(item("設定フォルダを開く", #selector(menuOpenFolder)))
        m.addItem(item("Kswime エンジン \(Engine.version)", #selector(menuNoop)))
        return m
    }

    @objc private func menuToggleLanguage() { toggleLastLanguage() }
    @objc private func menuStrength(_ sender: NSMenuItem) {
        Settings.shared.strength = Int32(sender.tag)
        engine.setStrength(Settings.shared.strength)
    }
    @objc private func menuLiveCandidates() { Settings.shared.liveCandidates.toggle() }
    @objc private func menuDirectApp() {
        guard let bundle = client()?.bundleIdentifier() else { return }
        var apps = Settings.shared.directApps
        if let i = apps.firstIndex(of: bundle) { apps.remove(at: i); mode = .auto } else { apps.append(bundle); commitIfNeeded(); mode = .direct }
        Settings.shared.directApps = apps
    }
    @objc private func menuOpenFolder() { NSWorkspace.shared.open(Settings.shared.directory) }
    @objc private func menuRecord() { Settings.shared.recordTyping.toggle() }
    @objc private func menuSecret() {
        Settings.shared.secretMode.toggle()
        engine.setLearningPaused(Settings.shared.secretMode)
    }
    @objc private func menuErase() {
        Settings.shared.eraseLearning()
        syncSharedLearningState()
    }
    @objc private func menuExplain() {
        let text = engine.explanation
        guard !text.isEmpty else { return }
        let alert = NSAlert()
        alert.messageText = "判定の理由"
        alert.informativeText = text
        alert.runModal()
    }
    @objc private func menuNoop() {}

    // MARK: - Key handling

    override func handle(_ event: NSEvent!, client sender: Any!) -> Bool {
        guard let event = event, event.type == .keyDown, let client = sender as? IMKTextInput else { return false }
        let flags = event.modifierFlags.intersection(.deviceIndependentFlagsMask)
        let keyCode = event.keyCode
        syncSharedLearningState()

        // JIS keys switch modes in every state.
        if keyCode == kVK_JIS_Eisu_ {
            commitIfNeeded()
            mode = .direct
            return true
        }
        if keyCode == kVK_JIS_Kana_ {
            mode = .auto
            return true
        }
        if mode == .direct {
            return false
        }

        // Command / Control shortcuts belong to the application.
        if flags.contains(.command) || flags.contains(.control) {
            if !engine.isEmpty { commitIfNeeded() }
            return false
        }

        switch Int(keyCode) {
        case kVK_Return, kVK_ANSI_KeypadEnter:
            guard !engine.isEmpty else { return false }
            commitIfNeeded()
            return true
        case kVK_Escape:
            guard !engine.isEmpty else { return false }
            engine.clear()
            candidateMode = false
            render(client)
            return true
        case kVK_Delete:
            guard !engine.isEmpty else { return false }
            if candidateMode {
                candidateMode = false
            } else {
                engine.backspace()
            }
            render(client)
            return true
        case kVK_Tab:
            guard !engine.isEmpty, lastPieces.contains(where: { $0.candidates.count > 1 }) else {
                if !engine.isEmpty { commitIfNeeded() }
                return false
            }
            candidateMode = true
            engine.nextCandidate(step: flags.contains(.shift) ? -1 : 1)
            render(client)
            return true
        case kVK_DownArrow where candidateMode:
            engine.nextCandidate(step: 1)
            render(client)
            return true
        case kVK_UpArrow where candidateMode:
            engine.nextCandidate(step: -1)
            render(client)
            return true
        case kVK_F6:
            return forceLastLanguage(.japanese, client)
        case kVK_F7:
            return selectKatakana(client)
        case kVK_F10:
            return forceLastLanguage(.english, client)
        case kVK_LeftArrow, kVK_RightArrow, kVK_UpArrow, kVK_DownArrow, kVK_Home, kVK_End, kVK_PageUp, kVK_PageDown:
            if !engine.isEmpty { commitIfNeeded() }
            return false
        default:
            break
        }

        guard let chars = event.characters, chars.count == 1, let scalar = chars.unicodeScalars.first else {
            if !engine.isEmpty { commitIfNeeded() }
            return false
        }
        // Dead keys, control characters and non-ASCII (Option-key symbols) go
        // straight to the application.
        guard scalar.value >= 0x20, scalar.value < 0x7f else {
            if !engine.isEmpty { commitIfNeeded() }
            return false
        }

        if candidateMode {
            if chars == " " {
                engine.nextCandidate(step: flags.contains(.shift) ? -1 : 1)
                render(client)
                return true
            }
            if let d = Int(chars), (1...9).contains(d) {
                let selected = lastPieces.last(where: { $0.candidates.count > 1 })?.selected ?? 0
                engine.selectCandidate(index: (selected / 9) * 9 + d - 1)
                commitIfNeeded()
                return true
            }
            // any other key: accept the candidate and continue typing
            candidateMode = false
        }

        if chars == " ", !engine.isEmpty, !flags.contains(.shift) {
            // Space on a Japanese run opens the candidate window instead of
            // inserting a space (the Japanese convention).  Decide on the
            // *final* judgement: while typing, a short tail is still undecided
            // ("tugi") or leaning English ("cpu,memori"), and the space would
            // otherwise be inserted literally (次 の, メモリ の).  Shift+Space
            // always inserts a space.
            let decided = engine.compose(final: true)
            if decided.last(where: { $0.lang != .neutral })?.lang == .japanese {
                candidateMode = true
                render(client)
                return true
            }
        }

        if engine.isEmpty {
            let (before, after) = surroundingText(client)
            lastBefore = before
            engine.setContext(before: before)
            engine.setContext(after: after)
        }
        engine.push(chars)
        render(client)
        return true
    }

    // MARK: - Rendering

    private func render(_ client: IMKTextInput) {
        let pieces = engine.compose(final: candidateMode)
        lastPieces = pieces
        if pieces.isEmpty {
            client.setMarkedText(NSAttributedString(string: ""), selectionRange: NSRange(location: 0, length: 0), replacementRange: notFound)
            panel.hide()
            return
        }
        let text = NSMutableAttributedString()
        let lastJapanese = pieces.lastIndex(where: { $0.candidates.count > 1 })
        for (i, p) in pieces.enumerated() {
            // confidence drives the underline: confident = thin, tentative or
            // undecided = raw (dotted) so the eye lands on what may be wrong
            let style: Int
            switch p.lang {
            case .japanese: style = (candidateMode && i == lastJapanese) ? kTSMHiliteSelectedConvertedText : (p.confidence == 0 ? kTSMHiliteConvertedText : kTSMHiliteRawText)
            case .english: style = p.confidence == 0 ? kTSMHiliteConvertedText : kTSMHiliteRawText
            case .neutral: style = p.pending ? kTSMHiliteRawText : kTSMHiliteConvertedText
            }
            var attrs = mark(forStyle: style, at: NSRange(location: text.length, length: p.text.utf16.count)) as? [NSAttributedString.Key: Any] ?? [:]
            if p.confidence == 2 { attrs[.foregroundColor] = NSColor.secondaryLabelColor }
            text.append(NSAttributedString(string: p.text, attributes: attrs))
        }
        client.setMarkedText(text, selectionRange: NSRange(location: text.length, length: 0), replacementRange: notFound)

        if let idx = lastJapanese, candidateMode || Settings.shared.liveCandidates, pieces[idx].candidates.count > 1 {
            var rect = NSRect.zero
            client.attributes(forCharacterIndex: 0, lineHeightRectangle: &rect)
            panel.show(candidates: pieces[idx].candidates, selected: pieces[idx].selected, caret: rect,
                       level: NSWindow.Level(rawValue: Int(client.windowLevel()) + 1))
        } else {
            panel.hide()
        }
    }

    private var notFound: NSRange { NSRange(location: NSNotFound, length: 0) }

    private func commitIfNeeded() {
        guard !engine.isEmpty, let client = client() else { return }
        let typed = engine.buffer
        let text = engine.commit()
        Settings.shared.appendRecord(before: lastBefore, typed: typed, committed: text)
        candidateMode = false
        lastPieces = []
        client.setMarkedText(NSAttributedString(string: ""), selectionRange: NSRange(location: 0, length: 0), replacementRange: notFound)
        client.insertText(text, replacementRange: notFound)
        panel.hide()
    }

    private func toggleLastLanguage() {
        guard !engine.isEmpty, let client = client() else { return }
        _ = engine.toggleLastLanguage()
        candidateMode = false
        render(client)
    }

    private func forceLastLanguage(_ lang: PieceLang, _ client: IMKTextInput) -> Bool {
        guard !engine.isEmpty, let last = lastPieces.last(where: { $0.lang != .neutral }) else { return false }
        if last.lang != lang {
            engine.forceLanguage(start: last.start, end: last.end, lang: lang)
        }
        candidateMode = false
        render(client)
        return true
    }

    private func selectKatakana(_ client: IMKTextInput) -> Bool {
        guard let last = lastPieces.last(where: { $0.lang == .japanese }) else { return false }
        let kata = String(last.kana.unicodeScalars.map { s -> Character in
            (0x3041...0x3096).contains(s.value) ? Character(UnicodeScalar(s.value + 0x60)!) : Character(s)
        })
        if let i = last.candidates.firstIndex(of: kata) {
            engine.selectCandidate(index: i)
            render(client)
        }
        return true
    }

    /// Up to 20 characters before and 10 after the caret, read only when
    /// nothing is marked (IMK clients misreport while composing).
    private func surroundingText(_ client: IMKTextInput) -> (String, String) {
        let marked = client.markedRange()
        guard marked.location == NSNotFound || marked.length == 0 else { return ("", "") }
        let sel = client.selectedRange()
        guard sel.location != NSNotFound else { return ("", "") }
        var before = ""
        if sel.location > 0 {
            let start = max(0, sel.location - 20)
            before = client.attributedSubstring(from: NSRange(location: start, length: sel.location - start))?.string ?? ""
        }
        var after = ""
        let total = client.length()
        let from = sel.location + sel.length
        if total != NSNotFound, from < total {
            after = client.attributedSubstring(from: NSRange(location: from, length: min(10, total - from)))?.string ?? ""
        }
        return (before, after)
    }
}
