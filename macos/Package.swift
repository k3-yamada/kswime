// swift-tools-version:5.9
// macOS Input Method Kit shell for kswime.  No third-party packages; the
// Rust engine is linked as a static library built by build.sh.
import PackageDescription

let package = Package(
    name: "KswimeIME",
    platforms: [.macOS(.v13)],
    targets: [
        .systemLibrary(name: "CKswime", path: "Sources/CKswime"),
        .executableTarget(
            name: "KswimeIME",
            dependencies: ["CKswime"],
            path: "Sources/KswimeIME",
            linkerSettings: [
                .linkedFramework("InputMethodKit"),
                .linkedFramework("Carbon"),
                .linkedFramework("Cocoa"),
                .unsafeFlags(["-L", "../target/universal/release"]),
            ]
        ),
    ]
)
