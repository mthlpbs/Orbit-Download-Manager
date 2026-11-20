// Link the GUI with Windows subsystem version 5.02 (D-008). Windows gives windows of programs
// linked this way the legacy frame metrics — dialogs get IDM's fixed frame (3 px borders,
// 26 px caption, measured: 566×122 outer for a 560×93 client) instead of the padded modern
// frame (8 px, 31 px). Resizable windows keep their 8 px frame, as IDM's main window has.
//
// Also embeds the exe's icon (Explorer, taskbar before the window sets its own) and version
// info.
fn main() {
    commit();
    let os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    let windows = os == "windows";
    assert!(windows, "Orbit Download Manager supports Windows only");
    // Windows uses the Direct3D 12 backend (renderer.rs).
    println!("cargo::rustc-check-cfg=cfg(gpu)");
    println!("cargo::rustc-cfg=gpu");
    if windows && std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc") {
        println!("cargo:rustc-link-arg-bins=/SUBSYSTEM:WINDOWS,5.02");
    }
    // Embed the Orbit icon and Windows version resources.
    #[cfg(windows)]
    if windows {
        println!("cargo:rerun-if-changed=assets/icons/app/odm.ico");
        let mut res = winresource::WindowsResource::new();
        res.set_icon("assets/icons/app/odm.ico")
            .set("ProductName", "Orbit Download Manager")
            .set("FileDescription", "Orbit Download Manager")
            .set("CompanyName", "Mithila Prabashwara")
            .set(
                "LegalCopyright",
                "Copyright (C) 2026 Mithila Prabashwara. GPL-3.0-or-later.",
            );
        if let Err(e) = res.compile() {
            // A missing resource compiler must not break the build; the window still sets
            // its icon at runtime.
            println!("cargo:warning=exe icon not embedded: {e}");
        }
    }
}

/// The commit this is built from (`ODM_COMMIT`, "unknown" outside a git checkout), for Help →
/// About and problem reports.
fn commit() {
    println!("cargo:rerun-if-changed=../../.git/HEAD");
    println!("cargo:rerun-if-changed=../../.git/logs/HEAD");
    let hash = std::process::Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned())
        .filter(|h| !h.is_empty())
        .unwrap_or_else(|| "unknown".into());
    println!("cargo:rustc-env=ODM_COMMIT={hash}");
}
