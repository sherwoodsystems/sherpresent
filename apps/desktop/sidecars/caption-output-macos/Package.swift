// swift-tools-version: 6.0
import PackageDescription

// Caption video output helper: renders caption lines to a transparent frame
// and publishes it to one or more frame sinks. Syphon today; the FrameSink
// protocol is the seam for NDI or others later. macOS 13 is the floor for the
// Swift concurrency used here (output/syphon.rs checks the same), so Syphon
// output works on Macs too old for the on-device speech provider.
let package = Package(
    name: "sherpresent-output",
    platforms: [.macOS(.v13)],
    targets: [
        // Vendored Syphon (Metal subset), built from source so no Xcode or
        // framework embedding is needed. See Sources/Syphon/VENDORED.md.
        .target(
            name: "Syphon",
            path: "Sources/Syphon",
            exclude: ["LICENSE.txt", "VENDORED.md"],
            publicHeadersPath: "include",
            cSettings: [
                .headerSearchPath("include/Syphon"),
                .define("SYPHON_CORE_SHARE"),
                // Upstream builds with a prefix header that imports Cocoa and
                // defines SYPHONLOG; every .m depends on it implicitly.
                .unsafeFlags([
                    "-include", Context.packageDirectory + "/Sources/Syphon/Syphon_Prefix.h",
                    "-fobjc-arc",
                    // Vendored code: upstream warnings are not ours to fix.
                    "-w",
                ]),
            ],
            linkerSettings: [
                .linkedFramework("Cocoa"),
                .linkedFramework("Metal"),
                .linkedFramework("IOSurface"),
            ]
        ),
        .executableTarget(
            name: "sherpresent-output",
            dependencies: ["Syphon"],
            path: "Sources/sherpresent-output",
            swiftSettings: [.swiftLanguageMode(.v6)],
            linkerSettings: [
                .linkedFramework("CoreText"),
            ]
        ),
        .testTarget(
            name: "sherpresent-output-tests",
            dependencies: ["sherpresent-output"],
            path: "Tests/sherpresent-output-tests",
            swiftSettings: [.swiftLanguageMode(.v6)]
        ),
        // Dev-only Syphon receiver for end-to-end checks; not bundled.
        .executableTarget(
            name: "syphon-probe",
            dependencies: ["Syphon"],
            path: "Sources/syphon-probe",
            swiftSettings: [.swiftLanguageMode(.v6)]
        ),
    ]
)
