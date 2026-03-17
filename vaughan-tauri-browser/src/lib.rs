//! Vaughan dApp browser library (stub).

pub fn run() {
    tauri::Builder::default()
        .run(tauri::generate_context!())
        .expect("error running tauri application");
}
