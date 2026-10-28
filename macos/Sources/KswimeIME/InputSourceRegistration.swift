// Registers the bundle as an input source and enables its modes, so that a
// fresh install (or an update) shows up in the input menu without logout.
import Carbon
import Foundation

enum InputSourceRegistration {
    static let parentID = "com.azally.inputmethod.Kswime"
    static let modeIDs = ["com.azally.inputmethod.Kswime.auto", "com.azally.inputmethod.Kswime.direct"]

    static func registerAndEnable() {
        let url = Bundle.main.bundleURL as CFURL
        let status = TISRegisterInputSource(url)
        if status != noErr {
            print("TISRegisterInputSource failed: \(status)")
        }
        for id in [parentID] + modeIDs {
            let filter = [kTISPropertyInputSourceID as String: id] as CFDictionary
            guard let list = TISCreateInputSourceList(filter, true)?.takeRetainedValue() as? [TISInputSource] else { continue }
            for source in list {
                let s = TISEnableInputSource(source)
                print("enable \(id): \(s == noErr ? "ok" : "status \(s)")")
            }
        }
        // refresh the input menu
        let task = Process()
        task.launchPath = "/usr/bin/killall"
        task.arguments = ["TextInputMenuAgent"]
        try? task.run()
    }
}
