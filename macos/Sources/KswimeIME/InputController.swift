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
    /// The user is editing the composition (Space on a Japanese run, or an
    /// arrow key): the judgement is final, one piece has the focus, and
    /// Space / Tab / Up / Down / digits / F6 / F7 / F10 act on that piece.
    /// Left / Right move the focus, Shift+Left / Shift+Right move its right
    /// boundary.  Typing a character leaves editing and appends.
    private var candidateMode = false
    /// Index into `lastPieces` of the focused piece while editing.
    private var focus: Int?
    private var lastPieces: [Piece] = []
    /// Which bunsetsu of the focused Japanese piece has the focus (0 for a
    /// piece with one bunsetsu).  Clamped on use: a re-render can change how
    /// many bunsetsu a piece has.
    private var segFocus = 0

    /// The focused bunsetsu of `piece`, kept inside its range.
    private func currentSeg(_ piece: Int) -> Int {
        min(segFocus, max(0, lastPieces[piece].segments.count - 1))
    }

    /// Pieces the focus can land on (words, not punctuation or spaces).
    private func focusable(_ i: Int) -> Bool { lastPieces[i].lang != .neutral }

    /// The focused piece, defaulting to the last word.
    private var focusedIndex: Int? {
        if let f = focus, f < lastPieces.count, focusable(f) { return f }
        return lastPieces.lastIndex(where: { $0.lang != .neutral })
    }

    /// Engine "run" number of a piece: pieces with candidates, counted in order.
    private func runIndex(of piece: Int) -> Int? {
        guard piece < lastPieces.count, !lastPieces[piece].candidates.isEmpty else { return nil }
        return lastPieces[..<piece].filter { !$0.candidates.isEmpty }.count
    }

    private func beginEditing(_ client: IMKTextInput) {
        if !candidateMode {
            candidateMode = true
            render(client)   // final judgement; piece indexes may change
            focus = focusedIndex
            // the default is the last word, and in it the last bunsetsu
            segFocus = focus.map { max(0, lastPieces[$0].segments.count - 1) } ?? 0
        }
    }

    private func moveFocus(_ step: Int, _ client: IMKTextInput) {
        beginEditing(client)
        guard var i = focusedIndex else { return }
        // inside a piece with several bunsetsu, the arrow walks them first
        let next = currentSeg(i) + step
        if next >= 0 && next < lastPieces[i].segments.count {
            segFocus = next
            render(client)
            return
        }
        var j = i + step
        while j >= 0 && j < lastPieces.count {
            if focusable(j) {
                i = j
                segFocus = step > 0 ? 0 : max(0, lastPieces[j].segments.count - 1)
                break
            }
            j += step
        }
        focus = i
        render(client)
    }

    private func cycleCandidate(_ step: Int, _ client: IMKTextInput) {
        beginEditing(client)
        guard let i = focusedIndex, let run = runIndex(of: i) else { render(client); return }
        engine.nextCandidate(run: run, step: step)
        render(client)
    }

    /// Move the right boundary of the focused piece by one keystroke: +1
    /// takes the next keystroke into this piece's language, -1 gives the
    /// last keystroke to the following word's language (or the other
    /// language).  Boundaries inside one Japanese run are the converter's;
    /// this moves language boundaries.  Nothing is learned per key press: the
    /// engine learns the final boundary once, on commit or when typing goes on.
    private func moveBoundary(_ delta: Int, _ client: IMKTextInput) {
        beginEditing(client)
        guard let i = focusedIndex else { return }
        let p = lastPieces[i]
        // the right end of a bunsetsu that is not the piece's last is a
        // bunsetsu boundary, not a language boundary
        if p.lang == .japanese, currentSeg(i) < p.segments.count - 1 {
            moveBunsetsu(forward: delta > 0, client)
            return
        }
        let total = engine.bufferLength
        if delta > 0 {
            guard p.end < total else { return }
            engine.nudgeLanguage(start: p.start, end: p.end + 1, lang: p.lang)
        } else {
            guard p.end - p.start > 1 else { return }
            let next = lastPieces[(i + 1)...].first(where: { $0.lang != .neutral })?.lang
            let other: PieceLang = next ?? (p.lang == .japanese ? .english : .japanese)
            engine.nudgeLanguage(start: p.end - 1, end: p.end, lang: other)
        }
        render(client)
        focus = lastPieces.firstIndex(where: { $0.start <= p.start && p.start < $0.end && $0.lang != .neutral }) ?? focusedIndex
        render(client)
    }

    /// Move the right end of the focused bunsetsu by one kana: the bunsetsu
    /// boundary there moves, or, on the last bunsetsu, a boundary appears one
    /// kana before the piece's end.  The converter re-converts around it.
    /// (Option+Shift+Left/Right; Shift+Left/Right do this too, except on the
    /// last bunsetsu, where they move the language boundary.)
    private func moveBunsetsu(forward: Bool, _ client: IMKTextInput) {
        beginEditing(client)
        guard let i = focusedIndex, lastPieces[i].lang == .japanese else { return }
        let segs = lastPieces[i].segments
        guard !segs.isEmpty, let moved = engine.stepCut(at: segs[currentSeg(i)].keyEnd, forward: forward) else { return }
        render(client)
        if let i2 = focusedIndex, let k = lastPieces[i2].segments.firstIndex(where: { $0.keyEnd == moved }) {
            segFocus = k
        }
        render(client)
    }

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
        m.addItem(item("選択中の語の日本語／英語を反転して学習 (F6/F10)", #selector(menuToggleLanguage)))
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
            focus = nil
            render(client)
            return true
        case kVK_Delete:
            guard !engine.isEmpty else { return false }
            if candidateMode {
                candidateMode = false
                focus = nil
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
            cycleCandidate(flags.contains(.shift) ? -1 : 1, client)
            return true
        case kVK_DownArrow where !engine.isEmpty:
            cycleCandidate(1, client)
            return true
        case kVK_UpArrow where !engine.isEmpty:
            cycleCandidate(-1, client)
            return true
        case kVK_LeftArrow where !engine.isEmpty:
            if flags.contains(.shift) && flags.contains(.option) {
                moveBunsetsu(forward: false, client)
            } else if flags.contains(.shift) {
                moveBoundary(-1, client)
            } else {
                moveFocus(-1, client)
            }
            return true
        case kVK_RightArrow where !engine.isEmpty:
            if flags.contains(.shift) && flags.contains(.option) {
                moveBunsetsu(forward: true, client)
            } else if flags.contains(.shift) {
                moveBoundary(1, client)
            } else {
                moveFocus(1, client)
            }
            return true
        case kVK_Home where !engine.isEmpty:
            beginEditing(client)
            focus = lastPieces.firstIndex(where: { $0.lang != .neutral })
            segFocus = 0
            render(client)
            return true
        case kVK_End where !engine.isEmpty:
            beginEditing(client)
            focus = lastPieces.lastIndex(where: { $0.lang != .neutral })
            segFocus = focus.map { max(0, lastPieces[$0].segments.count - 1) } ?? 0
            render(client)
            return true
        case kVK_F6:
            return selectHiragana(client)
        case kVK_F7:
            return selectKatakana(client)
        case kVK_F10:
            return forceFocusedLanguage(.english, client)
        case kVK_LeftArrow, kVK_RightArrow, kVK_UpArrow, kVK_DownArrow, kVK_Home, kVK_End, kVK_PageUp, kVK_PageDown:
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
                cycleCandidate(flags.contains(.shift) ? -1 : 1, client)
                return true
            }
            if let d = Int(chars), (1...9).contains(d), let i = focusedIndex, let run = runIndex(of: i) {
                let selected = lastPieces[i].selected
                engine.selectCandidate(run: run, index: (selected / 9) * 9 + d - 1)
                render(client)
                return true
            }
            // any other key: accept the candidates and continue typing
            candidateMode = false
            focus = nil
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
                beginEditing(client)
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
        let focused: Int? = candidateMode ? focusedIndex : nil
        let shown = focused ?? pieces.lastIndex(where: { $0.candidates.count > 1 })
        var focusedRange: NSRange?
        for (i, p) in pieces.enumerated() {
            // confidence drives the underline: confident = thin, tentative or
            // undecided = raw (dotted) so the eye lands on what may be wrong;
            // the focused piece is drawn as the selected segment
            let style: Int
            switch p.lang {
            case .japanese, .english: style = p.confidence == 0 ? kTSMHiliteConvertedText : kTSMHiliteRawText
            case .neutral: style = p.pending ? kTSMHiliteRawText : kTSMHiliteConvertedText
            }
            var attrs = mark(forStyle: style, at: NSRange(location: text.length, length: p.text.utf16.count)) as? [NSAttributedString.Key: Any] ?? [:]
            if p.confidence == 2 { attrs[.foregroundColor] = NSColor.secondaryLabelColor }
            let pieceStart = text.length
            text.append(NSAttributedString(string: p.text, attributes: attrs))
            if i == focused {
                // Only the focused part is drawn selected: the whole piece, or,
                // in a piece of several bunsetsu, the focused bunsetsu.
                var range = NSRange(location: pieceStart, length: p.text.utf16.count)
                if p.segments.count > 1 {
                    let k = currentSeg(i)
                    let from = k == 0 ? 0 : p.segments[k - 1].utf16End
                    range = NSRange(location: pieceStart + from, length: p.segments[k].utf16End - from)
                }
                // Hosts draw the selected range (not the attributes) as the
                // highlighted segment, so the selection follows the focus.
                focusedRange = range
                // kTSMHiliteSelectedConvertedText alone is hard to tell from the
                // other underlines in many hosts, so say it explicitly.
                var selected = mark(forStyle: kTSMHiliteSelectedConvertedText, at: range) as? [NSAttributedString.Key: Any] ?? [:]
                selected[.underlineStyle] = NSUnderlineStyle.thick.rawValue
                selected[.underlineColor] = NSColor.controlAccentColor
                selected[.markedClauseSegment] = i
                text.addAttributes(selected, range: range)
            }
        }
        client.setMarkedText(text, selectionRange: focusedRange ?? NSRange(location: text.length, length: 0), replacementRange: notFound)

        if let idx = shown, candidateMode || Settings.shared.liveCandidates, pieces[idx].candidates.count > 1 {
            var rect = NSRect.zero
            client.attributes(forCharacterIndex: 0, lineHeightRectangle: &rect)
            var title: String?
            if candidateMode {
                let words = pieces.indices.filter { pieces[$0].lang != .neutral }
                if let n = words.firstIndex(of: idx) {
                    let p = pieces[idx]
                    title = "\(n + 1)/\(words.count)  \(p.lang == .japanese ? p.kana : p.text)"
                    if p.segments.count > 1 {
                        // name the bunsetsu too: some hosts draw no highlight
                        let k = currentSeg(idx)
                        let from = k == 0 ? 0 : p.segments[k - 1].utf16End
                        let part = (p.text as NSString).substring(with: NSRange(location: from, length: p.segments[k].utf16End - from))
                        title = "\(n + 1)/\(words.count)  文節 \(k + 1)/\(p.segments.count)  \(part)"
                    }
                }
            }
            panel.show(candidates: pieces[idx].candidates, selected: pieces[idx].selected, caret: rect,
                       level: NSWindow.Level(rawValue: Int(client.windowLevel()) + 1), title: title)
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
        focus = nil
        lastPieces = []
        client.setMarkedText(NSAttributedString(string: ""), selectionRange: NSRange(location: 0, length: 0), replacementRange: notFound)
        client.insertText(text, replacementRange: notFound)
        panel.hide()
    }

    private func toggleLastLanguage() {
        guard !engine.isEmpty, let client = client() else { return }
        if let i = focusedIndex {
            let p = lastPieces[i]
            _ = forceFocusedLanguage(p.lang == .japanese ? .english : .japanese, client)
        } else {
            _ = engine.toggleLastLanguage()
            render(client)
        }
    }

    /// F6 / F10: the focused word (the last word unless the user moved the
    /// focus) becomes Japanese / English for this composition, and the
    /// engine learns from it.  Editing continues on the same word.
    private func forceFocusedLanguage(_ lang: PieceLang, _ client: IMKTextInput) -> Bool {
        guard !engine.isEmpty else { return false }
        beginEditing(client)
        guard let i = focusedIndex else { return false }
        let p = lastPieces[i]
        if p.lang != lang {
            engine.forceLanguage(start: p.start, end: p.end, lang: lang)
        }
        render(client)
        focus = lastPieces.firstIndex(where: { $0.start <= p.start && p.start < $0.end && $0.lang != .neutral })
        render(client)
        return true
    }

    /// F6 (the Japanese IME convention): the focused word in hiragana.  A word
    /// judged English becomes Japanese first, and the engine learns from that.
    private func selectHiragana(_ client: IMKTextInput) -> Bool {
        guard forceFocusedLanguage(.japanese, client) else { return false }
        guard let i = focusedIndex, lastPieces[i].lang == .japanese, let run = runIndex(of: i) else { return true }
        let p = lastPieces[i]
        if let k = p.candidates.firstIndex(of: p.kana) {
            engine.selectCandidate(run: run, index: k)
            render(client)
        }
        return true
    }

    /// F7: the focused Japanese word in katakana.
    private func selectKatakana(_ client: IMKTextInput) -> Bool {
        guard !engine.isEmpty else { return false }
        beginEditing(client)
        guard let i = focusedIndex, lastPieces[i].lang == .japanese, let run = runIndex(of: i) else { return true }
        let p = lastPieces[i]
        let kata = String(p.kana.unicodeScalars.map { s -> Character in
            (0x3041...0x3096).contains(s.value) ? Character(UnicodeScalar(s.value + 0x60)!) : Character(s)
        })
        if let k = p.candidates.firstIndex(of: kata) {
            engine.selectCandidate(run: run, index: k)
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
