// User settings and learned corrections on disk.
//   ~/Library/Application Support/Kswime/settings.plist   (strength, direct apps…)
//   ~/Library/Application Support/Kswime/corrections.tsv  (what the user taught)
import Foundation

final class Settings {
    static let shared = Settings()

    let directory: URL
    private let settingsURL: URL
    private let correctionsURL: URL
    private let weightsURL: URL
    private let recordURL: URL
    private var dict: [String: Any]

    /// 0 aggressive, 1 balanced, 2 conservative
    var strength: Int32 {
        get { Int32(dict["strength"] as? Int ?? 1) }
        set { dict["strength"] = Int(newValue); save() }
    }

    /// Bundle identifiers that always get direct (ASCII) input.
    var directApps: [String] {
        get { dict["directApps"] as? [String] ?? ["com.apple.Terminal", "com.googlecode.iterm2", "com.microsoft.VSCode"] }
        set { dict["directApps"] = newValue; save() }
    }

    /// Show the candidate window while typing (not only after Space).
    var liveCandidates: Bool {
        get { dict["liveCandidates"] as? Bool ?? false }
        set { dict["liveCandidates"] = newValue; save() }
    }

    private init() {
        let base = FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask).first!
        directory = base.appendingPathComponent("Kswime", isDirectory: true)
        try? FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        settingsURL = directory.appendingPathComponent("settings.plist")
        correctionsURL = directory.appendingPathComponent("corrections.tsv")
        weightsURL = directory.appendingPathComponent("weights.tsv")
        recordURL = directory.appendingPathComponent("typing-log.tsv")
        if let data = try? Data(contentsOf: settingsURL),
           let d = try? PropertyListSerialization.propertyList(from: data, format: nil) as? [String: Any] {
            dict = d
        } else {
            dict = [:]
        }
    }

    private func save() {
        if let data = try? PropertyListSerialization.data(fromPropertyList: dict, format: .xml, options: 0) {
            try? data.write(to: settingsURL, options: .atomic)
        }
    }

    func loadCorrections() -> String {
        (try? String(contentsOf: correctionsURL, encoding: .utf8)) ?? ""
    }

    func saveCorrections(_ text: String) {
        try? text.write(to: correctionsURL, atomically: true, encoding: .utf8)
    }

    func loadWeights() -> String { (try? String(contentsOf: weightsURL, encoding: .utf8)) ?? "" }
    func saveWeights(_ text: String) { try? text.write(to: weightsURL, atomically: true, encoding: .utf8) }

    /// Record typing (before-context, typed keys, committed text) for
    /// `kswime replay`.  Off by default; local only.
    var recordTyping: Bool {
        get { dict["recordTyping"] as? Bool ?? false }
        set { dict["recordTyping"] = newValue; save() }
    }

    /// Secret mode: no learning, no token memory, no typing record
    /// (screen sharing, passwords, other people's text).
    var secretMode: Bool {
        get { dict["secretMode"] as? Bool ?? false }
        set { dict["secretMode"] = newValue; save() }
    }

    func appendRecord(before: String, typed: String, committed: String) {
        guard recordTyping, !secretMode, !typed.isEmpty else { return }
        let line = "\(before.replacingOccurrences(of: "\t", with: " "))\t\(typed)\t\(committed)\n"
        if let h = try? FileHandle(forWritingTo: recordURL) {
            h.seekToEndOfFile(); h.write(line.data(using: .utf8)!); try? h.close()
        } else {
            try? line.write(to: recordURL, atomically: true, encoding: .utf8)
        }
    }

    /// Bumped on every erase.  Each input controller owns its own engine, so
    /// a controller that sees an older value must drop its in-memory learning
    /// before it saves, or it would write the erased data back to disk.
    private(set) var learningGeneration = 0

    /// Forget everything learned (weights, token memory, typing log).
    func eraseLearning() {
        for u in [correctionsURL, weightsURL, recordURL] { try? FileManager.default.removeItem(at: u) }
        learningGeneration += 1
    }
}
