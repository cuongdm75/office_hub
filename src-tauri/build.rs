fn main() {
    // ── Build the Office Web Add-in UI ─────────────────────────────────────
    // Skip if SKIP_ADDIN_BUILD=1 (e.g. CI, or dist already up-to-date).
    if std::env::var("SKIP_ADDIN_BUILD").as_deref() != Ok("1") {
        let addin_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("workspace root not found")
            .join("office-addin");

        if addin_dir.join("package.json").exists() {
            println!("cargo:warning=Building office-addin UI...");

            let status = std::process::Command::new("cmd")
                .args(["/c", "npm run build"])
                .current_dir(&addin_dir)
                .status()
                .expect("Failed to run `npm run build` in office-addin/");

            if !status.success() {
                panic!(
                    "office-addin `npm run build` failed with exit code {:?}. \
                     Set SKIP_ADDIN_BUILD=1 to skip.",
                    status.code()
                );
            }

            println!("cargo:warning=office-addin UI built successfully.");
        } else {
            println!("cargo:warning=office-addin/package.json not found, skipping UI build.");
        }
    }

    // Re-run this build script only when add-in source files change
    println!("cargo:rerun-if-changed=../office-addin/src");
    println!("cargo:rerun-if-changed=../office-addin/index.html");
    println!("cargo:rerun-if-changed=../office-addin/package.json");
    println!("cargo:rerun-if-changed=build.rs");

    tauri_build::build()
}
