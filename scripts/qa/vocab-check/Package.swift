// swift-tools-version: 6.0
import PackageDescription

let package = Package(
    name: "vocab-check",
    platforms: [.macOS("26.0")],
    targets: [
        .executableTarget(
            name: "vocab-check",
            path: "Sources/vocab-check",
            swiftSettings: [.swiftLanguageMode(.v5)]
        ),
        .executableTarget(
            name: "postprocess-probe",
            path: "Sources/postprocess-probe",
            swiftSettings: [.swiftLanguageMode(.v5)]
        )
    ]
)
