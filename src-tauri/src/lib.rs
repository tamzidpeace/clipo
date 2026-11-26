pub mod clipboard;
pub mod models;
pub mod paste;
pub mod search;
pub mod snippets;
pub mod storage;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .run(tauri::generate_context!())
        .expect("error while running Clipo application");
}
