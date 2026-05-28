mod commands;
mod report;
mod types;

use commands::{
    cancel_detection, generate_batch_html_reports, generate_html_report, inspect_file,
    pause_detection, read_folder_mp4, start_detection,
};
use tauri::Manager;
use types::AppState;

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(AppState::default())
        .invoke_handler(tauri::generate_handler![
            read_folder_mp4,
            start_detection,
            pause_detection,
            cancel_detection,
            generate_html_report,
            generate_batch_html_reports,
            inspect_file
        ])
        .setup(|app| {
            if let Some(window) = app.get_webview_window("main") {
                window.set_title("VideoInspector Pro")?;
            }
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running VideoInspector Pro");
}
