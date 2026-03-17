fn main() {
    // Ensure a minimal icon exists for `tauri::generate_context!()` even during early development.
    // This avoids compile-time panics when Tauri tries to load default icon paths.
    //
    // 1x1 transparent PNG.
    const ICON_PNG: &[u8] = &[
        0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48,
        0x44, 0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00,
        0x00, 0x1F, 0x15, 0xC4, 0x89, 0x00, 0x00, 0x00, 0x0A, 0x49, 0x44, 0x41, 0x54, 0x78,
        0x9C, 0x63, 0x00, 0x01, 0x00, 0x00, 0x05, 0x00, 0x01, 0x0D, 0x0A, 0x2D, 0xB4, 0x00,
        0x00, 0x00, 0x00, 0x49, 0x45, 0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82,
    ];

    let _ = std::fs::create_dir_all("icons");
    if std::fs::read("icons/icon.png").is_err() {
        let _ = std::fs::write("icons/icon.png", ICON_PNG);
    }

    // `tauri-build` on Windows requires a valid `.ico` for resource compilation.
    // During early development, this repo may carry a placeholder icon; in that case,
    // skip resource generation so `cargo check/build` can proceed.
    //
    // Before release, replace `icons/icon.ico` with a real Windows ICO (3.00 format).
    match std::fs::read("icons/icon.ico") {
        Ok(bytes) => {
            let looks_like_ico =
                bytes.len() >= 4 && bytes[0] == 0 && bytes[1] == 0 && bytes[2] == 1 && bytes[3] == 0;
            if !looks_like_ico {
                println!("cargo:warning=Skipping tauri-build (icons/icon.ico is not a valid ICO header)");
                return;
            }
        }
        Err(_) => {
            println!("cargo:warning=Skipping tauri-build (icons/icon.ico missing)");
            return;
        }
    }

    tauri_build::build()
}
