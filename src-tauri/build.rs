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

    tauri_build::build()
}
