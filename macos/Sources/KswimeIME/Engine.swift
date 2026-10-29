// Swift wrapper around the Rust engine's C ABI (Sources/CKswime/kswime.h).
// This is the only file that talks to CKswime; the controller works with
// `Piece` values.
import Foundation
import CKswime

enum PieceLang: Int32 {
    case japanese = 0
    case english = 1
    case neutral = 2
}

struct Piece {
    let lang: PieceLang
    let start: Int
    let end: Int
    let text: String
    let kana: String
    let candidates: [String]
    let selected: Int
    let pending: Bool
    /// 0 sure, 1 tentative, 2 undecided
    let confidence: Int32
    /// bunsetsu of the selected candidate (Japanese pieces only)
    let segments: [Segment]

    /// One bunsetsu: where it ends as a keystroke position, and in UTF-16
    /// units of `text` (what `NSRange` counts).
    struct Segment {
        let keyEnd: Int
        let utf16End: Int
    }
}

final class Engine {
    private let handle: OpaquePointer

    init() {
        handle = ksw_new()
    }

    deinit {
        ksw_free(handle)
    }

    static var version: String { String(cString: ksw_version()) }

    // MARK: configuration

    func setStrength(_ s: Int32) { ksw_set_strength(handle, s) }

    @discardableResult
    func setParam(_ key: String, _ value: Double) -> Bool { ksw_set_param(handle, key, value) != 0 }

    // MARK: context

    func setContext(before: String) { ksw_set_context(handle, before) }
    func setContext(after: String) { ksw_set_context_after(handle, after) }
    func resetContext() { ksw_reset_context(handle) }

    // MARK: buffer

    var bufferLength: Int { Int(ksw_buffer_len(handle)) }
    var buffer: String { String(cString: ksw_buffer(handle)) }
    var isEmpty: Bool { bufferLength == 0 }

    func push(_ s: String) { ksw_push(handle, s) }

    @discardableResult
    func backspace() -> Bool { ksw_backspace(handle) != 0 }

    func clear() { ksw_clear(handle) }

    // MARK: composition

    func compose(final: Bool) -> [Piece] {
        let n = ksw_compose(handle, final ? 1 : 0)
        var pieces: [Piece] = []
        pieces.reserveCapacity(Int(n))
        for i in 0..<n {
            let count = ksw_piece_candidate_count(handle, i)
            var cands: [String] = []
            for c in 0..<count {
                cands.append(String(cString: ksw_piece_candidate(handle, i, c)))
            }
            var segments: [Piece.Segment] = []
            for j in 0..<ksw_piece_segment_count(handle, i) {
                segments.append(Piece.Segment(keyEnd: Int(ksw_piece_segment_key_end(handle, i, j)),
                                              utf16End: Int(ksw_piece_segment_utf16_end(handle, i, j))))
            }
            pieces.append(Piece(
                lang: PieceLang(rawValue: ksw_piece_lang(handle, i)) ?? .neutral,
                start: Int(ksw_piece_start(handle, i)),
                end: Int(ksw_piece_end(handle, i)),
                text: String(cString: ksw_piece_text(handle, i)),
                kana: String(cString: ksw_piece_kana(handle, i)),
                candidates: cands,
                selected: Int(ksw_piece_selected(handle, i)),
                pending: ksw_piece_pending(handle, i) != 0,
                confidence: ksw_piece_confidence(handle, i),
                segments: segments))
        }
        return pieces
    }

    var rendered: String { String(cString: ksw_rendered(handle)) }

    func probabilityEnglish(at keystroke: Int) -> Double { ksw_p_en(handle, Int32(keystroke)) }

    @discardableResult
    func nextCandidate(run: Int = -1, step: Int = 1) -> Bool { ksw_next_candidate(handle, Int32(run), Int32(step)) != 0 }

    @discardableResult
    func selectCandidate(run: Int = -1, index: Int) -> Bool { ksw_select_candidate(handle, Int32(run), Int32(index)) != 0 }

    // MARK: learning

    /// Returns the new language of the last token, or nil when empty.
    func toggleLastLanguage() -> PieceLang? {
        let v = ksw_toggle_last_language(handle)
        return v < 0 ? nil : PieceLang(rawValue: v)
    }

    func feedback(token: String, lang: PieceLang) { ksw_feedback(handle, token, lang.rawValue) }

    /// Force keystrokes [start, end) to `lang` for this composition and learn.
    func forceLanguage(start: Int, end: Int, lang: PieceLang) { ksw_force_language(handle, Int32(start), Int32(end), lang.rawValue) }

    /// Move the bunsetsu boundary at keystroke `pos` of a Japanese run by one
    /// kana; the new keystroke position, or nil when there is no kana left.
    func stepCut(at pos: Int, forward: Bool) -> Int? {
        let p = ksw_step_cut(handle, Int32(pos), forward ? 1 : 0)
        return p < 0 ? nil : Int(p)
    }

    /// Move a boundary without learning; the engine learns the final result once.
    func nudgeLanguage(start: Int, end: Int, lang: PieceLang) { ksw_nudge_language(handle, Int32(start), Int32(end), lang.rawValue) }

    var weightsTSV: String { String(cString: ksw_export_weights(handle)) }
    func importWeights(_ text: String) { ksw_import_weights(handle, text) }
    func resetLearning() { ksw_reset_learning(handle) }
    func setLearningPaused(_ paused: Bool) { ksw_set_learning_paused(handle, paused ? 1 : 0) }
    var explanation: String { String(cString: ksw_explain(handle)) }

    var corrections: String { String(cString: ksw_export_corrections(handle)) }
    func importCorrections(_ text: String) { ksw_import_corrections(handle, text) }

    // MARK: commit

    func commit() -> String { String(cString: ksw_commit(handle)) }
}
