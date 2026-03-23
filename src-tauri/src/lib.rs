mod auth;
mod conference;
mod connector;
mod session_bridge;

use auth::AuthState;
use conference::{
    cancel_generation, clear_room, create_room, delete_room,
    export_minutes, get_aichat_status, get_room_messages,
    list_rooms, save_rooms, send_message, set_room_mode,
    ConferenceEngine, load_rooms_from_disk,
};
use auth::{
    delete_api_key, get_auth_status, list_providers,
    open_console, open_login_window, save_api_key, test_connection,
    save_config_value, get_config_value,
};
use tauri::{Manager, RunEvent};
use tokio::sync::Mutex;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let mut engine = ConferenceEngine::default();
    engine.rooms = load_rooms_from_disk();

    tauri::Builder::default()
        .plugin(tauri_plugin_log::Builder::new().build())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_store::Builder::default().build())
        .manage(Mutex::new(engine))
        .manage(AuthState::new())
        .invoke_handler(tauri::generate_handler![
            create_room, list_rooms, delete_room, set_room_mode,
            get_room_messages, clear_room, send_message,
            cancel_generation, export_minutes, save_rooms, get_aichat_status,
            list_providers, get_auth_status, save_api_key, delete_api_key,
            test_connection, open_console, open_login_window,
            save_config_value, get_config_value,
            session_bridge::import_claude_session,
            session_bridge::import_grok_session,
            session_bridge::check_installed_apps,
        ])
        .setup(|app| {
            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                let mut interval = tokio::time::interval(std::time::Duration::from_secs(30));
                loop {
                    interval.tick().await;
                    let engine: tauri::State<Mutex<ConferenceEngine>> = handle.state();
                    let eng = engine.lock().await;
                    conference::save_rooms_to_disk(&eng.rooms).ok();
                }
            });
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error building app")
        .run(|app_handle, event| {
            if let RunEvent::Exit = event {
                let engine: tauri::State<Mutex<ConferenceEngine>> = app_handle.state();
                tauri::async_runtime::block_on(async {
                    let eng = engine.lock().await;
                    conference::save_rooms_to_disk(&eng.rooms).ok();
                });
            }
        });
}
