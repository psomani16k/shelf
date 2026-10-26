pub mod app_data;
pub mod commands;
pub mod error;
pub mod libsrc;
pub mod native;
pub mod state;

use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_http::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_store::Builder::new().build())
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
        ])
        .setup(|app| {
            if cfg!(debug_assertions) {
                app.handle().plugin(
                    tauri_plugin_log::Builder::default()
                        .level(log::LevelFilter::Debug)
                        // Everything below us is chatty at debug; keep theirs at info.
                        .level_for("tauri", log::LevelFilter::Info)
                        .level_for("wry", log::LevelFilter::Info)
                        .level_for("reqwest", log::LevelFilter::Info)
                        .level_for("hyper_util", log::LevelFilter::Info)
                        .level_for("notify", log::LevelFilter::Info)
                        .build(),
                )?;
            }
            app.manage(state::AppState::load(app.handle())?);
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while building tauri application");
}
