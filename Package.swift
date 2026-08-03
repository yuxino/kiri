// swift-tools-version: 6.0

import PackageDescription

let package = Package(
    name: "kiri",
    platforms: [
        .macOS(.v14)
    ],
    products: [
        .library(name: "KiriCore", targets: ["KiriCore"]),
        .executable(name: "kiri", targets: ["KiriApp"]),
        .executable(name: "kiri-core-tests", targets: ["KiriCoreTests"])
    ],
    targets: [
        .target(
            name: "KiriCore",
            path: "Sources/KiriCore"
        ),
        .executableTarget(
            name: "KiriApp",
            dependencies: ["KiriCore"],
            path: "Sources/KiriApp",
            exclude: ["Info.plist"],
            linkerSettings: [
                .linkedFramework("AppKit"),
                .linkedFramework("AVFoundation"),
                .linkedFramework("Carbon"),
                .linkedFramework("CoreMedia"),
                .linkedFramework("CoreVideo"),
                .linkedFramework("ImageIO"),
                .linkedFramework("ScreenCaptureKit")
            ]
        ),
        .executableTarget(
            name: "KiriCoreTests",
            dependencies: ["KiriCore"],
            path: "Tests/KiriCoreTests"
        )
    ]
)
