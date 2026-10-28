// Entry point.  `KswimeIME --register` registers/enables the input source
// without logging out; otherwise starts the IMK server (launched by TSM).
import Cocoa
import InputMethodKit
import CKswime

let connectionName = "com.azally.inputmethod.Kswime_Connection"

if CommandLine.arguments.contains("--register") {
    InputSourceRegistration.registerAndEnable()
    exit(0)
}
if CommandLine.arguments.contains("--version") {
    print("kswime engine \(String(cString: ksw_version()))")
    exit(0)
}

guard let server = IMKServer(name: connectionName, bundleIdentifier: Bundle.main.bundleIdentifier) else {
    NSLog("Kswime: IMKServer failed to start")
    exit(1)
}
_ = server
// Load the engine model while the app is starting so the first keystroke is
// not delayed.
DispatchQueue.global(qos: .userInitiated).async { ksw_preload() }
NSApplication.shared.run()
