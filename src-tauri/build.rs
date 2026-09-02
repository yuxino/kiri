fn main() {
    // Tauri embeds the ICNS in dev binaries and bundles the same desktop
    // assets for production. Keep icon-only edits from reusing a stale build.
    for icon in [
        "icons/32x32.png",
        "icons/128x128.png",
        "icons/128x128@2x.png",
        "icons/icon.icns",
        "icons/icon.ico",
    ] {
        println!("cargo:rerun-if-changed={icon}");
    }

    #[cfg(target_os = "macos")]
    {
        println!("cargo:rerun-if-changed=src/macos_media.m");
        cc::Build::new()
            .file("src/macos_media.m")
            .flag("-fobjc-arc")
            .flag("-fmodules")
            .compile("kiri_macos_media");
        for framework in [
            "AVFoundation",
            "CoreGraphics",
            "CoreMedia",
            "CoreVideo",
            "Foundation",
            "ImageIO",
            "UniformTypeIdentifiers",
        ] {
            println!("cargo:rustc-link-lib=framework={framework}");
        }
    }

    tauri_build::build()
}
