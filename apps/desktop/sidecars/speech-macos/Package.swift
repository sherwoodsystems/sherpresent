// swift-tools-version: 6.0
import PackageDescription

// Deployment target is macOS 26: SpeechAnalyzer/SpeechTranscriber and the
// non-SwiftUI `TranslationSession(installedSource:target:)` initializer both
// landed there. Nothing here builds against an earlier SDK.
let package = Package(
    name: "sherpresent-speech",
    platforms: [.macOS("26.0")],
    targets: [
        .executableTarget(
            name: "sherpresent-speech",
            path: "Sources/sherpresent-speech",
            swiftSettings: [.swiftLanguageMode(.v6)]
        )
    ]
)
