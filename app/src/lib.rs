pub mod app_data;
pub mod commands;
pub mod error;
pub mod libsrc;
pub mod logging;
pub mod native;
pub mod state;

use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_http::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_store::Builder::new().build())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        // Only for its `log` command: the frontend's records reach `tracing` through `LogTracer`.
        .plugin(tauri_plugin_log::Builder::new().skip_logger().build())
        .plugin(native::init())
        .invoke_handler(tauri::generate_handler![
            native::storage_granted,
            native::request_storage,
            native::pick_folder,
            native::system_insets,
            commands::list_libraries,
            commands::add_local_library,
            commands::add_remote_library,
            commands::remove_library,
            commands::select_library,
            commands::open_library,
            commands::list_books,
            commands::list_directories,
            commands::get_progress,
            commands::set_progress,
            commands::get_state,
            commands::set_state,
            commands::get_config,
            commands::set_config,
            commands::rebuild_index,
            commands::get_cover,
            commands::get_book_file,
            logging::log_settings,
            logging::set_log_files,
            logging::list_log_files,
            logging::copy_logs,
            logging::save_logs,
            logging::save_log_file,
        ])
        .setup(|app| {
            // The log-file count is in the app data, so it's read before logging starts.
            let mut data = app_data::AppData::load(app.handle())?;
            app.manage(logging::init(app.handle(), data.log_files())?);
            for problem in data.take_load_problems() {
                tracing::warn!(%problem, "app data");
            }
            app.manage(state::AppState::new(data));
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while building tauri application");
}
